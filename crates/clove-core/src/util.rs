use anyhow::{Context, Result, bail};
#[cfg(target_os = "macos")]
use std::ffi::CString;
#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Copy-on-write clone of a single file (or directory) with `clonefile(2)`.
/// O(1) for a file, which is the whole point: an image clone costs one syscall.
#[cfg(target_os = "macos")]
pub fn clonefile(src: &Path, dst: &Path) -> Result<()> {
    let s = CString::new(src.as_os_str().as_bytes())?;
    let d = CString::new(dst.as_os_str().as_bytes())?;
    // SAFETY: both pointers are valid NUL-terminated strings for the call.
    let rc = unsafe { libc::clonefile(s.as_ptr(), d.as_ptr(), 0) };
    if rc != 0 {
        let err = std::io::Error::last_os_error();
        bail!(
            "clonefile {} -> {}: {err} (both paths must be on the same APFS volume)",
            src.display(),
            dst.display()
        );
    }
    Ok(())
}

/// Reflink copy of one file with the FICLONE ioctl (btrfs, XFS); a plain
/// copy where the filesystem can't share blocks. Directories aren't supported.
#[cfg(target_os = "linux")]
pub fn clonefile(src: &Path, dst: &Path) -> Result<()> {
    use std::os::fd::AsRawFd;
    // _IOW(0x94, 9, int)
    const FICLONE: u64 = 0x4004_9409;
    let from = std::fs::File::open(src).with_context(|| format!("open {}", src.display()))?;
    let to = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)
        .with_context(|| format!("create {}", dst.display()))?;
    // SAFETY: both fds are open for the call; FICLONE takes the source fd by value.
    let rc = unsafe { libc::ioctl(to.as_raw_fd(), FICLONE as _, from.as_raw_fd()) };
    if rc == 0 {
        return Ok(());
    }
    drop(to);
    std::fs::remove_file(dst)?;
    std::fs::copy(src, dst).with_context(|| format!("copy {}", src.display()))?;
    Ok(())
}

/// Runs a command and returns trimmed stdout, failing with stderr on non-zero exit.
pub fn run(cmd: &mut Command) -> Result<String> {
    let out = output(cmd)?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn output(cmd: &mut Command) -> Result<Output> {
    let desc = describe(cmd);
    let out = cmd.output().with_context(|| format!("running {desc}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let msg = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        bail!("{desc} failed ({}): {msg}", out.status);
    }
    Ok(out)
}

pub fn describe(cmd: &Command) -> String {
    let mut s = cmd.get_program().to_string_lossy().to_string();
    for a in cmd.get_args() {
        s.push(' ');
        s.push_str(&a.to_string_lossy());
    }
    s
}

pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Expands `~` and `{key}` placeholders.
pub fn expand(template: &str, vars: &[(&str, &str)]) -> PathBuf {
    let mut s = template.to_string();
    if let Some(rest) = s.strip_prefix("~/") {
        s = format!("{}/{rest}", home().display());
    }
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    PathBuf::from(s)
}

/// Allocated bytes on disk (what `du` reports), not the apparent size.
pub fn allocated_bytes(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).map(|m| m.blocks() * 512).unwrap_or(0)
}

/// Bytes of `path` not shared with any clone: what deleting it would free.
/// Uses APFS's ATTR_CMNEXT_PRIVATESIZE; falls back to allocated size.
#[cfg(target_os = "macos")]
pub fn private_bytes(path: &Path) -> u64 {
    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
        return 0;
    };
    // SAFETY: zeroed attrlist is valid; we set the fields getattrlist reads.
    let mut al: libc::attrlist = unsafe { std::mem::zeroed() };
    al.bitmapcount = libc::ATTR_BIT_MAP_COUNT;
    al.forkattr = libc::ATTR_CMNEXT_PRIVATESIZE;
    #[repr(C, packed(4))]
    struct Buf {
        len: u32,
        private: libc::off_t,
    }
    let mut buf = Buf { len: 0, private: 0 };
    // SAFETY: buf is large enough for the one attribute requested.
    let rc = unsafe {
        libc::getattrlist(
            c.as_ptr(),
            (&mut al as *mut libc::attrlist).cast(),
            (&mut buf as *mut Buf).cast(),
            std::mem::size_of::<Buf>(),
            libc::FSOPT_ATTR_CMN_EXTENDED,
        )
    };
    if rc != 0 {
        return allocated_bytes(path);
    }
    let private = buf.private;
    private.max(0) as u64
}

/// Linux has no per-file equivalent; the btrfs backend reads clone sizes from
/// qgroups instead. Falls back to the allocated size.
#[cfg(target_os = "linux")]
pub fn private_bytes(path: &Path) -> u64 {
    allocated_bytes(path)
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Writes a file atomically (temp file + rename).
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}

/// Advisory exclusive lock on a file, released on drop.
pub struct FileLock(std::fs::File);

impl FileLock {
    pub fn acquire(path: &Path) -> Result<Self> {
        use std::os::fd::AsRawFd;
        let f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .with_context(|| format!("open lock {}", path.display()))?;
        // SAFETY: fd is valid for the lifetime of `f`.
        let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) };
        if rc != 0 {
            bail!("lock {}: {}", path.display(), std::io::Error::last_os_error());
        }
        Ok(Self(f))
    }

    /// Fails immediately when another process holds the lock.
    pub fn try_acquire(path: &Path) -> Result<Self> {
        use std::os::fd::AsRawFd;
        let f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        // SAFETY: fd is valid for the lifetime of `f`.
        let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            bail!("{} is held by another clove process", path.display());
        }
        Ok(Self(f))
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        // SAFETY: fd is still open.
        unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b}B")
    } else if v < 10.0 {
        format!("{v:.1}{}", UNITS[i])
    } else {
        format!("{v:.0}{}", UNITS[i])
    }
}

pub fn human_age(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        3600..=86_399 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_templates() {
        let p = expand(
            "{repo_parent}/{repo}.clove",
            &[("repo", "mono"), ("repo_parent", "/src")],
        );
        assert_eq!(p, PathBuf::from("/src/mono.clove"));
    }

    #[test]
    fn human_units() {
        assert_eq!(human_bytes(512), "512B");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024 / 2), "1.5G");
        assert_eq!(human_age(7200), "2h");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod clone_tests {
    #[test]
    fn clonefile_copies_contents_and_refuses_to_overwrite() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a");
        std::fs::write(&a, b"hello").unwrap();
        let b = d.path().join("b");
        super::clonefile(&a, &b).unwrap();
        assert_eq!(std::fs::read(&b).unwrap(), b"hello");
        assert!(super::clonefile(&a, &b).is_err());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod private_size_tests {
    #[test]
    fn clone_shares_blocks() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a");
        std::fs::write(&a, vec![7u8; 4 << 20]).unwrap();
        let b = d.path().join("b");
        super::clonefile(&a, &b).unwrap();
        assert!(
            super::private_bytes(&b) < 1 << 20,
            "fresh clone shares its blocks"
        );
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&b).unwrap();
            f.write_all(&vec![9u8; 4 << 20]).unwrap();
            // APFS allocates on flush; measuring before it can read low.
            f.sync_all().unwrap();
        }
        assert!(super::private_bytes(&b) >= 4 << 20);
    }
}
