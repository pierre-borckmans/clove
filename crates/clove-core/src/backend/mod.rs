//! Storage backends. Each one gives ops a golden worktree to warm, numbered
//! golden generations, and O(1) clones of a generation.
//!
//! - macOS (`Asif`, `Sparse`): one disk image per clone. A clone is a
//!   `clonefile(2)` copy of the golden image file, mounted on its own.
//! - Linux (`Btrfs`): one btrfs image per repo, mounted once at the clone
//!   root. The golden worktree and each clone are subvolumes inside it.

#[cfg(target_os = "linux")]
pub mod btrfs;
#[cfg(target_os = "macos")]
mod image;
#[cfg(target_os = "linux")]
mod sys_linux;
#[cfg(target_os = "macos")]
mod sys_macos;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("clove supports macOS and Linux");

#[cfg(target_os = "linux")]
pub use sys_linux::{fs_type, is_mounted, users};
#[cfg(target_os = "macos")]
pub use sys_macos::{detach, fs_type, is_mounted, users};

use crate::repo::Clove;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// Apple Sparse Image Format via `diskutil image` (macOS 26+). Fastest.
    Asif,
    /// Classic `.sparseimage` via `hdiutil`, for older macOS.
    Sparse,
    /// One btrfs image per repo; clones are subvolume snapshots (Linux).
    Btrfs,
}

impl Backend {
    #[cfg(target_os = "macos")]
    pub fn detect() -> Result<Self> {
        image::detect()
    }

    #[cfg(target_os = "linux")]
    pub fn detect() -> Result<Self> {
        btrfs::detect()
    }

    /// The backend this OS uses, without probing tools. For cleanup paths
    /// that run before a repo has a record.
    fn platform_default() -> Self {
        if cfg!(target_os = "macos") {
            Self::Asif
        } else {
            Self::Btrfs
        }
    }

    pub fn file_name(self) -> &'static str {
        match self {
            Self::Asif => "image.asif",
            Self::Sparse => "image.sparseimage",
            Self::Btrfs => "pool.btrfs",
        }
    }

    /// The implementation of this backend on this OS.
    pub(crate) fn store(self) -> Result<&'static dyn Store> {
        match self {
            #[cfg(target_os = "macos")]
            Self::Asif => Ok(&image::ASIF),
            #[cfg(target_os = "macos")]
            Self::Sparse => Ok(&image::SPARSE),
            #[cfg(target_os = "linux")]
            Self::Btrfs => Ok(&btrfs::POOL),
            #[allow(unreachable_patterns)]
            other => bail!(
                "this repo's golden image uses the {other:?} backend, which this OS can't open; \
                 `clove deinit` and `clove init` again"
            ),
        }
    }
}

impl Clove {
    /// The store for this repo's recorded backend, or the OS default before init.
    pub(crate) fn store(&self) -> Result<&'static dyn Store> {
        match self.record()? {
            Some(r) => r.backend.store(),
            None => Backend::platform_default().store(),
        }
    }
}

impl Clove {
    /// Clones share one mounted volume per repo (Linux) rather than one each.
    pub fn shared_volume(&self) -> bool {
        self.record()
            .ok()
            .flatten()
            .map_or(Backend::platform_default(), |r| r.backend)
            == Backend::Btrfs
    }

    /// Unmounts the repo's shared volume unless something uses it (`force`
    /// unmounts lazily). Errors on macOS, where each clone is its own volume.
    pub fn unmount_volume(&self, force: bool) -> Result<()> {
        if !self.store()?.unmount_volume(self, force)? {
            bail!("each clone is its own volume here; unmount clones by name");
        }
        Ok(())
    }

    /// Linux: the /etc/fstab line that lets this repo's image mount at its
    /// clone root without sudo, when /etc/fstab doesn't have it yet.
    pub fn missing_fstab_line(&self) -> Option<String> {
        #[cfg(target_os = "linux")]
        if self.shared_volume() {
            let image = btrfs::image_path(self);
            if btrfs::fstab_target(&image).is_none() {
                return Some(btrfs::fstab_line(&image, &self.config.clone_root));
            }
        }
        None
    }

    /// Appends `line` to /etc/fstab through `sudo tee -a` (asks for a password).
    pub fn add_fstab_line(&self, line: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        return btrfs::add_fstab_line(line);
        #[cfg(not(target_os = "linux"))]
        bail!("no /etc/fstab line needed here ({line})")
    }
}

/// What ops needs from a backend. Golden generations are numbered as in
/// `golden/gen-N`; a clone is named, and `mount` is where its volume (macOS)
/// or subvolume (btrfs) appears.
pub(crate) trait Store: Sync {
    /// Directory holding the golden worktree (`<this>/<repo>`) while warming.
    fn golden_mount(&self, c: &Clove) -> PathBuf;
    /// Clears what an interrupted init or refresh left at the golden mount.
    fn golden_reset(&self, c: &Clove) -> Result<()>;
    /// Creates generation `g` empty (init).
    fn golden_create(&self, c: &Clove, g: u32) -> Result<()>;
    /// Makes generation `to` a writable copy of `from` (refresh).
    fn golden_derive(&self, c: &Clove, from: u32, to: u32) -> Result<()>;
    /// Makes generation `g` available at the golden mount.
    fn golden_attach(&self, c: &Clove, g: u32) -> Result<()>;
    /// Freezes generation `g` after warming, so clones can be taken from it.
    fn golden_seal(&self, c: &Clove, g: u32) -> Result<()>;
    /// Best-effort undo when init or refresh bails.
    fn golden_abort(&self, c: &Clove);
    /// True when an interrupted refresh left the golden image attached.
    fn golden_stuck(&self, c: &Clove) -> bool;
    /// Deletes generation `g`'s storage outside `golden/gen-N` (if any).
    fn golden_drop(&self, c: &Clove, g: u32) -> Result<()>;
    fn golden_bytes(&self, c: &Clove, g: u32) -> u64;

    /// O(1) copy of generation `g` for clone `name`.
    fn clone_create(&self, c: &Clove, g: u32, name: &str, mount: &Path) -> Result<()>;
    fn clone_attach(&self, c: &Clove, name: &str, mount: &Path) -> Result<()>;
    fn clone_detach(&self, c: &Clove, name: &str, mount: &Path, force: bool) -> Result<()>;
    /// Deletes the clone's storage outside `clones/<name>` (if any).
    fn clone_delete(&self, c: &Clove, name: &str, mount: &Path) -> Result<()>;
    fn clone_attached(&self, c: &Clove, name: &str, mount: &Path) -> bool;
    /// (bytes the clone references, bytes only it holds).
    fn clone_bytes(&self, c: &Clove, name: &str, mount: &Path) -> (u64, u64);

    /// Unmounts the volume all clones share, when the backend has one.
    /// Ok(false) when clones are mounted one by one (macOS).
    fn unmount_volume(&self, c: &Clove, force: bool) -> Result<bool>;

    /// Releases repo-wide storage before `deinit` deletes the state folder.
    fn deinit(&self, c: &Clove) -> Result<()>;
}

/// Device id of the volume holding `path`; two paths can share clones only if equal.
pub fn volume_id(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| m.dev())
}

/// A process with files (or its cwd) open on a volume.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VolumeUser {
    pub pid: i32,
    pub command: String,
}

/// A human name for a process holding a volume, from its full command line.
pub fn describe(u: &VolumeUser) -> String {
    let args = std::process::Command::new("ps")
        .args(["-o", "args=", "-p", &u.pid.to_string()])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
        .unwrap_or_default();
    let named = [
        ("fsmonitor--daemon", "git fsmonitor"),
        ("bazel", "bazel"),
        ("gradle", "gradle daemon"),
        ("kotlin-daemon", "kotlin daemon"),
        ("turbo", "turbo daemon"),
        ("nx/", "nx daemon"),
        ("watchman", "watchman"),
    ];
    match named.iter().find(|(needle, _)| args.contains(needle)) {
        Some((_, name)) if !name.contains(u.command.as_str()) => format!("{name} ({})", u.command),
        Some((_, name)) => name.to_string(),
        None => u.command.clone(),
    }
}

/// Stops every process using the volume: SIGTERM, then SIGKILL after `grace`.
/// Only for volumes clove owns outright (the golden image while warming), where
/// anything still running is a daemon a warm step started (bazel, gradle, turbo).
pub fn stop_users(mount_point: &Path, grace: std::time::Duration) -> Vec<VolumeUser> {
    let users = users(mount_point);
    stop(&users, grace);
    users
}

/// Background servers started from inside `mount` (a turbo, nx or bazel
/// daemon): their executable lives there and init is their parent, so no
/// terminal or editor owns them, and they can't outlive the volume. Left
/// alone, they hold a removed clone forever (and some spin once their files
/// are gone). Stops them and returns them.
pub fn stop_orphan_daemons(mount: &Path, grace: std::time::Duration) -> Vec<VolumeUser> {
    let roots = [
        mount.to_path_buf(),
        mount.canonicalize().unwrap_or(mount.to_path_buf()),
    ];
    let orphans: Vec<VolumeUser> = users(mount)
        .into_iter()
        .filter(|u| {
            proc_parent_and_exe(u.pid)
                .is_some_and(|(ppid, exe)| ppid == 1 && roots.iter().any(|r| exe.starts_with(r)))
        })
        .collect();
    stop(&orphans, grace);
    orphans
}

/// Every process: (pid, parent pid, executable).
pub fn processes() -> Vec<(i32, i32, PathBuf)> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir("/proc")
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok())
                    .filter_map(|pid| proc_parent_and_exe(pid).map(|(pp, exe)| (pid, pp, exe)))
                    .collect()
            })
            .unwrap_or_default()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let Ok(out) = std::process::Command::new("ps")
            .args(["-axo", "pid=,ppid=,comm="])
            .output()
        else {
            return vec![];
        };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(parse_ps_line)
            .collect()
    }
}

/// `  123     1 /path/may have spaces/bin` from `ps -o pid=,ppid=,comm=`.
#[cfg_attr(target_os = "linux", allow(dead_code))]
fn parse_ps_line(l: &str) -> Option<(i32, i32, PathBuf)> {
    let (pid, rest) = l.trim_start().split_once(char::is_whitespace)?;
    let (ppid, exe) = rest.trim_start().split_once(char::is_whitespace)?;
    Some((pid.parse().ok()?, ppid.parse().ok()?, PathBuf::from(exe.trim())))
}

/// Stops these processes: TERM, then KILL after `grace`.
pub fn stop_processes(pids: &[i32], grace: std::time::Duration) {
    let users: Vec<VolumeUser> = pids
        .iter()
        .map(|&pid| VolumeUser {
            pid,
            command: String::new(),
        })
        .collect();
    stop(&users, grace);
}

#[cfg(target_os = "linux")]
fn proc_parent_and_exe(pid: i32) -> Option<(i32, PathBuf)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // pid (comm) state ppid ...; comm may hold spaces and parens.
    let ppid = stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()?;
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    Some((ppid, exe))
}

#[cfg(not(target_os = "linux"))]
fn proc_parent_and_exe(pid: i32) -> Option<(i32, PathBuf)> {
    let out = std::process::Command::new("ps")
        .args(["-o", "ppid=,comm=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let (ppid, exe) = s.trim().split_once(' ')?;
    Some((ppid.trim().parse().ok()?, PathBuf::from(exe.trim())))
}

fn stop(users: &[VolumeUser], grace: std::time::Duration) {
    if users.is_empty() {
        return;
    }
    for u in users {
        // SAFETY: kill(2) with a pid we just read; no memory involved.
        unsafe { libc::kill(u.pid, libc::SIGTERM) };
    }
    let deadline = std::time::Instant::now() + grace;
    while std::time::Instant::now() < deadline {
        // SAFETY: signal 0 only checks existence.
        if users.iter().all(|u| unsafe { libc::kill(u.pid, 0) } != 0) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    for u in users {
        // SAFETY: as above.
        unsafe { libc::kill(u.pid, libc::SIGKILL) };
    }
}

/// Error for a busy volume, naming who holds it.
pub(crate) fn busy_error(who: &[VolumeUser]) -> String {
    let detail = if who.is_empty() {
        "a shell, editor or dev server is using it".to_string()
    } else {
        who.iter()
            .map(|u| format!("{} (pid {})", u.command, u.pid))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("the volume is busy: {detail}; close it or pass --force")
}

#[cfg(test)]
mod daemon_tests {
    use super::*;

    #[test]
    fn reads_a_process_parent_and_executable() {
        let mut child = std::process::Command::new("sleep").arg("5").spawn().unwrap();
        let (ppid, exe) = proc_parent_and_exe(child.id() as i32).unwrap();
        child.kill().unwrap();
        let _ = child.wait();
        assert_eq!(ppid, std::process::id() as i32);
        assert!(exe.to_string_lossy().ends_with("sleep"), "{}", exe.display());
    }

    #[test]
    fn parses_ps_lines_with_spaces_in_paths() {
        assert_eq!(
            parse_ps_line("  4242     1 /Users/a b/x.clove/c/bin/turbo"),
            Some((4242, 1, PathBuf::from("/Users/a b/x.clove/c/bin/turbo")))
        );
        assert_eq!(parse_ps_line("garbage"), None);
    }

    #[test]
    fn lists_this_process() {
        let me = std::process::id() as i32;
        let exe = std::env::current_exe().unwrap();
        let found = processes().into_iter().find(|(p, _, _)| *p == me).unwrap();
        assert_eq!(found.2.file_name(), exe.file_name());
    }

    #[test]
    fn a_daemon_with_a_live_parent_is_not_an_orphan() {
        let tmp = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .current_dir(tmp.path())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let stopped = stop_orphan_daemons(tmp.path(), std::time::Duration::from_secs(1));
        let alive = child.try_wait().unwrap().is_none();
        child.kill().unwrap();
        let _ = child.wait();
        assert!(stopped.is_empty());
        assert!(alive);
    }
}
