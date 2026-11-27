//! macOS: one APFS disk image per golden generation and per clone. A clone
//! is a `clonefile(2)` copy of the golden image file, mounted on its own.

use super::sys_macos::{detach, is_mounted};
use super::{Backend, Store};
use crate::repo::Clove;
use crate::util::{allocated_bytes, clonefile, private_bytes, run};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) static ASIF: Image = Image(Backend::Asif);
pub(super) static SPARSE: Image = Image(Backend::Sparse);

pub(super) fn detect() -> Result<Backend> {
    // macOS 15 has `diskutil image` but not the ASIF format; check the help text.
    let asif = std::env::var("CLOVE_BACKEND")
        .map(|b| b != "sparse")
        .unwrap_or(true)
        && Command::new("diskutil")
            .args(["image", "create", "blank", "--help"])
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("ASIF"))
            .unwrap_or(false);
    Ok(if asif { Backend::Asif } else { Backend::Sparse })
}

pub(super) struct Image(Backend);

impl Image {
    fn golden_image(&self, c: &Clove, g: u32) -> PathBuf {
        c.golden_gen_dir(g).join(self.0.file_name())
    }

    fn clone_image(&self, c: &Clove, name: &str) -> PathBuf {
        c.clone_dir(name).join(self.0.file_name())
    }

    fn spare_image(&self, c: &Clove) -> PathBuf {
        c.spare_dir().join(self.0.file_name())
    }

    fn create(&self, path: &Path, size: &str, volume_name: &str) -> Result<()> {
        match self.0 {
            Backend::Asif => run(Command::new("diskutil")
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
            _ => run(Command::new("hdiutil")
                .args(["create", "-quiet", "-type", "SPARSE", "-fs", "APFS"])
                .args(["-size", size, "-volname", volume_name])
                .arg(path))?,
        };
        Ok(())
    }

    fn attach(&self, image: &Path, mount_point: &Path) -> Result<()> {
        if is_mounted(mount_point) {
            return Ok(());
        }
        std::fs::create_dir_all(mount_point)
            .with_context(|| format!("create mount point {}", mount_point.display()))?;
        match self.0 {
            Backend::Asif => run(Command::new("diskutil")
                .args(["image", "attach", "--nobrowse", "--mountPoint"])
                .arg(mount_point)
                .arg(image))?,
            _ => run(Command::new("hdiutil")
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

impl Store for Image {
    fn golden_mount(&self, c: &Clove) -> PathBuf {
        c.golden_mount()
    }

    fn golden_reset(&self, c: &Clove) -> Result<()> {
        detach(&c.golden_mount(), true)
    }

    fn golden_create(&self, c: &Clove, g: u32) -> Result<()> {
        self.create(
            &self.golden_image(c, g),
            &c.config.image_size,
            &format!("{}-clove", c.name),
        )
    }

    fn golden_derive(&self, c: &Clove, from: u32, to: u32) -> Result<()> {
        clonefile(&self.golden_image(c, from), &self.golden_image(c, to))
    }

    fn golden_attach(&self, c: &Clove, g: u32) -> Result<()> {
        self.attach(&self.golden_image(c, g), &c.golden_mount())
    }

    fn golden_seal(&self, c: &Clove, _g: u32) -> Result<()> {
        detach(&c.golden_mount(), false)
    }

    fn golden_abort(&self, c: &Clove) {
        let _ = detach(&c.golden_mount(), true);
    }

    fn golden_stuck(&self, c: &Clove) -> bool {
        is_mounted(&c.golden_mount())
    }

    fn golden_drop(&self, _c: &Clove, _g: u32) -> Result<()> {
        Ok(())
    }

    fn golden_bytes(&self, c: &Clove, g: u32) -> u64 {
        allocated_bytes(&self.golden_image(c, g))
    }

    fn clone_create(&self, c: &Clove, g: u32, name: &str, _mount: &Path) -> Result<()> {
        clonefile(&self.golden_image(c, g), &self.clone_image(c, name))
    }

    fn clone_attach(&self, c: &Clove, name: &str, mount: &Path) -> Result<()> {
        self.attach(&self.clone_image(c, name), mount)
    }

    fn clone_detach(&self, _c: &Clove, _name: &str, mount: &Path, force: bool) -> Result<()> {
        detach(mount, force)
    }

    fn clone_delete(&self, _c: &Clove, _name: &str, _mount: &Path) -> Result<()> {
        Ok(())
    }

    fn clone_attached(&self, _c: &Clove, _name: &str, mount: &Path) -> bool {
        is_mounted(mount)
    }

    fn clone_bytes(&self, c: &Clove, name: &str, _mount: &Path) -> (u64, u64) {
        let image = self.clone_image(c, name);
        (allocated_bytes(&image), private_bytes(&image))
    }

    fn spare_create(&self, c: &Clove, g: u32) -> Result<bool> {
        std::fs::create_dir_all(c.spare_dir())?;
        clonefile(&self.golden_image(c, g), &self.spare_image(c))?;
        Ok(true)
    }

    fn spare_attach(&self, c: &Clove) -> Result<()> {
        self.attach(&self.spare_image(c), &c.spare_mount())
    }

    fn spare_attached(&self, c: &Clove) -> bool {
        is_mounted(&c.spare_mount()) && self.spare_image(c).is_file()
    }

    // APFS lets a mounted volume's mount point be renamed, and the mount
    // table follows: the clone is mounted at its path without a new mount.
    fn spare_claim(&self, c: &Clove, name: &str, mount: &Path) -> Result<()> {
        let (image, to) = (self.spare_image(c), self.clone_image(c, name));
        if let Some(parent) = mount.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&image, &to).with_context(|| format!("move {}", image.display()))?;
        let moved = std::fs::rename(c.spare_mount(), mount)
            .with_context(|| format!("move the spare's mount point to {}", mount.display()));
        if moved.is_ok() && is_mounted(mount) {
            return Ok(());
        }
        if moved.is_ok() {
            let _ = std::fs::rename(mount, c.spare_mount());
        }
        let _ = std::fs::rename(&to, &image);
        moved?;
        bail!("the mount did not follow its renamed mount point")
    }

    fn spare_discard(&self, c: &Clove) -> Result<()> {
        detach(&c.spare_mount(), true)?;
        let _ = std::fs::remove_dir(c.spare_mount());
        match std::fs::remove_file(self.spare_image(c)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    fn unmount_volume(&self, _c: &Clove, _force: bool) -> Result<bool> {
        Ok(false)
    }

    fn deinit(&self, _c: &Clove) -> Result<()> {
        Ok(())
    }
}
