//! Linux mount queries (/proc/self/mountinfo, statfs) and volume users via /proc.

use super::VolumeUser;
use std::path::{Path, PathBuf};

fn statfs(path: &Path) -> Option<libc::statfs> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: zeroed statfs is a valid out-parameter; c is NUL-terminated.
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statfs(c.as_ptr(), &mut st) };
    (rc == 0).then_some(st)
}

pub(crate) const BTRFS_MAGIC: u64 = 0x9123_683E;

/// statfs f_type of the filesystem holding `path`.
pub(crate) fn fs_magic(path: &Path) -> Option<u64> {
    #[allow(clippy::unnecessary_cast)]
    statfs(path).map(|st| st.f_type as u64)
}

/// Bytes in use on the filesystem holding `path`.
pub(crate) fn fs_used_bytes(path: &Path) -> Option<u64> {
    #[allow(clippy::unnecessary_cast)]
    statfs(path).map(|st| (st.f_blocks as u64).saturating_sub(st.f_bfree as u64) * st.f_bsize as u64)
}

/// Filesystem type name of the volume holding `path` ("btrfs", "ext4", ...).
pub fn fs_type(path: &Path) -> Option<String> {
    let name = match fs_magic(path)? {
        BTRFS_MAGIC => "btrfs",
        0xEF53 => "ext4",
        0x5846_5342 => "xfs",
        0x0102_1994 => "tmpfs",
        0x794C_7630 => "overlay",
        0x2FC1_2FC1 => "zfs",
        0x6969 => "nfs",
        0x6573_5546 => "fuse",
        other => return Some(format!("0x{other:x}")),
    };
    Some(name.into())
}

/// (mount point, source device) from /proc/self/mountinfo, octal escapes undone.
fn mounts() -> Vec<(PathBuf, String)> {
    let Ok(s) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return vec![];
    };
    s.lines()
        .filter_map(|l| {
            let point = l.split(' ').nth(4)?;
            // After the " - " separator: fstype, source, super options.
            let source = l.split_once(" - ")?.1.split(' ').nth(1)?;
            Some((PathBuf::from(unescape(point)), unescape(source)))
        })
        .collect()
}

/// Where `dev` (e.g. /dev/loop3) is mounted, if anywhere.
pub(crate) fn mount_point_of(dev: &str) -> Option<PathBuf> {
    mounts().into_iter().find(|(_, s)| s == dev).map(|(p, _)| p)
}

/// Device mounted at `path` (e.g. /dev/loop3).
pub(crate) fn mount_source(path: &Path) -> Option<String> {
    let want = path.canonicalize().ok()?;
    mounts()
        .into_iter()
        .rev()
        .find(|(p, _)| *p == want)
        .map(|(_, s)| s)
}

pub(crate) fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 4], 8)
        {
            out.push(v);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// True when a filesystem is mounted exactly at `path`. statfs can't say on
/// Linux, and st_dev changes at every btrfs subvolume, so read mountinfo.
pub fn is_mounted(path: &Path) -> bool {
    let Ok(want) = path.canonicalize() else {
        return false;
    };
    mounts().iter().any(|(p, _)| *p == want)
}

/// Processes with their cwd, root or an open file under `dir`.
pub fn users(dir: &Path) -> Vec<VolumeUser> {
    let Ok(want) = dir.canonicalize() else {
        return vec![];
    };
    let me = std::process::id() as i32;
    let mut out = vec![];
    let Ok(procs) = std::fs::read_dir("/proc") else {
        return out;
    };
    for p in procs.flatten() {
        let Some(pid) = p.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else {
            continue;
        };
        if pid == me {
            continue;
        }
        let base = p.path();
        let under = |l: &Path| std::fs::read_link(l).is_ok_and(|t| t.starts_with(&want));
        let hit = under(&base.join("cwd"))
            || under(&base.join("root"))
            || std::fs::read_dir(base.join("fd"))
                .map(|fds| fds.flatten().any(|f| under(&f.path())))
                .unwrap_or(false);
        if hit {
            let command = std::fs::read_to_string(base.join("comm"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            out.push(VolumeUser { pid, command });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescapes_mountinfo() {
        assert_eq!(unescape(r"/a\040b"), "/a b");
        assert_eq!(unescape("/plain"), "/plain");
    }

    #[test]
    fn root_is_mounted_and_tmp_dir_is_not() {
        assert!(is_mounted(Path::new("/")));
        let d = tempfile::tempdir().unwrap();
        assert!(!is_mounted(d.path()));
    }

    #[test]
    fn finds_a_child_by_cwd() {
        let d = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .current_dir(d.path())
            .spawn()
            .unwrap();
        let found = users(d.path()).iter().any(|u| u.pid == child.id() as i32);
        let _ = child.kill();
        let _ = child.wait();
        assert!(found);
    }
}
