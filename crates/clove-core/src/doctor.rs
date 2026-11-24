//! `clove doctor`: checks the machine and repo, with a fix for each problem.

use crate::backend::{self, Backend};
use crate::detect::required_tool;
use crate::repo::{Clove, state_home};
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

pub fn machine() -> Vec<Check> {
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
        Err(e) => out.push(check("disk images", Level::Fail, format!("{e:#}"), None)),
    }
    let home = state_home();
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
    out
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
    if c.uses_pnpm() {
        match c.pnpm_store() {
            Some(store) => {
                let global = run(Command::new("pnpm").args(["config", "get", "store-dir", "--global"]))
                    .unwrap_or_default();
                if global.trim() == store {
                    out.push(check("pnpm store", Level::Ok, format!("pinned to {store}"), None));
                } else {
                    out.push(check(
                        "pnpm store",
                        Level::Warn,
                        "clove steps pin the store, but a plain `pnpm add` in a clone would start an empty store on the clone's volume",
                        Some(format!("pnpm config set store-dir {store} --global")),
                    ));
                }
            }
            None => out.push(check(
                "pnpm store",
                Level::Warn,
                "could not run `pnpm store path`",
                None,
            )),
        }
    }
    match c.golden_meta() {
        Ok(Some(m)) => {
            let behind =
                crate::git::count_commits(&c.root, &format!("{}..{}", m.commit, m.base)).unwrap_or(0);
            let level = if behind > 200 { Level::Warn } else { Level::Ok };
            out.push(check(
                "golden",
                level,
                format!(
                    "gen {} at {} ({behind} commits behind {})",
                    m.generation,
                    &m.commit[..m.commit.len().min(10)],
                    m.base
                ),
                (behind > 200).then(|| "clove refresh".to_string()),
            ));
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
