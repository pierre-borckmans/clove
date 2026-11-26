//! Tauri commands. Every call that touches disk or git runs on the blocking
//! pool so the webview never waits on the main thread.

use crate::ops::block;
use clove_core::doctor::{self, Check};
use clove_core::settings::{self, FieldState, Layer};
use clove_core::{CloneInfo, Clove, Config, Freshness, MergeCheck, OtherWorktree, RepoStatus, known_repos};
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

/// Drops remembered git answers: a manual refresh asks git again.
#[tauri::command]
pub fn forget_cache() {
    clove_core::memo::clear();
}

#[tauri::command]
pub async fn repo_status(root: PathBuf) -> CmdResult<RepoStatus> {
    block(move || Clove::open(&root)?.status()).await
}

#[tauri::command]
pub async fn clones(root: PathBuf, status: bool) -> CmdResult<Vec<CloneInfo>> {
    block(move || Clove::open(&root)?.list(status)).await
}

/// The repo's plain git worktrees: not the main checkout, clones or golden.
#[tauri::command]
pub async fn worktrees(root: PathBuf, status: bool) -> CmdResult<Vec<OtherWorktree>> {
    block(move || Clove::open(&root)?.other_worktrees(status)).await
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

/// Git repos clove doesn't manage yet, found in the usual code folders.
#[tauri::command]
pub async fn repo_candidates() -> CmdResult<Vec<clove_core::RepoCandidate>> {
    block(|| Ok(clove_core::candidates::find())).await
}

/// What `init` would do for the repo containing `dir`, without changing anything.
#[tauri::command]
pub async fn init_preview(dir: PathBuf) -> CmdResult<clove_core::InitPreview> {
    let dir = match dir.strip_prefix("~") {
        Ok(rest) => clove_core::util::home().join(rest),
        Err(_) => dir,
    };
    block(move || clove_core::preview::init_preview(&dir)).await
}

#[tauri::command]
pub async fn installed_apps() -> CmdResult<Vec<String>> {
    block(|| Ok(crate::apps::installed())).await
}

/// Opens a folder in an editor, a terminal, or the file manager (`finder`).
#[tauri::command]
pub async fn open_in(path: PathBuf, app: String) -> CmdResult<()> {
    block(move || crate::apps::open(&path, &app)).await
}

/// Opens the user's terminal in a clone stopped on conflicts, with the
/// resolver (`update.resolver`) started there with clove's prompt. Runs
/// through the login shell, so the resolver is found on the user's PATH.
#[tauri::command]
pub async fn resolve_in_terminal(root: PathBuf, name: String, terminal: String) -> CmdResult<()> {
    block(move || {
        let c = Clove::open(&root)?;
        let resolver = c
            .config
            .update
            .resolver
            .clone()
            .ok_or_else(|| anyhow::anyhow!("update.resolver is off"))?;
        let prompt = c.resolve_prompt(&name)?;
        let wt = c.info(&name, false)?.path;
        let dir = std::env::temp_dir().join(format!("clove-resolve-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let prompt_file = dir.join(format!("{name}.prompt"));
        std::fs::write(&prompt_file, prompt)?;
        let q = |p: &std::path::Path| format!("'{}'", p.to_string_lossy().replace('\'', r"'\''"));
        let script = dir.join(format!("{name}.command"));
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ncd {} || exit 1\nexec \"${{SHELL:-{}}}\" -ilc '{} \"$(cat \"$0\")\"' {}\n",
                q(&wt),
                if cfg!(target_os = "macos") {
                    "/bin/zsh"
                } else {
                    "/bin/sh"
                },
                resolver.replace('\'', r"'\''"),
                q(&prompt_file)
            ),
        )?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
        crate::apps::run_in_terminal(&terminal, &wt, &script)
    })
    .await
}

#[cfg(target_os = "macos")]
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

/// Linux has no one clipboard command (X11 or Wayland), so the app holds the
/// clipboard itself, which works under both.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn copy_text(app: tauri::AppHandle, text: String) -> CmdResult<()> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(text).map_err(|e| e.to_string())
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
pub async fn config_set(
    root: PathBuf,
    layer: Layer,
    key: String,
    value: Option<String>,
) -> CmdResult<String> {
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

/// Opens a config file in the default text editor (`xdg-open` on Linux),
/// creating it from the shared template first so CLI, TUI and app start from
/// the same file.
#[tauri::command]
pub async fn edit_config(path: PathBuf) -> CmdResult<()> {
    block(move || {
        clove_core::config::ensure_file(&path)?;
        if cfg!(target_os = "macos") {
            clove_core::util::output(Command::new("open").arg("-t").arg(&path))?;
            Ok(())
        } else {
            crate::apps::open_default(&path)
        }
    })
    .await
}

/// Whether the background service is installed: both launchd agents on
/// macOS, all the systemd user units on Linux.
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

/// Whether coding agents know clove; null when none is installed here.
#[tauri::command]
pub async fn agents_status() -> CmdResult<Option<clove_core::agents::State>> {
    block(|| {
        let ts = clove_core::agents::status()?;
        Ok((!ts.is_empty()).then(|| clove_core::agents::summary(&ts)))
    })
    .await
}

/// `clove agents install` / `remove`; returns the new state.
#[tauri::command]
pub async fn agents_set(install: bool) -> CmdResult<Option<clove_core::agents::State>> {
    block(move || {
        if install {
            clove_core::agents::install()?;
        } else {
            clove_core::agents::remove()?;
        }
        let ts = clove_core::agents::status()?;
        Ok((!ts.is_empty()).then(|| clove_core::agents::summary(&ts)))
    })
    .await
}

#[derive(Serialize)]
pub struct HistoryEntry {
    pub repo: String,
    pub root: PathBuf,
    #[serde(flatten)]
    pub removed: clove_core::history::Removed,
    /// The command that brings the branch back, when it was deleted.
    pub restore: Option<String>,
}

/// What clove removed, newest first, in every repo (`clove log --all-repos`).
#[tauri::command]
pub async fn history(limit: usize) -> CmdResult<Vec<HistoryEntry>> {
    block(move || {
        let mut v: Vec<HistoryEntry> = known_repos()
            .into_iter()
            .filter_map(|r| Clove::open(&r.root).ok())
            .flat_map(|c| {
                c.history().into_iter().map(move |e| HistoryEntry {
                    repo: c.name.clone(),
                    root: c.root.clone(),
                    restore: e.restore(),
                    removed: e,
                })
            })
            .collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.removed.at));
        v.truncate(limit);
        Ok(v)
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

/// Leftovers plus clones and plain worktrees whose PR is merged. `apply`
/// removes the provably merged ones with their branches and the leftovers,
/// like the TUI's cleanup.
#[tauri::command]
pub async fn cleanup(root: PathBuf, apply: bool) -> CmdResult<clove_core::Cleanup> {
    block(move || Clove::open(&root)?.cleanup(apply, &clove_core::Silent)).await
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
