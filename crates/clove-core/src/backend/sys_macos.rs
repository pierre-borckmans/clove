//! macOS mount queries (statfs), detaching with diskutil, volume users via lsof.

use super::{VolumeUser, busy_error};
use crate::util::{output, run};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Unmounts and detaches the image mounted at `mount_point`. With `force`,
/// processes holding files open lose them.
pub fn detach(mount_point: &Path, force: bool) -> Result<()> {
    if !is_mounted(mount_point) {
        return Ok(());
    }
    let mut plain = output(Command::new("diskutil").arg("eject").arg(mount_point));
    // Nobody we can see holds it: a system service (Spotlight on a fresh
    // volume) does, briefly. Wait it out before calling the volume busy.
    for wait_ms in [300, 700, 1500] {
        if plain.is_ok() || !users(mount_point).is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(wait_ms));
        plain = output(Command::new("diskutil").arg("eject").arg(mount_point));
    }
    if plain.is_ok() || !is_mounted(mount_point) {
        return Ok(());
    }
    if !force {
        let msg = busy_error(&users(mount_point));
        return plain.map(|_| ()).with_context(|| msg);
    }
    let dev = mounted_device(mount_point).unwrap_or_default();
    // A volume mounted moments ago can be held by Spotlight or fseventsd for a
    // beat, and even a forced unmount refuses then: stop what we can, retry.
    let mut last: Result<()> = Ok(());
    for attempt in 0..4 {
        if attempt > 0 {
            super::stop_users(mount_point, std::time::Duration::from_millis(500));
            std::thread::sleep(std::time::Duration::from_millis(500 * attempt));
        }
        last = run(Command::new("diskutil")
            .args(["unmount", "force"])
            .arg(mount_point))
        .map(|_| ());
        if last.is_ok() || !is_mounted(mount_point) {
            last = Ok(());
            break;
        }
    }
    last?;
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

fn c_chars(buf: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = buf.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// True when a filesystem is mounted exactly at `path`.
pub fn is_mounted(path: &Path) -> bool {
    let Some(st) = statfs(path) else { return false };
    let on = PathBuf::from(c_chars(&st.f_mntonname));
    let want = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    on == want
}

fn mounted_device(path: &Path) -> Option<String> {
    statfs(path).map(|st| c_chars(&st.f_mntfromname))
}

/// Filesystem type name of the volume holding `path` ("apfs", "hfs", ...).
pub fn fs_type(path: &Path) -> Option<String> {
    statfs(path).map(|st| c_chars(&st.f_fstypename))
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
            // lsof inherits our cwd, which may be inside the volume.
            && c != "lsof"
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
