//! Disk image backends. An image holds one APFS volume with a full worktree in it.
//! Cloning the image file is O(1); mounting the clone gives an isolated
//! copy-on-write worktree.

use crate::util::{output, run};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// Apple Sparse Image Format via `diskutil image` (macOS 26+). Fastest.
    Asif,
    /// Classic `.sparseimage` via `hdiutil`, for older macOS.
    Sparse,
}

impl Backend {
    pub fn detect() -> Result<Self> {
        if !cfg!(target_os = "macos") {
            bail!("clove needs macOS (APFS clonefile + disk images); Linux support is planned");
        }
        // macOS 15 has `diskutil image` but not the ASIF format; check the help text.
        let asif = std::env::var("CLOVE_BACKEND")
            .map(|b| b != "sparse")
            .unwrap_or(true)
            && Command::new("diskutil")
                .args(["image", "create", "blank", "--help"])
                .output()
                .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("ASIF"))
                .unwrap_or(false);
        Ok(if asif { Self::Asif } else { Self::Sparse })
    }

    pub fn file_name(self) -> &'static str {
        match self {
            Self::Asif => "image.asif",
            Self::Sparse => "image.sparseimage",
        }
    }

    pub fn create(self, path: &Path, size: &str, volume_name: &str) -> Result<()> {
        match self {
            Self::Asif => run(Command::new("diskutil")
                .args([
                    "image",
                    "create",
                    "blank",
                    "--format",
                    "ASIF",
                    "--size",
                    size,
                    "--volumeName",
                    volume_name,
                    "--fs",
                    "APFS",
                ])
                .arg(path))?,
            Self::Sparse => run(Command::new("hdiutil")
                .args(["create", "-quiet", "-type", "SPARSE", "-fs", "APFS"])
                .args(["-size", size, "-volname", volume_name])
                .arg(path))?,
        };
        Ok(())
    }

    pub fn attach(self, image: &Path, mount_point: &Path) -> Result<()> {
        if is_mounted(mount_point) {
            return Ok(());
        }
        std::fs::create_dir_all(mount_point)
            .with_context(|| format!("create mount point {}", mount_point.display()))?;
        match self {
            Self::Asif => run(Command::new("diskutil")
                .args(["image", "attach", "--nobrowse", "--mountPoint"])
                .arg(mount_point)
                .arg(image))?,
            Self::Sparse => run(Command::new("hdiutil")
                .args(["attach", "-quiet", "-nobrowse", "-noautoopen", "-mountpoint"])
                .arg(mount_point)
                .arg(image))?,
        };
        if !is_mounted(mount_point) {
            bail!(
                "attached {} but nothing is mounted at {}",
                image.display(),
                mount_point.display()
            );
        }
        Ok(())
    }
}

/// Unmounts and detaches the image mounted at `mount_point`. With `force`,
/// processes holding files open lose them.
pub fn detach(mount_point: &Path, force: bool) -> Result<()> {
    if !is_mounted(mount_point) {
        return Ok(());
    }
    let plain = output(Command::new("diskutil").arg("eject").arg(mount_point));
    if plain.is_ok() {
        return Ok(());
    }
    if !force {
        let who = users(mount_point);
        let detail = if who.is_empty() {
            "a shell, editor or dev server is using it".to_string()
        } else {
            who.iter()
                .map(|u| format!("{} (pid {})", u.command, u.pid))
                .collect::<Vec<_>>()
                .join(", ")
        };
        return plain
            .map(|_| ())
            .with_context(|| format!("the volume is busy: {detail}; close it or pass --force"));
    }
    let dev = mounted_device(mount_point).unwrap_or_default();
    run(Command::new("diskutil")
        .args(["unmount", "force"])
        .arg(mount_point))?;
    if !dev.is_empty() {
        let _ = output(Command::new("diskutil").arg("eject").arg(&dev));
    }
    Ok(())
}

fn statfs(path: &Path) -> Option<libc::statfs> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: zeroed statfs is a valid out-parameter; c is NUL-terminated.
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statfs(c.as_ptr(), &mut st) };
    (rc == 0).then_some(st)
}

#[cfg(target_os = "macos")]
fn c_chars(buf: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = buf.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// True when a filesystem is mounted exactly at `path`.
#[cfg(target_os = "macos")]
pub fn is_mounted(path: &Path) -> bool {
    let Some(st) = statfs(path) else { return false };
    let on = PathBuf::from(c_chars(&st.f_mntonname));
    let want = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    on == want
}

#[cfg(not(target_os = "macos"))]
pub fn is_mounted(path: &Path) -> bool {
    let _ = statfs(path);
    false
}

#[cfg(target_os = "macos")]
fn mounted_device(path: &Path) -> Option<String> {
    statfs(path).map(|st| c_chars(&st.f_mntfromname))
}

#[cfg(not(target_os = "macos"))]
fn mounted_device(_: &Path) -> Option<String> {
    None
}

/// Filesystem type name of the volume holding `path` ("apfs", "hfs", ...).
#[cfg(target_os = "macos")]
pub fn fs_type(path: &Path) -> Option<String> {
    statfs(path).map(|st| c_chars(&st.f_fstypename))
}

#[cfg(not(target_os = "macos"))]
pub fn fs_type(_: &Path) -> Option<String> {
    None
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

/// Processes using the filesystem mounted at `mount_point`.
pub fn users(mount_point: &Path) -> Vec<VolumeUser> {
    let Ok(out) = Command::new("lsof")
        .args(["-n", "-P", "-F", "pc", "+f", "--"])
        .arg(mount_point)
        .output()
    else {
        return vec![];
    };
    let me = std::process::id() as i32;
    let mut users: Vec<VolumeUser> = vec![];
    let mut pid = 0;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse().unwrap_or(0);
        } else if let Some(c) = line.strip_prefix('c')
            && pid != 0
            && pid != me
            && !users.iter().any(|u| u.pid == pid)
        {
            users.push(VolumeUser {
                pid,
                command: c.to_string(),
            });
        }
    }
    users
}

/// Stops every process using the volume: SIGTERM, then SIGKILL after `grace`.
/// Only for volumes clove owns outright (the golden image while warming), where
/// anything still running is a daemon a warm step started (bazel, gradle, turbo).
pub fn stop_users(mount_point: &Path, grace: std::time::Duration) -> Vec<VolumeUser> {
    let users = users(mount_point);
    if users.is_empty() {
        return users;
    }
    for u in &users {
        // SAFETY: kill(2) with a pid we just read; no memory involved.
        unsafe { libc::kill(u.pid, libc::SIGTERM) };
    }
    let deadline = std::time::Instant::now() + grace;
    while std::time::Instant::now() < deadline {
        // SAFETY: signal 0 only checks existence.
        if users.iter().all(|u| unsafe { libc::kill(u.pid, 0) } != 0) {
            return users;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    for u in &users {
        // SAFETY: as above.
        unsafe { libc::kill(u.pid, libc::SIGKILL) };
    }
    users
}
