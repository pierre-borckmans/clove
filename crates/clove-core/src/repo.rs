//! A repository under clove management: where its state lives and the records
//! clove keeps about the golden image and each clone.

use crate::backend::Backend;
use crate::config::Config;
use crate::events::Timing;
use crate::git;
use crate::util::{home, write_atomic};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Root of all clove state. `CLOVE_HOME` overrides `~/.clove`.
pub fn state_home() -> PathBuf {
    std::env::var_os("CLOVE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".clove"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRecord {
    pub root: PathBuf,
    pub name: String,
    pub backend: Backend,
    /// Name of the golden worktree's admin dir under `.git/worktrees/`.
    pub golden_admin: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenMeta {
    pub generation: u32,
    pub commit: String,
    pub base: String,
    pub created_at: u64,
    pub timings: Vec<Timing>,
    pub steps: Vec<String>,
    /// Warm steps left tracked files modified; clones would inherit the changes.
    pub dirty: bool,
    /// Fingerprint of each step's definition, so a changed command reruns on refresh.
    #[serde(default)]
    pub step_hashes: std::collections::BTreeMap<String, String>,
    /// Files copied from the main checkout (see `copy`).
    #[serde(default)]
    pub copied: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneMeta {
    pub name: String,
    pub branch: Option<String>,
    pub path: PathBuf,
    pub mount: PathBuf,
    pub base: String,
    pub golden_commit: String,
    pub golden_generation: u32,
    pub created_at: u64,
    pub timings: Vec<Timing>,
    /// Wall-clock time of `new` (fetch overlaps the mount, so not the sum of timings).
    #[serde(default)]
    pub total_ms: f64,
    #[serde(default)]
    pub failed_steps: Vec<String>,
    /// HEAD when setup steps last ran; `clove setup` diffs against it.
    #[serde(default)]
    pub setup_commit: Option<String>,
}

/// What `clove ls` shows for one clone.
#[derive(Debug, Clone, Serialize)]
pub struct CloneInfo {
    pub name: String,
    pub path: PathBuf,
    pub mounted: bool,
    pub branch: Option<String>,
    pub head: Option<String>,
    /// None when unmounted or not computed.
    pub dirty: Option<bool>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    /// Allocated size of the image, mostly blocks shared with the golden image.
    pub image_bytes: u64,
    /// Bytes this clone does not share: what removing it frees.
    pub own_bytes: u64,
    pub created_at: u64,
    pub golden_generation: u32,
    pub timings: Vec<Timing>,
    pub total_ms: f64,
    pub failed_steps: Vec<String>,
}

pub struct Clove {
    /// Main worktree of the repository.
    pub root: PathBuf,
    pub common_dir: PathBuf,
    pub name: String,
    pub id: String,
    pub state: PathBuf,
    pub config: Config,
}

pub fn repo_id(root: &Path, name: &str) -> String {
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    let hex: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("{name}-{hex}")
}

impl Clove {
    /// Finds the repository containing `dir` (a clone, the main checkout, or any worktree).
    pub fn discover(dir: &Path) -> Result<Self> {
        let common_dir = git::common_dir(dir).context("not inside a git repository")?;
        if common_dir.file_name().is_none_or(|n| n != ".git") {
            bail!(
                "bare repositories are not supported yet ({}); run clove from a normal checkout",
                common_dir.display()
            );
        }
        let root = common_dir
            .parent()
            .context("git dir has no parent")?
            .to_path_buf();
        Self::open(&root)
    }

    pub fn open(root: &Path) -> Result<Self> {
        let root = root
            .canonicalize()
            .with_context(|| format!("open {}", root.display()))?;
        let common_dir = root.join(".git");
        let name = root
            .file_name()
            .context("repository root has no name")?
            .to_string_lossy()
            .to_string();
        let id = repo_id(&root, &name);
        let state = state_home().join("repos").join(&id);
        let config = Config::load(&root, &name, || git::default_base(&root))?;
        Ok(Self {
            root,
            common_dir,
            name,
            id,
            state,
            config,
        })
    }

    // ---- paths ---------------------------------------------------------

    pub fn record_path(&self) -> PathBuf {
        self.state.join("repo.json")
    }
    pub fn lock_path(&self) -> PathBuf {
        self.state.join("lock")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.state.join("logs")
    }
    pub fn golden_dir(&self) -> PathBuf {
        self.state.join("golden")
    }
    pub fn golden_gen_dir(&self, generation: u32) -> PathBuf {
        self.golden_dir().join(format!("gen-{generation}"))
    }
    pub fn golden_mount(&self) -> PathBuf {
        self.state.join("mnt/golden")
    }
    pub fn golden_worktree(&self) -> PathBuf {
        self.golden_mount().join(&self.name)
    }
    pub fn clone_dir(&self, name: &str) -> PathBuf {
        self.state.join("clones").join(name)
    }
    /// Where clones appear: `clone_root`, unless the repo's Linux volume was
    /// mounted elsewhere (udisks picks /run/media/...), recorded in `state/mount`.
    pub fn clone_root(&self) -> PathBuf {
        #[cfg(target_os = "linux")]
        if let Some(p) = self.recorded_mount() {
            return p;
        }
        self.config.clone_root.clone()
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn recorded_mount(&self) -> Option<PathBuf> {
        let s = std::fs::read_to_string(self.state.join("mount")).ok()?;
        let s = s.trim();
        (!s.is_empty()).then(|| PathBuf::from(s))
    }
    pub fn clone_mount(&self, name: &str) -> PathBuf {
        self.clone_root().join(name)
    }
    /// The worktree sits one level inside the volume so the volume's own files
    /// (`.fseventsd`, `.Trashes`) stay out of `git status`, and the folder keeps
    /// the repo's name for tools that care.
    pub fn clone_worktree(&self, name: &str) -> PathBuf {
        self.clone_mount(name).join(&self.name)
    }
    pub fn admin_dir(&self, admin: &str) -> PathBuf {
        self.common_dir.join("worktrees").join(admin)
    }
    pub fn clone_admin(&self, name: &str) -> PathBuf {
        self.admin_dir(&format!("clove-{name}"))
    }

    // ---- records -------------------------------------------------------

    pub fn record(&self) -> Result<Option<RepoRecord>> {
        read_json(&self.record_path())
    }

    pub fn require_record(&self) -> Result<RepoRecord> {
        self.record()?
            .with_context(|| format!("{} has no golden image yet; run `clove init` first", self.name))
    }

    pub fn save_record(&self, r: &RepoRecord) -> Result<()> {
        write_json(&self.record_path(), r)
    }

    /// Current golden generation, if any.
    pub fn current_generation(&self) -> Result<Option<u32>> {
        let link = self.golden_dir().join("current");
        match std::fs::read_link(&link) {
            Ok(t) => {
                let s = t.to_string_lossy();
                let n = s
                    .strip_prefix("gen-")
                    .and_then(|n| n.parse().ok())
                    .with_context(|| format!("bad golden link {s}"))?;
                Ok(Some(n))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_current_generation(&self, generation: u32) -> Result<()> {
        let link = self.golden_dir().join("current");
        let tmp = self.golden_dir().join("current.tmp");
        let _ = std::fs::remove_file(&tmp);
        std::os::unix::fs::symlink(format!("gen-{generation}"), &tmp)?;
        std::fs::rename(&tmp, &link)?;
        Ok(())
    }

    pub fn golden_meta(&self) -> Result<Option<GoldenMeta>> {
        match self.current_generation()? {
            Some(g) => read_json(&self.golden_gen_dir(g).join("meta.json")),
            None => Ok(None),
        }
    }

    pub fn clone_meta(&self, name: &str) -> Result<Option<CloneMeta>> {
        read_json(&self.clone_dir(name).join("meta.json"))
    }

    pub fn clone_names(&self) -> Result<Vec<String>> {
        let dir = self.state.join("clones");
        let mut names = vec![];
        match std::fs::read_dir(&dir) {
            Ok(rd) => {
                for e in rd.flatten() {
                    if e.path().join("meta.json").exists() {
                        names.push(e.file_name().to_string_lossy().to_string());
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        names.sort();
        Ok(names)
    }
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(
            serde_json::from_slice(&b).with_context(|| format!("parse {}", path.display()))?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

pub fn write_json<T: Serialize>(path: &Path, v: &T) -> Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    write_atomic(path, &serde_json::to_vec_pretty(v)?)
}

/// Every repository clove knows about on this machine.
pub fn known_repos() -> Vec<RepoRecord> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(state_home().join("repos")) {
        for e in rd.flatten() {
            if let Ok(Some(r)) = read_json::<RepoRecord>(&e.path().join("repo.json")) {
                out.push(r);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Clone names: letters, digits, `.`, `_`, `-`. Branch slashes become dashes.
pub fn sanitize_name(branch: &str) -> String {
    let s: String = branch
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    s.trim_matches(|c| c == '-' || c == '.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(sanitize_name("pierre/feat-x"), "pierre-feat-x");
        assert_eq!(sanitize_name("a b/c"), "a-b-c");
        assert_eq!(sanitize_name("../x"), "x");
    }
}
