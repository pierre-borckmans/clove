//! Tauri commands. Every call that touches disk or git runs on the blocking
//! pool so the webview never waits on the main thread.

use crate::ops::block;
use clove_core::doctor::{self, Check};
use clove_core::settings::{self, FieldState, Layer};
use clove_core::{CloneInfo, Clove, Config, Freshness, MergeCheck, RepoStatus, Verdict, known_repos};
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

pub type CmdResult<T> = Result<T, String>;

pub fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[derive(Serialize)]
pub struct RepoSummary {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
    pub clones: usize,
    pub mounted: usize,
    pub generation: Option<u32>,
    pub behind: Option<u32>,
    /// A new clone would rerun a step, or past the age/commit safety net.
    pub stale: bool,
    pub refreshing: bool,
    /// Clones the last PR check found merged and clean.
    pub prunable: usize,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn repos() -> CmdResult<Vec<RepoSummary>> {
    block(|| {
        Ok(known_repos()
            .into_iter()
            .map(|r| {
                let id = clove_core::repo::repo_id(&r.root, &r.name);
                match Clove::open(&r.root).and_then(|c| Ok((c.status()?, c.summary()))) {
                    Ok((s, sum)) => RepoSummary {
                        id: s.id,
                        name: s.name,
                        root: s.root,
                        clones: s.clones,
                        mounted: s.mounted,
                        generation: s.golden.as_ref().map(|g| g.meta.generation),
                        behind: s.golden.and_then(|g| g.behind),
                        stale: sum.stale,
                        refreshing: sum.refreshing,
                        prunable: sum.prunable,
                        error: None,
                    },
                    Err(e) => RepoSummary {
                        id,
                        name: r.name,
                        root: r.root,
                        clones: 0,
                        mounted: 0,
                        generation: None,
                        behind: None,
                        stale: false,
                        refreshing: false,
                        prunable: 0,
                        error: Some(err(e)),
                    },
                }
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn repo_status(root: PathBuf) -> CmdResult<RepoStatus> {
    block(move || Clove::open(&root)?.status()).await
}

#[tauri::command]
pub async fn clones(root: PathBuf, status: bool) -> CmdResult<Vec<CloneInfo>> {
    block(move || Clove::open(&root)?.list(status)).await
}

#[derive(Serialize)]
pub struct GitRef {
    pub name: String,
    /// `local`, `remote` or `tag`.
    pub kind: &'static str,
    pub date: u64,
    pub subject: String,
}

#[derive(Serialize)]
pub struct Refs {
    pub base: String,
    pub refs: Vec<GitRef>,
}

/// Branches, remote branches and tags, newest first, for autocomplete.
#[tauri::command]
pub async fn refs(root: PathBuf) -> CmdResult<Refs> {
    block(move || {
        let c = Clove::open(&root)?;
        let out = clove_core::git::out(
            &c.root,
            &[
                "for-each-ref",
                "--sort=-committerdate",
                "--count=400",
                "--format=%(refname)\t%(committerdate:unix)\t%(subject)",
                "refs/heads",
                "refs/remotes",
                "refs/tags",
            ],
        )?;
        let refs = out
            .lines()
            .filter_map(|l| {
                let mut it = l.splitn(3, '\t');
                let full = it.next()?;
                let date = it.next()?.parse().unwrap_or(0);
                let subject = it.next().unwrap_or("").to_string();
                let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
                    ("local", n)
                } else if let Some(n) = full.strip_prefix("refs/remotes/") {
                    ("remote", n)
                } else {
                    ("tag", full.strip_prefix("refs/tags/")?)
                };
                (!name.ends_with("/HEAD")).then(|| GitRef {
                    name: name.to_string(),
                    kind,
                    date,
                    subject,
                })
            })
            .collect();
        Ok(Refs {
            base: c.config.base.clone(),
            refs,
        })
    })
    .await
}

#[tauri::command]
pub async fn doctor_machine() -> CmdResult<Vec<Check>> {
    block(|| Ok(doctor::machine())).await
}

#[tauri::command]
pub async fn doctor_repo(root: PathBuf) -> CmdResult<Vec<Check>> {
    block(move || Ok(doctor::repo(&Clove::open(&root)?))).await
}

#[tauri::command]
pub async fn gc(root: PathBuf, apply: bool) -> CmdResult<Vec<String>> {
    block(move || Clove::open(&root)?.gc(apply)).await
}

#[tauri::command]
pub async fn mount_clone(root: PathBuf, name: String) -> CmdResult<PathBuf> {
    block(move || Clove::open(&root)?.mount(&name)).await
}

#[tauri::command]
pub async fn unmount_clone(root: PathBuf, name: String, force: bool) -> CmdResult<()> {
    block(move || Clove::open(&root)?.unmount(&name, force)).await
}

#[derive(Serialize)]
pub struct Discovered {
    pub root: PathBuf,
    pub name: String,
    pub initialized: bool,
    pub base: String,
    pub steps: Vec<String>,
}

/// Resolves a folder the user picked to its repository, without changing anything.
#[tauri::command]
pub async fn discover(dir: PathBuf) -> CmdResult<Discovered> {
    block(move || {
        let c = Clove::discover(&dir)?;
        Ok(Discovered {
            initialized: c.record()?.is_some(),
            base: c.config.base.clone(),
            steps: c.config.steps.iter().map(|s| s.name.clone()).collect(),
            root: c.root,
            name: c.name,
        })
    })
    .await
}

/// Apps the "Open in" actions can target, by bundle name under /Applications.
const APPS: &[(&str, &str)] = &[
    ("terminal", "Terminal"),
    ("ghostty", "Ghostty"),
    ("iterm", "iTerm"),
    ("warp", "Warp"),
    ("vscode", "Visual Studio Code"),
    ("cursor", "Cursor"),
    ("zed", "Zed"),
];

fn app_installed(bundle: &str) -> bool {
    let name = format!("{bundle}.app");
    [
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        clove_core::util::home().join("Applications"),
    ]
    .iter()
    .any(|d| d.join(&name).exists())
}

#[tauri::command]
pub async fn installed_apps() -> CmdResult<Vec<String>> {
    block(|| {
        Ok(APPS
            .iter()
            .filter(|(_, b)| app_installed(b))
            .map(|(id, _)| id.to_string())
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn open_in(path: PathBuf, app: String) -> CmdResult<()> {
    block(move || {
        let mut cmd = Command::new("open");
        if app == "finder" {
            cmd.arg(&path);
        } else {
            let bundle = APPS
                .iter()
                .find(|(id, _)| *id == app)
                .map(|(_, b)| *b)
                .ok_or_else(|| anyhow::anyhow!("unknown app {app}"))?;
            cmd.args(["-a", bundle]).arg(&path);
        }
        clove_core::util::output(&mut cmd)?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn copy_text(text: String) -> CmdResult<()> {
    block(move || {
        use std::io::Write;
        let mut child = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("pbcopy stdin"))?
            .write_all(text.as_bytes())?;
        child.wait()?;
        Ok(())
    })
    .await
}

#[derive(Serialize)]
pub struct ConfigLayer {
    pub label: &'static str,
    pub path: PathBuf,
    pub exists: bool,
}

#[derive(Serialize)]
pub struct RepoConfig {
    pub config: Config,
    /// Lowest precedence first: all repos, team, you for this repo.
    pub layers: Vec<ConfigLayer>,
    /// Files copied into clones (configured, or detected when `config.copy` is null).
    pub copy: Vec<String>,
    pub freshness: Option<Freshness>,
    /// clove's state folder on this machine.
    pub state_home: PathBuf,
    /// Every editable setting with its value per layer (see clove_core::settings).
    pub fields: Vec<FieldState>,
}

/// The resolved config for the settings page, with where each layer lives.
#[tauri::command]
pub async fn repo_config(root: PathBuf) -> CmdResult<RepoConfig> {
    block(move || {
        let c = Clove::open(&root)?;
        let status = c.status()?;
        let layers = clove_core::config::layer_paths(&c.root, &c.name)
            .into_iter()
            .map(|(label, path)| ConfigLayer {
                label,
                exists: path.exists(),
                path,
            })
            .collect();
        Ok(RepoConfig {
            fields: settings::fields(&c.root, &c.name, &c.config)?,
            config: c.config.clone(),
            layers,
            copy: status.copy,
            freshness: status.freshness,
            state_home: clove_core::state_home(),
        })
    })
    .await
}

/// Sets one setting in one config file, or removes it when `value` is null.
/// Returns what to tell the user.
#[tauri::command]
pub async fn config_set(root: PathBuf, layer: Layer, key: String, value: Option<String>) -> CmdResult<String> {
    block(move || {
        let c = Clove::open(&root)?;
        let path = layer.path(&c.root, &c.name);
        Ok(match value {
            Some(v) => format!("{key} = {}", settings::set(&path, &key, &v)?),
            None if settings::unset(&path, &key)? => format!("{key} reset"),
            None => format!("{key} isn't set in {}", layer.label()),
        })
    })
    .await
}

/// Opens a config file in the default text editor, creating it from the
/// shared template first so CLI, TUI and app start from the same file.
#[tauri::command]
pub async fn edit_config(path: PathBuf) -> CmdResult<()> {
    block(move || {
        clove_core::config::ensure_file(&path)?;
        clove_core::util::output(Command::new("open").arg("-t").arg(&path))?;
        Ok(())
    })
    .await
}

/// Whether both launchd agents are installed.
#[tauri::command]
pub async fn service_status() -> CmdResult<bool> {
    block(|| Ok(clove_core::service::installed().iter().all(|(_, on)| *on))).await
}

#[tauri::command]
pub async fn service_set(install: bool) -> CmdResult<bool> {
    block(move || {
        if install {
            clove_core::service::install(clove_core::service::DEFAULT_EVERY)?;
        } else {
            clove_core::service::uninstall()?;
        }
        Ok(install)
    })
    .await
}

/// The last PR check per clone (from `clove prune`, the service, or cleanup).
#[tauri::command]
pub async fn prs(root: PathBuf) -> CmdResult<Vec<MergeCheck>> {
    block(move || {
        Ok(Clove::open(&root)?
            .prune_cache()
            .map(|p| p.checks)
            .unwrap_or_default())
    })
    .await
}

/// Existing branches worth offering for a new clone (same rules as the CLI
/// and TUI: no base, scaffolding, merged or checked-out branches).
#[tauri::command]
pub async fn branch_candidates(root: PathBuf) -> CmdResult<Vec<clove_core::BranchCandidate>> {
    block(move || Ok(Clove::open(&root)?.branch_candidates())).await
}

/// Asks GitHub about the clones' PRs (without mounting any) and returns the
/// fresh checks; they also land in the cache `prs` reads.
#[tauri::command]
pub async fn check_prs(root: PathBuf) -> CmdResult<Vec<MergeCheck>> {
    block(move || Ok(Clove::open(&root)?.merge_checks(false)?.0)).await
}

#[derive(Serialize)]
pub struct Cleanup {
    pub leftovers: Vec<String>,
    pub checks: Vec<MergeCheck>,
    /// GitHub could not be asked; only local proofs counted.
    pub warning: Option<String>,
    pub removed: Vec<String>,
    /// Merged clones that could not be removed, with why.
    pub kept: Vec<String>,
}

/// Leftovers plus clones whose PR is merged. `apply` removes the provably
/// merged clones with their branches and the leftovers, like the TUI's cleanup.
#[tauri::command]
pub async fn cleanup(root: PathBuf, apply: bool) -> CmdResult<Cleanup> {
    block(move || {
        let c = Clove::open(&root)?;
        let leftovers = c.gc(apply)?;
        let (checks, warning) = c.merge_checks(true)?;
        let (mut removed, mut kept) = (vec![], vec![]);
        if apply {
            for m in checks.iter().filter(|m| m.verdict == Verdict::Prune) {
                let opts = clove_core::RemoveOptions {
                    force: false,
                    delete_branch: true,
                    branch_merged: true,
                };
                match c.remove(&m.clone, &opts, &clove_core::Silent) {
                    Ok(()) => removed.push(m.clone.clone()),
                    Err(e) => kept.push(format!("{}: {}", m.clone, err(e))),
                }
            }
        }
        Ok(Cleanup {
            leftovers,
            checks,
            warning,
            removed,
            kept,
        })
    })
    .await
}

#[derive(Serialize)]
pub struct ShellStatus {
    /// zsh, bash or fish; None when the login shell is something else.
    pub shell: Option<&'static str>,
    pub installed: bool,
    /// The rc file the line goes in.
    pub rc: Option<PathBuf>,
    pub line: Option<&'static str>,
}

fn shell_status_now() -> ShellStatus {
    use clove_core::shellrc;
    let shell = shellrc::detect();
    ShellStatus {
        shell,
        installed: shell.is_some_and(shellrc::installed),
        rc: shell.map(shellrc::path),
        line: shell.map(shellrc::line),
    }
}

/// Whether the shell integration (`clove new`/`cd`/`ui` changing directory) is in the rc file.
#[tauri::command]
pub async fn shell_status() -> CmdResult<ShellStatus> {
    block(|| Ok(shell_status_now())).await
}

/// Adds the integration line to the login shell's rc file (no-op when present).
#[tauri::command]
pub async fn shell_install() -> CmdResult<ShellStatus> {
    block(|| {
        let shell = clove_core::shellrc::detect().ok_or_else(|| {
            anyhow::anyhow!("unsupported login shell: add `clove shell-init` to your rc by hand")
        })?;
        clove_core::shellrc::install(shell)?;
        Ok(shell_status_now())
    })
    .await
}
