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
    /// Skip that fetch when the repo was fetched less than this many seconds ago.
    pub fetch_interval: Option<u64>,
    /// Detect steps from repo files when none are configured.
    pub detect: Option<bool>,
    /// Wrap step commands in an environment: auto, direnv, mise or none.
    pub exec: Option<Exec>,
    /// Extra environment for every step and `clove run`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Files that exist only in your checkout (`.env`, `*.local`), copied into the
    /// golden image before warming and into every new clone. Git pathspecs; globs
    /// allowed. Unset: env-like files git ignores are detected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copy: Option<Vec<String>>,
    /// When the golden image counts as stale, and whether clove refreshes it on its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh: Option<RefreshFile>,
    /// Replaces the detected steps.
    #[serde(rename = "step", skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,
    /// Appended after the detected or configured steps.
    #[serde(rename = "add_step", skip_serializing_if = "Vec::is_empty")]
    pub add_steps: Vec<Step>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RefreshFile {
    /// Refresh in the background when a `clove new` finds the golden image stale.
    pub auto: Option<bool>,
    /// Stale after this long, e.g. "12h", "2d".
    pub max_age: Option<String>,
    /// Stale when the base has moved this many commits past the golden commit.
    pub max_behind: Option<u32>,
}

/// Resolved freshness policy.
#[derive(Debug, Clone, Serialize)]
pub struct RefreshPolicy {
    pub auto: bool,
    pub max_age_secs: u64,
    pub max_behind: u32,
}

/// Parses "90", "30m", "12h", "2d" into seconds.
pub fn parse_duration(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num, mult) = match s.char_indices().last() {
        Some((i, 's')) => (&s[..i], 1),
        Some((i, 'm')) => (&s[..i], 60),
        Some((i, 'h')) => (&s[..i], 3600),
        Some((i, 'd')) => (&s[..i], 86_400),
        _ => (s, 1),
    };
    let n: u64 = num
        .trim()
        .parse()
        .with_context(|| format!("bad duration {s:?} (use 30m, 12h, 2d)"))?;
    Ok(n * mult)
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Step {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    /// Pathspecs; empty means setup always runs.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub watch: Vec<String>,
    /// Run inside the `exec` environment (direnv/mise). Off for steps that set it up.
    #[serde(skip_serializing_if = "is_true")]
    pub wrap: bool,
    /// Extra environment for this step only.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            name: String::new(),
            warm: None,
            setup: None,
            watch: vec![],
            wrap: true,
            env: BTreeMap::new(),
        }
    }
}

fn is_true(b: &bool) -> bool {
    *b
}

/// Fully resolved configuration for one repository.
#[derive(Debug, Clone, Serialize)]
pub struct Config {
    pub base: String,
    pub clone_root: PathBuf,
    pub image_size: String,
    pub fetch: bool,
    pub fetch_interval: u64,
    pub exec: Exec,
    pub env: BTreeMap<String, String>,
    pub steps: Vec<Step>,
    pub steps_detected: bool,
    /// Explicit copy pathspecs, or None to detect env-like ignored files.
    pub copy: Option<Vec<String>>,
    pub refresh: RefreshPolicy,
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
        // Lowest to highest precedence: personal defaults, the repo's shared
        // file, then personal overrides for this repo.
        let user_path = user_config_path();
        let personal_path = user_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("repos")
            .join(format!("{repo_name}.toml"));
        let mut layers = vec![];
        for path in [user_path, repo_root.join(REPO_FILE), personal_path] {
            if let Some(f) = read(&path)? {
                sources.push(path);
                layers.push(f);
            }
        }
        fn pick<T: Clone>(layers: &[FileConfig], f: impl Fn(&FileConfig) -> Option<T>) -> Option<T> {
            layers.iter().rev().find_map(f)
        }

        let parent = repo_root
            .parent()
            .unwrap_or(repo_root)
            .to_string_lossy()
            .to_string();
        let root_tpl = std::env::var("CLOVE_CLONE_ROOT")
            .ok()
            .or_else(|| pick(&layers, |l| l.clone_root.clone()))
            .unwrap_or_else(|| "{repo_parent}/{repo}.clove".into());
        let clone_root = expand(&root_tpl, &[("repo", repo_name), ("repo_parent", &parent)]);

        let mut env = BTreeMap::new();
        for l in &layers {
            env.extend(l.env.clone());
        }

        let detect_on = pick(&layers, |l| l.detect).unwrap_or(true);
        let configured = layers.iter().rev().find(|l| !l.steps.is_empty());
        let (mut steps, steps_detected) = match configured {
            Some(l) => (l.steps.clone(), false),
            None if detect_on => (detect::steps(repo_root), true),
            None => (vec![], false),
        };
        for l in &layers {
            steps.extend(l.add_steps.iter().cloned());
        }

        let exec = pick(&layers, |l| l.exec).unwrap_or_default();
        Ok(Self {
            base: pick(&layers, |l| l.base.clone()).unwrap_or_else(default_base),
            clone_root,
            image_size: pick(&layers, |l| l.image_size.clone()).unwrap_or_else(|| "300G".into()),
            fetch: pick(&layers, |l| l.fetch).unwrap_or(true),
            fetch_interval: pick(&layers, |l| l.fetch_interval).unwrap_or(300),
            exec: detect::resolve_exec(exec, repo_root),
            env,
            steps,
            steps_detected,
            copy: pick(&layers, |l| l.copy.clone()),
            refresh: RefreshPolicy {
                auto: pick(&layers, |l| l.refresh.as_ref().and_then(|r| r.auto)).unwrap_or(true),
                max_age_secs: match pick(&layers, |l| l.refresh.as_ref().and_then(|r| r.max_age.clone())) {
                    Some(d) => parse_duration(&d)?,
                    None => 24 * 3600,
                },
                max_behind: pick(&layers, |l| l.refresh.as_ref().and_then(|r| r.max_behind)).unwrap_or(100),
            },
            sources,
        })
    }

    /// TOML for a `.clove.toml` that pins the current (possibly detected) steps.
    pub fn to_repo_toml(&self) -> String {
        let file = FileConfig {
            base: Some(self.base.clone()),
            copy: self.copy.clone(),
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

#[cfg(test)]
mod default_tests {
    use super::*;

    #[test]
    fn detected_steps_wrap_by_default() {
        assert!(Step::default().wrap);
        let out = toml::to_string(&FileConfig {
            steps: vec![Step {
                name: "x".into(),
                ..Default::default()
            }],
            ..Default::default()
        })
        .unwrap();
        assert!(!out.contains("wrap"), "{out}");
    }
}

#[cfg(test)]
mod layer_tests {
    use super::*;

    #[test]
    fn personal_overrides_repo_and_add_steps_append() {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("app");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(
            repo.join(REPO_FILE),
            "base = \"origin/dev\"\nfetch_interval = 10\n",
        )
        .unwrap();
        let cfg = d.path().join("cfg");
        std::fs::create_dir_all(cfg.join("repos")).unwrap();
        std::fs::write(
            cfg.join("config.toml"),
            "fetch_interval = 99\nimage_size = \"5G\"\n",
        )
        .unwrap();
        std::fs::write(
            cfg.join("repos/app.toml"),
            "base = \"origin/main\"\n[[add_step]]\nname = \"build\"\nwarm = \"make\"\n",
        )
        .unwrap();
        // SAFETY: the only test that touches CLOVE_CONFIG.
        unsafe { std::env::set_var("CLOVE_CONFIG", cfg.join("config.toml")) };
        let c = Config::load(&repo, "app", || "HEAD".into()).unwrap();
        assert_eq!(c.base, "origin/main", "personal file wins");
        assert_eq!(c.fetch_interval, 10, "repo file beats user defaults");
        assert_eq!(c.image_size, "5G");
        let names: Vec<_> = c.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["pnpm", "build"], "detected steps plus add_step");
        assert!(c.steps_detected);
        assert_eq!(c.sources.len(), 3);
    }
}

#[cfg(test)]
mod duration_tests {
    #[test]
    fn durations() {
        assert_eq!(super::parse_duration("90").unwrap(), 90);
        assert_eq!(super::parse_duration("30m").unwrap(), 1800);
        assert_eq!(super::parse_duration("12h").unwrap(), 43_200);
        assert_eq!(super::parse_duration("2d").unwrap(), 172_800);
        assert!(super::parse_duration("soon").is_err());
    }
}
