//! Linux: one sparse image file per repo, formatted btrfs and mounted once.
//! Inside it:
//!   .clove-golden/<repo>    the golden worktree (writable subvolume, warmed in place)
//!   .clove-gens/gen-N       read-only snapshot of .clove-golden per generation
//!   <clone>/<repo>          a clone: writable snapshot of a generation
//!   .clove-checkpoints/<clone>/<id>   read-only snapshot of a clone (checkpoint)
//!
//! Mounting, in order: an /etc/fstab line (`user` mount at clone_root, no
//! sudo, works over SSH), udisks (desktop sessions; it picks /run/media/...,
//! which is then recorded as the clone root), `sudo -n` as a last resort.
//! Sizes come from squota/qgroup sysfs files, unprivileged.

use super::sys_linux::{
    BTRFS_MAGIC, fs_magic, fs_used_bytes, is_mounted, mount_point_of, mount_source, users,
};
use super::{Backend, Store, busy_error};
use crate::repo::Clove;
use crate::util::{run, which};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) static POOL: Pool = Pool;

const GOLDEN: &str = ".clove-golden";
const GENS: &str = ".clove-gens";
/// `.clove-checkpoints/<clone>/<id>`: read-only snapshots of a clone.
const CHECKPOINTS: &str = ".clove-checkpoints";
const FSTAB_OPTS: &str = "loop,user,noauto,exec,user_subvol_rm_allowed";

pub(super) fn detect() -> Result<Backend> {
    for b in ["mkfs.btrfs", "btrfs"] {
        if which(b).is_none() && !Path::new("/usr/sbin").join(b).is_file() {
            bail!("clove needs btrfs-progs on Linux ({b} not found): sudo apt install btrfs-progs");
        }
    }
    Ok(Backend::Btrfs)
}

/// `mkfs.btrfs` often lives in /usr/sbin, which a user PATH may lack.
fn bin(name: &str) -> PathBuf {
    which(name).unwrap_or_else(|| Path::new("/usr/sbin").join(name))
}

fn udisks() -> bool {
    which("udisksctl").is_some()
}

/// "300G" style sizes, binary units.
pub fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: u64 = num.parse().with_context(|| format!("bad size {s:?}"))?;
    let shift = match unit
        .trim_end_matches(['B', 'b', 'i'])
        .to_ascii_uppercase()
        .as_str()
    {
        "" => 0,
        "K" => 10,
        "M" => 20,
        "G" => 30,
        "T" => 40,
        _ => bail!("bad size {s:?}: use K, M, G or T"),
    };
    n.checked_mul(1 << shift)
        .with_context(|| format!("size {s:?} too large"))
}

fn kernel_at_least(major: u32, minor: u32) -> bool {
    let rel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let mut it = rel.split(['.', '-']).map(|p| p.parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0)) >= (major, minor)
}

/// Creates a sparse file of `size` bytes and formats it btrfs with quotas on
/// from birth: squota (kernel 6.7+), else full qgroups, else none.
pub fn create_image(path: &Path, size: u64, label: &str) -> Result<()> {
    let dir = path.parent().context("image dir")?;
    std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    // No COW for the image on a btrfs host; must happen while it's empty.
    if fs_magic(dir) == Some(BTRFS_MAGIC) {
        let _ = run(Command::new("chattr").arg("+C").arg(path));
    }
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)?
        .set_len(size)?;
    // --rootdir from a user-owned empty dir makes the filesystem root the user's.
    let seed = dir.join(".mkfs-root");
    let _ = std::fs::remove_dir_all(&seed);
    std::fs::create_dir_all(&seed)?;
    let features: &[Option<&str>] = if kernel_at_least(6, 7) {
        &[Some("squota"), Some("quota"), None]
    } else {
        &[Some("quota"), None]
    };
    let mut last = None;
    for f in features {
        let mut cmd = Command::new(bin("mkfs.btrfs"));
        cmd.args(["-q", "-f", "-L", label]);
        if let Some(f) = f {
            cmd.args(["-O", f]);
        }
        match run(cmd.arg("--rootdir").arg(&seed).arg(path)) {
            Ok(_) => {
                last = None;
                break;
            }
            Err(e) => last = Some(e),
        }
    }
    let _ = std::fs::remove_dir_all(&seed);
    match last {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

// ---- fstab ---------------------------------------------------------------

fn fstab_escape(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\\', "\\134")
        .replace(' ', "\\040")
        .replace('\t', "\\011")
}

/// The line that lets the user `mount <at>` the image without sudo. `user`
/// implies noexec, hence `exec`.
pub fn fstab_line(image: &Path, at: &Path) -> String {
    format!(
        "{} {} btrfs {FSTAB_OPTS} 0 0",
        fstab_escape(image),
        fstab_escape(at)
    )
}

/// Mount point an fstab text gives `image`, if any.
fn fstab_entry(fstab: &str, image: &Path) -> Option<PathBuf> {
    let want = fstab_escape(image);
    fstab
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| {
            let mut f = l.split_whitespace();
            (f.next()? == want).then(|| f.next().map(|t| PathBuf::from(super::sys_linux::unescape(t))))?
        })
}

/// Mount point /etc/fstab gives `image`, if any.
pub fn fstab_target(image: &Path) -> Option<PathBuf> {
    fstab_entry(&std::fs::read_to_string("/etc/fstab").ok()?, image)
}

/// Appends `line` to /etc/fstab with `sudo tee -a` (asks for the password).
pub fn add_fstab_line(line: &str) -> Result<()> {
    use std::io::Write;
    let mut child = Command::new("sudo")
        .args(["tee", "-a", "/etc/fstab"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .context("run sudo tee -a /etc/fstab")?;
    child
        .stdin
        .take()
        .context("sudo stdin")?
        // Lead with a newline: a file without a trailing one would otherwise
        // get clove's line glued onto its last entry.
        .write_all(format!("\n# clove: mounts this repo's image as you (noauto)\n{line}\n").as_bytes())?;
    if !child.wait()?.success() {
        bail!("sudo tee -a /etc/fstab failed");
    }
    Ok(())
}

// ---- attach / detach -----------------------------------------------------

/// Loop device already backed by `image`, if any. Attaching twice would show
/// btrfs one fsid on two devices.
pub fn loop_device(image: &Path) -> Option<String> {
    let out = run(Command::new("losetup")
        .args(["-n", "-O", "NAME", "-j"])
        .arg(image))
    .ok()?;
    out.lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
}

/// "Mounted /dev/loop3 at /run/media/u/clove-x" (some versions add a '.').
fn parse_udisks_mount(out: &str) -> Option<PathBuf> {
    let (_, at) = out.trim().split_once(" at ")?;
    Some(PathBuf::from(at.trim().trim_end_matches('.')))
}

/// Mounts `image` and returns where: its fstab mount point, the place udisks
/// picks, or `fallback` through `sudo -n`.
pub fn attach(image: &Path, fallback: &Path) -> Result<PathBuf> {
    if let Some(dev) = loop_device(image)
        && let Some(at) = mount_point_of(&dev)
    {
        return Ok(at);
    }
    if let Some(at) = fstab_target(image) {
        std::fs::create_dir_all(&at)?;
        run(Command::new("mount").arg(&at)).with_context(|| format!("mount {}", at.display()))?;
        return Ok(at);
    }
    let mut errors = vec![];
    if udisks() {
        let img = image.to_string_lossy();
        let dev = match loop_device(image) {
            Some(d) => Ok(d),
            None => run(Command::new("udisksctl").args(["loop-setup", "--no-user-interaction", "-f", &img]))
                .and_then(|out| {
                    out.split_whitespace()
                        .find(|w| w.starts_with("/dev/loop"))
                        .map(|d| d.trim_end_matches('.').to_string())
                        .with_context(|| format!("unexpected udisksctl output: {out}"))
                }),
        };
        match dev.and_then(|dev| {
            let out = run(Command::new("udisksctl").args(["mount", "--no-user-interaction", "-b", &dev]));
            if out.is_err() {
                let _ =
                    run(Command::new("udisksctl").args(["loop-delete", "--no-user-interaction", "-b", &dev]));
            }
            let out = out?;
            parse_udisks_mount(&out).with_context(|| format!("unexpected udisksctl output: {out}"))
        }) {
            Ok(at) => return Ok(at),
            Err(e) => errors.push(format!("udisks: {e:#}")),
        }
    }
    match sudo_attach(image, fallback) {
        Ok(()) => Ok(fallback.to_path_buf()),
        Err(e) => {
            errors.push(format!("sudo: {e:#}"));
            bail!(
                "can't mount {}: {}. Add this line to /etc/fstab to mount it without sudo:\n  {}",
                image.display(),
                errors.join("; "),
                fstab_line(image, fallback)
            )
        }
    }
}

fn sudo_attach(image: &Path, at: &Path) -> Result<()> {
    let sudo = |args: &[&str]| run(Command::new("sudo").arg("-n").args(args));
    let dev = match loop_device(image) {
        Some(d) => d,
        None => sudo(&["losetup", "--find", "--show", &image.to_string_lossy()])?,
    };
    std::fs::create_dir_all(at)?;
    sudo(&[
        "mount",
        "-o",
        "user_subvol_rm_allowed",
        &dev,
        &at.to_string_lossy(),
    ])?;
    Ok(())
}

/// Unmounts the image mounted at `at` and frees its loop device. `force`
/// unmounts lazily.
pub fn detach(at: &Path, force: bool) -> Result<()> {
    if !is_mounted(at) {
        return Ok(());
    }
    let dev = mount_source(at).unwrap_or_default();
    let image_in_fstab = std::fs::read_to_string("/etc/fstab").is_ok_and(|f| {
        f.lines()
            .any(|l| l.split_whitespace().nth(1) == Some(&fstab_escape(at)))
    });
    if image_in_fstab {
        // A `user` mount: the user may unmount it; util-linux frees the loop.
        let mut cmd = Command::new("umount");
        if force {
            cmd.arg("-l");
        }
        run(cmd.arg(at))?;
        return Ok(());
    }
    let mut via_udisks = false;
    if udisks() && !dev.is_empty() {
        let mut cmd = Command::new("udisksctl");
        cmd.args(["unmount", "--no-user-interaction", "-b", &dev]);
        if force {
            cmd.arg("--force");
        }
        via_udisks = run(&mut cmd).is_ok();
    }
    if !via_udisks {
        let mut args = vec!["-n", "umount"];
        if force {
            args.push("-l");
        }
        let at_s = at.to_string_lossy();
        args.push(&at_s);
        run(Command::new("sudo").args(&args)).with_context(|| format!("unmount {}", at.display()))?;
    }
    if dev.starts_with("/dev/loop") {
        let freed = udisks()
            && run(Command::new("udisksctl").args(["loop-delete", "--no-user-interaction", "-b", &dev]))
                .is_ok();
        if !freed {
            let _ = run(Command::new("sudo").args(["-n", "losetup", "-d", &dev]));
        }
    }
    Ok(())
}

// ---- subvolumes and sizes ------------------------------------------------

pub fn subvolume_create(path: &Path) -> Result<()> {
    run(Command::new(bin("btrfs")).args(["subvolume", "create"]).arg(path))?;
    Ok(())
}

/// O(1) snapshot of subvolume `src` at `dst`.
pub fn snapshot(src: &Path, dst: &Path, readonly: bool) -> Result<()> {
    let mut cmd = Command::new(bin("btrfs"));
    cmd.args(["subvolume", "snapshot"]);
    if readonly {
        cmd.arg("-r");
    }
    run(cmd.arg(src).arg(dst))?;
    Ok(())
}

/// Deletes a subvolume: `btrfs subvolume delete` (needs user_subvol_rm_allowed),
/// else make it writable, empty it and rmdir it (unprivileged since 4.18).
/// Space comes back later, when the btrfs cleaner runs.
pub fn subvolume_delete(path: &Path) -> Result<()> {
    if run(Command::new(bin("btrfs")).args(["subvolume", "delete"]).arg(path)).is_ok() {
        return Ok(());
    }
    let _ = run(Command::new(bin("btrfs"))
        .args(["property", "set", "-ts"])
        .arg(path)
        .args(["ro", "false"]));
    std::fs::remove_dir_all(path).with_context(|| format!("delete {}", path.display()))
}

/// A subvolume's root directory is always inode 256 on btrfs.
pub fn is_subvolume(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && m.ino() == 256)
        && fs_magic(path) == Some(BTRFS_MAGIC)
}

fn open_dir(path: &Path) -> Option<std::fs::File> {
    std::fs::File::open(path).ok()
}

/// Subvolume (tree) id holding `path`, via BTRFS_IOC_INO_LOOKUP (what
/// `btrfs inspect-internal rootid` does; unprivileged for objectid 256).
pub fn subvolume_id(path: &Path) -> Option<u64> {
    use std::os::fd::AsRawFd;
    #[repr(C)]
    struct Args {
        treeid: u64,
        objectid: u64,
        name: [u8; 4080],
    }
    const IOC_INO_LOOKUP: u64 = 0xD000_9412;
    let f = open_dir(path)?;
    let mut a = Args {
        treeid: 0,
        objectid: 256,
        name: [0; 4080],
    };
    // SAFETY: `a` matches struct btrfs_ioctl_ino_lookup_args (4096 bytes).
    let rc = unsafe { libc::ioctl(f.as_raw_fd(), IOC_INO_LOOKUP as _, &mut a) };
    (rc == 0).then_some(a.treeid)
}

/// Filesystem UUID (the /sys/fs/btrfs/<fsid> name), via BTRFS_IOC_FS_INFO.
pub fn fsid(path: &Path) -> Option<String> {
    use std::os::fd::AsRawFd;
    #[repr(C)]
    struct Args {
        max_id: u64,
        num_devices: u64,
        fsid: [u8; 16],
        rest: [u8; 1024 - 32],
    }
    const IOC_FS_INFO: u64 = 0x8400_941F;
    let f = open_dir(path)?;
    let mut a = Args {
        max_id: 0,
        num_devices: 0,
        fsid: [0; 16],
        rest: [0; 1024 - 32],
    };
    // SAFETY: `a` matches struct btrfs_ioctl_fs_info_args (1024 bytes).
    let rc = unsafe { libc::ioctl(f.as_raw_fd(), IOC_FS_INFO as _, &mut a) };
    if rc != 0 {
        return None;
    }
    let h: String = a.fsid.iter().map(|b| format!("{b:02x}")).collect();
    Some(format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    ))
}

fn qgroups_dir(path: &Path) -> Option<PathBuf> {
    Some(PathBuf::from("/sys/fs/btrfs").join(fsid(path)?).join("qgroups"))
}

/// Quotas are on for the filesystem holding `path` (None: not btrfs).
pub fn quota_enabled(path: &Path) -> Option<bool> {
    Some(qgroups_dir(path)?.is_dir())
}

/// (referenced, exclusive) bytes of the subvolume at `path`, from its qgroup
/// in sysfs (never `btrfs qgroup show`, which needs root). None when quotas are off.
pub fn qgroup_bytes(path: &Path) -> Option<(u64, u64)> {
    let dir = qgroups_dir(path)?.join(format!("0_{}", subvolume_id(path)?));
    let read = |f: &str| -> Option<u64> { std::fs::read_to_string(dir.join(f)).ok()?.trim().parse().ok() };
    Some((read("referenced")?, read("exclusive")?))
}

/// Exclusive data bytes by walking the tree (`btrfs filesystem du`), for
/// images without quotas. Slow (about 1s per 200k files): on demand only.
pub fn du_exclusive(path: &Path) -> Option<u64> {
    let out = run(Command::new(bin("btrfs"))
        .args(["filesystem", "du", "-s", "--raw"])
        .arg(path))
    .ok()?;
    // "Total Exclusive Set shared Filename" then one row.
    out.lines().nth(1)?.split_whitespace().nth(1)?.parse().ok()
}

// ---- the store -----------------------------------------------------------

/// The btrfs backend: one image per repo, snapshots for everything else.
pub(super) struct Pool;

/// The repo's image file.
pub fn image_path(c: &Clove) -> PathBuf {
    c.state.join(Backend::Btrfs.file_name())
}

impl Pool {
    fn golden(c: &Clove) -> PathBuf {
        c.clone_root().join(GOLDEN)
    }
    fn generation(c: &Clove, g: u32) -> PathBuf {
        c.clone_root().join(GENS).join(format!("gen-{g}"))
    }

    /// Mounts the repo's image if needed, and records where clones live.
    fn attach(c: &Clove) -> Result<()> {
        if is_mounted(&c.clone_root()) {
            return Ok(());
        }
        let image = image_path(c);
        if !image.exists() {
            bail!("{} is missing; run `clove init`", image.display());
        }
        let want = &c.config.clone_root;
        if c.recorded_mount().is_none_or(|r| r == *want)
            && !is_mounted(want)
            && std::fs::read_dir(want).is_ok_and(|mut d| d.next().is_some())
        {
            bail!(
                "{} is not empty; clove mounts the repo's image there (set clone_root to a folder of its own)",
                want.display()
            );
        }
        let at = attach(&image, want)?;
        match c.recorded_mount() {
            Some(r) if r != at => {
                let _ = detach(&at, false);
                bail!(
                    "the image mounted at {} but its clones were made at {}; \
                     undo the /etc/fstab change, or recreate them (`clove rm --all`, `clove deinit`, `clove init`)",
                    at.display(),
                    r.display()
                )
            }
            Some(_) => Ok(()),
            None => crate::util::write_atomic(&c.state.join("mount"), at.to_string_lossy().as_bytes()),
        }
    }

    fn checkpoint(c: &Clove, name: &str, id: &str) -> PathBuf {
        c.clone_root().join(CHECKPOINTS).join(name).join(id)
    }

    fn delete_if_present(path: &Path) -> Result<()> {
        if is_subvolume(path) {
            subvolume_delete(path)?;
        }
        Ok(())
    }
}

impl Store for Pool {
    fn golden_mount(&self, c: &Clove) -> PathBuf {
        Self::golden(c)
    }

    fn golden_reset(&self, _c: &Clove) -> Result<()> {
        // golden_create and golden_derive rebuild .clove-golden from scratch.
        Ok(())
    }

    fn golden_create(&self, c: &Clove, _g: u32) -> Result<()> {
        let image = image_path(c);
        if !image.exists() {
            std::fs::create_dir_all(&c.state)?;
            create_image(
                &image,
                parse_size(&c.config.image_size)?,
                &format!("clove-{}", c.id),
            )?;
        }
        Self::attach(c)?;
        // Leftovers of an interrupted init.
        if let Ok(rd) = std::fs::read_dir(c.clone_root().join(GENS)) {
            for e in rd.flatten() {
                Self::delete_if_present(&e.path())?;
            }
        }
        Self::delete_if_present(&Self::golden(c))?;
        subvolume_create(&Self::golden(c))
    }

    fn golden_derive(&self, c: &Clove, from: u32, _to: u32) -> Result<()> {
        Self::attach(c)?;
        let src = Self::generation(c, from);
        if !is_subvolume(&src) {
            bail!("golden generation {from} is missing from {}", src.display());
        }
        Self::delete_if_present(&Self::golden(c))?;
        snapshot(&src, &Self::golden(c), false)
    }

    fn golden_attach(&self, c: &Clove, _g: u32) -> Result<()> {
        Self::attach(c)
    }

    fn golden_seal(&self, c: &Clove, g: u32) -> Result<()> {
        let dst = Self::generation(c, g);
        std::fs::create_dir_all(dst.parent().context("gens dir")?)?;
        Self::delete_if_present(&dst)?;
        snapshot(&Self::golden(c), &dst, true)
    }

    fn golden_abort(&self, _c: &Clove) {}

    fn golden_stuck(&self, _c: &Clove) -> bool {
        false
    }

    fn golden_drop(&self, c: &Clove, g: u32) -> Result<()> {
        if !is_mounted(&c.clone_root()) {
            return Ok(());
        }
        Self::delete_if_present(&Self::generation(c, g))
    }

    fn golden_bytes(&self, c: &Clove, _g: u32) -> u64 {
        // squota charges warm-step writes to whichever subvolume made them, which
        // refresh replaces; so: what the volume uses, minus the clones' own bytes.
        let root = c.clone_root();
        let Some(used) = is_mounted(&root).then(|| fs_used_bytes(&root)).flatten() else {
            return 0;
        };
        let clones: u64 = c
            .clone_names()
            .unwrap_or_default()
            .iter()
            .filter_map(|n| qgroup_bytes(&c.clone_mount(n)).map(|(_, own)| own))
            .sum();
        used.saturating_sub(clones)
    }

    fn clone_create(&self, c: &Clove, g: u32, _name: &str, mount: &Path) -> Result<()> {
        Self::attach(c)?;
        let src = Self::generation(c, g);
        if !is_subvolume(&src) {
            bail!("golden generation {g} is missing from {}", src.display());
        }
        if mount.parent() != Some(c.clone_root().as_path()) {
            bail!(
                "{} is outside the repo's volume at {}",
                mount.display(),
                c.clone_root().display()
            );
        }
        // `new` checked it's empty; a snapshot needs the name free.
        if mount.is_dir() {
            std::fs::remove_dir(mount)?;
        }
        snapshot(&src, mount, false)
    }

    fn clone_attach(&self, c: &Clove, _name: &str, mount: &Path) -> Result<()> {
        Self::attach(c)?;
        if !is_subvolume(mount) {
            bail!("{} is missing from the repo's volume", mount.display());
        }
        Ok(())
    }

    fn clone_detach(&self, _c: &Clove, _name: &str, mount: &Path, force: bool) -> Result<()> {
        // Clones share the repo's mount; nothing to unmount on its own.
        // Busy check by path: each subvolume has its own st_dev.
        if !force && mount.is_dir() {
            let who = users(mount);
            if !who.is_empty() {
                bail!("{}", busy_error(&who));
            }
        }
        Ok(())
    }

    fn clone_delete(&self, c: &Clove, _name: &str, mount: &Path) -> Result<()> {
        if image_path(c).exists() {
            Self::attach(c)?;
        }
        Self::delete_if_present(mount)
    }

    fn clone_attached(&self, c: &Clove, _name: &str, mount: &Path) -> bool {
        is_mounted(&c.clone_root()) && mount.is_dir()
    }

    fn clone_bytes(&self, _c: &Clove, _name: &str, mount: &Path) -> (u64, u64) {
        qgroup_bytes(mount).unwrap_or((0, 0))
    }

    fn checkpoint_create(&self, c: &Clove, name: &str, mount: &Path, id: &str) -> Result<()> {
        Self::attach(c)?;
        let dst = Self::checkpoint(c, name, id);
        std::fs::create_dir_all(dst.parent().context("checkpoints dir")?)?;
        // A snapshot flushes the subvolume's pending writes first.
        snapshot(mount, &dst, true)
    }

    // Rename the live subvolume aside, snapshot the checkpoint into its place,
    // delete the old one. Processes inside keep the old files until they
    // close them; `force` only skips the busy check.
    fn checkpoint_restore(&self, c: &Clove, name: &str, mount: &Path, id: &str, force: bool) -> Result<()> {
        Self::attach(c)?;
        let saved = Self::checkpoint(c, name, id);
        if !is_subvolume(&saved) {
            bail!("checkpoint {id} is missing from {}", saved.display());
        }
        if !force {
            let who = users(mount);
            if !who.is_empty() {
                bail!("{}", busy_error(&who));
            }
        }
        let aside = Self::checkpoint(c, name, ".rewinding");
        Self::delete_if_present(&aside)?;
        std::fs::rename(mount, &aside).with_context(|| format!("move {} aside", mount.display()))?;
        if let Err(e) = snapshot(&saved, mount, false) {
            let _ = std::fs::rename(&aside, mount);
            return Err(e);
        }
        let _ = Self::delete_if_present(&aside);
        Ok(())
    }

    fn checkpoint_rename(&self, c: &Clove, name: &str, from: &str, to: &str) -> Result<()> {
        Self::attach(c)?;
        let to = Self::checkpoint(c, name, to);
        Self::delete_if_present(&to)?;
        std::fs::rename(Self::checkpoint(c, name, from), &to)?;
        Ok(())
    }

    fn checkpoint_delete(&self, c: &Clove, name: &str, id: &str) -> Result<()> {
        if image_path(c).exists() {
            Self::attach(c)?;
        }
        let path = Self::checkpoint(c, name, id);
        Self::delete_if_present(&path)?;
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir(dir);
        }
        Ok(())
    }

    fn checkpoint_bytes(&self, c: &Clove, name: &str, id: &str) -> u64 {
        qgroup_bytes(&Self::checkpoint(c, name, id)).map_or(0, |(_, own)| own)
    }

    fn clone_fork(
        &self,
        c: &Clove,
        _from: &str,
        from_mount: &Path,
        _to: &str,
        to_mount: &Path,
    ) -> Result<()> {
        Self::attach(c)?;
        if !is_subvolume(from_mount) {
            bail!("{} is missing from the repo's volume", from_mount.display());
        }
        if to_mount.parent() != Some(c.clone_root().as_path()) {
            bail!(
                "{} is outside the repo's volume at {}",
                to_mount.display(),
                c.clone_root().display()
            );
        }
        if to_mount.is_dir() {
            std::fs::remove_dir(to_mount)?;
        }
        snapshot(from_mount, to_mount, false)
    }

    fn unmount_volume(&self, c: &Clove, force: bool) -> Result<bool> {
        let at = c.clone_root();
        if !is_mounted(&at) {
            return Ok(true);
        }
        if !force {
            let who = users(&at);
            if !who.is_empty() {
                bail!("{}", busy_error(&who));
            }
        }
        detach(&at, force)?;
        Ok(true)
    }

    fn deinit(&self, c: &Clove) -> Result<()> {
        let at = c.clone_root();
        detach(&at, true)?;
        let _ = std::fs::remove_dir(&at);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("300G").unwrap(), 300 << 30);
        assert_eq!(parse_size("2GiB").unwrap(), 2 << 30);
        assert_eq!(parse_size("512").unwrap(), 512);
        assert!(parse_size("3X").is_err());
        assert!(parse_size("G").is_err());
    }

    #[test]
    fn plain_dir_is_not_a_subvolume() {
        let d = tempfile::tempdir().unwrap();
        assert!(!is_subvolume(d.path()));
    }

    #[test]
    fn fstab_line_round_trips() {
        let img = Path::new("/home/u/.clove/repos/my app-1/pool.btrfs");
        let at = Path::new("/home/u/src/my app.clove");
        let line = fstab_line(img, at);
        assert_eq!(
            line,
            "/home/u/.clove/repos/my\\040app-1/pool.btrfs /home/u/src/my\\040app.clove btrfs \
             loop,user,noauto,exec,user_subvol_rm_allowed 0 0"
        );
        let fstab = format!("# comment\nUUID=x / ext4 defaults 0 1\n{line}\n");
        assert_eq!(fstab_entry(&fstab, img), Some(at.to_path_buf()));
        assert_eq!(fstab_entry(&fstab, Path::new("/other.img")), None);
    }

    #[test]
    fn udisks_mount_output() {
        assert_eq!(
            parse_udisks_mount("Mounted /dev/loop3 at /run/media/u/clove-x\n"),
            Some(PathBuf::from("/run/media/u/clove-x"))
        );
        assert_eq!(
            parse_udisks_mount("Mounted /dev/loop3 at /media/u/clove-x."),
            Some(PathBuf::from("/media/u/clove-x"))
        );
    }
}
