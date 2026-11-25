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

use std::path::PathBuf;

/// Default fallback poll, for repos nobody fetches in.
pub const DEFAULT_EVERY: u64 = 4 * 3600;

/// What `install` sets up, for prompts.
pub const AGENTS: &str = if cfg!(target_os = "macos") {
    "two launchd agents"
} else {
    "systemd user units"
};

/// FETCH_HEAD of every repo clove knows: git rewrites it on each fetch and pull.
fn watch_paths() -> Vec<PathBuf> {
    crate::known_repos()
        .into_iter()
        .filter_map(|r| crate::Clove::open(&r.root).ok())
        .map(|c| c.common_dir.join("FETCH_HEAD"))
        .collect()
}
