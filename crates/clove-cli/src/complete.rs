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

pub fn branches(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let Some(c) = repo() else { return vec![] };
    let out = clove_core::git::out(
        &c.root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)",
            "refs/heads",
            "refs/remotes/origin",
        ],
    )
    .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    out.lines()
        .map(|l| l.strip_prefix("origin/").unwrap_or(l))
        .filter(|b| *b != "HEAD" && b.starts_with(prefix.as_ref()) && seen.insert(b.to_string()))
        .take(200)
        .map(CompletionCandidate::new)
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
