//! Plain git worktrees: ones clove did not make (made before clove, by an
//! agent's `git worktree add`, by another tool). clove lists them, removes
//! merged ones with plain `git worktree remove`, and can turn one into a clone.
//! It never forces anything on them: locked, dirty or detached work stays.

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
    /// Detached only: commits on no branch, tag or remote, which removing
    /// the worktree would orphan.
    pub detached_commits: Option<u32>,
    /// From the last PR check, if its branch still matches.
    pub pr: Option<Pr>,
    /// Why clove won't remove or adopt it (see [`OtherWorktree::blocker`]).
    /// Only filled with `status`.
    pub blocker: Option<String>,
}

impl OtherWorktree {
    /// Why removing (or adopting) it could lose something, if it could.
    pub fn blocker(&self) -> Option<String> {
        if self.locked {
            let why = self
                .lock_reason
                .as_deref()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default();
            return Some(format!(
                "{} is locked{why}; `git worktree unlock` it first if it's really done",
                self.name
            ));
        }
        if self.missing {
            return Some(format!(
                "{} is gone from disk; `git worktree prune` drops its registration",
                self.name
            ));
        }
        match self.dirty {
            Some(true) => return Some(format!("{} has uncommitted changes", self.name)),
            None => return Some(format!("couldn't check {} for uncommitted changes", self.name)),
            Some(false) => {}
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
                    s.spawn(move || {
                        w.dirty = git::is_dirty(&w.path).ok();
                        if w.branch.is_none() {
                            w.detached_commits = git::out(
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
                            .and_then(|n| n.parse().ok());
                        }
                        w.blocker = w.blocker();
                    });
                }
            });
            for w in list.iter_mut().filter(|w| w.missing) {
                w.blocker = w.blocker();
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

    /// Removes a plain worktree with `git worktree remove` (never `--force`).
    /// Refuses locked, dirty, missing, and detached-with-orphan-commits
    /// worktrees. The branch goes only on `delete_branch`, by the same rule
    /// as clones: `-D` when merged, else `-d`.
    pub fn remove_worktree(&self, path: &Path, opts: &RemoveOptions, r: &dyn Reporter) -> Result<()> {
        if opts.force {
            bail!(
                "clove never force-removes a plain git worktree; use `git worktree remove --force` yourself"
            );
        }
        let w = self
            .other_worktrees(true)?
            .into_iter()
            .find(|w| w.path == path)
            .with_context(|| format!("{} is not a plain git worktree of {}", path.display(), self.name))?;
        if let Some(why) = w.blocker() {
            bail!("{why}");
        }
        git::run_ok(&self.root, &["worktree", "remove", &w.path.to_string_lossy()])
            .with_context(|| format!("git worktree remove {}", w.path.display()))?;
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
    /// git won't check a branch out twice, so the old worktree lets go of it
    /// first, by detaching (its files stay). Then the clone is made on the
    /// branch. Only once the clone exists is the old worktree removed; if the
    /// clone fails, the old worktree is switched back to its branch.
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
        if let Some(why) = w.blocker() {
            bail!("{why}");
        }
        let Some(branch) = w.branch.clone() else {
            bail!(
                "{} is detached; a clone takes a branch (`git switch -c <branch>` in it first)",
                w.name
            );
        };
        let name = sanitize_name(opts.name.as_deref().unwrap_or(&branch));
        if name.is_empty() {
            bail!("cannot derive a clone name; pass a name");
        }
        if self.clone_meta(&name)?.is_some() || self.clone_dir(&name).exists() {
            bail!("clone `{name}` already exists; pick another name");
        }
        r.event(Event::Info {
            message: format!("detaching {} so the clone can take {branch}", w.name),
        });
        git::run_ok(&w.path, &["switch", "--quiet", "--detach"])
            .with_context(|| format!("detach {}", w.name))?;
        let new = NewOptions {
            branch: Some(branch.clone()),
            name: Some(name),
            no_setup: opts.no_setup,
            ..Default::default()
        };
        let info = match create(&new) {
            Ok(i) => i,
            Err(e) => {
                return Err(match git::run_ok(&w.path, &["switch", "--quiet", &branch]) {
                    Ok(()) => e.context(format!("no clone made; {} is back on {branch}", w.name)),
                    Err(back) => e.context(format!(
                        "no clone made, and {} could not go back on {branch} ({back:#}); run `git -C {} switch {branch}`",
                        w.name,
                        w.path.display()
                    )),
                });
            }
        };
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
        let force = RemoveOptions {
            force: true,
            ..Default::default()
        };
        let clean = r.add("clean", &["-b", "c"]);
        assert!(r.clove.remove_worktree(&clean, &force, &Silent).is_err());
        assert!(dirty.exists() && locked.exists() && det.exists() && clean.exists());
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

    #[test]
    fn adopt_refuses_dirty_detached_and_locked() {
        let r = repo();
        let never = |_: &NewOptions| -> Result<CloneInfo> { panic!("must not create") };
        let dirty = r.add("dirty", &["-b", "d"]);
        std::fs::write(dirty.join("x"), "x").unwrap();
        assert!(
            r.clove
                .adopt_with(&dirty, &AdoptOptions::default(), &Silent, never)
                .is_err()
        );
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
        assert_eq!(r.find("dirty").branch.as_deref(), Some("d"));
    }
}
