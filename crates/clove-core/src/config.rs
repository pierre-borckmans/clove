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
    /// Whether the service removes merged clones on its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune: Option<PruneFile>,
    /// How `clove update` brings a clone up to the base.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<UpdateFile>,
    /// How many nested projects' dependency installs detection warms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nested_installs: Option<u32>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PruneFile {
    /// The service removes clones whose PR merged, once they are clean and idle.
    pub auto: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateFile {
    /// rebase (default) or merge.
    pub strategy: Option<crate::update::Strategy>,
}

/// How a setting is edited, validated and stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Text,
    Bool,
    /// Whole seconds in the file; accepts "90", "5m", "2h".
    Seconds,
    /// A string like "24h" in the file, checked with [`parse_duration`].
    Duration,
    Int,
    /// "300G" style, binary units K/M/G/T.
    Size,
    Choice {
        options: &'static [&'static str],
    },
    /// Comma separated in the UIs, a TOML array in the file.
    List,
    /// `NAME=value, ...` in the UIs, a TOML table in the file.
    Map,
}

/// The value shown when no layer sets a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultValue {
    Value(&'static str),
    /// Depends on the repo: read from the resolved [`Config`] at the same key.
    Resolved,
}

/// One setting of [`FileConfig`]. CLI, TUI and app render and edit settings
/// from [`FIELDS`] alone, so a new key needs only its field and a descriptor.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Field {
    /// TOML key, dotted inside tables: `refresh.max_age`.
    pub key: &'static str,
    pub label: &'static str,
    /// Groups settings on screen, in order of first appearance.
    pub section: &'static str,
    pub kind: Kind,
    pub help: &'static str,
    pub default: DefaultValue,
}

const fn field(
    key: &'static str,
    label: &'static str,
    section: &'static str,
    kind: Kind,
    default: DefaultValue,
    help: &'static str,
) -> Field {
    Field {
        key,
        label,
        section,
        kind,
        help,
        default,
    }
}

use DefaultValue::{Resolved, Value as Is};

/// Every [`FileConfig`] key except [`FILE_ONLY`]; a test keeps them in sync.
pub const FIELDS: &[Field] = &[
    field(
        "base",
        "base",
        "clones",
        Kind::Text,
        Resolved,
        "ref new clones branch from, e.g. origin/main",
    ),
    field(
        "clone_root",
        "live in",
        "clones",
        Kind::Text,
        Resolved,
        "where clones mount; {repo}, {repo_parent} and ~ expand",
    ),
    field(
        "image_size",
        "image size",
        "clones",
        Kind::Size,
        Is("300G"),
        "maximum size of each image; sparse, only used blocks cost disk",
    ),
    field(
        "fetch",
        "fetch",
        "clones",
        Kind::Bool,
        Is("true"),
        "fetch the base ref before creating a clone",
    ),
    field(
        "fetch_interval",
        "fetch at most",
        "clones",
        Kind::Seconds,
        Is("5m"),
        "skip that fetch when the last one is this recent, e.g. 5m",
    ),
    field(
        "exec",
        "environment",
        "clones",
        Kind::Choice {
            options: &["auto", "direnv", "mise", "none"],
        },
        Is("auto"),
        "wrap step commands in direnv or mise; auto picks from the repo",
    ),
    field(
        "env",
        "env",
        "clones",
        Kind::Map,
        Is(""),
        "extra environment for every step and clove run: NAME=value, ...",
    ),
    field(
        "copy",
        "local files",
        "clones",
        Kind::List,
        Is("detected"),
        "files only your checkout has, copied into clones; comma separated pathspecs",
    ),
    field(
        "detect",
        "detect steps",
        "steps",
        Kind::Bool,
        Is("true"),
        "detect steps from repo files when none are configured",
    ),
    field(
        "nested_installs",
        "nested installs",
        "steps",
        Kind::Int,
        Is("10"),
        "nested projects with their own lockfile whose installs are warmed; above it, none are and doctor names them; 0 turns them off",
    ),
    field(
        "refresh.auto",
        "auto refresh",
        "freshness",
        Kind::Bool,
        Is("true"),
        "refresh in the background when a new clone finds the image stale",
    ),
    field(
        "refresh.max_age",
        "max age",
        "freshness",
        Kind::Duration,
        Is("24h"),
        "stale after this long, e.g. 12h, 2d",
    ),
    field(
        "refresh.max_behind",
        "max behind",
        "freshness",
        Kind::Int,
        Is("100"),
        "stale when the base is this many commits past the golden commit",
    ),
    field(
        "prune.auto",
        "auto prune",
        "cleanup",
        Kind::Bool,
        Is("false"),
        "the service removes clones whose PR merged, once clean and idle for an hour; clove log lists them",
    ),
    field(
        "update.strategy",
        "update by",
        "clones",
        Kind::Choice {
            options: &["rebase", "merge"],
        },
        Is("rebase"),
        "how clove update brings a clone's branch onto the latest base",
    ),
];

/// [`FileConfig`] keys edited in the file only (lists of tables).
pub const FILE_ONLY: &[&str] = &["step", "add_step"];

/// Resolved freshness policy.
#[derive(Debug, Clone, Serialize)]
pub struct RefreshPolicy {
    pub auto: bool,
    pub max_age_secs: u64,
    pub max_behind: u32,
}

/// Resolved cleanup policy.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PrunePolicy {
    pub auto: bool,
}

/// Resolved `clove update` policy.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdatePolicy {
    pub strategy: crate::update::Strategy,
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
    /// Nested projects detection installs at most.
    pub nested_installs: usize,
    /// Explicit copy pathspecs, or None to detect env-like ignored files.
    pub copy: Option<Vec<String>>,
    pub refresh: RefreshPolicy,
    pub prune: PrunePolicy,
    pub update: UpdatePolicy,
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

/// The config files for a repo, lowest precedence first, labelled for display:
/// personal defaults, the repo's shared file, personal overrides for this repo.
pub fn layer_paths(repo_root: &Path, repo_name: &str) -> [(&'static str, PathBuf); 3] {
    let user_path = user_config_path();
    let personal = user_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("repos")
        .join(format!("{repo_name}.toml"));
    [
        ("all repos", user_path),
        ("team", repo_root.join(REPO_FILE)),
        ("you, this repo", personal),
    ]
}

/// Starting point for a new config file: every key optional and commented out.
pub const TEMPLATE: &str = "\
# clove settings. Every key is optional; later layers win:
# ~/.config/clove/config.toml < .clove.toml (team) < ~/.config/clove/repos/<repo>.toml (you)
# https://github.com/pierre-borckmans/clove#configure

# base = \"origin/main\"
# copy = [\".env\", \"apps/*/.env.local\"]

# [refresh]
# max_age = \"24h\"
# max_behind = 100

# [prune]
# auto = true   # the service removes merged, clean, idle clones; `clove log` lists them

# [[add_step]]
# name = \"build-deps\"
# warm = \"pnpm run build-deps\"
# setup = \"pnpm run build-deps\"
# watch = [\"packages/shared\", \"pnpm-lock.yaml\"]
";

/// Creates a missing config file from [`TEMPLATE`] so an editor opens on
/// something useful. Leaves an existing file alone.
pub fn ensure_file(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, TEMPLATE).with_context(|| format!("write {}", path.display()))
}

pub(crate) fn read(path: &Path) -> Result<Option<FileConfig>> {
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
        let mut layers = vec![];
        for (_, path) in layer_paths(repo_root, repo_name) {
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
        let nested_installs =
            pick(&layers, |l| l.nested_installs).map_or(detect::DEFAULT_NESTED_INSTALLS, |n| n as usize);
        let configured = layers.iter().rev().find(|l| !l.steps.is_empty());
        let (mut steps, steps_detected) = match configured {
            Some(l) => (l.steps.clone(), false),
            None if detect_on => (detect::steps(repo_root, nested_installs), true),
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
            nested_installs,
            copy: pick(&layers, |l| l.copy.clone()),
            refresh: RefreshPolicy {
                auto: pick(&layers, |l| l.refresh.as_ref().and_then(|r| r.auto)).unwrap_or(true),
                max_age_secs: match pick(&layers, |l| l.refresh.as_ref().and_then(|r| r.max_age.clone())) {
                    Some(d) => parse_duration(&d)?,
                    None => 24 * 3600,
                },
                max_behind: pick(&layers, |l| l.refresh.as_ref().and_then(|r| r.max_behind)).unwrap_or(100),
            },
            prune: PrunePolicy {
                auto: pick(&layers, |l| l.prune.as_ref().and_then(|p| p.auto)).unwrap_or(false),
            },
            update: UpdatePolicy {
                strategy: pick(&layers, |l| l.update.as_ref().and_then(|u| u.strategy)).unwrap_or_default(),
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
    fn template_parses() {
        assert!(toml::from_str::<FileConfig>(TEMPLATE).is_ok());
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
mod schema_tests {
    use super::*;

    /// Leaf keys of a serialized config; tables recurse unless a field covers
    /// them whole (maps).
    fn leaves(t: &toml::Table, prefix: &str, out: &mut Vec<String>) {
        for (k, v) in t {
            let key = format!("{prefix}{k}");
            match v {
                toml::Value::Table(sub) if !FIELDS.iter().any(|f| f.key == key) => {
                    leaves(sub, &format!("{key}."), out)
                }
                _ => out.push(key),
            }
        }
    }

    #[test]
    fn every_file_key_has_a_descriptor() {
        let full = FileConfig {
            base: Some("b".into()),
            clone_root: Some("r".into()),
            image_size: Some("1G".into()),
            fetch: Some(true),
            fetch_interval: Some(1),
            detect: Some(true),
            exec: Some(Exec::Auto),
            env: [("A".to_string(), "1".to_string())].into(),
            copy: Some(vec![".env".into()]),
            refresh: Some(RefreshFile {
                auto: Some(true),
                max_age: Some("1h".into()),
                max_behind: Some(1),
            }),
            nested_installs: Some(4),
            prune: Some(PruneFile { auto: Some(true) }),
            update: Some(UpdateFile {
                strategy: Some(crate::update::Strategy::Merge),
            }),
            steps: vec![Step::default()],
            add_steps: vec![Step::default()],
        };
        let t = toml::Table::try_from(&full).unwrap();
        let mut keys = vec![];
        leaves(&t, "", &mut keys);
        for k in &keys {
            assert!(
                FIELDS.iter().any(|f| f.key == k) || FILE_ONLY.contains(&k.as_str()),
                "FileConfig key {k:?} has no descriptor in FIELDS (or FILE_ONLY)"
            );
        }
        for f in FIELDS {
            assert!(
                keys.iter().any(|k| k == f.key),
                "descriptor {:?} is not a FileConfig key",
                f.key
            );
        }
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
