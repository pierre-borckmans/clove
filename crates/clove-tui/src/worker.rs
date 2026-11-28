//! Background threads: loading repo state, reading refs, running operations.
//! Everything reports back through one channel as [`Msg`]s.

use crate::model::*;
use clove_core::{Clove, Event, known_repos};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// The repos to offer and which one to start on. Falls back to the first known
/// repo when `start` is not inside one.
pub fn discover(start: &Path) -> (Vec<RepoTab>, usize, Option<String>) {
    let mut tabs = repo_tabs();
    match Clove::discover(start) {
        Ok(c) => {
            let idx = match tabs.iter().position(|t| same(&t.root, &c.root)) {
                Some(i) => i,
                None => {
                    tabs.insert(
                        0,
                        RepoTab {
                            name: c.name.clone(),
                            root: c.root.clone(),
                            summary: None,
                        },
                    );
                    0
                }
            };
            (tabs, idx, None)
        }
        // With no repo at all the TUI opens on the add-repo screen.
        Err(_) => (tabs, 0, None),
    }
}

fn same(a: &Path, b: &Path) -> bool {
    a == b || a.canonicalize().ok().as_deref() == Some(b)
}

pub fn repo_tabs() -> Vec<RepoTab> {
    known_repos()
        .into_iter()
        .map(|r| RepoTab {
            summary: Clove::open(&r.root).ok().map(|c| c.summary()),
            name: r.name,
            root: r.root,
        })
        .collect()
}

/// Repos loaded once already by this process: later loads skip the preview.
static PREVIEWED: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> =
    std::sync::LazyLock::new(Default::default);

pub fn load(tx: Sender<Msg>, root: PathBuf) {
    std::thread::spawn(move || {
        // The first load of a repo runs git status in every clone, seconds
        // with many: show the list without it first.
        let first = PREVIEWED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(root.clone());
        if first && let Ok(d) = guarded(|| repo_data(&root, false)) {
            let _ = tx.send(Msg::Preview(d));
        }
        let data = guarded(|| repo_data(&root, true));
        let _ = tx.send(Msg::Loaded {
            root,
            data,
            repos: repo_tabs(),
            service: clove_core::service::installed().iter().all(|(_, on)| *on),
            agents: agents_state(),
        });
    });
}

/// Whether the coding agents here know clove; None when there are none.
pub fn agents_state() -> Option<clove_core::agents::State> {
    let ts = clove_core::agents::status().ok()?;
    (!ts.is_empty()).then(|| clove_core::agents::summary(&ts))
}

fn repo_data(root: &Path, git_status: bool) -> Result<Box<RepoData>, String> {
    let root = root.to_path_buf();
    let c = Clove::open(&root).map_err(|e| format!("{e:#}"))?;
    let status = c.status().map_err(|e| format!("{e:#}"))?;
    let clones = c.list(git_status).map_err(|e| format!("{e:#}"))?;
    let worktrees = c.other_worktrees(git_status).unwrap_or_default();
    let prs = c
        .prune_cache()
        .map(|p| p.checks.into_iter().map(|m| (m.clone.clone(), m)).collect())
        .unwrap_or_default();
    Ok(Box::new(RepoData {
        root: root.clone(),
        summary: c.summary(),
        status,
        clones,
        worktrees,
        fields: clove_core::settings::fields(&c.root, &c.name, &c.config).unwrap_or_default(),
        config: c.config.clone(),
        history: c.history().into_iter().take(200).collect(),
        created: c.created(),
        golden_builds: c.golden_builds(),
        prs,
        watch: {
            let mut t = c.watch_targets();
            t.push(clove_core::watch::repos_target());
            t
        },
    }))
}

/// Asks GitHub about the clones' PRs; the result lands in the prune cache,
/// which the next load reads. Unmounted clones are not mounted for this.
pub fn check_prs(tx: Sender<Msg>, root: PathBuf) {
    std::thread::spawn(move || {
        let _ = guarded(|| {
            let c = Clove::open(&root).map_err(|e| format!("{e:#}"))?;
            c.merge_checks(false).map_err(|e| format!("{e:#}"))
        });
        let _ = tx.send(Msg::PrsChecked);
    });
}

/// The latest release: from the hourly cache unless `force`.
pub fn check_upgrade(tx: Sender<Msg>, force: bool) {
    std::thread::spawn(move || {
        if let Ok(status) = guarded(|| Ok(clove_core::upgrade::check(force))) {
            let _ = tx.send(Msg::Upgrade {
                status: Box::new(status),
                forced: force,
            });
        }
    });
}

/// Reads a clone's changes, parses them, and starts highlighting.
pub fn load_diff(tx: Sender<Msg>, root: PathBuf, name: String) {
    std::thread::spawn(move || {
        let result = clove_core::Clove::open(&root)
            .and_then(|c| c.diff(&name))
            .map(|d| {
                let data = std::sync::Arc::new(crate::diffview::DiffData::new(d));
                let hl = crate::diffview::highlight(data.clone(), tx.clone());
                (data, hl)
            })
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(Msg::Diff { root, name, result });
    });
}

/// A clone's checkpoints, newest first.
pub fn load_checkpoints(tx: Sender<Msg>, root: PathBuf, name: String) {
    std::thread::spawn(move || {
        let result = guarded(|| {
            let c = Clove::open(&root).map_err(|e| format!("{e:#}"))?;
            let mut v = c.checkpoints(&name).map_err(|e| format!("{e:#}"))?;
            v.reverse();
            Ok(v)
        });
        let _ = tx.send(Msg::Checkpoints { root, name, result });
    });
}

/// The processes a "volume is busy" refusal names (`backend::busy_error`).
pub fn busy_users(err: &str) -> Option<Vec<String>> {
    let rest = &err[err.find("the volume is busy: ")? + "the volume is busy: ".len()..];
    let who = rest.split("; close it").next().unwrap_or(rest).trim();
    if who.is_empty() || who.starts_with("a shell, editor or dev server") {
        return Some(vec![]);
    }
    Some(who.split(", ").map(str::to_string).collect())
}

pub fn load_refs(tx: Sender<Msg>, root: PathBuf) {
    std::thread::spawn(move || {
        let out = clove_core::git::out(
            &root,
            &[
                "for-each-ref",
                "--sort=-committerdate",
                "--format=%(refname)",
                "refs/heads",
                "refs/remotes",
                "refs/tags",
            ],
        )
        .unwrap_or_default();
        let candidates = Clove::open(&root)
            .map(|c| c.branch_candidates())
            .unwrap_or_default();
        let _ = tx.send(Msg::Refs {
            root,
            refs: parse_refs(&out),
            candidates,
        });
    });
}

pub fn parse_refs(out: &str) -> Vec<GitRef> {
    out.lines()
        .filter_map(|l| {
            if let Some(n) = l.strip_prefix("refs/heads/") {
                Some(GitRef {
                    name: n.into(),
                    kind: RefKind::Local,
                })
            } else if let Some(n) = l.strip_prefix("refs/remotes/") {
                (!n.ends_with("/HEAD") && n.contains('/')).then(|| GitRef {
                    name: n.into(),
                    kind: RefKind::Remote,
                })
            } else {
                l.strip_prefix("refs/tags/").map(|n| GitRef {
                    name: n.into(),
                    kind: RefKind::Tag,
                })
            }
        })
        .collect()
}

pub fn run_task(tx: Sender<Msg>, id: u64, root: Option<PathBuf>, task: Task) {
    std::thread::spawn(move || {
        let events = tx.clone();
        let reporter = move |e: Event| {
            let _ = events.send(Msg::Event(id, e));
        };
        let res = guarded(|| execute(root.as_deref(), task, &reporter));
        let _ = tx.send(Msg::Done(id, res));
    });
}

fn execute(root: Option<&Path>, task: Task, r: &dyn clove_core::Reporter) -> Result<Outcome, String> {
    let e = |e: anyhow::Error| format!("{e:#}");
    if let Task::Service { install } = task {
        let res = if install {
            clove_core::service::install(clove_core::service::DEFAULT_EVERY).map(|_| ())
        } else {
            clove_core::service::uninstall().map(|_| ())
        };
        return res.map(|_| Outcome::Service(install)).map_err(e);
    }
    if let Task::Agents { install } = task {
        let (opencode_skipped, skipped_foreign) = if install {
            let report = clove_core::agents::install_with_report().map_err(e)?;
            (report.opencode_skipped, report.skipped.len())
        } else {
            clove_core::agents::remove().map_err(e)?;
            (false, 0)
        };
        return Ok(Outcome::Agents {
            state: agents_state(),
            install,
            opencode_skipped,
            skipped_foreign,
        });
    }
    if let Task::ShellInstall = task {
        let sh = clove_core::shellrc::detect().ok_or("can't tell your shell from $SHELL")?;
        return clove_core::shellrc::install(sh)
            .map(|(p, changed)| Outcome::ShellInstalled(p, changed))
            .map_err(e);
    }
    if let Task::Upgrade = task {
        return clove_core::upgrade::upgrade(r, &Default::default())
            .map(Outcome::Upgraded)
            .map_err(e);
    }
    if let Task::Doctor = task {
        let mut checks = clove_core::doctor::machine();
        if let Some(root) = root
            && let Ok(c) = Clove::open(root)
        {
            checks.extend(clove_core::doctor::repo(&c));
        }
        return Ok(Outcome::Doctor(checks));
    }
    let root = root.ok_or("no repository")?;
    let c = Clove::open(root).map_err(e)?;
    match task {
        Task::Init => c
            .init(r)
            .inspect(|_| c.spawn_spare_fill())
            .map(Outcome::Golden)
            .map_err(e),
        Task::Refresh => c
            .refresh(r)
            .inspect(|_| c.spawn_spare_fill())
            .map(Outcome::Golden)
            .map_err(e),
        Task::New(opts) => c
            .new_clone(&opts, r)
            .inspect(|_| c.spawn_spare_fill())
            .map(Outcome::Clone)
            .map_err(e),
        Task::Remove { name, opts } => match c.remove_or_defer(&name, &opts, r).map_err(e)? {
            clove_core::Removal::Removed => Ok(Outcome::Removed),
            clove_core::Removal::WhenFree(users) => {
                spawn_pending_waiter(&c.root);
                Ok(Outcome::RemovedWhenFree(users))
            }
        },
        Task::RemoveWorktree {
            path,
            delete_branch,
            force,
            unlock,
            ..
        } => {
            let opts = clove_core::RemoveOptions {
                delete_branch,
                force,
                unlock,
                ..Default::default()
            };
            c.remove_worktree(&path, &opts, r)
                .map(|_| Outcome::Removed)
                .map_err(e)
        }
        Task::RemoveTree { name, opts, forks } => {
            let mut users = vec![];
            // Deepest first, then the clone they came from.
            for n in forks.iter().chain([&name]) {
                match c
                    .remove_or_defer(n, &opts, r)
                    .map_err(|err| format!("{n}: {err:#}"))?
                {
                    clove_core::Removal::Removed => {}
                    clove_core::Removal::WhenFree(u) => users.extend(u),
                }
            }
            if users.is_empty() {
                Ok(Outcome::Removed)
            } else {
                spawn_pending_waiter(&c.root);
                users.dedup();
                Ok(Outcome::RemovedWhenFree(users))
            }
        }
        Task::Checkpoint { name } => c.checkpoint(&name, None, r).map(Outcome::Checkpointed).map_err(e),
        Task::Rewind { name, id, force } => {
            let opts = clove_core::RewindOptions {
                id: Some(id),
                force,
                no_save: false,
            };
            c.rewind(&name, &opts, r).map(Outcome::Rewound).map_err(e)
        }
        Task::Fork { from, name } => {
            let opts = clove_core::ForkOptions {
                names: vec![name],
                ..Default::default()
            };
            c.fork(&from, &opts, r)
                .map_err(e)?
                .pop()
                .map(Outcome::Forked)
                .ok_or_else(|| "fork made nothing".to_string())
        }
        Task::Adopt { path, .. } => c
            .adopt(&path, &clove_core::AdoptOptions::default(), r)
            .map(Outcome::Clone)
            .map_err(e),
        Task::Mount { name } => c.mount(&name).map(Outcome::Mounted).map_err(e),
        Task::Update { name, step } => match step {
            UpdateStep::Run => c
                .update(&name, &clove_core::update::UpdateOptions::default(), r)
                .map(|u| Outcome::Updated(Some(u))),
            UpdateStep::Continue => c.update_continue(&name, r).map(|u| Outcome::Updated(Some(u))),
            UpdateStep::Abort => c.update_abort(&name).map(|()| Outcome::Updated(None)),
        }
        .map_err(e),
        Task::Unmount { name } => c.unmount(&name, false).map(|_| Outcome::Unmounted).map_err(e),
        Task::Gc { apply } => c.cleanup(apply, r).map(Outcome::Gc).map_err(e),
        Task::Doctor | Task::Service { .. } | Task::Agents { .. } | Task::ShellInstall | Task::Upgrade => {
            unreachable!("handled above")
        }
    }
}

/// A detached `clove remove-pending` (this binary), outside any clone, that
/// removes marked clones once nothing uses them.
fn spawn_pending_waiter(root: &Path) {
    let Ok(exe) = std::env::current_exe() else { return };
    use std::os::unix::process::CommandExt;
    let _ = std::process::Command::new(exe)
        .arg("-C")
        .arg(root)
        .arg("remove-pending")
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn();
}

/// Turns a panic in a worker into an error message instead of a dead thread.
pub(crate) fn guarded<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match std::panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown".into());
            Err(format!("internal error: {msg}"))
        }
    }
}

/// Writes one setting (see [`ConfigWrite`]). Returns what to tell the user.
pub fn write_config(w: &ConfigWrite) -> anyhow::Result<String> {
    use clove_core::settings;
    let label = w.layer.label();
    Ok(match &w.value {
        Some(v) => format!("{} = {} · {label}", w.key, settings::set(&w.path, w.key, v)?),
        None if settings::unset(&w.path, w.key)? => format!("{} reset · {label}", w.key),
        None => format!("{} isn't set in {label}", w.key),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_who_holds_a_busy_clone() {
        assert_eq!(
            busy_users("rewind: the volume is busy: zsh (pid 12), node (pid 40); close it or pass --force"),
            Some(vec!["zsh (pid 12)".to_string(), "node (pid 40)".to_string()])
        );
        assert_eq!(
            busy_users(
                "the volume is busy: a shell, editor or dev server is using it; close it or pass --force"
            ),
            Some(vec![])
        );
        assert_eq!(busy_users("no such checkpoint"), None);
    }

    #[test]
    fn parses_refs() {
        let r = parse_refs(
            "refs/heads/main\nrefs/remotes/origin/HEAD\nrefs/remotes/origin/feat/x\nrefs/tags/v1\nrefs/remotes/origin\n",
        );
        assert_eq!(
            r,
            vec![
                GitRef {
                    name: "main".into(),
                    kind: RefKind::Local
                },
                GitRef {
                    name: "origin/feat/x".into(),
                    kind: RefKind::Remote
                },
                GitRef {
                    name: "v1".into(),
                    kind: RefKind::Tag
                },
            ]
        );
    }
}
