//! `clove update`: bring a clone's branch up to the latest base. Fetches the
//! base, rebases (or merges) onto it, then reruns the setup steps whose
//! inputs changed, so new dependencies and generated code come along. On a
//! conflict it stops with git's usual state and lists the files; `--continue`
//! and `--abort` finish or undo it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::events::{Event, Reporter};
use crate::git;
use crate::repo::Clove;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    #[default]
    Rebase,
    Merge,
}

impl Strategy {
    pub fn word(self) -> &'static str {
        match self {
            Strategy::Rebase => "rebase",
            Strategy::Merge => "merge",
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdateOptions {
    /// None: the repo's `update.strategy`.
    pub strategy: Option<Strategy>,
    /// Set uncommitted changes aside and reapply them after.
    pub autostash: bool,
    /// Fetch the base first (off when the caller already did, e.g. `--all`).
    pub fetch: bool,
}

impl Default for UpdateOptions {
    fn default() -> Self {
        Self {
            strategy: None,
            autostash: false,
            fetch: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Update {
    UpToDate {
        base: String,
    },
    Updated {
        base: String,
        strategy: Strategy,
        /// Base commits brought in; None after `--continue`.
        commits: Option<u32>,
        /// What to run next, e.g. a force push after rebasing a pushed branch.
        hint: Option<String>,
        failed_steps: Vec<String>,
    },
    Conflict {
        base: String,
        strategy: Strategy,
        files: Vec<String>,
    },
}

/// A rebase or merge stopped half way in this git dir.
pub fn operation(git_dir: &Path) -> Option<Strategy> {
    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        Some(Strategy::Rebase)
    } else if git_dir.join("MERGE_HEAD").exists() {
        Some(Strategy::Merge)
    } else {
        None
    }
}

/// Files git left unmerged.
fn conflicted(wt: &Path) -> Vec<String> {
    git::out(wt, &["diff", "--name-only", "--diff-filter=U"])
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Runs git without ever opening an editor (rebase and merge messages stay as they are).
fn run(wt: &Path, args: &[&str]) -> Result<std::process::Output> {
    git::cmd(wt)
        .args(["-c", "core.editor=true"])
        .args(args)
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .output()
        .with_context(|| format!("git {}", args.join(" ")))
}

fn stderr(o: &std::process::Output) -> String {
    let e = String::from_utf8_lossy(&o.stderr);
    let out = String::from_utf8_lossy(&o.stdout);
    let s = if e.trim().is_empty() { out } else { e };
    s.trim().lines().take(6).collect::<Vec<_>>().join("\n")
}

impl Clove {
    fn update_target(&self, name: &str) -> Result<(PathBuf, PathBuf)> {
        self.clone_meta(name)?.ok_or_else(|| self.no_clone(name))?;
        let wt = self.mount(name)?;
        Ok((wt, self.clone_admin(name)))
    }

    /// Brings the clone's branch up to the latest base. Refuses uncommitted
    /// changes unless `autostash`, a detached HEAD, and a clone already mid
    /// rebase or merge.
    pub fn update(&self, name: &str, opts: &UpdateOptions, r: &dyn Reporter) -> Result<Update> {
        let (wt, admin) = self.update_target(name)?;
        if let Some(op) = operation(&admin) {
            bail!(
                "`{name}` is in the middle of a {}: fix the conflicts, `git add` them and `clove update --continue {name}`, or `clove update --abort {name}`",
                op.word()
            );
        }
        // A detached HEAD has no branch to bring forward.
        git::current_branch(&wt).with_context(|| {
            format!("`{name}` has no branch (detached HEAD), so there is nothing to update")
        })?;
        if !opts.autostash && git::is_dirty(&wt).unwrap_or(false) {
            bail!(
                "`{name}` has uncommitted changes; commit them, or `clove update --autostash {name}` to set them aside and reapply them after"
            );
        }
        let base = self.config.base.clone();
        let mut timings = vec![];
        if opts.fetch {
            self.fetch_base(r, &mut timings, &base, Some(true))?;
        }
        crate::memo::forget_tree(&wt);
        let behind = git::count_commits(&wt, &format!("HEAD..{base}")).unwrap_or(0);
        if behind == 0 {
            return Ok(Update::UpToDate { base });
        }
        let strategy = opts.strategy.unwrap_or(self.config.update.strategy);
        let mut args = match strategy {
            Strategy::Rebase => vec!["rebase"],
            Strategy::Merge => vec!["merge", "--no-edit"],
        };
        if opts.autostash {
            args.push("--autostash");
        }
        args.push(&base);
        let step = format!("{} onto {base}", strategy.word());
        let out = crate::events::timed(r, &mut timings, &step, || run(&wt, &args))?;
        crate::memo::forget_tree(&wt);
        if !out.status.success() {
            if operation(&admin).is_some() {
                return Ok(Update::Conflict {
                    base,
                    strategy,
                    files: conflicted(&wt),
                });
            }
            bail!("git {} failed: {}", args.join(" "), stderr(&out));
        }
        self.after_update(name, &wt, base, strategy, Some(behind), r)
    }

    /// After the conflicts are fixed and added: finishes the rebase or merge,
    /// which may stop again on the next commit's conflicts.
    pub fn update_continue(&self, name: &str, r: &dyn Reporter) -> Result<Update> {
        let (wt, admin) = self.update_target(name)?;
        let strategy = operation(&admin).with_context(|| format!("`{name}` has no update in progress"))?;
        let left = conflicted(&wt);
        if !left.is_empty() {
            bail!(
                "still conflicted in `{name}`: {}; fix them and `git add` them first",
                left.join(", ")
            );
        }
        let args: &[&str] = match strategy {
            Strategy::Rebase => &["rebase", "--continue"],
            Strategy::Merge => &["commit", "--no-edit"],
        };
        let mut timings = vec![];
        let out = crate::events::timed(
            r,
            &mut timings,
            &format!("{} --continue", strategy.word()),
            || run(&wt, args),
        )?;
        crate::memo::forget_tree(&wt);
        let base = self.config.base.clone();
        if !out.status.success() {
            if operation(&admin).is_some() {
                return Ok(Update::Conflict {
                    base,
                    strategy,
                    files: conflicted(&wt),
                });
            }
            bail!("git {} failed: {}", args.join(" "), stderr(&out));
        }
        self.after_update(name, &wt, base, strategy, None, r)
    }

    /// Undoes a stopped update: the branch goes back to where it was.
    pub fn update_abort(&self, name: &str) -> Result<()> {
        let (wt, admin) = self.update_target(name)?;
        let strategy = operation(&admin).with_context(|| format!("`{name}` has no update in progress"))?;
        let out = run(&wt, &[strategy.word(), "--abort"])?;
        crate::memo::forget_tree(&wt);
        if !out.status.success() {
            bail!("git {} --abort failed: {}", strategy.word(), stderr(&out));
        }
        Ok(())
    }

    fn after_update(
        &self,
        name: &str,
        wt: &Path,
        base: String,
        strategy: Strategy,
        commits: Option<u32>,
        r: &dyn Reporter,
    ) -> Result<Update> {
        // Reapplying stashed changes can conflict after a clean rebase.
        let left = conflicted(wt);
        if !left.is_empty() {
            r.event(Event::Warn {
                message: format!(
                    "your set-aside changes conflicted when reapplied: {}; fix them (the stash is kept, `git stash list`)",
                    left.join(", ")
                ),
            });
        }
        let info = self.setup_clone(name, false, r)?;
        let pushed = git::out(
            wt,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
        )
        .is_ok();
        let hint = match strategy {
            Strategy::Rebase if pushed => {
                Some("the branch was pushed before: push it with `git push --force-with-lease`".into())
            }
            Strategy::Merge if pushed => Some("push it with `git push`".into()),
            _ => None,
        };
        Ok(Update::Updated {
            base,
            strategy,
            commits,
            hint,
            failed_steps: info.failed_steps,
        })
    }
}
