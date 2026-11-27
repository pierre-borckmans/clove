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

/// Processes using the filesystem mounted at `mount_point`. clove's own UIs
/// holding only directory fds don't count: those are FSEvents watch roots,
/// opened `O_EVTONLY`, which never keep a volume from unmounting.
pub fn users(mount_point: &Path) -> Vec<VolumeUser> {
    let Ok(out) = Command::new("lsof")
        .args(["-n", "-P", "-F", "pcft", "+f", "--"])
        .arg(mount_point)
        .output()
    else {
        return vec![];
    };
    parse_users(&String::from_utf8_lossy(&out.stdout), std::process::id() as i32)
}

fn parse_users(lsof: &str, me: i32) -> Vec<VolumeUser> {
    // pid → (command, holds only numbered directory fds)
    let mut seen: Vec<(i32, String, bool)> = vec![];
    let mut fd = String::new();
    for line in lsof.lines() {
        let Some((tag, v)) = line.split_at_checked(1) else {
            continue;
        };
        match tag {
            "p" => seen.push((v.parse().unwrap_or(0), String::new(), true)),
            "c" => {
                if let Some(cur) = seen.last_mut() {
                    cur.1 = v.to_string();
                }
            }
            "f" => fd = v.to_string(),
            "t" => {
                if let Some(cur) = seen.last_mut() {
                    let watch = v == "DIR" && fd.bytes().all(|b| b.is_ascii_digit());
                    cur.2 &= watch;
                }
            }
            _ => {}
        }
    }
    seen.into_iter()
        .filter(|(pid, c, only_dirs)| {
            *pid != 0
                && *pid != me
                // lsof inherits our cwd, which may be inside the volume.
                && c != "lsof"
                && !(*only_dirs && matches!(c.as_str(), "clove" | "clove-desktop"))
        })
        .map(|(pid, command, _)| VolumeUser { pid, command })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clove_watchers_dont_count_as_users() {
        let lsof = "p1\ncclove-desktop\nf512\ntDIR\nf513\ntDIR\n\
                    p2\ncclove\nfcwd\ntDIR\n\
                    p3\nczsh\nfcwd\ntDIR\n\
                    p4\ncnode\nf20\ntDIR\n\
                    p5\nclsof\nfcwd\ntDIR\n\
                    p6\ncclove\nf7\ntREG\n";
        let got: Vec<(i32, String)> = parse_users(lsof, 99)
            .into_iter()
            .map(|u| (u.pid, u.command))
            .collect();
        // The app's watch roots go; a clove with its cwd inside, any other
        // process, and a clove with an open file stay.
        assert_eq!(
            got,
            vec![
                (2, "clove".into()),
                (3, "zsh".into()),
                (4, "node".into()),
                (6, "clove".into())
            ]
        );
    }
}
