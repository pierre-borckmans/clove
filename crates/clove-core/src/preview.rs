//! What `clove init` would do for a repository, computed without changing
//! anything: for the confirm step of every "add repo" flow.

use crate::config::Exec;
use crate::detect::required_tool;
use crate::doctor::{self, Check, Level};
use crate::git;
use crate::repo::Clove;
use crate::util::which;
use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct PreviewStep {
    pub name: String,
    /// Runs once in the golden image.
    pub warm: Option<String>,
    /// Runs in each new clone (when its inputs changed).
    pub setup: Option<String>,
    /// The tool it needs on PATH, if any.
    pub tool: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InitPreview {
    pub root: PathBuf,
    pub name: String,
    /// Clove already has a golden image for it: init would refuse.
    pub initialized: bool,
    pub base: String,
    /// The base resolves to a commit (as of the last fetch).
    pub base_found: bool,
    pub steps: Vec<PreviewStep>,
    pub steps_detected: bool,
    /// Projects with their own lockfile left out: too many to warm by default.
    pub nested_skipped: Vec<String>,
    /// Local files copied into the image and clones.
    pub copy: Vec<String>,
    pub exec: Exec,
    /// Upper bound of each sparse image; it takes only what is written.
    pub image_size: String,
    /// Where clones will mount.
    pub clone_root: PathBuf,
    /// Where the images and clove's records go.
    pub state: PathBuf,
    /// Tools steps need that are not on PATH (as `clove doctor` reports them).
    pub missing_tools: Vec<String>,
    /// Machine problems from `clove doctor` (warnings and failures only).
    pub machine: Vec<Check>,
}

/// Previews `init` for the repository containing `dir`. Reads only.
pub fn init_preview(dir: &Path) -> Result<InitPreview> {
    let c = Clove::discover(dir)?;
    Ok(preview_of(&c, doctor::machine()))
}

pub fn preview_of(c: &Clove, machine: Vec<Check>) -> InitPreview {
    let steps: Vec<PreviewStep> = c
        .config
        .steps
        .iter()
        .map(|s| PreviewStep {
            name: s.name.clone(),
            warm: s.warm.clone(),
            setup: s.setup.clone(),
            tool: required_tool(s).map(str::to_string),
        })
        .collect();
    let nested = if c.config.steps_detected {
        crate::detect::plan(&c.root, c.config.nested_installs).skipped()
    } else {
        vec![]
    };
    let mut missing_tools = vec![];
    if c.config.exec == Exec::None {
        for t in steps.iter().filter_map(|s| s.tool.as_deref()) {
            if which(t).is_none() && !missing_tools.iter().any(|m| m == t) {
                missing_tools.push(t.to_string());
            }
        }
    }
    InitPreview {
        root: c.root.clone(),
        name: c.name.clone(),
        initialized: c.record().ok().flatten().is_some(),
        base_found: git::ref_exists(&c.root, &c.config.base),
        base: c.config.base.clone(),
        steps,
        steps_detected: c.config.steps_detected,
        nested_skipped: nested,
        copy: c.resolve_copy(),
        exec: c.config.exec,
        image_size: c.config.image_size.clone(),
        clone_root: c.clone_root(),
        state: c.state.clone(),
        missing_tools,
        machine: machine.into_iter().filter(|m| m.level != Level::Ok).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new(crate::git::bin())
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn previews_without_changing_anything() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path().canonicalize().unwrap().join("app");
        std::fs::create_dir_all(r.join("site")).unwrap();
        std::fs::write(r.join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(r.join("site/Cargo.lock"), "").unwrap();
        std::fs::write(r.join(".env"), "X=1").unwrap();
        std::fs::write(r.join(".gitignore"), ".env\n").unwrap();
        git(&r, &["init", "-q", "-b", "main"]);
        git(&r, &["add", "pnpm-lock.yaml", "site", ".gitignore"]);
        git(
            &r,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "x",
            ],
        );
        // Built by hand: Config::load would read the user's own config files.
        let state = d.path().join("state");
        let c = Clove {
            root: r.clone(),
            common_dir: r.join(".git"),
            name: "app".into(),
            id: "app-1".into(),
            state: state.clone(),
            config: crate::Config {
                base: "main".into(),
                clone_root: d.path().join("app.clove"),
                image_size: "300G".into(),
                fetch: true,
                fetch_interval: 300,
                spare: true,
                exec: Exec::None,
                env: Default::default(),
                steps: crate::detect::steps(&r, 10),
                steps_detected: true,
                nested_installs: 10,
                copy: None,
                refresh: crate::config::RefreshPolicy {
                    auto: true,
                    max_age_secs: 86_400,
                    max_behind: 100,
                },
                prune: Default::default(),
                update: Default::default(),
                sources: vec![],
            },
        };
        let machine = vec![
            Check {
                name: "git".into(),
                level: Level::Ok,
                detail: "2.50".into(),
                fix: None,
            },
            Check {
                name: "disk images".into(),
                level: Level::Warn,
                detail: "slow".into(),
                fix: None,
            },
        ];
        let p = preview_of(&c, machine);
        assert_eq!(p.name, "app");
        assert!(!p.initialized);
        assert!(p.base_found);
        let names: Vec<_> = p.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["pnpm", "cargo-site"]);
        assert_eq!(p.steps[0].warm.as_deref(), Some("pnpm install --frozen-lockfile"));
        assert_eq!(p.steps[1].tool.as_deref(), Some("cargo"));
        assert!(p.nested_skipped.is_empty());
        assert_eq!(p.copy, [".env"]);
        let machine: Vec<_> = p.machine.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(machine, ["disk images"], "only problems");
        assert!(!state.exists(), "a preview writes nothing");
    }

    #[test]
    fn outside_a_repo_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(init_preview(d.path()).is_err());
    }
}
