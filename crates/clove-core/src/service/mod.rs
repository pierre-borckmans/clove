//! The background service: remount clones at login; refresh stale golden
//! images when a repo's base is fetched (FETCH_HEAD changes) and on a slow
//! timer otherwise. launchd agents on macOS, systemd user units on Linux.

#[cfg(target_os = "macos")]
mod launchd;
#[cfg(target_os = "linux")]
mod systemd;

#[cfg(target_os = "macos")]
pub use launchd::{install, installed, sync, uninstall};
#[cfg(target_os = "linux")]
pub use systemd::{install, installed, sync, uninstall};

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Default fallback poll, for repos nobody fetches in.
pub const DEFAULT_EVERY: u64 = 4 * 3600;

/// What `install` sets up, for prompts.
pub const AGENTS: &str = if cfg!(target_os = "macos") {
    "two launchd agents"
} else {
    "systemd user units"
};

/// The `clove` CLI the agents run. From the CLI that is itself. From the app,
/// whose binary takes no CLI arguments, it is the CLI on PATH or in the usual
/// Homebrew and cargo places.
pub fn cli_exe() -> Result<PathBuf> {
    let me = std::env::current_exe()?;
    if me.file_name().is_some_and(|n| n == "clove") {
        return Ok(stable(&me));
    }
    let home = crate::util::home();
    crate::util::which("clove")
        .into_iter()
        .chain(
            [
                "/opt/homebrew/bin/clove",
                "/usr/local/bin/clove",
                "/home/linuxbrew/.linuxbrew/bin/clove",
            ]
            .map(PathBuf::from),
        )
        .chain([home.join(".cargo/bin/clove")])
        .find(|p| p.is_file())
        .map(|p| stable(&p))
        .context(
            "the service runs the clove CLI, and none was found: brew install pierre-borckmans/tap/clove",
        )
}

/// A path that survives upgrades: Homebrew's `<prefix>/bin/clove` link, not
/// the versioned `<prefix>/Cellar/clove/<version>/bin/clove` it points to,
/// which `brew upgrade` deletes.
fn stable(p: &Path) -> PathBuf {
    let real = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let s = real.to_string_lossy();
    if let Some((prefix, _)) = s.split_once("/Cellar/clove/") {
        let link = Path::new(prefix).join("bin/clove");
        if link.is_file() {
            return link;
        }
    }
    real
}

/// FETCH_HEAD of every repo clove knows: git rewrites it on each fetch and pull.
fn watch_paths() -> Vec<PathBuf> {
    crate::known_repos()
        .into_iter()
        .filter_map(|r| crate::Clove::open(&r.root).ok())
        .map(|c| c.common_dir.join("FETCH_HEAD"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_homebrew_install_runs_through_the_bin_link() {
        let t = tempfile::tempdir().unwrap();
        let cellar = t.path().join("Cellar/clove/0.1.14/bin");
        std::fs::create_dir_all(&cellar).unwrap();
        std::fs::write(cellar.join("clove"), "").unwrap();
        std::fs::create_dir_all(t.path().join("bin")).unwrap();
        std::os::unix::fs::symlink(cellar.join("clove"), t.path().join("bin/clove")).unwrap();
        let link = t.path().canonicalize().unwrap().join("bin/clove");
        assert_eq!(stable(&cellar.join("clove")), link);
        assert_eq!(stable(&t.path().join("bin/clove")), link);
        // Anything else stays where it is.
        let other = t.path().join("dev/clove");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, "").unwrap();
        assert_eq!(stable(&other), other.canonicalize().unwrap());
    }
}
