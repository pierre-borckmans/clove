//! Configuration: `.clove.toml` in the repo (shared with the team) layered over
//! `~/.config/clove/config.toml` (personal defaults). Steps come from the repo
//! file, or are detected from the files in the repo when it lists none.

use crate::detect;
use crate::util::{expand, home};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const REPO_FILE: &str = ".clove.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FileConfig {
    /// Ref new clones branch from, e.g. `origin/main`. Default: the remote's HEAD.
    pub base: Option<String>,
    /// Where clone volumes mount. `{repo}`, `{repo_parent}` and `~` expand.
    pub clone_root: Option<String>,
    /// Maximum size of each image. Sparse, so only used blocks cost disk.
    pub image_size: Option<String>,
    /// Fetch the base ref before creating a clone.
    pub fetch: Option<bool>,
    /// Detect steps from repo files when none are configured.
    pub detect: Option<bool>,
    /// Wrap step commands in an environment: auto, direnv, mise or none.
    pub exec: Option<Exec>,
    /// Extra environment for every step and `clove run`.
    pub env: BTreeMap<String, String>,
    #[serde(rename = "step")]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Exec {
    #[default]
    Auto,
    Direnv,
    Mise,
    None,
}

/// One unit of environment preparation.
///
/// `warm` runs in the golden image on `init` and `refresh`, where it is allowed
/// to be slow. `setup` runs in each new clone and should be incremental; it is
/// skipped when none of the `watch` paths differ between the golden commit and
/// the clone's HEAD.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Step {
    pub name: String,
    pub warm: Option<String>,
    pub setup: Option<String>,
    /// Pathspecs; empty means setup always runs.
    pub watch: Vec<String>,
    /// Run inside the `exec` environment (direnv/mise). Off for steps that set it up.
    #[serde(default = "yes")]
    pub wrap: bool,
    /// Extra environment for this step only.
    pub env: BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}

/// Fully resolved configuration for one repository.
#[derive(Debug, Clone, Serialize)]
pub struct Config {
    pub base: String,
    pub clone_root: PathBuf,
    pub image_size: String,
    pub fetch: bool,
    pub exec: Exec,
    pub env: BTreeMap<String, String>,
    pub steps: Vec<Step>,
    pub steps_detected: bool,
    pub sources: Vec<PathBuf>,
}

pub fn user_config_path() -> PathBuf {
    std::env::var_os("CLOVE_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let base = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home().join(".config"));
            base.join("clove/config.toml")
        })
}

fn read(path: &Path) -> Result<Option<FileConfig>> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(
            toml::from_str(&s).with_context(|| format!("parse {}", path.display()))?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

impl Config {
    pub fn load(repo_root: &Path, repo_name: &str, default_base: impl FnOnce() -> String) -> Result<Self> {
        let mut sources = vec![];
        let user_path = user_config_path();
        let user = read(&user_path)?
            .inspect(|_| sources.push(user_path.clone()))
            .unwrap_or_default();
        let repo_path = repo_root.join(REPO_FILE);
        let repo = read(&repo_path)?
            .inspect(|_| sources.push(repo_path.clone()))
            .unwrap_or_default();

        let parent = repo_root
            .parent()
            .unwrap_or(repo_root)
            .to_string_lossy()
            .to_string();
        let root_tpl = std::env::var("CLOVE_CLONE_ROOT")
            .ok()
            .or(repo.clone_root.clone())
            .or(user.clone_root.clone())
            .unwrap_or_else(|| "{repo_parent}/{repo}.clove".into());
        let clone_root = expand(&root_tpl, &[("repo", repo_name), ("repo_parent", &parent)]);

        let mut env = user.env.clone();
        env.extend(repo.env.clone());

        let detect_on = repo.detect.or(user.detect).unwrap_or(true);
        let (steps, steps_detected) = if !repo.steps.is_empty() {
            (repo.steps.clone(), false)
        } else if !user.steps.is_empty() {
            (user.steps.clone(), false)
        } else if detect_on {
            (detect::steps(repo_root), true)
        } else {
            (vec![], false)
        };

        let exec = repo.exec.or(user.exec).unwrap_or_default();
        Ok(Self {
            base: repo.base.or(user.base).unwrap_or_else(default_base),
            clone_root,
            image_size: repo
                .image_size
                .or(user.image_size)
                .unwrap_or_else(|| "300G".into()),
            fetch: repo.fetch.or(user.fetch).unwrap_or(true),
            exec: detect::resolve_exec(exec, repo_root),
            env,
            steps,
            steps_detected,
            sources,
        })
    }

    /// TOML for a `.clove.toml` that pins the current (possibly detected) steps.
    pub fn to_repo_toml(&self) -> String {
        let file = FileConfig {
            base: Some(self.base.clone()),
            steps: self.steps.clone(),
            ..Default::default()
        };
        let body = toml::to_string_pretty(&file).unwrap_or_default();
        format!(
            "# clove: copy-on-write worktrees. https://github.com/pierre-borckmans/clove\n\
             # `warm` runs in the golden image (init/refresh); `setup` runs in each new\n\
             # clone, and only when a `watch` path changed since the golden commit.\n\n{body}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steps() {
        let c: FileConfig = toml::from_str(
            r#"
            base = "origin/main"
            [[step]]
            name = "pnpm"
            warm = "pnpm install"
            setup = "pnpm install --offline"
            watch = ["pnpm-lock.yaml"]
            "#,
        )
        .unwrap();
        assert_eq!(c.steps.len(), 1);
        assert!(c.steps[0].wrap);
        assert_eq!(c.steps[0].watch, vec!["pnpm-lock.yaml"]);
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(toml::from_str::<FileConfig>("nope = 1").is_err());
    }
}
