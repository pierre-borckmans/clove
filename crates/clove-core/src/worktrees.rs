//! Plain git worktrees: ones clove did not make (made before clove, by an
//! agent's `git worktree add`, by another tool). clove lists them, removes
//! merged ones with plain `git worktree remove`, and can turn one into a clone.
//! Locked worktrees and commits on no branch always stay; uncommitted changes
//! go only when asked (`--force`), after they're listed.

use crate::events::{Event, Reporter};
use crate::git;
use crate::ops::{NewOptions, RemoveOptions};
use crate::prune::{Pr, Verdict, WorktreeCheck};
use crate::repo::{CloneInfo, Clove, sanitize_name};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// One plain git worktree of the repo.
#[derive(Debug, Clone, Serialize)]
pub struct OtherWorktree {
    /// Folder name, for display and `clove rm <name>`.
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    /// Short commit.
    pub head: Option<String>,
    pub locked: bool,
    pub lock_reason: Option<String>,
    /// git's reason when it would prune the registration (folder gone).
    pub prunable: Option<String>,
    /// The folder does not exist.
    pub missing: bool,
    /// None when not computed or the folder is missing.
    pub dirty: Option<bool>,
    /// `git status --short` lines when dirty, at most [`MAX_CHANGES`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<String>,
    /// How many changed paths there are in all (may exceed `changes`).
    #[serde(default)]
    pub changes_total: usize,
    /// Detached only: commits on no branch, tag or remote, which removing
    /// the worktree would orphan.
    pub detached_commits: Option<u32>,
    /// From the last PR check, if its branch still matches.
    pub pr: Option<Pr>,
    /// Why clove won't remove or adopt it (see [`OtherWorktree::blocker`]).
    /// Only filled with `status`.
    pub blocker: Option<String>,
    /// Only uncommitted changes block it: `--force` may discard them (see
    /// [`OtherWorktree::forceable`]). Only filled with `status`.
    #[serde(default)]
    pub can_force: bool,
    /// Why it can't become a clone (see [`OtherWorktree::adopt_blocker`]).
    /// Only filled with `status`.
    #[serde(default)]
    pub adopt_blocker: Option<String>,
}

/// Changed paths kept per worktree, to show what removing it would discard.
pub const MAX_CHANGES: usize = 12;

impl OtherWorktree {
    /// Why removing it could lose something (no overrides).
    pub fn blocker(&self) -> Option<String> {
        self.removal_blocker(false, false)
    }

    /// Why it can't become a clone. Uncommitted changes don't stop it: they
    /// move to the clone (see [`Clove::adopt`]).
    pub fn adopt_blocker(&self) -> Option<String> {
        if self.missing {
            return Some(format!("{} is gone from disk", self.name));
        }
        if self.locked {
            let why = self
                .lock_reason
                .as_deref()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default();
            return Some(format!("{} is locked{why}", self.name));
        }
        if self.branch.is_none() {
            return Some(format!(
                "{} is detached; a clone takes a branch (`git switch -c <branch>` in it first)",
                self.name
            ));
        }
        if self.dirty.is_none() {
            return Some(format!("couldn't check {} for uncommitted changes", self.name));
        }
        None
    }

    /// Something stands in the way of removing it, but only things the user
    /// may override: a lock (`unlock`) and uncommitted changes (`discard`).
    /// Commits on no branch never are.
    pub fn forceable(&self) -> bool {
        self.removal_blocker(false, false).is_some() && self.removal_blocker(true, true).is_none()
    }

    /// Why removing it could lose something, unless `discard` (uncommitted
    /// changes may go) and `unlock` (its lock may be lifted). A missing folder
    /// is no reason: removing then only drops git's record of it.
    pub fn removal_blocker(&self, discard: bool, unlock: bool) -> Option<String> {
        if self.locked && !unlock {
            let why = self
                .lock_reason
                .as_deref()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default();
            return Some(format!("{} is locked{why}", self.name));
        }
        if !self.missing {
            match self.dirty {
                Some(true) if discard => {}
                Some(true) => return Some(format!("{} has uncommitted changes", self.name)),
                None => return Some(format!("couldn't check {} for uncommitted changes", self.name)),
                Some(false) => {}
            }
        }
        if let Some(n) = self.detached_commits.filter(|n| *n > 0) {
            return Some(format!(
                "{} is detached with {n} commit{} on no branch; `git switch -c <branch>` in it first",
                self.name,
                if n == 1 { "" } else { "s" }
            ));
        }
        None
    }
}

/// One `git worktree list --porcelain` entry.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Entry {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub locked: Option<String>,
    pub prunable: Option<String>,
}

pub(crate) fn parse_porcelain(s: &str) -> Vec<Entry> {
    let mut out = vec![];
    let mut cur: Option<Entry> = None;
    for line in s.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            out.extend(cur.take());
            cur = Some(Entry {
                path: PathBuf::from(p),
                ..Default::default()
            });
            continue;
        }
        let Some(e) = cur.as_mut() else { continue };
        let (k, v) = line.split_once(' ').unwrap_or((line, ""));
        match k {
            "HEAD" => e.head = Some(v.to_string()),
            "branch" => e.branch = Some(v.strip_prefix("refs/heads/").unwrap_or(v).to_string()),
            "bare" => e.bare = true,
            "locked" => e.locked = Some(v.to_string()),
            "prunable" => e.prunable = Some(v.to_string()),
            _ => {}
        }
    }
    out.extend(cur);
    out
}

/// The admin dir a linked worktree's `.git` file points at.
fn admin_dir(worktree: &Path) -> Option<PathBuf> {
    let s = std::fs::read_to_string(worktree.join(".git")).ok()?;
    Some(PathBuf::from(s.trim().strip_prefix("gitdir:")?.trim()))
}

/// Clone options for [`Clove::adopt`].
#[derive(Debug, Clone, Default)]
pub struct AdoptOptions {
    /// Clone name. Default: the branch with `/` turned into `-`.
    pub name: Option<String>,
    pub no_setup: bool,
}

impl Clove {
    /// Worktree paths clove owns: its clones and the golden worktree, found
    /// by their admin dirs under `.git/worktrees`.
    fn owned_worktrees(&self) -> Vec<PathBuf> {
        let golden = self.record().ok().flatten().map(|r| r.golden_admin);
        let mut out = vec![self.golden_worktree()];
        if let Ok(rd) = std::fs::read_dir(self.common_dir.join("worktrees")) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if !n.starts_with("clove-") && golden.as_deref() != Some(n.as_str()) {
                    continue;
                }
                if let Ok(g) = std::fs::read_to_string(e.path().join("gitdir"))
                    && let Some(wt) = Path::new(g.trim()).parent()
                {
                    out.push(wt.to_path_buf());
                }
            }
        }
        for n in self.clone_names().unwrap_or_default() {
            out.push(self.clone_worktree(&n));
        }
        out
    }

    /// Every git worktree that is not the main checkout, a clone, or the
    /// golden worktree. `status` adds dirty and orphan-commit counts (a
    /// `git status` each, run in parallel).
    pub fn other_worktrees(&self, status: bool) -> Result<Vec<OtherWorktree>> {
        let out = git::out(&self.root, &["worktree", "list", "--porcelain"])?;
        let owned = self.owned_worktrees();
        let is_owned =
            |p: &Path| p.starts_with(&self.state) || owned.iter().any(|o| o == p || same_file(o, p));
        let cache = self.prune_cache().map(|c| c.worktrees).unwrap_or_default();
        let mut list: Vec<OtherWorktree> = parse_porcelain(&out)
            .into_iter()
            .skip(1)
            .filter(|e| !e.bare && !is_owned(&e.path))
            .map(|e| {
                let missing = !e.path.exists();
                let pr = cache
                    .iter()
                    .find(|c| c.path == e.path && c.branch == e.branch)
                    .and_then(|c| c.pr.clone());
                OtherWorktree {
                    name: e
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| e.path.display().to_string()),
                    head: e.head.as_ref().map(|h| h.chars().take(12).collect()),
                    branch: e.branch,
                    locked: e.locked.is_some(),
                    lock_reason: e.locked.filter(|r| !r.is_empty()),
                    prunable: e.prunable,
                    missing,
                    dirty: None,
                    changes: vec![],
                    changes_total: 0,
                    can_force: false,
                    adopt_blocker: None,
                    detached_commits: None,
                    pr,
                    blocker: None,
                    path: e.path,
                }
            })
            .collect();
        if status {
            std::thread::scope(|s| {
                for w in list.iter_mut().filter(|w| !w.missing) {
                    let common = &self.common_dir;
                    s.spawn(move || {
                        // Reused until the worktree's index, HEAD or branch
                        // moves, or its tree changes (see `memo`).
                        let mut files = vec![common.join("packed-refs")];
                        if let Some(admin) = admin_dir(&w.path) {
                            files.extend([admin.join("HEAD"), admin.join("index")]);
                        }
                        if let Some(b) = &w.branch {
                            files.push(common.join("refs/heads").join(b));
                        }
                        let key = format!("wt-status:{}", w.path.display());
                        let detached = w.branch.is_none();
                        let (st, detached_commits) = crate::memo::get_tree(&key, &w.path, &files, || {
                            let st = git::out(&w.path, &["status", "--short", "--untracked-files=normal"])
                                .map_err(|e| format!("{e:#}"));
                            let n = if detached {
                                git::out(
                                    &w.path,
                                    &[
                                        "rev-list",
                                        "--count",
                                        "HEAD",
                                        "--not",
                                        "--branches",
                                        "--tags",
                                        "--remotes",
                                    ],
                                )
                                .ok()
                                .and_then(|n| n.parse::<u32>().ok())
                            } else {
                                None
                            };
                            (st, n)
                        });
                        w.dirty = st.as_ref().ok().map(|s| !s.trim().is_empty());
                        if let Ok(s) = st {
                            let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
                            w.changes_total = lines.len();
                            w.changes = lines.iter().take(MAX_CHANGES).map(|l| l.to_string()).collect();
                        }
                        if detached {
                            w.detached_commits = detached_commits;
                        }
                        w.blocker = w.removal_blocker(false, false);
                        w.can_force = w.forceable();
                        w.adopt_blocker = w.adopt_blocker();
                    });
                }
            });
            for w in list.iter_mut().filter(|w| w.missing) {
                w.blocker = w.removal_blocker(false, false);
                w.can_force = w.forceable();
                w.adopt_blocker = w.adopt_blocker();
            }
        }
        Ok(list)
    }

    /// Finds a plain worktree by path (or a path inside it, `.` for here),
    /// folder name, or branch.
    pub fn resolve_worktree(&self, query: &str) -> Result<OtherWorktree> {
        let list = self.other_worktrees(true)?;
        let p = if query == "." {
            std::env::current_dir()?
        } else {
            PathBuf::from(query)
        };
        if let Ok(p) = p.canonicalize()
            && let Some(w) = list
                .iter()
                .find(|w| p == w.path || p.starts_with(&w.path) || same_file(&w.path, &p))
        {
            return Ok(w.clone());
        }
        if let Some(w) = list.iter().find(|w| w.path == Path::new(query)) {
            return Ok(w.clone());
        }
        let named: Vec<&OtherWorktree> = list
            .iter()
            .filter(|w| w.name == query || w.branch.as_deref() == Some(query))
            .collect();
        match named.as_slice() {
            [one] => Ok((*one).clone()),
            [] => bail!("no clone or git worktree matches `{query}` (see `clove ls`)"),
            _ => bail!("several git worktrees match `{query}`; pass its path"),
        }
    }

    /// Removes a plain worktree with `git worktree remove`. Refuses detached
    /// ones with orphan commits; locked ones unless `unlock` (the lock goes
    /// back if removal fails); dirty ones unless `force` (`--force`, which
    /// discards their changes). A missing folder just loses its record. The
    /// branch goes only on `delete_branch`, by the same rule as clones: `-D`
    /// when merged, else `-d`.
    pub fn remove_worktree(&self, path: &Path, opts: &RemoveOptions, r: &dyn Reporter) -> Result<()> {
        let w = self
            .other_worktrees(true)?
            .into_iter()
            .find(|w| w.path == path)
            .with_context(|| format!("{} is not a plain git worktree of {}", path.display(), self.name))?;
        if let Some(why) = w.removal_blocker(opts.force, opts.unlock) {
            let how = match (w.locked && !opts.unlock, w.dirty == Some(true) && !opts.force) {
                _ if !w.forceable() => "",
                (true, true) => "; --unlock --force lifts the lock and discards the changes",
                (true, false) => "; --unlock lifts the lock",
                _ => "; --force discards them",
            };
            bail!("{why}{how}");
        }
        let p = w.path.to_string_lossy().to_string();
        if w.locked {
            git::run_ok(&self.root, &["worktree", "unlock", &p]).context("git worktree unlock")?;
        }
        let mut args = vec!["worktree", "remove"];
        if opts.force && w.dirty == Some(true) {
            args.push("--force");
        }
        args.push(&p);
        if let Err(e) = git::run_ok(&self.root, &args) {
            if w.locked {
                // Put the lock back as it was.
                let mut lock = vec!["worktree", "lock"];
                if let Some(r) = &w.lock_reason {
                    lock.extend(["--reason", r.as_str()]);
                }
                lock.push(&p);
                let _ = git::run_ok(&self.root, &lock);
            }
            return Err(e).with_context(|| format!("git {}", args.join(" ")));
        }
        if opts.delete_branch
            && let Some(b) = &w.branch
        {
            let merged = git::ok(&self.root, &["merge-base", "--is-ancestor", b, &self.config.base]);
            let flag = if merged || opts.branch_merged { "-D" } else { "-d" };
            if let Err(e) = git::run_ok(&self.root, &["branch", flag, b]) {
                r.event(Event::Warn {
                    message: format!("kept branch {b}: {e:#}"),
                });
            }
        }
        Ok(())
    }

    /// Removes the worktrees a check found merged and clean, with their
    /// branches. Returns the paths that went and, for the rest, why not
    /// (also sent as warnings).
    pub fn remove_merged_worktrees(
        &self,
        checks: &[WorktreeCheck],
        r: &dyn Reporter,
    ) -> (Vec<PathBuf>, Vec<String>) {
        let opts = RemoveOptions {
            force: false,
            delete_branch: true,
            branch_merged: true,
            unlock: false,
        };
        let (mut removed, mut kept) = (vec![], vec![]);
        for c in checks.iter().filter(|c| c.verdict == Verdict::Prune) {
            match self.remove_worktree(&c.path, &opts, r) {
                Ok(()) => removed.push(c.path.clone()),
                Err(e) => {
                    let msg = format!("kept {}: {e:#}", c.name);
                    r.event(Event::Warn { message: msg.clone() });
                    kept.push(msg);
                }
            }
        }
        (removed, kept)
    }

    /// Turns a plain worktree into a clone on the same branch.
    ///
    /// Uncommitted changes (untracked files too, not ignored ones) go into a
    /// stash first; stashes are shared by every worktree of a repo. git won't
    /// check a branch out twice, so the old worktree then lets go of it by
    /// detaching. The clone is made on the branch and the stash applied in it.
    /// Only then is the old worktree removed. If the clone fails, the old
    /// worktree goes back on its branch with its changes; if applying fails,
    /// the stash stays and the warning says how to apply it.
    pub fn adopt(&self, path: &Path, opts: &AdoptOptions, r: &dyn Reporter) -> Result<CloneInfo> {
        self.require_record()?;
        self.adopt_with(path, opts, r, |o| self.new_clone(o, r))
    }

    pub(crate) fn adopt_with(
        &self,
        path: &Path,
        opts: &AdoptOptions,
        r: &dyn Reporter,
        create: impl FnOnce(&NewOptions) -> Result<CloneInfo>,
    ) -> Result<CloneInfo> {
        let w = self
            .other_worktrees(true)?
            .into_iter()
            .find(|w| w.path == path)
            .with_context(|| format!("{} is not a plain git worktree of {}", path.display(), self.name))?;
        if let Some(why) = w.adopt_blocker() {
            bail!("{why}");
        }
        let Some(branch) = w.branch.clone() else {
            bail!("{} is detached", w.name);
        };
        let name = sanitize_name(opts.name.as_deref().unwrap_or(&branch));
        if name.is_empty() {
            bail!("cannot derive a clone name; pass a name");
        }
        if self.clone_meta(&name)?.is_some() || self.clone_dir(&name).exists() {
            bail!("clone `{name}` already exists; pick another name");
        }
        // Carry uncommitted changes over in a stash.
        let stash = if w.dirty == Some(true) {
            r.event(Event::Info {
                message: format!(
                    "stashing {} uncommitted change{} to carry them into the clone",
                    w.changes_total,
                    if w.changes_total == 1 { "" } else { "s" }
                ),
            });
            let msg = format!("clove: {}'s changes, moving to a clone", w.name);
            git::run_ok(
                &w.path,
                &["stash", "push", "--include-untracked", "--quiet", "-m", &msg],
            )
            .with_context(|| format!("stash {}'s changes", w.name))?;
            Some(git::out(&w.path, &["rev-parse", "refs/stash"]).context("find the stash")?)
        } else {
            None
        };
        r.event(Event::Info {
            message: format!("detaching {} so the clone can take {branch}", w.name),
        });
        if let Err(e) = git::run_ok(&w.path, &["switch", "--quiet", "--detach"]) {
            if let Some(sha) = &stash {
                let _ = unstash(&w.path, sha);
            }
            return Err(e).with_context(|| format!("detach {}", w.name));
        }
        let new = NewOptions {
            branch: Some(branch.clone()),
            name: Some(name),
            no_setup: opts.no_setup,
            ..Default::default()
        };
        let info = match create(&new) {
            Ok(i) => i,
            Err(e) => {
                let back = git::run_ok(&w.path, &["switch", "--quiet", &branch]);
                if let Some(sha) = &stash
                    && let Err(se) = unstash(&w.path, sha)
                {
                    r.event(Event::Warn {
                        message: format!(
                            "{}'s changes stay in stash {sha} ({se:#}); `git -C {} stash apply {sha}`",
                            w.name,
                            w.path.display()
                        ),
                    });
                }
                return Err(match back {
                    Ok(()) => e.context(format!("no clone made; {} is back on {branch}", w.name)),
                    Err(back) => e.context(format!(
                        "no clone made, and {} could not go back on {branch} ({back:#}); run `git -C {} switch {branch}`",
                        w.name,
                        w.path.display()
                    )),
                });
            }
        };
        if let Some(sha) = &stash {
            match unstash(&info.path, sha) {
                Ok(()) => r.event(Event::Info {
                    message: format!("moved {}'s uncommitted changes into {}", w.name, info.name),
                }),
                Err(e) => r.event(Event::Warn {
                    message: format!(
                        "couldn't apply {}'s changes in {} ({e:#}); they stay in stash {sha}: `git stash apply {sha}` there",
                        w.name, info.name
                    ),
                }),
            }
        }
        if let Err(e) = git::run_ok(&self.root, &["worktree", "remove", &w.path.to_string_lossy()]) {
            r.event(Event::Warn {
                message: format!(
                    "clone {} is ready, but the old worktree stays (detached) at {}: {e:#}",
                    info.name,
                    w.path.display()
                ),
            });
        }
        Ok(info)
    }
}

/// Applies stash `sha` in `dir` (index too) and drops it from the stash list.
fn unstash(dir: &Path, sha: &str) -> Result<()> {
    git::run_ok(dir, &["stash", "apply", "--index", "--quiet", sha])?;
    let list = git::out(dir, &["stash", "list", "--format=%H"]).unwrap_or_default();
    if let Some(i) = list.lines().position(|l| l.trim() == sha) {
        let _ = git::run_ok(dir, &["stash", "drop", "--quiet", &format!("stash@{{{i}}}")]);
    }
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Silent;
    use crate::prune::PrState;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    struct Repo {
        _tmp: tempfile::TempDir,
        dir: PathBuf,
        clove: Clove,
    }

    /// A repo on `main` with one commit, and a clove whose state lives in the
    /// temp dir (nothing touches ~/.clove).
    fn repo() -> Repo {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let dir = base.join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.email", "t@t"]);
        git(&dir, &["config", "user.name", "t"]);
        git(&dir, &["config", "commit.gpgsign", "false"]);
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-qm", "init"]);
        let mut clove = Clove::open(&dir).unwrap();
        clove.state = base.join("state");
        clove.config.base = "main".into();
        Repo {
            _tmp: tmp,
            dir,
            clove,
        }
    }

    impl Repo {
        fn add(&self, name: &str, args: &[&str]) -> PathBuf {
            let p = self.dir.parent().unwrap().join(name);
            let mut a = vec!["worktree", "add", "-q"];
            a.extend(args);
            a.push(p.to_str().unwrap());
            git(&self.dir, &a);
            p
        }
        fn find(&self, name: &str) -> OtherWorktree {
            self.clove
                .other_worktrees(true)
                .unwrap()
                .into_iter()
                .find(|w| w.name == name)
                .unwrap_or_else(|| panic!("no worktree {name}"))
        }
    }

    #[test]
    fn parses_porcelain() {
        let s = "worktree /r\nHEAD aaa\nbranch refs/heads/main\n\n\
                 worktree /w1\nHEAD bbb\ndetached\nlocked\n\n\
                 worktree /w2\nHEAD ccc\nbranch refs/heads/feat/x\nlocked agent busy\nprunable gitdir file points to non-existent location\n\n";
        let e = parse_porcelain(s);
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].branch.as_deref(), Some("main"));
        assert_eq!(e[1].branch, None);
        assert_eq!(e[1].locked.as_deref(), Some(""));
        assert_eq!(e[2].branch.as_deref(), Some("feat/x"));
        assert_eq!(e[2].locked.as_deref(), Some("agent busy"));
        assert!(e[2].prunable.is_some());
    }

    #[test]
    fn lists_plain_worktrees_with_their_state() {
        let r = repo();
        let feat = r.add("feat", &["-b", "feat/x"]);
        r.add("det", &["--detach"]);
        r.add("locked", &["-b", "lk"]);
        git(
            &r.dir,
            &[
                "worktree",
                "lock",
                "--reason",
                "agent busy",
                r.dir.parent().unwrap().join("locked").to_str().unwrap(),
            ],
        );
        let gone = r.add("gone", &["-b", "gone"]);
        std::fs::remove_dir_all(&gone).unwrap();
        // A clove clone's registration (admin dir clove-*) is not listed.
        let fake_clone = r.add("fakeclone", &["-b", "fc"]);
        std::fs::rename(
            r.dir.join(".git/worktrees/fakeclone"),
            r.dir.join(".git/worktrees/clove-fakeclone"),
        )
        .unwrap();
        std::fs::write(
            fake_clone.join(".git"),
            format!(
                "gitdir: {}\n",
                r.dir.join(".git/worktrees/clove-fakeclone").display()
            ),
        )
        .unwrap();
        std::fs::write(feat.join("new.txt"), "x").unwrap();

        let list = r.clove.other_worktrees(true).unwrap();
        let names: Vec<&str> = list.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["det", "feat", "gone", "locked"]);
        let f = r.find("feat");
        assert_eq!(f.branch.as_deref(), Some("feat/x"));
        assert_eq!(f.dirty, Some(true));
        let d = r.find("det");
        assert_eq!(d.branch, None);
        assert_eq!(d.dirty, Some(false));
        assert_eq!(d.detached_commits, Some(0));
        let l = r.find("locked");
        assert!(l.locked);
        assert_eq!(l.lock_reason.as_deref(), Some("agent busy"));
        let g = r.find("gone");
        assert!(g.missing && g.prunable.is_some() && g.dirty.is_none());
        // Without status, nothing runs git status.
        assert!(
            r.clove
                .other_worktrees(false)
                .unwrap()
                .iter()
                .all(|w| w.dirty.is_none())
        );
    }

    #[test]
    fn remove_refuses_what_would_lose_work() {
        let r = repo();
        let opts = RemoveOptions::default();
        let dirty = r.add("dirty", &["-b", "d"]);
        std::fs::write(dirty.join("a.txt"), "changed").unwrap();
        let e = r.clove.remove_worktree(&dirty, &opts, &Silent).unwrap_err();
        assert!(format!("{e:#}").contains("uncommitted"), "{e:#}");

        let locked = r.add("locked", &["-b", "l"]);
        git(&r.dir, &["worktree", "lock", locked.to_str().unwrap()]);
        let e = r.clove.remove_worktree(&locked, &opts, &Silent).unwrap_err();
        assert!(format!("{e:#}").contains("locked"), "{e:#}");

        let det = r.add("det", &["--detach"]);
        git(
            &det,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "orphan",
            ],
        );
        let e = r.clove.remove_worktree(&det, &opts, &Silent).unwrap_err();
        assert!(format!("{e:#}").contains("1 commit on no branch"), "{e:#}");

        // Not a plain worktree: the main checkout.
        assert!(r.clove.remove_worktree(&r.dir, &opts, &Silent).is_err());
        assert!(dirty.exists() && locked.exists() && det.exists());

        // Forced: the dirty one goes (its changes listed first); locked and
        // orphan commits still don't.
        let force = RemoveOptions {
            force: true,
            ..Default::default()
        };
        let w = r.clove.other_worktrees(true).unwrap();
        let d = w.iter().find(|w| w.path == dirty).unwrap();
        assert!(d.forceable());
        assert_eq!(d.changes_total, 1);
        assert!(d.changes[0].ends_with("a.txt"), "{:?}", d.changes);
        let e = r.clove.remove_worktree(&dirty, &opts, &Silent).unwrap_err();
        assert!(format!("{e:#}").contains("--force discards them"), "{e:#}");
        r.clove.remove_worktree(&dirty, &force, &Silent).unwrap();
        assert!(!dirty.exists());
        assert!(r.clove.remove_worktree(&locked, &force, &Silent).is_err());
        assert!(r.clove.remove_worktree(&det, &force, &Silent).is_err());
        assert!(locked.exists() && det.exists());
        let e = r.clove.remove_worktree(&locked, &force, &Silent).unwrap_err();
        assert!(format!("{e:#}").contains("--unlock"), "{e:#}");

        // Unlocked on request; orphan commits still stop it.
        let unlock = RemoveOptions {
            unlock: true,
            ..Default::default()
        };
        r.clove.remove_worktree(&locked, &unlock, &Silent).unwrap();
        assert!(!locked.exists());
        assert!(r.clove.remove_worktree(&det, &unlock, &Silent).is_err());

        // Locked with its folder gone: unlocking drops git's record of it.
        let gone = r.add("gone", &["-b", "g"]);
        git(
            &r.dir,
            &["worktree", "lock", "--reason", "deploy", gone.to_str().unwrap()],
        );
        std::fs::remove_dir_all(&gone).unwrap();
        let w = r.clove.other_worktrees(true).unwrap();
        let g = w.iter().find(|w| w.path == gone).unwrap();
        assert!(g.missing && g.locked && g.forceable(), "{g:?}");
        r.clove.remove_worktree(&gone, &unlock, &Silent).unwrap();
        assert!(
            !r.clove
                .other_worktrees(false)
                .unwrap()
                .iter()
                .any(|w| w.path == gone)
        );
    }

    #[test]
    fn removes_a_clean_worktree_and_its_merged_branch() {
        let r = repo();
        let wt = r.add("done", &["-b", "done"]);
        let opts = RemoveOptions {
            delete_branch: true,
            ..Default::default()
        };
        r.clove.remove_worktree(&wt, &opts, &Silent).unwrap();
        assert!(!wt.exists());
        // `done` points at main's tip, so it is merged and goes too.
        assert!(!git::ref_exists(&r.dir, "refs/heads/done"));
        assert!(r.clove.other_worktrees(false).unwrap().is_empty());
    }

    #[test]
    fn cleanup_takes_only_merged_clean_unlocked_worktrees() {
        let r = repo();
        let mk = |name: &str| {
            let p = r.add(name, &["-b", name]);
            git(
                &p,
                &[
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "user.name=t",
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    name,
                ],
            );
            p
        };
        let merged = mk("merged");
        let dirty = mk("dirtym");
        std::fs::write(dirty.join("x"), "x").unwrap();
        let locked = mk("lockedm");
        git(&r.dir, &["worktree", "lock", locked.to_str().unwrap()]);
        let open = mk("open");
        r.add("det", &["--detach"]);
        let pr = |w: &OtherWorktree, state: PrState| Pr {
            number: 1,
            state,
            head_oid: git(&w.path, &["rev-parse", "HEAD"]),
            title: String::new(),
            url: String::new(),
        };
        let checks: Vec<WorktreeCheck> = r
            .clove
            .other_worktrees(true)
            .unwrap()
            .iter()
            .map(|w| {
                let p = match w.name.as_str() {
                    "det" => None,
                    "open" => Some(pr(w, PrState::Open)),
                    _ => Some(pr(w, PrState::Merged)),
                };
                r.clove.classify_worktree(w, p)
            })
            .collect();
        let v = |n: &str| checks.iter().find(|c| c.name == n).unwrap().verdict;
        assert_eq!(v("merged"), Verdict::Prune);
        assert_eq!(v("dirtym"), Verdict::Review);
        assert_eq!(v("lockedm"), Verdict::Review);
        assert_eq!(v("open"), Verdict::Keep);
        assert_eq!(v("det"), Verdict::Keep);
        let (removed, kept) = r.clove.remove_merged_worktrees(&checks, &Silent);
        assert_eq!(removed, vec![merged.clone()]);
        assert!(kept.is_empty());
        assert!(!merged.exists() && dirty.exists() && locked.exists() && open.exists());
        // Merged by its PR (not by ancestry): -D still deletes the branch.
        assert!(!git::ref_exists(&r.dir, "refs/heads/merged"));
        assert!(git::ref_exists(&r.dir, "refs/heads/open"));
    }

    #[test]
    fn adopt_rolls_back_when_the_clone_fails() {
        let r = repo();
        let wt = r.add("feat", &["-b", "feat/y"]);
        std::fs::write(wt.join("ignored-but-kept"), "x").unwrap();
        std::fs::write(r.dir.join(".git/info/exclude"), "ignored-but-kept\n").unwrap();
        let mut asked = None;
        let e = r
            .clove
            .adopt_with(&wt, &AdoptOptions::default(), &Silent, |o| {
                // git must have let go of the branch by now.
                let held = git(&r.dir, &["worktree", "list", "--porcelain"]);
                assert!(!held.contains("branch refs/heads/feat/y"), "{held}");
                asked = Some(o.clone());
                anyhow::bail!("image copy failed")
            })
            .unwrap_err();
        let msg = format!("{e:#}");
        assert!(
            msg.contains("back on feat/y") && msg.contains("image copy failed"),
            "{msg}"
        );
        let o = asked.unwrap();
        assert_eq!(o.branch.as_deref(), Some("feat/y"));
        assert_eq!(o.name.as_deref(), Some("feat-y"));
        // Nothing lost: same worktree, same branch, its files still there.
        let w = r.find("feat");
        assert_eq!(w.branch.as_deref(), Some("feat/y"));
        assert!(wt.join("ignored-but-kept").exists());
    }

    #[test]
    fn adopt_removes_the_old_worktree_once_the_clone_exists() {
        let r = repo();
        let wt = r.add("feat", &["-b", "feat/z"]);
        let info = r
            .clove
            .adopt_with(&wt, &AdoptOptions::default(), &Silent, |o| {
                // Stands in for new_clone: check the branch out elsewhere.
                let other = r.dir.parent().unwrap().join("clone");
                git(
                    &r.dir,
                    &["worktree", "add", "-q", other.to_str().unwrap(), "feat/z"],
                );
                Ok(CloneInfo {
                    name: o.name.clone().unwrap(),
                    path: other,
                    mounted: true,
                    branch: o.branch.clone(),
                    head: None,
                    dirty: None,
                    ahead: None,
                    behind: None,
                    image_bytes: 0,
                    own_bytes: 0,
                    created_at: 0,
                    golden_generation: 0,
                    timings: vec![],
                    total_ms: 0.0,
                    failed_steps: vec![],
                    issue: None,
                    removing_when_free: false,
                })
            })
            .unwrap();
        assert_eq!(info.name, "feat-z");
        assert!(!wt.exists());
        assert!(git::ref_exists(&r.dir, "refs/heads/feat/z"));
    }

    /// Stands in for new_clone: checks the branch out at `path`.
    fn fake_clone(r: &Repo, o: &NewOptions, path: &Path) -> CloneInfo {
        git(
            &r.dir,
            &[
                "worktree",
                "add",
                "-q",
                path.to_str().unwrap(),
                o.branch.as_deref().unwrap(),
            ],
        );
        CloneInfo {
            name: o.name.clone().unwrap(),
            path: path.to_path_buf(),
            mounted: true,
            branch: o.branch.clone(),
            head: None,
            dirty: None,
            ahead: None,
            behind: None,
            image_bytes: 0,
            own_bytes: 0,
            created_at: 0,
            golden_generation: 0,
            timings: vec![],
            total_ms: 0.0,
            failed_steps: vec![],
            issue: None,
            removing_when_free: false,
        }
    }

    /// A worktree on branch `b` with a committed file, then a modified,
    /// a staged and an untracked one.
    fn dirty_worktree(r: &Repo, name: &str, b: &str) -> PathBuf {
        let wt = r.add(name, &["-b", b]);
        std::fs::write(wt.join("tracked.txt"), "v1").unwrap();
        std::fs::write(wt.join("staged.txt"), "v1").unwrap();
        git(&wt, &["add", "."]);
        git(
            &wt,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "files",
            ],
        );
        std::fs::write(wt.join("tracked.txt"), "v2").unwrap();
        std::fs::write(wt.join("staged.txt"), "v2").unwrap();
        git(&wt, &["add", "staged.txt"]);
        std::fs::write(wt.join("new.txt"), "new").unwrap();
        wt
    }

    #[test]
    fn adopt_carries_uncommitted_changes_into_the_clone() {
        let r = repo();
        let wt = dirty_worktree(&r, "wip", "wip/a");
        let other = r.dir.parent().unwrap().join("clone-wip");
        let info = r
            .clove
            .adopt_with(&wt, &AdoptOptions::default(), &Silent, |o| {
                Ok(fake_clone(&r, o, &other))
            })
            .unwrap();
        assert!(!wt.exists());
        let p = &info.path;
        assert_eq!(std::fs::read_to_string(p.join("tracked.txt")).unwrap(), "v2");
        assert_eq!(std::fs::read_to_string(p.join("new.txt")).unwrap(), "new");
        // Staged stays staged.
        assert_eq!(git(p, &["diff", "--cached", "--name-only"]), "staged.txt");
        assert_eq!(git(&r.dir, &["stash", "list"]), "", "the stash is dropped");
    }

    #[test]
    fn adopt_puts_changes_back_when_the_clone_fails() {
        let r = repo();
        let wt = dirty_worktree(&r, "wip", "wip/b");
        let e = r
            .clove
            .adopt_with(&wt, &AdoptOptions::default(), &Silent, |_| {
                anyhow::bail!("no space")
            })
            .unwrap_err();
        assert!(format!("{e:#}").contains("back on wip/b"), "{e:#}");
        assert_eq!(r.find("wip").branch.as_deref(), Some("wip/b"));
        assert_eq!(std::fs::read_to_string(wt.join("tracked.txt")).unwrap(), "v2");
        assert!(wt.join("new.txt").exists());
        assert_eq!(git(&wt, &["diff", "--cached", "--name-only"]), "staged.txt");
        assert_eq!(git(&r.dir, &["stash", "list"]), "");
    }

    #[test]
    fn adopt_refuses_detached_and_locked() {
        let r = repo();
        let never = |_: &NewOptions| -> Result<CloneInfo> { panic!("must not create") };
        let det = r.add("det", &["--detach"]);
        assert!(
            r.clove
                .adopt_with(&det, &AdoptOptions::default(), &Silent, never)
                .is_err()
        );
        let locked = r.add("locked", &["-b", "l"]);
        git(&r.dir, &["worktree", "lock", locked.to_str().unwrap()]);
        assert!(
            r.clove
                .adopt_with(&locked, &AdoptOptions::default(), &Silent, never)
                .is_err()
        );
    }
}
