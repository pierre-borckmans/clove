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
                        },
                    );
                    0
                }
            };
            (tabs, idx, None)
        }
        Err(e) if tabs.is_empty() => (
            tabs,
            0,
            Some(format!(
                "{e:#}. Run `clove ui` inside a git repository, or `clove init` in one first."
            )),
        ),
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
            name: r.name,
            root: r.root,
        })
        .collect()
}

pub fn load(tx: Sender<Msg>, root: PathBuf) {
    std::thread::spawn(move || {
        let data = guarded(|| {
            let c = Clove::open(&root).map_err(|e| format!("{e:#}"))?;
            let status = c.status().map_err(|e| format!("{e:#}"))?;
            let clones = c.list(true).map_err(|e| format!("{e:#}"))?;
            Ok(Box::new(RepoData {
                root: root.clone(),
                status,
                clones,
            }))
        });
        let _ = tx.send(Msg::Loaded {
            root,
            data,
            repos: repo_tabs(),
        });
    });
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
        let _ = tx.send(Msg::Refs {
            root,
            refs: parse_refs(&out),
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
        Task::Init => c.init(r).map(Outcome::Golden).map_err(e),
        Task::Refresh => c.refresh(r).map(Outcome::Golden).map_err(e),
        Task::New(opts) => c.new_clone(&opts, r).map(Outcome::Clone).map_err(e),
        Task::Remove { name, opts } => c.remove(&name, &opts, r).map(|_| Outcome::Removed).map_err(e),
        Task::Mount { name } => c.mount(&name).map(Outcome::Mounted).map_err(e),
        Task::Unmount { name } => c.unmount(&name, false).map(|_| Outcome::Unmounted).map_err(e),
        Task::Gc { apply } => c.gc(apply).map(Outcome::Gc).map_err(e),
        Task::Doctor => unreachable!("handled above"),
    }
}

/// Turns a panic in a worker into an error message instead of a dead thread.
fn guarded<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
