//! Tauri commands. Every call that touches disk or git runs on the blocking
//! pool so the webview never waits on the main thread.

use crate::ops::block;
use clove_core::doctor::{self, Check};
use clove_core::{CloneInfo, Clove, RepoStatus, known_repos};
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
    pub error: Option<String>,
}

#[tauri::command]
pub async fn repos() -> CmdResult<Vec<RepoSummary>> {
    block(|| {
        Ok(known_repos()
            .into_iter()
            .map(|r| {
                let id = clove_core::repo::repo_id(&r.root, &r.name);
                match Clove::open(&r.root).and_then(|c| c.status()) {
                    Ok(s) => RepoSummary {
                        id: s.id,
                        name: s.name,
                        root: s.root,
                        clones: s.clones,
                        mounted: s.mounted,
                        generation: s.golden.as_ref().map(|g| g.meta.generation),
                        behind: s.golden.and_then(|g| g.behind),
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
