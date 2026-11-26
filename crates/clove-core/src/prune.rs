//! Which clones are done: their pull request is merged and nothing on them is
//! missing from the base. Squash merges leave no ancestry, so the proof is the
//! PR's head commit from GitHub (via `gh`), not `git branch --merged`.

use crate::git;
use crate::history::By;
use crate::repo::{CloneInfo, Clove, read_json, write_json};
use crate::util::now_unix;
use crate::worktrees::OtherWorktree;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pr {
    pub number: u32,
    pub state: PrState,
    pub head_oid: String,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Merged, nothing left that the base lacks, no local changes.
    Prune,
    /// Merged, but something would be lost: commits after the merge, or edits.
    Review,
    Keep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeCheck {
    pub clone: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub pr: Option<Pr>,
    pub verdict: Verdict,
    pub reason: String,
    /// What removing it frees.
    pub own_bytes: u64,
}

/// A plain git worktree (not a clone) judged the same way: `Prune` means
/// `git worktree remove` loses nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeCheck {
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub pr: Option<Pr>,
    pub verdict: Verdict,
    pub reason: String,
}

/// Last check, so `clove ls` can show PRs without asking GitHub.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PruneCache {
    pub checked_at: u64,
    pub checks: Vec<MergeCheck>,
    #[serde(default)]
    pub worktrees: Vec<WorktreeCheck>,
}

/// Clones and plain worktrees from one PR check.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Checks {
    pub clones: Vec<MergeCheck>,
    pub worktrees: Vec<WorktreeCheck>,
    /// GitHub could not be asked; only local proofs counted.
    pub warning: Option<String>,
}

/// What cleanup found and, when applied, did: leftovers from interrupted
/// runs, merged clones and merged plain worktrees.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Cleanup {
    pub leftovers: Vec<String>,
    pub checks: Vec<MergeCheck>,
    pub worktrees: Vec<WorktreeCheck>,
    /// GitHub could not be asked; only local proofs counted.
    pub warning: Option<String>,
    /// Clones removed.
    pub removed: Vec<String>,
    /// Plain worktrees removed.
    pub removed_worktrees: Vec<PathBuf>,
    /// Merged ones that could not be removed, with why.
    pub kept: Vec<String>,
}

impl Clove {
    /// Finds leftovers and merged clones and worktrees. `apply` removes the
    /// leftovers and every `Prune` one, with its branch; a failure becomes a
    /// warning and a `kept` line. Shared by the TUI and the app.
    pub fn cleanup(&self, apply: bool, r: &dyn crate::events::Reporter) -> Result<Cleanup> {
        let leftovers = self.gc(apply)?;
        let checks = self.all_merge_checks(true)?;
        let mut out = Cleanup {
            leftovers,
            checks: checks.clones,
            worktrees: checks.worktrees,
            warning: checks.warning,
            ..Default::default()
        };
        if !apply {
            return Ok(out);
        }
        for m in out.checks.iter().filter(|m| m.verdict == Verdict::Prune) {
            let opts = prune_options(By::Prune, &m.reason);
            match self.remove(&m.clone, &opts, r) {
                Ok(()) => out.removed.push(m.clone.clone()),
                Err(e) => {
                    let msg = format!("kept {}: {e:#}", m.clone);
                    r.event(crate::events::Event::Warn { message: msg.clone() });
                    out.kept.push(msg);
                }
            }
        }
        let (removed, kept) = self.remove_merged_worktrees(&out.worktrees, r);
        out.removed_worktrees = removed;
        out.kept.extend(kept);
        Ok(out)
    }

    /// With `prune.auto` on: removes the clones a fresh check proves merged
    /// and clean, unless something has them open or git saw activity in them
    /// within [`AUTO_IDLE_SECS`]. Never mounts a clone to look inside, never
    /// touches plain worktrees, and does nothing when GitHub can't be asked.
    /// Each removal lands in the history.
    pub fn auto_prune(&self, r: &dyn crate::events::Reporter) -> Result<Vec<String>> {
        if !self.config.prune.auto {
            return Ok(vec![]);
        }
        let (checks, warning) = self.merge_checks(false)?;
        if warning.is_some() {
            return Ok(vec![]);
        }
        Ok(self.auto_prune_checks(&checks, r))
    }

    /// The removal half of [`Clove::auto_prune`], on checks already made.
    pub fn auto_prune_checks(&self, checks: &[MergeCheck], r: &dyn crate::events::Reporter) -> Vec<String> {
        let now = crate::util::now_unix();
        let mut removed = vec![];
        for m in checks.iter().filter(|m| m.verdict == Verdict::Prune) {
            if !matches!(self.clone_meta(&m.clone), Ok(Some(_))) {
                continue;
            }
            if now.saturating_sub(self.last_activity(&m.clone)) < AUTO_IDLE_SECS {
                continue;
            }
            // Something holding it (often a TUI's or the app's watcher, which
            // lets go once it is marked) defers it to the remove-pending waiter.
            let opts = prune_options(By::AutoPrune, &m.reason);
            match self.remove_or_defer(&m.clone, &opts, r) {
                Ok(crate::ops::Removal::Removed) => removed.push(m.clone.clone()),
                Ok(crate::ops::Removal::WhenFree(_)) => {}
                Err(e) => r.event(crate::events::Event::Warn {
                    message: format!("auto-prune kept {}: {e:#}", m.clone),
                }),
            }
        }
        removed
    }

    /// Newest of: clone creation, and git's own files for it (index, HEAD,
    /// reflog), which change on every commit, checkout or staged edit.
    fn last_activity(&self, name: &str) -> u64 {
        let admin = self.clone_admin(name);
        let mtime = |p: std::path::PathBuf| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs())
        };
        [
            mtime(self.clone_dir(name)),
            mtime(admin.join("index")),
            mtime(admin.join("HEAD")),
            mtime(admin.join("logs").join("HEAD")),
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
    }

    fn prune_cache_path(&self) -> std::path::PathBuf {
        self.state.join("prune.json")
    }

    pub fn prune_cache(&self) -> Option<PruneCache> {
        read_json(&self.prune_cache_path()).ok().flatten()
    }

    /// Classifies every clone. With `mount`, unmounted clones are mounted for a
    /// moment to check for local edits; without, they land in review.
    /// Returns the checks and a warning when GitHub could not be asked (then
    /// only local proofs count, and nothing is called merged without one).
    pub fn merge_checks(&self, mount: bool) -> Result<(Vec<MergeCheck>, Option<String>)> {
        let c = self.all_merge_checks(mount)?;
        Ok((c.clones, c.warning))
    }

    /// [`Clove::merge_checks`] plus the repo's plain git worktrees, in the
    /// same GitHub request. Both land in the prune cache.
    pub fn all_merge_checks(&self, mount: bool) -> Result<Checks> {
        self.merge_checks_with(mount, |asks| match github_repo(&self.root, &self.config.base) {
            Some((owner, repo)) => match lookup_prs(&self.root, &owner, &repo, asks) {
                Ok(p) => (p, None),
                Err(e) => (
                    BTreeMap::new(),
                    Some(format!("GitHub lookup failed, so only local proofs count: {e:#}")),
                ),
            },
            None => (
                BTreeMap::new(),
                Some("no GitHub remote (or no `gh`): squash-merged PRs can't be detected".into()),
            ),
        })
    }

    /// [`Clove::all_merge_checks`] with the PR lookup given: (clone name or
    /// worktree key, branch) pairs in, PRs by key and a warning out.
    pub fn merge_checks_with(
        &self,
        mount: bool,
        lookup: impl FnOnce(&[(String, String)]) -> (BTreeMap<String, Pr>, Option<String>),
    ) -> Result<Checks> {
        let mut infos = vec![];
        for name in self.clone_names()? {
            // Without `mount`, status waits until a PR says it matters.
            if !mount {
                infos.push(self.info(&name, false)?);
                continue;
            }
            let was_mounted = self.info(&name, false).is_ok_and(|i| i.mounted);
            let _ = self.mount(&name);
            infos.push(self.info(&name, true)?);
            if !was_mounted {
                let _ = self.unmount(&name, false);
            }
        }
        let mut others = self.other_worktrees(false).unwrap_or_default();
        let base_short = self
            .config
            .base
            .split_once('/')
            .map_or(self.config.base.as_str(), |(_, b)| b);
        let mut asks: Vec<(String, String)> = infos
            .iter()
            .filter_map(|i| Some((i.name.clone(), i.branch.clone()?)))
            .collect();
        // A worktree on the base branch has no PR of its own.
        asks.extend(others.iter().filter_map(|w| {
            let b = w.branch.clone().filter(|b| b != base_short)?;
            Some((wt_key(&w.path), b))
        }));
        let (prs, warning) = lookup(&asks);
        // Uncommitted changes only matter where a PR (or a detached head
        // already in the base) would let it go: ask git there only.
        let candidate = |key: &str, branch: &Option<String>| match prs.get(key) {
            Some(p) => p.state == PrState::Merged,
            None => branch.is_none(),
        };
        for i in infos.iter_mut() {
            if i.mounted
                && i.dirty.is_none()
                && candidate(&i.name, &i.branch)
                && let Ok(full) = self.info(&i.name, true)
            {
                *i = full;
            }
        }
        for w in others.iter_mut() {
            if candidate(&wt_key(&w.path), &w.branch) {
                self.fill_status(w);
            }
        }
        let checks: Vec<MergeCheck> = infos
            .iter()
            .map(|i| self.classify(i, prs.get(&i.name).cloned()))
            .collect();
        let worktrees: Vec<WorktreeCheck> = others
            .iter()
            .map(|w| self.classify_worktree(w, prs.get(&wt_key(&w.path)).cloned()))
            .collect();
        let _ = write_json(
            &self.prune_cache_path(),
            &PruneCache {
                checked_at: now_unix(),
                checks: checks.clone(),
                worktrees: worktrees.clone(),
            },
        );
        Ok(Checks {
            clones: checks,
            worktrees,
            warning,
        })
    }

    /// Plain worktrees only go on a merged PR's proof: no PR (a detached
    /// checkout, another tool's worktree) always stays.
    pub(crate) fn classify_worktree(&self, w: &OtherWorktree, pr: Option<Pr>) -> WorktreeCheck {
        let mk = |verdict, reason: String, pr: Option<Pr>| WorktreeCheck {
            name: w.name.clone(),
            path: w.path.clone(),
            branch: w.branch.clone(),
            head: w.head.clone(),
            pr,
            verdict,
            reason,
        };
        if w.missing {
            return mk(Verdict::Keep, "folder is gone".into(), pr);
        }
        let (Some(_), Some(head)) = (&w.branch, w.head.clone()) else {
            return mk(Verdict::Keep, "detached, no PR".into(), pr);
        };
        let proven = match &pr {
            Some(p) if p.state == PrState::Merged => {
                match merged_proof(&self.root, &self.config.base, &head, p) {
                    Ok(r) => r,
                    Err(r) => return mk(Verdict::Review, r, pr),
                }
            }
            Some(p) => return mk(Verdict::Keep, pr_word(p), pr),
            None => return mk(Verdict::Keep, "no PR".into(), pr),
        };
        if w.locked {
            let why = w
                .lock_reason
                .as_deref()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default();
            return mk(Verdict::Review, format!("{proven}, but locked{why}"), pr);
        }
        match w.dirty {
            Some(true) => mk(
                Verdict::Review,
                format!("{proven}, but has uncommitted changes"),
                pr,
            ),
            None => mk(
                Verdict::Review,
                format!("{proven}, but couldn't check for changes"),
                pr,
            ),
            Some(false) => mk(Verdict::Prune, proven, pr),
        }
    }

    fn classify(&self, i: &CloneInfo, pr: Option<Pr>) -> MergeCheck {
        let base = &self.config.base;
        let root = &self.root;
        let mk = |verdict, reason: String, pr: Option<Pr>| MergeCheck {
            clone: i.name.clone(),
            branch: i.branch.clone(),
            head: i.head.clone(),
            pr,
            verdict,
            reason,
            own_bytes: i.own_bytes,
        };
        // Mid-rebase or merge, HEAD is a temporary commit: nothing to judge.
        if let Some(op) = i.operation {
            return mk(Verdict::Keep, format!("{} stopped on conflicts", op.word()), pr);
        }
        let Some(head) = i.head.clone() else {
            return mk(Verdict::Keep, "no commit".into(), pr);
        };
        let in_base = git::ok(root, &["merge-base", "--is-ancestor", &head, base]);
        let proven = match &pr {
            Some(p) if p.state == PrState::Merged => match merged_proof(root, base, &head, p) {
                Ok(r) => Some(r),
                Err(r) => return mk(Verdict::Review, r, pr),
            },
            Some(p) => return mk(Verdict::Keep, pr_word(p), pr),
            // No PR: a branch may be fresh work or a pulled base, which look the
            // same, so it stays. A detached checkout of base history holds nothing.
            None if in_base && i.branch.is_none() => Some(format!("detached, already in {base}")),
            None => return mk(Verdict::Keep, "no PR".into(), pr),
        };
        let reason = proven.unwrap_or_default();
        match i.dirty {
            Some(true) => mk(
                Verdict::Review,
                format!("{reason}, but has uncommitted changes"),
                pr,
            ),
            None => mk(
                Verdict::Review,
                format!("{reason}, but couldn't mount it to check for changes"),
                pr,
            ),
            Some(false) => mk(Verdict::Prune, reason, pr),
        }
    }
}

/// Auto-prune leaves a clone alone until git has been quiet in it this long.
pub const AUTO_IDLE_SECS: u64 = 3600;

/// Options for removing what a check proved merged.
pub fn prune_options(by: By, reason: &str) -> crate::ops::RemoveOptions {
    crate::ops::RemoveOptions {
        force: false,
        delete_branch: true,
        branch_merged: true,
        unlock: false,
        by,
        reason: Some(reason.to_string()),
    }
}

/// Key for a plain worktree among the clone names in a PR lookup.
fn wt_key(path: &Path) -> String {
    format!("worktree:{}", path.display())
}

fn pr_word(p: &Pr) -> String {
    let s = if p.state == PrState::Open {
        "open"
    } else {
        "closed, not merged"
    };
    format!("PR #{} {s}", p.number)
}

/// Whether merged PR `p` covers `head`: Ok with the proof, or Err saying
/// what came after it.
fn merged_proof(root: &Path, base: &str, head: &str, p: &Pr) -> Result<String, String> {
    ensure_object(root, &p.head_oid, p.number);
    let in_base = git::ok(root, &["merge-base", "--is-ancestor", head, base]);
    if p.head_oid.starts_with(head) || head.starts_with(&p.head_oid) {
        Ok(format!("PR #{} merged", p.number))
    } else if git::ok(root, &["merge-base", "--is-ancestor", head, &p.head_oid]) {
        Ok(format!("PR #{} merged (contains this tip)", p.number))
    } else if in_base || merge_is_noop(root, base, head) {
        Ok(format!("PR #{} merged, nothing left that {base} lacks", p.number))
    } else {
        let extra = git::count_commits(root, &format!("{}..{head}", p.head_oid)).unwrap_or(0);
        Err(format!(
            "PR #{} merged, but {extra} commit{} after it",
            p.number,
            if extra == 1 { "" } else { "s" }
        ))
    }
}

/// A merge of `head` into `base` would change nothing.
fn merge_is_noop(root: &Path, base: &str, head: &str) -> bool {
    let tree = git::out(root, &["merge-tree", "--write-tree", base, head]);
    let base_tree = git::out(root, &["rev-parse", &format!("{base}^{{tree}}")]);
    matches!((tree, base_tree), (Ok(t), Ok(b)) if t.lines().next() == Some(b.trim()))
}

/// The PR head may never have been fetched (someone else pushed it, or the
/// branch was deleted). GitHub keeps it as refs/pull/N/head.
fn ensure_object(root: &Path, oid: &str, number: u32) {
    if git::ok(root, &["cat-file", "-e", &format!("{oid}^{{commit}}")]) {
        return;
    }
    let _ = git::run_ok(
        root,
        &["fetch", "--quiet", "origin", &format!("refs/pull/{number}/head")],
    );
}

/// (owner, repo) of the base's remote when it is on GitHub and `gh` exists.
fn github_repo(root: &Path, base: &str) -> Option<(String, String)> {
    let remote = git::split_remote_ref(root, base)
        .map(|(r, _)| r)
        .unwrap_or_else(|| "origin".into());
    let url = git::out(root, &["remote", "get-url", &remote]).ok()?;
    let path = url
        .trim()
        .strip_prefix("git@github.com:")
        .or_else(|| url.trim().strip_prefix("https://github.com/"))
        .or_else(|| url.trim().strip_prefix("ssh://git@github.com/"))?;
    let (owner, repo) = path
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .split_once('/')?;
    which("gh").then(|| (owner.to_string(), repo.to_string()))
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

/// One GraphQL request for every (id, branch): PRs whose head is the branch
/// (newest first, same repo only), or the PR itself for `pr/N` branches.
fn lookup_prs(
    root: &Path,
    owner: &str,
    repo: &str,
    asks: &[(String, String)],
) -> Result<BTreeMap<String, Pr>> {
    const FIELDS: &str = "number state headRefOid title url isCrossRepository";
    let mut parts = vec![];
    let mut keys = BTreeMap::new();
    for (n, (id, b)) in asks.iter().enumerate() {
        let key = format!("c{n}");
        let pr_number = b.strip_prefix("pr/").and_then(|s| s.parse::<u32>().ok());
        parts.push(match pr_number {
            Some(num) => format!("{key}: pullRequest(number: {num}) {{ {FIELDS} }}"),
            None => format!(
                "{key}: pullRequests(headRefName: {}, first: 5, orderBy: {{field: CREATED_AT, direction: DESC}}) {{ nodes {{ {FIELDS} }} }}",
                serde_json::to_string(b)?
            ),
        });
        keys.insert(key, id.clone());
    }
    if parts.is_empty() {
        return Ok(BTreeMap::new());
    }
    let query = format!(
        "query {{ repository(owner: {}, name: {}) {{ {} }} }}",
        serde_json::to_string(owner)?,
        serde_json::to_string(repo)?,
        parts.join(" ")
    );
    let mut last = None;
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(500 << attempt));
        }
        let out = Command::new("gh")
            .current_dir(root)
            .args(["api", "graphql", "-f", &format!("query={query}")])
            .output()
            .context("running gh")?;
        if out.status.success() {
            let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
            return Ok(parse_prs(&v, &keys));
        }
        last = Some(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    bail!("{}", last.unwrap_or_default())
}

fn parse_prs(v: &serde_json::Value, keys: &BTreeMap<String, String>) -> BTreeMap<String, Pr> {
    let mut out = BTreeMap::new();
    let repo = &v["data"]["repository"];
    for (key, clone) in keys {
        let node = &repo[key];
        // pullRequest(number) is one object, asked for explicitly (forks count);
        // pullRequests by branch name is a newest-first list where a fork's
        // branch of the same name is someone else's work.
        let candidates: Vec<&serde_json::Value> = match node["nodes"].as_array() {
            Some(list) => list.iter().filter(|n| n["isCrossRepository"] != true).collect(),
            None if node.is_object() => vec![node],
            None => vec![],
        };
        let pr = candidates.into_iter().find_map(|n| {
            Some(Pr {
                number: n["number"].as_u64()? as u32,
                state: match n["state"].as_str()? {
                    "MERGED" => PrState::Merged,
                    "OPEN" => PrState::Open,
                    _ => PrState::Closed,
                },
                head_oid: n["headRefOid"].as_str()?.to_string(),
                title: n["title"].as_str().unwrap_or_default().to_string(),
                url: n["url"].as_str().unwrap_or_default().to_string(),
            })
        });
        if let Some(pr) = pr {
            out.insert(clone.clone(), pr);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_query_shapes_and_skips_forks() {
        let v = serde_json::json!({"data": {"repository": {
            "c0": {"nodes": [
                {"number": 9, "state": "OPEN", "headRefOid": "f", "title": "fork", "url": "", "isCrossRepository": true},
                {"number": 7, "state": "MERGED", "headRefOid": "abc", "title": "t", "url": "u", "isCrossRepository": false}
            ]},
            "c1": {"number": 12, "state": "CLOSED", "headRefOid": "def", "title": "x", "url": "", "isCrossRepository": false},
            "c2": {"nodes": []}
        }}});
        let keys: BTreeMap<_, _> = [("c0", "a"), ("c1", "b"), ("c2", "c")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let prs = parse_prs(&v, &keys);
        assert_eq!(prs["a"].number, 7);
        assert_eq!(prs["a"].state, PrState::Merged);
        assert_eq!(prs["b"].state, PrState::Closed);
        assert!(!prs.contains_key("c"));
    }
}
