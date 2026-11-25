//! Dynamic completion candidates, computed from the repo in the current directory.

use clap_complete::engine::CompletionCandidate;
use clove_core::Clove;
use std::ffi::OsStr;

fn repo() -> Option<Clove> {
    Clove::discover(&std::env::current_dir().ok()?).ok()
}

pub fn clones(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let Some(c) = repo() else { return vec![] };
    c.list(false)
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i.name.starts_with(prefix.as_ref()))
        .map(|i| {
            let help = match (&i.branch, i.mounted) {
                (Some(b), true) => b.clone(),
                (Some(b), false) => format!("{b} (unmounted)"),
                (None, _) => "detached".into(),
            };
            CompletionCandidate::new(i.name).help(Some(help.into()))
        })
        .collect()
}

/// Plain git worktrees (not clones), by folder name.
pub fn worktrees(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let Some(c) = repo() else { return vec![] };
    c.other_worktrees(false)
        .unwrap_or_default()
        .into_iter()
        .filter(|w| w.name.starts_with(prefix.as_ref()))
        .map(|w| {
            let help = w.branch.unwrap_or_else(|| "detached".into());
            CompletionCandidate::new(w.name).help(Some(format!("{help} (git worktree)").into()))
        })
        .collect()
}

/// Clones, then plain git worktrees: what `clove rm` takes.
pub fn removable(current: &OsStr) -> Vec<CompletionCandidate> {
    let mut v = clones(current);
    v.extend(worktrees(current));
    v
}

/// Existing branches worth a clone: the same list the TUI and app offer
/// (no base, no agent scaffolding, nothing merged or checked out elsewhere).
pub fn branches(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let Some(c) = repo() else { return vec![] };
    c.branch_candidates()
        .into_iter()
        .filter(|b| b.name.starts_with(prefix.as_ref()))
        .take(200)
        .map(|b| {
            let help = if b.remote { "on origin" } else { "local" };
            CompletionCandidate::new(b.name).help(Some(help.into()))
        })
        .collect()
}

pub fn refs(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let Some(c) = repo() else { return vec![] };
    clove_core::git::out(
        &c.root,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )
    .unwrap_or_default()
    .lines()
    .filter(|r| r.starts_with(prefix.as_ref()))
    .take(300)
    .map(CompletionCandidate::new)
    .collect()
}
