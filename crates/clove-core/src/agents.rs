//! What coding agents need to know about clove, and installing it where they
//! read it: skills, global instruction blocks, and hooks or extensions.

use std::io::{ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

pub const OPENCODE_PLUGIN: &str = include_str!("plugins/opencode.js");
pub const PI_EXTENSION: &str = include_str!("plugins/pi.js");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Installed,
    /// Needs attention: older, conflicting, or no longer in use.
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
    StaleBlock,
    Hooks,
    File(&'static str),
}

#[derive(Debug, Clone)]
struct Env {
    pub home: PathBuf,
    pub xdg_config_home: Option<PathBuf>,
    pub opencode_config_dir: Option<PathBuf>,
    pub opencode_disable_claude_prompt: bool,
    pub pi_agent_dir: Option<PathBuf>,
    pub has_opencode_bin: bool,
    pub has_pi_bin: bool,
}

impl Env {
    pub fn from_env() -> Result<Self> {
        let var = |name| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let path = std::env::var_os("PATH").unwrap_or_default();
        Ok(Self {
            home: home()?,
            xdg_config_home: var("XDG_CONFIG_HOME"),
            opencode_config_dir: var("OPENCODE_CONFIG_DIR"),
            opencode_disable_claude_prompt: [
                "OPENCODE_DISABLE_CLAUDE_CODE",
                "OPENCODE_DISABLE_CLAUDE_CODE_PROMPT",
            ]
            .iter()
            .any(|name| std::env::var(name).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))),
            pi_agent_dir: var("PI_CODING_AGENT_DIR"),
            has_opencode_bin: on_path(&path, "opencode"),
            has_pi_bin: on_path(&path, "pi"),
        })
    }

    pub fn for_home(home: &Path) -> Self {
        Self {
            home: home.to_path_buf(),
            xdg_config_home: None,
            opencode_config_dir: None,
            opencode_disable_claude_prompt: false,
            pi_agent_dir: None,
            has_opencode_bin: false,
            has_pi_bin: false,
        }
    }
}

fn on_path(path: &std::ffi::OsStr, name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;

    !path.is_empty()
        && std::env::split_paths(path).any(|d| {
            d.join(name)
                .metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

fn targets(env: &Env) -> Vec<(&'static str, PathBuf, Kind)> {
    let mut v = vec![];
    let claude = env.home.join(".claude");
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
    let codex = env.home.join(".codex");
    if codex.is_dir() {
        v.push(("Codex instructions", codex.join("AGENTS.md"), Kind::Block));
    }
    let opencode = env.opencode_config_dir.clone().unwrap_or_else(|| {
        env.xdg_config_home
            .clone()
            .unwrap_or_else(|| env.home.join(".config"))
            .join("opencode")
    });
    if opencode.is_dir() || env.has_opencode_bin {
        v.push((
            "OpenCode skill",
            opencode.join("skills/clove/SKILL.md"),
            Kind::Skill,
        ));
        let instructions = opencode.join("AGENTS.md");
        // OpenCode falls back to Claude's instructions when it has none.
        if instructions.exists()
            || env.opencode_disable_claude_prompt
            || (!claude.is_dir() && !claude.join("CLAUDE.md").exists())
        {
            v.push(("OpenCode instructions", instructions, Kind::Block));
        }
        v.push((
            "OpenCode plugin",
            opencode.join("plugins/clove.js"),
            Kind::File(OPENCODE_PLUGIN),
        ));
    }
    let pi_home = env.home.join(".pi");
    let pi = env.pi_agent_dir.clone().unwrap_or_else(|| pi_home.join("agent"));
    if pi.is_dir() || pi_home.is_dir() || env.has_pi_bin {
        let candidates = [
            "AGENTS.override.md",
            "AGENTS.md",
            "AGENTS.MD",
            "CLAUDE.md",
            "CLAUDE.MD",
        ]
        .map(|name| pi.join(name));
        let instructions = candidates
            .iter()
            .find(|p| p.is_file())
            .cloned()
            .unwrap_or_else(|| pi.join("AGENTS.md"));
        v.push(("Pi skill", pi.join("skills/clove/SKILL.md"), Kind::Skill));
        let mut seen: std::collections::HashSet<_> = instructions.canonicalize().into_iter().collect();
        v.push(("Pi instructions", instructions, Kind::Block));
        for path in candidates {
            if path.is_file()
                && block_of(&read(&path)).is_some()
                && path.canonicalize().is_ok_and(|p| seen.insert(p))
            {
                v.push(("Pi inactive instructions", path, Kind::StaleBlock));
            }
        }
        v.push((
            "Pi extension",
            pi.join("extensions/clove.js"),
            Kind::File(PI_EXTENSION),
        ));
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
    status_with(&Env::for_home(home))
}

fn status_with(env: &Env) -> Vec<Target> {
    targets(env)
        .into_iter()
        .map(|(what, path, kind)| {
            let state = state(&path, kind);
            Target { what, path, state }
        })
        .collect()
}

pub fn status() -> Result<Vec<Target>> {
    Ok(status_with(&Env::from_env()?))
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

fn read_existing(path: &Path) -> Result<Option<String>> {
    let Some(meta) = existing(path, path.symlink_metadata())? else {
        return Ok(None);
    };
    if meta.file_type().is_symlink() {
        anyhow::bail!("{} is a symlink; refusing to modify it", path.display());
    }
    std::fs::read_to_string(path)
        .map(Some)
        .with_context(|| format!("read {}", path.display()))
}

fn existing<T>(path: &Path, result: std::io::Result<T>) -> Result<Option<T>> {
    match result {
        Ok(contents) => Ok(Some(contents)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WholeFile {
    Missing,
    Current,
    OwnedOld,
    Foreign,
}

impl WholeFile {
    fn state(self) -> State {
        match self {
            Self::Missing => State::Missing,
            Self::Current => State::Installed,
            Self::OwnedOld | Self::Foreign => State::Outdated,
        }
    }
}

const MANAGED_FILE_PREFIX: &[u8] = b"// Managed by clove ";

fn is_managed_file(bytes: &[u8], body: &str) -> bool {
    bytes == body.as_bytes() || bytes.starts_with(MANAGED_FILE_PREFIX)
}

fn is_managed_skill(text: &str) -> bool {
    text == SKILL
}

fn whole_file(path: &Path, kind: Kind) -> Result<WholeFile> {
    let Some(meta) = existing(path, path.symlink_metadata())? else {
        return Ok(WholeFile::Missing);
    };
    // Don't follow or replace a symlink, directory, or other user-owned path.
    if !meta.is_file() {
        return Ok(WholeFile::Foreign);
    }
    let contents = match kind {
        Kind::Skill => read_existing(path)?.map(|s| (s == SKILL, is_managed_skill(&s))),
        Kind::File(body) => {
            existing(path, std::fs::read(path))?.map(|b| (b == body.as_bytes(), is_managed_file(&b, body)))
        }
        _ => unreachable!(),
    };
    Ok(match contents {
        None => WholeFile::Missing,
        Some((true, _)) => WholeFile::Current,
        Some((false, true)) => WholeFile::OwnedOld,
        Some((false, false)) => WholeFile::Foreign,
    })
}

fn block_state(text: &str, kind: Kind) -> State {
    match block_of(text) {
        None => State::Missing,
        Some(b) if b == BLOCK && matches!(kind, Kind::Block) => State::Installed,
        Some(_) => State::Outdated,
    }
}

fn hooks_state(v: &serde_json::Value) -> State {
    match (
        has_hook(v, "WorktreeCreate", HOOK_CREATE),
        has_hook(v, "WorktreeRemove", HOOK_REMOVE),
    ) {
        (true, true) => State::Installed,
        (false, false) => State::Missing,
        _ => State::Outdated,
    }
}

fn state(path: &Path, kind: Kind) -> State {
    if path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
        return State::Outdated;
    }
    match kind {
        Kind::Skill | Kind::File(_) => whole_file(path, kind).map_or(State::Outdated, WholeFile::state),
        Kind::Block | Kind::StaleBlock => block_state(&read(path), kind),
        Kind::Hooks => {
            let v: serde_json::Value = serde_json::from_str(&read(path)).unwrap_or_default();
            hooks_state(&v)
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
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().context("agent file has no parent")?;
    std::fs::create_dir_all(parent)?;
    let (tmp, mut file) = loop {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let tmp = parent.join(format!(".clove-{}-{n}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
        {
            Ok(file) => break (tmp, file),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("create {}", tmp.display())),
        }
    };
    let result = (|| -> std::io::Result<()> {
        file.write_all(body.as_bytes())?;
        match std::fs::metadata(path) {
            Ok(meta) => file.set_permissions(meta.permissions())?,
            Err(e) if e.kind() == ErrorKind::NotFound => {} // Keep the temp file's umask-limited mode.
            Err(e) => return Err(e),
        }
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.with_context(|| format!("write {}", path.display()))
}

/// Writes or takes out every target. Returns what changed.
pub fn apply_in(home: &Path, on: bool) -> Result<Vec<Target>> {
    apply_with(&Env::for_home(home), on)
}

fn apply_with(env: &Env, on: bool) -> Result<Vec<Target>> {
    let unchanged = |on, before| (on && before == State::Installed) || (!on && before == State::Missing);
    let mut changed = vec![];
    let targets = targets(env);
    if on {
        for (_, path, kind) in &targets {
            if matches!(kind, Kind::Skill | Kind::File(_)) && whole_file(path, *kind)? == WholeFile::Foreign {
                anyhow::bail!(
                    "{} already exists and was not written by clove; move or remove it first",
                    path.display()
                );
            }
        }
    }
    for (what, path, kind) in targets {
        // Inactive Pi context files should not retain (or gain) our block.
        let on = on && !matches!(kind, Kind::StaleBlock);
        match kind {
            Kind::Skill | Kind::File(_) => {
                let file = whole_file(&path, kind)?;
                if file == WholeFile::Foreign {
                    if on {
                        anyhow::bail!(
                            "{} already exists and was not written by clove; move or remove it first",
                            path.display()
                        );
                    }
                    continue;
                }
                if unchanged(on, file.state()) {
                    continue;
                }
                let (body, folders) = match kind {
                    Kind::File(body) => (body, 1),
                    _ => (SKILL, 2),
                };
                if on {
                    write(&path, body)?;
                } else {
                    std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
                    for d in path.ancestors().skip(1).take(folders) {
                        let _ = std::fs::remove_dir(d);
                    }
                }
            }
            Kind::Block | Kind::StaleBlock => {
                let text = read_existing(&path)?.unwrap_or_default();
                if unchanged(on, block_state(&text, kind)) {
                    continue;
                }
                let s = with_block(&text, on.then_some(BLOCK));
                if s.is_empty() && !on {
                    std::fs::remove_file(&path)?;
                } else {
                    write(&path, &s)?;
                }
            }
            Kind::Hooks => {
                let text = read_existing(&path)?.unwrap_or_default();
                let v: serde_json::Value = if text.trim().is_empty() {
                    serde_json::json!({})
                } else {
                    serde_json::from_str(&text)
                        .with_context(|| format!("{} is not valid JSON; fix it first", path.display()))?
                };
                if unchanged(on, hooks_state(&v)) {
                    continue;
                }
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
    apply_with(&Env::from_env()?, true)
}

pub fn remove() -> Result<Vec<Target>> {
    apply_with(&Env::from_env()?, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

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
    fn executable_detection_ignores_directories_and_non_executable_files() {
        let h = home_with(&["first/opencode", "second"]);
        let second = h.path().join("second");
        let path = std::env::join_paths([h.path().join("first"), second.clone()]).unwrap();
        let bin = second.join("opencode");
        std::fs::write(&bin, "test").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!on_path(&path, "opencode"));
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(on_path(&path, "opencode"));
    }

    #[test]
    fn opencode_detection_respects_config_overrides_and_path() {
        let h = home_with(&[".config/opencode", "xdg/opencode", "override"]);
        let h = h.path();
        let mut env = Env::for_home(h);
        let plugin = |e: &Env| {
            status_with(e)
                .into_iter()
                .find(|t| t.what == "OpenCode plugin")
                .map(|t| t.path)
        };
        assert_eq!(plugin(&env), Some(h.join(".config/opencode/plugins/clove.js")));
        env.xdg_config_home = Some(h.join("xdg"));
        assert_eq!(plugin(&env), Some(h.join("xdg/opencode/plugins/clove.js")));
        env.opencode_config_dir = Some(h.join("override"));
        assert_eq!(plugin(&env), Some(h.join("override/plugins/clove.js")));
        env.opencode_config_dir = Some(h.join("new"));
        assert_eq!(plugin(&env), None);
        env.has_opencode_bin = true;
        assert_eq!(plugin(&env), Some(h.join("new/plugins/clove.js")));
        assert_eq!(apply_with(&env, true).unwrap().len(), 3);
    }

    #[test]
    fn pi_detects_agent_dir_parent_override_and_path() {
        let h = home_with(&[".pi/agent", "override"]);
        let h = h.path();
        let mut env = Env::for_home(h);
        let extension = |e: &Env| {
            status_with(e)
                .into_iter()
                .find(|t| t.what == "Pi extension")
                .map(|t| t.path)
        };
        assert_eq!(extension(&env), Some(h.join(".pi/agent/extensions/clove.js")));
        std::fs::remove_dir(h.join(".pi/agent")).unwrap();
        assert_eq!(extension(&env), Some(h.join(".pi/agent/extensions/clove.js")));
        std::fs::remove_dir(h.join(".pi")).unwrap();
        env.pi_agent_dir = Some(h.join("override"));
        assert_eq!(extension(&env), Some(h.join("override/extensions/clove.js")));
        env.pi_agent_dir = Some(h.join("new"));
        assert_eq!(extension(&env), None);
        env.has_pi_bin = true;
        assert_eq!(extension(&env), Some(h.join("new/extensions/clove.js")));
        assert_eq!(apply_with(&env, true).unwrap().len(), 3);
    }

    #[test]
    fn opencode_does_not_shadow_claude_instructions() {
        let h = home_with(&[".claude", ".config/opencode"]);
        let h = h.path();
        let instructions = h.join(".config/opencode/AGENTS.md");
        let claude = h.join(".claude/CLAUDE.md");

        assert!(!claude.exists());
        apply_in(h, true).unwrap();
        assert_eq!(block_of(&read(&claude)), Some(BLOCK));
        assert!(!instructions.exists());

        apply_in(h, false).unwrap();
        std::fs::write(&claude, "# Claude\n").unwrap();
        apply_in(h, true).unwrap();
        assert!(!instructions.exists());
        assert!(!status_in(h).iter().any(|t| t.what == "OpenCode instructions"));
        let mut env = Env::for_home(h);
        env.opencode_disable_claude_prompt = true;
        apply_with(&env, true).unwrap();
        assert_eq!(block_of(&read(&instructions)), Some(BLOCK));

        std::fs::write(&instructions, "# OpenCode\n").unwrap();
        apply_in(h, true).unwrap();
        assert_eq!(block_of(&read(&instructions)), Some(BLOCK));
        apply_in(h, false).unwrap();
        assert_eq!(read(&instructions), "# OpenCode\n");

        let h = home_with(&[".config/opencode"]);
        let h = h.path();
        let instructions = h.join(".config/opencode/AGENTS.md");
        assert!(!h.join(".claude").exists());
        apply_in(h, true).unwrap();
        assert_eq!(block_of(&read(&instructions)), Some(BLOCK));
    }

    #[test]
    fn pi_edits_the_first_context_file_without_shadowing_it() {
        let h = home_with(&[".pi/agent"]);
        let h = h.path();
        let pi = h.join(".pi/agent");
        let names = [
            "AGENTS.override.md",
            "AGENTS.md",
            "AGENTS.MD",
            "CLAUDE.md",
            "CLAUDE.MD",
        ];
        for (i, name) in names.iter().enumerate() {
            // Case-insensitive filesystems alias AGENTS.md/AGENTS.MD (and CLAUDE.md/CLAUDE.MD).
            for lower in &names[i + 1..] {
                if !lower.eq_ignore_ascii_case(name) {
                    std::fs::write(pi.join(lower), "# untouched\n").unwrap();
                }
            }
            let selected = pi.join(name);
            std::fs::write(&selected, "# mine\n").unwrap();
            let changed = apply_in(h, true).unwrap();
            let target = changed.iter().find(|t| t.what == "Pi instructions").unwrap();
            assert_eq!(
                target.path.canonicalize().unwrap(),
                selected.canonicalize().unwrap()
            );
            assert_eq!(block_of(&read(&selected)), Some(BLOCK));
            if !name.starts_with("AGENTS") {
                assert!(!pi.join("AGENTS.md").exists());
            }
            for lower in &names[i + 1..] {
                if !lower.eq_ignore_ascii_case(name) {
                    assert_eq!(read(&pi.join(lower)), "# untouched\n");
                }
            }
            apply_in(h, false).unwrap();
            assert_eq!(read(&selected), "# mine\n");
            for entry in std::fs::read_dir(&pi).unwrap() {
                std::fs::remove_file(entry.unwrap().path()).unwrap();
            }
        }
    }

    #[test]
    fn pi_reports_and_removes_blocks_in_inactive_context_files() {
        for install_again in [false, true] {
            let h = home_with(&[".pi/agent"]);
            let h = h.path();
            let primary = h.join(".pi/agent/AGENTS.override.md");
            let secondary = h.join(".pi/agent/AGENTS.md");
            std::fs::write(&secondary, "# Keep this\n").unwrap();
            apply_in(h, true).unwrap();
            std::fs::write(&primary, "# New context\n").unwrap();

            let inactive: Vec<_> = status_in(h)
                .into_iter()
                .filter(|t| t.what == "Pi inactive instructions")
                .collect();
            assert_eq!(inactive.len(), 1, "case aliases must not appear twice");
            assert_eq!(inactive[0].path, secondary);
            assert_eq!(inactive[0].state, State::Outdated);

            apply_in(h, install_again).unwrap();
            assert_eq!(read(&secondary), "# Keep this\n");
            assert_eq!(block_of(&read(&primary)), install_again.then_some(BLOCK));
            assert!(read(&primary).starts_with("# New context\n"));
        }
    }

    #[test]
    fn plugin_and_extension_install_update_and_remove() {
        let h = home_with(&[".config/opencode", ".pi/agent"]);
        let h = h.path();
        assert_eq!(summary(&status_in(h)), State::Missing);
        assert_eq!(apply_in(h, true).unwrap().len(), 6);
        assert_eq!(summary(&status_in(h)), State::Installed);
        assert!(apply_in(h, true).unwrap().is_empty());

        let plugins = [
            (h.join(".config/opencode/plugins/clove.js"), OPENCODE_PLUGIN),
            (h.join(".pi/agent/extensions/clove.js"), PI_EXTENSION),
        ];
        for (p, body) in &plugins {
            assert!(body.as_bytes().starts_with(MANAGED_FILE_PREFIX));
            assert_eq!(read(p), *body);
            std::fs::write(p, [MANAGED_FILE_PREFIX, b"old version\n"].concat()).unwrap();
        }
        let outdated = status_in(h)
            .into_iter()
            .filter(|t| t.state == State::Outdated)
            .count();
        assert_eq!(outdated, 2);
        assert_eq!(apply_in(h, true).unwrap().len(), 2);
        for (p, body) in &plugins {
            assert_eq!(read(p), *body);
        }
        assert_eq!(apply_in(h, false).unwrap().len(), 6);
        assert!(apply_in(h, false).unwrap().is_empty());
        assert_eq!(summary(&status_in(h)), State::Missing);
        for (p, _) in &plugins {
            assert!(!p.parent().unwrap().exists());
        }
        for d in [".config/opencode/skills", ".pi/agent/skills"] {
            assert!(!h.join(d).exists());
        }
    }

    #[test]
    fn installer_never_touches_foreign_plugins_or_skills() {
        for (name, bytes) in [
            (".config/opencode/plugins/clove.js", &b"// my plugin\n"[..]),
            (".pi/agent/extensions/clove.js", b"// my extension\n"),
            (".config/opencode/plugins/clove.js", b""),
            (".pi/agent/extensions/clove.js", b""),
            (
                ".config/opencode/skills/clove/SKILL.md",
                b"---\nname: clove\n---\nMy skill says to run `clove guide`.\n",
            ),
            (".claude/skills/clove/SKILL.md", b""),
        ] {
            let h = home_with(&[]);
            std::fs::create_dir_all(h.path().join(".claude")).unwrap();
            let p = h.path().join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, bytes).unwrap();
            let entry = status_in(h.path()).into_iter().find(|t| t.path == p).unwrap();
            assert_eq!(entry.state, State::Outdated, "{name}");
            let err = apply_in(h.path(), true).unwrap_err().to_string();
            assert!(err.contains(&p.display().to_string()), "{err}");
            assert!(err.contains("was not written by clove"), "{err}");
            assert_eq!(std::fs::read(&p).unwrap(), bytes, "{name}");
            for target in status_in(h.path()) {
                if target.path != p {
                    assert_eq!(target.state, State::Missing, "{name}: {}", target.path.display());
                    assert!(!target.path.exists(), "{name}: {}", target.path.display());
                }
            }
            apply_in(h.path(), false).unwrap();
            assert_eq!(std::fs::read(&p).unwrap(), bytes, "{name}");
        }
    }

    #[test]
    fn agent_install_and_remove_preserve_private_instruction_modes() {
        let h = home_with(&[".claude", ".codex"]);
        let files = [
            h.path().join(".claude/CLAUDE.md"),
            h.path().join(".codex/AGENTS.md"),
        ];
        for path in &files {
            std::fs::write(path, "# Private instructions\n").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        for on in [true, false] {
            apply_in(h.path(), on).unwrap();
            for path in &files {
                assert_eq!(
                    std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
                assert_eq!(block_of(&read(path)), on.then_some(BLOCK));
            }
        }
        let settings = h.path().join(".claude/settings.json");
        assert_eq!(
            std::fs::metadata(settings).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn install_and_remove_refuse_to_mutate_non_utf8_text() {
        for name in [
            ".codex/AGENTS.md",
            ".claude/settings.json",
            ".claude/skills/clove/SKILL.md",
        ] {
            let h = home_with(&[]);
            let p = h.path().join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            let bytes = b"do not replace \xff\n";
            std::fs::write(&p, bytes).unwrap();
            for on in [true, false] {
                let err = apply_in(h.path(), on).unwrap_err();
                assert!(err.to_string().contains(&p.display().to_string()), "{err:#}");
                assert_eq!(std::fs::read(&p).unwrap(), bytes);
            }
        }
    }

    #[test]
    fn an_unreadable_instruction_link_is_not_replaced() {
        let h = home_with(&[".codex"]);
        let p = h.path().join(".codex/AGENTS.md");
        let missing = h.path().join("not-created");
        std::os::unix::fs::symlink(&missing, &p).unwrap();
        assert_eq!(state(&p, Kind::Block), State::Outdated);
        assert!(apply_in(h.path(), true).is_err());
        assert_eq!(std::fs::read_link(&p).unwrap(), missing);
    }

    #[test]
    fn a_valid_instruction_link_is_not_modified() {
        let h = home_with(&[".codex"]);
        let p = h.path().join(".codex/AGENTS.md");
        let destination = h.path().join("instructions");
        std::fs::write(&destination, "user instructions\n").unwrap();
        std::os::unix::fs::symlink(&destination, &p).unwrap();
        assert_eq!(state(&p, Kind::Block), State::Outdated);
        for on in [true, false] {
            assert!(apply_in(h.path(), on).is_err());
            assert_eq!(std::fs::read_link(&p).unwrap(), destination);
            assert_eq!(read(&destination), "user instructions\n");
        }
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
