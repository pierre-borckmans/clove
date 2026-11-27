//! What coding agents need to know about clove, and installing it where they
//! read it: a Claude Code skill, a short block in the agents' global
//! instructions files, and Claude Code's worktree hooks.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

/// `clove guide`: everything an agent needs, for this version of clove.
pub const GUIDE: &str = r#"# clove: copy-on-write git worktrees

clove makes a ready worktree (a "clone") of a repo in about a second, with
dependencies installed and the dev environment set up, by copying a
prepared "golden image" instead of checking out and installing again.

## When to use it

In a repo clove manages (`clove repos` lists them; `clove status` in the repo
says so), use a clone wherever you would use `git worktree add`: a separate
branch to work on, a PR to review or test, parallel work next to another
agent. Never `git worktree add` there. In other repos, clove isn't set up;
use git as usual.

## Make one

    clove new <branch> --json      # prints {"path": ..., "branch": ...}
    clove new pr:123 --json        # check out pull request #123
    clove new -b origin/main x     # new branch x from a given start point

A missing branch is created from the base (usually origin/main). Work in the
printed `path` like any git worktree: edit, commit, push, open PRs. Clones
are named after the branch, with `/` turned into `-`.

## Work in it

    clove path <clone>             # its path, mounting it if needed
    clove run <clone> -- <cmd>     # run a command there, in its dev environment
    clove setup                    # after a pull/rebase changed lockfiles: reinstall what changed
    clove ls --json                # clones with branch, changes, PR state
    clove stats --json             # how long clones take, and which step took it
    clove diff <clone> --stat      # what it changed against the base (--json: per file and package)

`clove cd <clone>` only works in an interactive shell with clove's shell
integration; use `clove path` from scripts and agents.

A clone has the dependencies of the projects clove warms. If a nested
project (its own lockfile, outside the root workspace) has no
`node_modules`, `clove status` or `clove doctor` names it; install it in the
clone as usual (`cd <dir> && pnpm install`) and tell the user it could be
warmed with `nested_installs` or an `[[add_step]]`.

## Bring it up to date

    clove update [clone]           # fetch the base, rebase onto it, rerun changed setup steps
    clove update --merge           # merge the base instead (default: the update.strategy setting)
    clove update --autostash       # with uncommitted changes: set them aside, reapply after

Without `--autostash`, uncommitted changes are refused. On conflicts it
stops in git's usual rebase or merge state, lists the files (`--json`:
`files`) and exits 1. Fix them, `git add` them, then
`clove update --continue`; `clove update --abort` puts the branch back. After
rebasing a branch that was pushed, push with `git push --force-with-lease`.
`clove update --resolve` hands conflicts to another agent for a person at a
terminal; as an agent, resolve them yourself.

## Remove it

    clove rm <clone>               # unmount and delete; keeps the branch
    clove rm -D <clone>            # also delete the branch

Only remove clones you made in this session. Others may belong to the user
or to another agent. `clove rm` refuses uncommitted changes; if a process
still uses the clone it is removed as soon as nothing does. Don't pass
`--force` unless the user asked to discard changes.

## Merged PRs and missing clones

`clove prune` lists clones whose PR merged and removes them on confirmation
(`--apply` skips it). It touches every clone of the repo, so run it only
when the user asks.

With `prune.auto` on (`clove config get prune.auto`), clove's background
service removes merged clones by itself, but only when they are clean, have
nothing after the merged commit, nothing has them open, and git has been
quiet in them for an hour. If a clone you used is gone:

    clove log                      # what was removed, when, by what, why
    clove log --json               # includes the command to restore the branch

Commands on a removed clone also say who removed it and how to get the
branch back.

## Other commands

    clove status                   # golden image age, clones, disk use
    clove refresh                  # rebuild the golden image now (the service does it on its own)
    clove adopt <path>             # turn a clean plain git worktree into a clone
    clove mount / clove unmount    # after a reboot the service remounts clones at login
    clove gc                       # leftovers from interrupted operations
    clove doctor                   # machine and repo checks
    clove config                   # settings and the files they come from
    clove init / clove deinit      # set a repo up, or remove clove from it (ask the user first)

Every command takes `--json`. Output for people goes to stderr; with
`--json`, stdout carries only JSON.
"#;

/// The instructions-file block: enough to reach for clove at the right time.
const BLOCK: &str = "\
## clove
In repos clove manages (`clove repos` lists them), make worktrees with `clove new <branch> --json` (ready in ~1s with dependencies installed; prints the path), never `git worktree add`. Run `clove guide` before using clove: it covers running commands in a clone, removing only your own clones, and finding clones the service pruned after their PR merged (`clove log`).";

const BEGIN: &str =
    "<!-- clove:begin (written by `clove agents install`; `clove agents remove` takes it out) -->";
const END: &str = "<!-- clove:end -->";

const SKILL: &str = "\
---
name: clove
description: Use when you need a separate git worktree or branch checkout (instead of git worktree add), work in a clove clone (under <repo>.clove/), or a clove clone went missing. clove makes ready worktrees with dependencies installed in ~1s.
---

# clove

Run `clove guide` first and follow it: it matches the installed version.

- In repos clove manages (`clove repos`): `clove new <branch> --json` instead of `git worktree add`; use the printed path.
- `clove run <clone> -- <cmd>` runs in the clone's dev environment; `clove path <clone>` prints its path.
- Remove only clones you made: `clove rm <clone>`. Run `clove prune` only when asked.
- A clone gone? `clove log` says who removed it (often auto-prune after its PR merged) and how to get the branch back.
";

const HOOK_CREATE: &str = "clove hook worktree-create";
const HOOK_REMOVE: &str = "clove hook worktree-remove";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Installed,
    /// Installed by an older clove.
    Outdated,
    Missing,
}

/// One place agents read clove's instructions from.
#[derive(Debug, Clone, Serialize)]
pub struct Target {
    pub what: &'static str,
    pub path: PathBuf,
    pub state: State,
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    Skill,
    Block,
    Hooks,
}

fn targets(home: &Path) -> Vec<(&'static str, PathBuf, Kind)> {
    let mut v = vec![];
    let claude = home.join(".claude");
    if claude.is_dir() {
        v.push((
            "Claude Code skill",
            claude.join("skills/clove/SKILL.md"),
            Kind::Skill,
        ));
        v.push(("Claude Code instructions", claude.join("CLAUDE.md"), Kind::Block));
        v.push((
            "Claude Code worktree hooks",
            claude.join("settings.json"),
            Kind::Hooks,
        ));
    }
    let codex = home.join(".codex");
    if codex.is_dir() {
        v.push(("Codex instructions", codex.join("AGENTS.md"), Kind::Block));
    }
    v
}

pub fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")
}

/// Where each agent stands; empty when no supported agent is installed.
pub fn status_in(home: &Path) -> Vec<Target> {
    targets(home)
        .into_iter()
        .map(|(what, path, kind)| {
            let state = state(&path, kind);
            Target { what, path, state }
        })
        .collect()
}

pub fn status() -> Result<Vec<Target>> {
    Ok(status_in(&home()?))
}

/// Overall: installed only when every target is.
pub fn summary(ts: &[Target]) -> State {
    if ts.is_empty() || ts.iter().all(|t| t.state == State::Missing) {
        State::Missing
    } else if ts.iter().all(|t| t.state == State::Installed) {
        State::Installed
    } else {
        State::Outdated
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn state(path: &Path, kind: Kind) -> State {
    let s = read(path);
    match kind {
        Kind::Skill if s.is_empty() => State::Missing,
        Kind::Skill if s == SKILL => State::Installed,
        Kind::Skill => State::Outdated,
        Kind::Block => match block_of(&s) {
            None => State::Missing,
            Some(b) if b == BLOCK => State::Installed,
            Some(_) => State::Outdated,
        },
        Kind::Hooks => {
            let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
            match (
                has_hook(&v, "WorktreeCreate", HOOK_CREATE),
                has_hook(&v, "WorktreeRemove", HOOK_REMOVE),
            ) {
                (true, true) => State::Installed,
                (false, false) => State::Missing,
                _ => State::Outdated,
            }
        }
    }
}

/// The text between the markers, if any.
fn block_of(s: &str) -> Option<&str> {
    let start = s.find(BEGIN)? + BEGIN.len();
    let end = start + s[start..].find(END)?;
    Some(s[start..end].trim_matches('\n'))
}

/// `s` with the block set to `body`, or taken out when `None`. The rest of
/// the file stays as it was.
fn with_block(s: &str, body: Option<&str>) -> String {
    let span = s.find(BEGIN).and_then(|a| {
        let e = a + s[a..].find(END)? + END.len();
        Some((a, e))
    });
    let new = body.map(|b| format!("{BEGIN}\n{b}\n{END}"));
    match (span, new) {
        (Some((a, e)), Some(n)) => format!("{}{n}{}", &s[..a], &s[e..]),
        (Some((a, e)), None) => {
            let before = s[..a].trim_end_matches('\n');
            let after = s[e..].trim_start_matches('\n');
            match (before.is_empty(), after.is_empty()) {
                (true, _) => after.to_string(),
                (false, true) => format!("{before}\n"),
                (false, false) => format!("{before}\n\n{after}"),
            }
        }
        (None, Some(n)) if s.trim().is_empty() => format!("{n}\n"),
        (None, Some(n)) => format!("{}\n\n{n}\n", s.trim_end_matches('\n')),
        (None, None) => s.to_string(),
    }
}

fn has_hook(v: &serde_json::Value, event: &str, cmd: &str) -> bool {
    v["hooks"][event].as_array().is_some_and(|groups| {
        groups.iter().any(|g| {
            g["hooks"]
                .as_array()
                .is_some_and(|hs| hs.iter().any(|h| h["command"].as_str() == Some(cmd)))
        })
    })
}

/// Settings with clove's worktree hooks added (or taken out), other hooks
/// and keys untouched.
fn with_hooks(mut v: serde_json::Value, on: bool) -> serde_json::Value {
    if !v.is_object() {
        v = serde_json::json!({});
    }
    for (event, cmd) in [("WorktreeCreate", HOOK_CREATE), ("WorktreeRemove", HOOK_REMOVE)] {
        if on {
            if has_hook(&v, event, cmd) {
                continue;
            }
            if !v["hooks"].is_object() {
                v["hooks"] = serde_json::json!({});
            }
            if !v["hooks"][event].is_array() {
                v["hooks"][event] = serde_json::json!([]);
            }
            if let Some(a) = v["hooks"][event].as_array_mut() {
                a.push(serde_json::json!({ "hooks": [{ "type": "command", "command": cmd }] }));
            }
        } else if let Some(groups) = v["hooks"][event].as_array_mut() {
            for g in groups.iter_mut() {
                if let Some(hs) = g["hooks"].as_array_mut() {
                    hs.retain(|h| h["command"].as_str() != Some(cmd));
                }
            }
            groups.retain(|g| g["hooks"].as_array().is_none_or(|hs| !hs.is_empty()));
            if groups.is_empty()
                && let Some(m) = v["hooks"].as_object_mut()
            {
                m.remove(event);
            }
        }
    }
    if v["hooks"].as_object().is_some_and(|m| m.is_empty())
        && let Some(m) = v.as_object_mut()
    {
        m.remove("hooks");
    }
    v
}

fn write(path: &Path, body: &str) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    crate::util::write_atomic(path, body.as_bytes())
}

/// Writes or takes out every target. Returns what changed.
pub fn apply_in(home: &Path, on: bool) -> Result<Vec<Target>> {
    let mut changed = vec![];
    for (what, path, kind) in targets(home) {
        let before = state(&path, kind);
        if (on && before == State::Installed) || (!on && before == State::Missing) {
            continue;
        }
        match kind {
            Kind::Skill if on => write(&path, SKILL)?,
            Kind::Skill => {
                std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
                // The folders too, when nothing else is in them.
                for d in path.ancestors().skip(1).take(2) {
                    let _ = std::fs::remove_dir(d);
                }
            }
            Kind::Block => {
                let s = with_block(&read(&path), on.then_some(BLOCK));
                if s.is_empty() && !on {
                    std::fs::remove_file(&path)?;
                } else {
                    write(&path, &s)?;
                }
            }
            Kind::Hooks => {
                let text = read(&path);
                let v: serde_json::Value = if text.trim().is_empty() {
                    serde_json::json!({})
                } else {
                    serde_json::from_str(&text)
                        .with_context(|| format!("{} is not valid JSON; fix it first", path.display()))?
                };
                if !text.is_empty() {
                    // One copy of what was there before clove touched it.
                    let backup = path.with_extension("json.before-clove");
                    if !backup.exists() {
                        std::fs::copy(&path, &backup)?;
                    }
                }
                let out = serde_json::to_string_pretty(&with_hooks(v, on))? + "\n";
                write(&path, &out)?;
            }
        }
        changed.push(Target {
            what,
            path: path.clone(),
            state: state(&path, kind),
        });
    }
    Ok(changed)
}

pub fn install() -> Result<Vec<Target>> {
    apply_in(&home()?, true)
}

pub fn remove() -> Result<Vec<Target>> {
    apply_in(&home()?, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home_with(dirs: &[&str]) -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        for d in dirs {
            std::fs::create_dir_all(t.path().join(d)).unwrap();
        }
        t
    }

    #[test]
    fn only_agents_that_exist_are_targets() {
        let h = home_with(&[]);
        assert!(status_in(h.path()).is_empty());
        let h = home_with(&[".codex"]);
        let s = status_in(h.path());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].what, "Codex instructions");
    }

    #[test]
    fn install_then_remove_leaves_the_users_files_as_they_were() {
        let h = home_with(&[".claude", ".codex"]);
        let claude_md = "# mine\n\nkeep this\n";
        let settings = r#"{
  "model": "opus",
  "hooks": {
    "Stop": [{ "hooks": [{ "type": "command", "command": "say done" }] }]
  }
}
"#;
        std::fs::write(h.path().join(".claude/CLAUDE.md"), claude_md).unwrap();
        std::fs::write(h.path().join(".claude/settings.json"), settings).unwrap();

        let changed = apply_in(h.path(), true).unwrap();
        assert_eq!(changed.len(), 4);
        assert_eq!(summary(&status_in(h.path())), State::Installed);
        let md = read(&h.path().join(".claude/CLAUDE.md"));
        assert!(md.starts_with(claude_md.trim_end()), "{md}");
        assert!(md.contains("clove new <branch> --json"));
        let v: serde_json::Value =
            serde_json::from_str(&read(&h.path().join(".claude/settings.json"))).unwrap();
        assert_eq!(v["model"], "opus");
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "say done");
        assert!(has_hook(&v, "WorktreeCreate", HOOK_CREATE));
        // A second install changes nothing.
        assert!(apply_in(h.path(), true).unwrap().is_empty());

        apply_in(h.path(), false).unwrap();
        assert_eq!(summary(&status_in(h.path())), State::Missing);
        assert_eq!(read(&h.path().join(".claude/CLAUDE.md")), claude_md);
        let v: serde_json::Value =
            serde_json::from_str(&read(&h.path().join(".claude/settings.json"))).unwrap();
        let before: serde_json::Value = serde_json::from_str(settings).unwrap();
        assert_eq!(v, before);
        assert!(!h.path().join(".claude/skills/clove").exists());
        assert!(
            !h.path().join(".codex/AGENTS.md").exists(),
            "created by install, gone on remove"
        );
    }

    #[test]
    fn an_old_block_counts_as_outdated_and_install_replaces_it() {
        let h = home_with(&[".codex"]);
        let p = h.path().join(".codex/AGENTS.md");
        std::fs::write(&p, format!("top\n\n{BEGIN}\nold words\n{END}\n\nbottom\n")).unwrap();
        assert_eq!(status_in(h.path())[0].state, State::Outdated);
        apply_in(h.path(), true).unwrap();
        let s = read(&p);
        assert!(s.starts_with("top\n\n") && s.ends_with("\n\nbottom\n"), "{s}");
        assert_eq!(block_of(&s), Some(BLOCK));
    }

    #[test]
    fn broken_settings_json_is_left_alone() {
        let h = home_with(&[".claude"]);
        let p = h.path().join(".claude/settings.json");
        std::fs::write(&p, "{ not json").unwrap();
        assert!(apply_in(h.path(), true).is_err());
        assert_eq!(read(&p), "{ not json");
    }
}
