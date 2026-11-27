//! The spare: one clone per repo made and mounted ahead of time (macOS), so
//! `new` renames its image and mount point into place instead of mounting
//! (~0.5-1 s on a big repo). It mounts at `<clone_root>/.clove-spare`.
//! `clove spare` makes it, in the background after `new`, `init`, `refresh`
//! and at login. It only counts when its record names the current golden
//! generation and its volume is mounted; anything else is a leftover that the
//! next fill or `clove gc` detaches and deletes.

use crate::backend::Store;
use crate::repo::{Clove, GoldenMeta, read_json, write_json};
use crate::util::{FileLock, now_unix};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// `spare/spare.json`, written once the spare is mounted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpareMeta {
    pub generation: u32,
    pub commit: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpareState {
    /// `spare = false`, or this backend keeps none.
    Off,
    Ready(u32),
    /// A fill is running.
    Filling,
    /// Nothing there.
    Missing,
    /// Something is there that `new` won't use: why.
    Stale(String),
    /// A claim found that renaming a mount point fails here: why.
    Unsupported(String),
}

/// Whether a spare recorded as `meta` can become a clone of `golden`.
pub fn usable(meta: Option<&SpareMeta>, golden: &GoldenMeta, attached: bool) -> bool {
    meta.is_some_and(|m| m.generation == golden.generation && m.commit == golden.commit) && attached
}

/// Why a spare is not usable, or None when it is (or there is none).
pub fn stale_reason(
    meta: Option<&SpareMeta>,
    golden: Option<&GoldenMeta>,
    attached: bool,
    leftovers: bool,
) -> Option<String> {
    match (meta, golden) {
        (None, _) if attached || leftovers => Some("left half-made".into()),
        (None, _) => None,
        (Some(_), None) => Some("no golden image".into()),
        (Some(m), Some(g)) if m.generation != g.generation || m.commit != g.commit => Some(format!(
            "made from gen {}, golden is gen {}",
            m.generation, g.generation
        )),
        (Some(_), Some(_)) if !attached => Some("not mounted (reboot?)".into()),
        _ => None,
    }
}

const META: &str = "spare.json";
const UNSUPPORTED: &str = "unsupported";

impl Clove {
    fn spare_meta(&self) -> Option<SpareMeta> {
        read_json(&self.spare_dir().join(META)).ok().flatten()
    }

    fn spare_store(&self) -> Option<&'static dyn Store> {
        // btrfs snapshots need no mount of their own: nothing to get ahead of.
        if !self.config.spare || !cfg!(target_os = "macos") {
            return None;
        }
        self.record().ok().flatten()?.backend.store().ok()
    }

    /// Spare files on disk without a record: an interrupted fill.
    fn spare_leftovers(&self) -> bool {
        std::fs::read_dir(self.spare_dir()).is_ok_and(|rd| {
            rd.flatten()
                .any(|e| !matches!(e.file_name().to_str(), Some("fill.lock" | UNSUPPORTED | META)))
        })
    }

    /// Anything of a spare on disk or mounted, usable or not.
    pub fn spare_exists(&self) -> bool {
        self.spare_meta().is_some()
            || self.spare_leftovers()
            || crate::backend::is_mounted(&self.spare_mount())
    }

    pub fn spare_state(&self) -> SpareState {
        let Some(store) = self.spare_store() else {
            return SpareState::Off;
        };
        if let Ok(why) = std::fs::read_to_string(self.spare_dir().join(UNSUPPORTED)) {
            return SpareState::Unsupported(why.trim().to_string());
        }
        if !self.spare_dir().exists() {
            return SpareState::Missing;
        }
        if FileLock::try_acquire(&self.spare_dir().join("fill.lock")).is_err() {
            return SpareState::Filling;
        }
        let meta = self.spare_meta();
        let golden = self.golden_meta().ok().flatten();
        let attached = store.spare_attached(self);
        match stale_reason(meta.as_ref(), golden.as_ref(), attached, self.spare_leftovers()) {
            Some(why) => SpareState::Stale(why),
            None => match meta {
                Some(m) => SpareState::Ready(m.generation),
                None => SpareState::Missing,
            },
        }
    }

    /// Makes the spare unless a usable one exists or another fill runs.
    /// Background fills skip a machine where a claim failed; `retry` doesn't.
    pub fn fill_spare(&self, retry: bool) -> Result<SpareState> {
        let Some(store) = self.spare_store() else {
            self.discard_spare(false)?;
            return Ok(SpareState::Off);
        };
        let dir = self.spare_dir();
        std::fs::create_dir_all(&dir)?;
        // spare_state() holds the lock for an instant; a fill holds it for seconds.
        let lock = dir.join("fill.lock");
        let Some(_fill) = (0..5).find_map(|i| {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            FileLock::try_acquire(&lock).ok()
        }) else {
            return Ok(SpareState::Filling);
        };
        if retry {
            let _ = std::fs::remove_file(dir.join(UNSUPPORTED));
        } else if let Ok(why) = std::fs::read_to_string(dir.join(UNSUPPORTED)) {
            return Ok(SpareState::Unsupported(why.trim().to_string()));
        }
        // A refresh that lands while the spare mounts makes it stale: go again.
        for _ in 0..3 {
            let golden = {
                let _lock = self.lock()?;
                let Some(golden) = self.golden_meta()? else {
                    return Ok(SpareState::Missing);
                };
                if usable(self.spare_meta().as_ref(), &golden, store.spare_attached(self)) {
                    return Ok(SpareState::Ready(golden.generation));
                }
                let _ = std::fs::remove_file(dir.join(META));
                store.spare_discard(self)?;
                // Under the lock, like `new`: refresh prunes old generations.
                if !store.spare_create(self, golden.generation)? {
                    return Ok(SpareState::Off);
                }
                golden
            };
            if let Err(e) = store.spare_attach(self) {
                let _ = store.spare_discard(self);
                return Err(e);
            }
            let _lock = self.lock()?;
            if self.current_generation()? == Some(golden.generation) {
                let meta = SpareMeta {
                    generation: golden.generation,
                    commit: golden.commit.clone(),
                    created_at: now_unix(),
                };
                write_json(&dir.join(META), &meta)?;
                return Ok(SpareState::Ready(golden.generation));
            }
        }
        Ok(SpareState::Stale("the golden image kept changing".into()))
    }

    /// Turns the spare into clone `name` at `mount`. Call under the repo lock.
    /// False when there is no usable spare; the caller then copies the image.
    pub(crate) fn claim_spare(&self, golden: &GoldenMeta, name: &str, mount: &Path) -> bool {
        let Some(store) = self.spare_store() else {
            return false;
        };
        // A fill writes the record last, so none means none ready.
        if !usable(self.spare_meta().as_ref(), golden, store.spare_attached(self)) {
            return false;
        }
        let dir = self.spare_dir();
        let _ = std::fs::remove_file(dir.join(META));
        match store.spare_claim(self, name, mount) {
            Ok(()) => true,
            Err(e) => {
                // Renaming a mount point may fail on other macOS versions;
                // stop making spares there rather than mount each one twice.
                let _ = std::fs::write(dir.join(UNSUPPORTED), format!("{e:#}\n"));
                let _ = store.spare_discard(self);
                false
            }
        }
    }

    /// Detaches and deletes the spare. With `stale_only`, a usable one stays.
    /// Skips (Ok(false)) while a fill runs. For gc, deinit and `spare = false`.
    pub fn discard_spare(&self, stale_only: bool) -> Result<bool> {
        let dir = self.spare_dir();
        if !dir.exists() {
            return Ok(false);
        }
        let Ok(_fill) = FileLock::try_acquire(&dir.join("fill.lock")) else {
            return Ok(false);
        };
        let store = match self.record()? {
            Some(r) => r.backend.store()?,
            None => return Ok(false),
        };
        if stale_only && self.config.spare {
            let golden = self.golden_meta()?;
            let reason = stale_reason(
                self.spare_meta().as_ref(),
                golden.as_ref(),
                store.spare_attached(self),
                self.spare_leftovers(),
            );
            if reason.is_none() {
                return Ok(false);
            }
        }
        let _ = std::fs::remove_file(dir.join(META));
        store.spare_discard(self)?;
        Ok(true)
    }

    /// Starts `clove spare` detached, unless the spare is ready or off.
    pub fn spawn_spare_fill(&self) {
        if !matches!(self.spare_state(), SpareState::Missing | SpareState::Stale(_)) {
            return;
        }
        let Ok(exe) = crate::service::cli_exe() else {
            return;
        };
        let _ = std::fs::create_dir_all(self.logs_dir());
        let Ok(out) = std::fs::File::create(self.logs_dir().join("spare.log")) else {
            return;
        };
        let err = out.try_clone().ok();
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("-C")
            .arg(&self.root)
            .arg("spare")
            // Its cwd must not hold a clone it might detach.
            .current_dir(&self.root)
            .stdin(std::process::Stdio::null())
            .stdout(out)
            .process_group(0);
        if let Some(e) = err {
            cmd.stderr(e);
        }
        let _ = cmd.spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden(generation: u32, commit: &str) -> GoldenMeta {
        serde_json::from_value(serde_json::json!({
            "generation": generation,
            "commit": commit,
            "base": "origin/main",
            "created_at": 0,
            "timings": [],
            "steps": [],
            "dirty": false,
        }))
        .unwrap()
    }

    fn spare(generation: u32, commit: &str) -> SpareMeta {
        SpareMeta {
            generation,
            commit: commit.into(),
            created_at: 0,
        }
    }

    #[test]
    fn only_a_mounted_spare_of_the_current_golden_is_usable() {
        let g = golden(3, "abc");
        assert!(usable(Some(&spare(3, "abc")), &g, true));
        assert!(
            !usable(Some(&spare(3, "abc")), &g, false),
            "unmounted after a reboot"
        );
        assert!(!usable(Some(&spare(2, "abc")), &g, true), "older generation");
        // deinit + init restarts at gen 1: the commit tells them apart.
        assert!(!usable(Some(&spare(3, "old")), &g, true));
        assert!(!usable(None, &g, true), "a fill that has not finished");
    }

    #[test]
    fn leftovers_are_stale_and_nothing_is_not() {
        let g = golden(3, "abc");
        assert_eq!(stale_reason(None, Some(&g), false, false), None);
        assert!(
            stale_reason(None, Some(&g), true, false).is_some(),
            "mounted, no record"
        );
        assert!(
            stale_reason(None, Some(&g), false, true).is_some(),
            "image, no record"
        );
        assert!(stale_reason(Some(&spare(2, "x")), Some(&g), true, true).is_some());
        assert!(stale_reason(Some(&spare(3, "abc")), Some(&g), false, true).is_some());
        assert!(stale_reason(Some(&spare(3, "abc")), None, true, true).is_some());
        assert_eq!(stale_reason(Some(&spare(3, "abc")), Some(&g), true, true), None);
    }
}
