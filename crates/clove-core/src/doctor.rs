//! `clove doctor`: checks the machine and repo, with a fix for each problem.

use crate::backend::{self, Backend};
use crate::detect::required_tool;
use crate::repo::Clove;
use crate::util::{run, which};
use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub level: Level,
    pub detail: String,
    /// A command that fixes it, when there is one.
    pub fix: Option<String>,
}

fn check(name: &str, level: Level, detail: impl Into<String>, fix: Option<String>) -> Check {
    Check {
        name: name.into(),
        level,
        detail: detail.into(),
        fix,
    }
}

/// Disk images and the volume they live on.
#[cfg(target_os = "macos")]
fn storage() -> Vec<Check> {
    let mut out = vec![];
    match Backend::detect() {
        Ok(Backend::Asif) => out.push(check(
            "disk images",
            Level::Ok,
            "ASIF via diskutil (macOS 26+)",
            None,
        )),
        Ok(Backend::Sparse) => out.push(check(
            "disk images",
            Level::Warn,
            "falling back to hdiutil sparse images; macOS 26+ is faster",
            None,
        )),
        Ok(other) => out.push(check(
            "disk images",
            Level::Fail,
            format!("{other:?} on macOS"),
            None,
        )),
        Err(e) => out.push(check("disk images", Level::Fail, format!("{e:#}"), None)),
    }
    let home = crate::repo::state_home();
    let _ = std::fs::create_dir_all(&home);
    match backend::fs_type(&home).as_deref() {
        Some("apfs") => out.push(check(
            "state volume",
            Level::Ok,
            format!("{} is on APFS", home.display()),
            None,
        )),
        Some(t) => out.push(check(
            "state volume",
            Level::Fail,
            format!("{} is on {t}; images need APFS clonefile", home.display()),
            Some("export CLOVE_HOME=<a directory on an APFS volume>".into()),
        )),
        None => {}
    }
    out
}

/// btrfs-progs, kernel support, loop devices and udisks.
#[cfg(target_os = "linux")]
fn storage() -> Vec<Check> {
    let mut out = vec![];
    match Backend::detect() {
        Ok(_) => out.push(check(
            "btrfs-progs",
            Level::Ok,
            run(Command::new("btrfs").arg("--version"))
                .ok()
                .and_then(|v| v.lines().next().map(str::to_string))
                .unwrap_or_else(|| "installed".into()),
            None,
        )),
        Err(_) => out.push(check(
            "btrfs-progs",
            Level::Fail,
            "mkfs.btrfs and btrfs not found; clove keeps each repo in a btrfs image",
            Some("sudo apt install btrfs-progs".into()),
        )),
    }
    let fs = std::fs::read_to_string("/proc/filesystems").unwrap_or_default();
    if fs.lines().any(|l| l.split_whitespace().last() == Some("btrfs")) {
        out.push(check("kernel btrfs", Level::Ok, "btrfs is available", None));
    } else {
        out.push(check(
            "kernel btrfs",
            Level::Warn,
            "btrfs isn't loaded; mounting will try to load it",
            Some("sudo modprobe btrfs".into()),
        ));
    }
    if std::path::Path::new("/dev/loop-control").exists() {
        out.push(check(
            "loop devices",
            Level::Ok,
            "/dev/loop-control present",
            None,
        ));
    } else {
        out.push(check(
            "loop devices",
            Level::Fail,
            "no /dev/loop-control; images can't be attached",
            Some("sudo modprobe loop".into()),
        ));
    }
    if which("udisksctl").is_some() {
        out.push(check(
            "udisks2",
            Level::Ok,
            "udisksctl attaches images without sudo",
            None,
        ));
    } else {
        out.push(check(
            "udisks2",
            Level::Warn,
            "udisksctl not found; attaching images falls back to `sudo -n`",
            Some("sudo apt install udisks2".into()),
        ));
    }
    out
}

pub fn machine() -> Vec<Check> {
    let mut out = storage();
    match run(Command::new("git").arg("--version")) {
        Ok(v) => {
            let ver = v.trim_start_matches("git version ").to_string();
            let ok = ver
                .split('.')
                .take(2)
                .map(|p| p.parse::<u32>().unwrap_or(0))
                .collect::<Vec<_>>();
            let new_enough = ok.first().copied().unwrap_or(0) > 2
                || (ok.first() == Some(&2) && ok.get(1).copied().unwrap_or(0) >= 38);
            out.push(check(
                "git",
                if new_enough { Level::Ok } else { Level::Warn },
                if new_enough {
                    ver
                } else {
                    format!("{ver}; 2.38+ recommended")
                },
                None,
            ));
        }
        Err(_) => out.push(check("git", Level::Fail, "git not found", None)),
    }
    #[cfg(target_os = "macos")]
    out.extend(first_runs());
    out
}

/// macOS checks a binary the first time it runs, per file. Every clone holds
/// new copies of the repo's tools (esbuild, swc, native modules), so each one
/// pays it again in each clone, and agents in many clones pay it many times.
/// Programs started from an app listed under Developer Tools skip it. Probed
/// by running a freshly signed copy of `true` twice.
#[cfg(target_os = "macos")]
fn first_runs() -> Option<Check> {
    // Twice, so one slow start can't raise a false alarm.
    let delay = first_run_delay()?.min(first_run_delay()?);
    Some(if delay >= FIRST_RUN_SLOW {
        check(
            "first runs",
            Level::Warn,
            format!(
                "macOS checks each new binary on its first run ({}ms here), and every clone brings \
                 new copies; add your terminal to System Settings > Privacy & Security > Developer Tools",
                delay.as_millis()
            ),
            Some("open 'x-apple.systempreferences:com.apple.preference.security?Privacy_DevTools'".into()),
        )
    } else {
        check(
            "first runs",
            Level::Ok,
            "new binaries start without a check",
            None,
        )
    })
}

/// A first run this much slower than the second means macOS checked it.
#[cfg(target_os = "macos")]
const FIRST_RUN_SLOW: std::time::Duration = std::time::Duration::from_millis(30);

#[cfg(target_os = "macos")]
fn first_run_delay() -> Option<std::time::Duration> {
    use std::time::Instant;
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let probe = std::env::temp_dir().join(format!("clove-first-run-{}-{n}", std::process::id()));
    let delay = (|| {
        std::fs::copy("/usr/bin/true", &probe).ok()?;
        let signed = Command::new("codesign")
            .args(["-s", "-", "-f"])
            .arg(&probe)
            .output()
            .ok()?
            .status
            .success();
        if !signed {
            return None;
        }
        let time = || {
            let t = Instant::now();
            Command::new(&probe).status().ok().filter(|s| s.success())?;
            Some(t.elapsed())
        };
        let first = time()?;
        let second = time()?;
        Some(first.saturating_sub(second))
    })();
    let _ = std::fs::remove_file(&probe);
    delay
}

pub fn repo(c: &Clove) -> Vec<Check> {
    let mut out = vec![];
    let src = if c.config.steps_detected {
        "detected"
    } else {
        "from config"
    };
    let names: Vec<_> = c.config.steps.iter().map(|s| s.name.as_str()).collect();
    out.push(check(
        "steps",
        Level::Ok,
        if names.is_empty() {
            "none (clones get only the checkout; add [[step]] to .clove.toml)".to_string()
        } else {
            format!("{} ({src})", names.join(", "))
        },
        None,
    ));
    if c.config.steps_detected {
        let nested = crate::detect::nested_projects(&c.root);
        if nested.len() > crate::detect::MAX_NESTED {
            let shown: Vec<_> = nested.iter().take(6).map(String::as_str).collect();
            let more = nested.len().saturating_sub(shown.len());
            out.push(check(
                "nested projects",
                Level::Ok,
                format!(
                    "{} have their own lockfile and aren't warmed (too many to build by default): {}{}",
                    nested.len(),
                    shown.join(", "),
                    if more > 0 {
                        format!(" and {more} more")
                    } else {
                        String::new()
                    }
                ),
                Some("add [[add_step]] entries to .clove.toml for the ones you work on".into()),
            ));
        }
    }
    for s in &c.config.steps {
        if let Some(tool) = required_tool(s)
            && which(tool).is_none()
            && c.config.exec == crate::config::Exec::None
        {
            out.push(check(
                &format!("tool {tool}"),
                Level::Warn,
                format!("step {} needs {tool} on PATH", s.name),
                None,
            ));
        }
    }
    #[cfg(target_os = "linux")]
    out.extend(fstab(c));
    #[cfg(target_os = "linux")]
    out.extend(quota(c));
    match c.golden_meta() {
        Ok(Some(m)) => {
            let f = c.freshness().ok().flatten();
            let (level, detail, fix) = match f.as_ref().and_then(|f| f.reason.clone()) {
                _ if f.as_ref().is_some_and(|f| f.refreshing.is_some()) => {
                    (Level::Ok, "refreshing now".to_string(), None)
                }
                Some(why) => (
                    Level::Warn,
                    format!("stale: {why}"),
                    Some(if c.config.refresh.auto {
                        "clove refresh (or let the next `clove new` start one; `clove service install` checks hourly)"
                            .to_string()
                    } else {
                        "clove refresh".to_string()
                    }),
                ),
                None => (Level::Ok, "fresh".to_string(), None),
            };
            out.push(check(
                "golden",
                level,
                format!(
                    "gen {} at {}, {detail}",
                    m.generation,
                    &m.commit[..m.commit.len().min(10)]
                ),
                fix,
            ));
            if !m.copied.is_empty() {
                let how = if c.config.copy.is_some() {
                    "listed in `copy`"
                } else {
                    "gitignored env files, detected"
                };
                out.push(check(
                    "local files",
                    Level::Ok,
                    format!(
                        "copied into each clone, since git won't bring them ({how}): {}",
                        m.copied.join(", ")
                    ),
                    None,
                ));
            }
            if m.dirty {
                out.push(check(
                    "golden clean",
                    Level::Warn,
                    "warm steps modify tracked files; every clone starts dirty",
                    None,
                ));
            }
        }
        Ok(None) => out.push(check(
            "golden",
            Level::Warn,
            "not built yet",
            Some("clove init".into()),
        )),
        Err(e) => out.push(check("golden", Level::Fail, format!("{e:#}"), None)),
    }
    if let Ok(actions) = c.gc(false)
        && !actions.is_empty()
    {
        out.push(check(
            "leftovers",
            Level::Warn,
            actions.join("; "),
            Some("clove gc --apply".into()),
        ));
    }
    out
}

/// Mounting without sudo: an /etc/fstab line, else udisks under /run/media.
#[cfg(target_os = "linux")]
fn fstab(c: &Clove) -> Option<Check> {
    if !c.shared_volume() {
        return None;
    }
    let image = backend::btrfs::image_path(c);
    let fix = Some(format!(
        "echo '{}' | sudo tee -a /etc/fstab",
        backend::btrfs::fstab_line(&image, &c.config.clone_root)
    ));
    let recorded = c.recorded_mount();
    Some(match backend::btrfs::fstab_target(&image) {
        Some(at) if recorded.as_ref().is_none_or(|r| *r == at) => check(
            "mounting",
            Level::Ok,
            format!("/etc/fstab mounts the image at {} without sudo", at.display()),
            None,
        ),
        Some(at) => check(
            "mounting",
            Level::Warn,
            format!(
                "/etc/fstab mounts the image at {}, but its clones were made at {}; recreate them (clove rm --all, deinit, init)",
                at.display(),
                recorded.unwrap_or_default().display()
            ),
            None,
        ),
        None => match recorded {
            Some(r) if r != c.config.clone_root => check(
                "mounting",
                Level::Warn,
                format!(
                    "clones live under {} (udisks), which needs a desktop session to mount; an /etc/fstab line works over SSH too, after recreating the clones",
                    r.display()
                ),
                fix,
            ),
            _ => check(
                "mounting",
                Level::Warn,
                "no /etc/fstab line; mounting goes through udisks (desktop sessions) or sudo",
                fix,
            ),
        },
    })
}

/// Clone sizes come from squota/qgroups, set up when the image is made.
#[cfg(target_os = "linux")]
fn quota(c: &Clove) -> Option<Check> {
    let rec = c.record().ok().flatten()?;
    if rec.backend != Backend::Btrfs {
        return None;
    }
    let pool = c.clone_root();
    if !backend::is_mounted(&pool) {
        return Some(check(
            "quota",
            Level::Warn,
            format!("{} isn't mounted", pool.display()),
            Some("clove mount".into()),
        ));
    }
    Some(match backend::btrfs::quota_enabled(&pool) {
        Some(true) => check("quota", Level::Ok, "btrfs quotas on; clone sizes are exact", None),
        _ => check(
            "quota",
            Level::Warn,
            "the image has no quotas (made on a kernel older than 6.7?), so clone sizes are unknown",
            None,
        ),
    })
}

#[cfg(all(test, target_os = "macos"))]
mod first_run_tests {
    use super::*;

    #[test]
    fn probes_and_cleans_up() {
        let before = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .is_ok_and(|e| e.file_name().to_string_lossy().starts_with("clove-first-run-"))
            })
            .count();
        // Where the probe can't be signed (some CI runners), doctor leaves
        // the row out rather than guess.
        let _ = first_run_delay();
        let _ = first_runs();
        let after = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .is_ok_and(|e| e.file_name().to_string_lossy().starts_with("clove-first-run-"))
            })
            .count();
        assert_eq!(before, after, "probe files are removed");
    }
}
