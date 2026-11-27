//! The operations: init, refresh, new, remove, mount, list, gc.
//!
//! Layout under `~/.clove/repos/<id>/`:
//!   repo.json                 RepoRecord
//!   golden/current -> gen-N   symlink, swapped atomically by refresh
//!   golden/gen-N/             image + HEAD + index + meta.json (one snapshot)
//!   clones/<name>/            image (clonefile of a golden image) + meta.json
//!   mnt/golden                golden mount point (only while warming)
//! Clones mount at `<clone_root>/<name>` and the worktree is `<mount>/<repo>`.
//! On Linux the images live in one btrfs pool per repo instead (see backend).

use crate::backend::{self, Backend, Store};
use crate::config::Step;
use crate::events::{Event, Reporter, Timing, timed};
use crate::git;
use crate::repo::{
    CloneInfo, CloneMeta, Clove, GoldenMeta, RepoRecord, read_json, sanitize_name, write_json,
};
use crate::steps::{StepRun, run_step};
use crate::util::{FileLock, now_unix, run, write_atomic};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct NewOptions {
    /// Branch to check out (created from `base` if it does not exist).
    pub branch: Option<String>,
    /// Start point for a new branch. Default: config base.
    pub base: Option<String>,
    /// Clone name. Default: the branch with `/` turned into `-`.
    pub name: Option<String>,
    /// Detached HEAD at `base` instead of a branch.
    pub detach: bool,
    /// Skip setup steps.
    pub no_setup: bool,
    /// `Some(true)` always fetches, `Some(false)` never; `None` follows config
    /// and skips a fetch made less than `fetch_interval` ago.
    pub fetch: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct RefreshOptions {
    /// Rerun every warm step, even those whose inputs did not change.
    pub full: bool,
}

/// How current the golden image is, against the configured policy.
#[derive(Debug, Clone, Serialize)]
pub struct Freshness {
    pub age_secs: u64,
    /// Commits on the base not in the golden image (as of the last fetch).
    pub behind: Option<u32>,
    pub stale: bool,
    /// Steps a new clone would rerun: their watched paths changed on the base
    /// since the golden commit, or their definition changed since it was built.
    pub changed_steps: Vec<String>,
    /// Why it is stale, for display.
    pub reason: Option<String>,
    /// Pid of a refresh in progress.
    pub refreshing: Option<u32>,
}

/// A cheap overview of a repo for lists of repos: no git status per clone.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct RepoSummary {
    pub clones: usize,
    pub mounted: usize,
    /// None: no golden image yet (`clove init`).
    pub generation: Option<u32>,
    pub stale: bool,
    pub refreshing: bool,
    /// Clones the last PR check found merged and clean.
    pub prunable: usize,
}

/// An existing branch `new` can check out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchCandidate {
    pub name: String,
    /// Only on origin; `new` creates a local tracking branch.
    pub remote: bool,
}

/// What `remove_or_defer` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "result", content = "users")]
pub enum Removal {
    Removed,
    /// Something still uses the clone (named here); it is marked and goes as
    /// soon as nothing does.
    WhenFree(Vec<String>),
}

#[derive(Debug, Clone, Default)]
pub struct RemoveOptions {
    /// Remove even with uncommitted changes, and force-unmount busy volumes.
    pub force: bool,
    /// Also delete the branch (`git branch -d`, or `-D` with force).
    pub delete_branch: bool,
    /// The caller proved nothing on the branch is missing from the base (a
    /// squash-merged PR), so `-D` is safe where `-d` would refuse.
    pub branch_merged: bool,
    /// Plain worktrees only: lift a `git worktree lock` to remove it.
    pub unlock: bool,
    /// Who asked, and why, for the removal history.
    pub by: crate::history::By,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoldenStatus {
    #[serde(flatten)]
    pub meta: GoldenMeta,
    pub age_secs: u64,
    /// Commits on base not yet in the golden image.
    pub behind: Option<u32>,
    pub image_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoStatus {
    pub name: String,
    pub root: PathBuf,
    pub id: String,
    pub state: PathBuf,
    pub clone_root: PathBuf,
    pub base: String,
    pub backend: Option<Backend>,
    pub golden: Option<GoldenStatus>,
    pub clones: usize,
    pub mounted: usize,
    pub steps: Vec<Step>,
    pub steps_detected: bool,
    pub freshness: Option<Freshness>,
    /// Files clove copies from the main checkout into the golden image and clones.
    pub copy: Vec<String>,
}

/// Unmounts the golden image when an operation bails halfway.
struct GoldenGuard<'a> {
    clove: &'a Clove,
    store: &'static dyn Store,
    armed: bool,
}

impl Drop for GoldenGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.store.golden_abort(self.clove);
        }
    }
}

impl Clove {
    fn refresh_lock(&self) -> Result<FileLock> {
        std::fs::create_dir_all(&self.state)?;
        let path = self.state.join("refresh.lock");
        let lock = FileLock::acquire(&path)?;
        let _ = std::fs::write(&path, std::process::id().to_string());
        Ok(lock)
    }

    /// Pid of the process refreshing the golden image right now, if any.
    pub fn refreshing(&self) -> Option<u32> {
        let path = self.state.join("refresh.lock");
        if !path.exists() {
            return None;
        }
        match FileLock::try_acquire(&path) {
            Ok(_) => None,
            Err(_) => std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .or(Some(0)),
        }
    }

    pub fn summary(&self) -> RepoSummary {
        let names = self.clone_names().unwrap_or_default();
        let store = self.record().ok().flatten().and_then(|r| r.backend.store().ok());
        let mounted = names
            .iter()
            .filter(|n| store.is_some_and(|s| s.clone_attached(self, n, &self.clone_mount(n))))
            .count();
        let fresh = self.freshness().ok().flatten();
        RepoSummary {
            clones: names.len(),
            mounted,
            generation: self.golden_meta().ok().flatten().map(|m| m.generation),
            stale: fresh.as_ref().is_some_and(|f| f.stale),
            refreshing: fresh.as_ref().is_some_and(|f| f.refreshing.is_some()),
            prunable: self.prune_cache().map_or(0, |p| {
                p.checks
                    .iter()
                    .filter(|m| m.verdict == crate::prune::Verdict::Prune && names.contains(&m.clone))
                    .count()
            }),
        }
    }

    /// Golden image age and distance from its base, judged against `[refresh]`.
    /// The main signal is steps: the image is stale as soon as a new clone would
    /// have to rerun one. Commit count and age are a safety net.
    pub fn freshness(&self) -> Result<Option<Freshness>> {
        let Some(meta) = self.golden_meta()? else {
            return Ok(None);
        };
        let age_secs = now_unix().saturating_sub(meta.created_at);
        let base = &self.config.base;
        // A function of the golden commit, the base's position and the steps.
        let steps_key: Vec<String> = self.config.steps.iter().map(step_hash).collect();
        let key = format!(
            "freshness:{}:{}:{base}:{}",
            self.root.display(),
            meta.commit,
            steps_key.join(",")
        );
        let (behind, changed) =
            crate::memo::get(&key, &[crate::memo::Dep::git_ref(&self.common_dir, base)], || {
                let behind = git::count_commits(&self.root, &format!("{}..{base}", meta.commit));
                let changed = if behind.unwrap_or(0) > 0 || !meta.step_hashes.is_empty() {
                    self.changed_steps(&meta)
                } else {
                    vec![]
                };
                (behind, changed)
            });
        let policy = &self.config.refresh;
        let reason = if let Some((name, n)) = changed.first() {
            let names: Vec<&str> = changed.iter().map(|(s, _)| s.as_str()).collect();
            Some(match (changed.len(), n) {
                (1, 0) => format!("step {name} changed"),
                (1, n) => format!(
                    "{name} inputs changed in {n} commit{}",
                    if *n == 1 { "" } else { "s" }
                ),
                _ => format!("{} would rerun", names.join(", ")),
            })
        } else if behind.is_some_and(|b| b > policy.max_behind) {
            Some(format!("{} commits behind {base}", behind.unwrap_or(0)))
        } else if age_secs > policy.max_age_secs {
            Some(format!("{} old", crate::util::human_age(age_secs)))
        } else if meta.base != *base {
            Some(format!("built from {}, config says {base}", meta.base))
        } else {
            None
        };
        Ok(Some(Freshness {
            age_secs,
            behind,
            stale: reason.is_some(),
            changed_steps: changed.into_iter().map(|(s, _)| s).collect(),
            reason,
            refreshing: self.refreshing(),
        }))
    }

    /// Steps with watched paths whose inputs moved between the golden commit and
    /// the base, with the number of commits that touched them (0: the step's
    /// definition changed). Steps without `watch` always run, so they never count.
    fn changed_steps(&self, meta: &GoldenMeta) -> Vec<(String, u32)> {
        let base = &self.config.base;
        let mut out = vec![];
        // One diff over every step's watched paths settles the common case
        // (nothing moved) without a git call per step; the same commit needs
        // none. Changed step definitions are found below either way.
        let watched: Vec<&str> = self
            .config
            .steps
            .iter()
            .filter(|s| s.warm.is_some() || s.setup.is_some())
            .flat_map(|s| s.watch.iter().map(String::as_str))
            .collect();
        let at_base = git::commit_of(&self.root, base).as_deref() == Some(meta.commit.as_str());
        let inputs_moved = !at_base && git::any_changed(&self.root, &meta.commit, base, &watched);
        for step in &self.config.steps {
            if step.watch.is_empty() || (step.warm.is_none() && step.setup.is_none()) {
                continue;
            }
            // Images from before step hashes were recorded have an empty map.
            if !meta.step_hashes.is_empty() && meta.step_hashes.get(&step.name) != Some(&step_hash(step)) {
                out.push((step.name.clone(), 0));
                continue;
            }
            if !inputs_moved {
                continue;
            }
            let mut args = vec!["diff", "--quiet", meta.commit.as_str(), base.as_str(), "--"];
            args.extend(step.watch.iter().map(String::as_str));
            if git::ok(&self.root, &args) {
                continue;
            }
            let range = format!("{}..{base}", meta.commit);
            let mut args = vec!["rev-list", "--count", range.as_str(), "--"];
            args.extend(step.watch.iter().map(String::as_str));
            let n = git::out(&self.root, &args)
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(1)
                .max(1);
            out.push((step.name.clone(), n));
        }
        out
    }

    pub(crate) fn lock(&self) -> Result<FileLock> {
        std::fs::create_dir_all(&self.state)?;
        FileLock::acquire(&self.lock_path())
    }

    // ---- golden --------------------------------------------------------

    /// Builds the golden image: a checkout of `base` with every warm step run.
    pub fn init(&self, r: &dyn Reporter) -> Result<GoldenMeta> {
        let _no_fsmonitor = git::golden_git();
        let _refresh = self.refresh_lock()?;
        if self.current_generation()?.is_some() {
            bail!(
                "{} already has a golden image; use `clove refresh` to update it",
                self.name
            );
        }
        let backend = Backend::detect()?;
        let store = backend.store()?;
        let mut timings = vec![];
        std::fs::create_dir_all(&self.state)?;
        // Time Machine would back up every image; they are caches. tmutil can
        // take 10s+, so don't hold init (and its progress UI) on it.
        #[cfg(target_os = "macos")]
        if let Ok(mut child) = Command::new("tmutil")
            .arg("addexclusion")
            .arg(&self.state)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            std::thread::spawn(move || child.wait());
        }

        let generation = 1;
        let gen_dir = self.golden_gen_dir(generation);
        let _ = std::fs::remove_dir_all(self.golden_dir());
        std::fs::create_dir_all(&gen_dir)?;
        store.golden_reset(self)?;

        timed(r, &mut timings, "create image", || {
            store.golden_create(self, generation)
        })?;
        timed(r, &mut timings, "mount", || store.golden_attach(self, generation))?;
        // After attach: on Linux, udisks decides where the volume mounts.
        let mount = store.golden_mount(self);
        let mut guard = GoldenGuard {
            clove: self,
            store,
            armed: true,
        };
        // Keep Spotlight off the volume; clones inherit the marker.
        #[cfg(target_os = "macos")]
        let _ = std::fs::write(mount.join(".metadata_never_index"), "");

        self.fetch_base(r, &mut timings, &self.config.base, Some(self.config.fetch))?;
        let wt = mount.join(&self.name);
        self.drop_registration(&wt)?;
        timed(r, &mut timings, "checkout", || {
            git::run_ok(
                &self.root,
                &[
                    "worktree",
                    "add",
                    "--quiet",
                    "--detach",
                    &wt.to_string_lossy(),
                    &self.config.base,
                ],
            )
        })?;
        git::run_ok(
            &self.root,
            &[
                "worktree",
                "lock",
                "--reason",
                "clove golden image (detached between refreshes)",
                &wt.to_string_lossy(),
            ],
        )?;
        let admin = admin_name_of(&wt)?;
        let record = RepoRecord {
            root: self.root.clone(),
            name: self.name.clone(),
            backend,
            golden_admin: admin,
            created_at: now_unix(),
        };
        self.save_record(&record)?;

        let meta = self.warm_and_snapshot(r, &mut timings, generation, &gen_dir, &wt, None)?;
        timed(r, &mut timings, "unmount", || {
            self.unmount_golden(r, store, generation)
        })?;
        guard.armed = false;
        self.set_current_generation(generation)?;
        Ok(GoldenMeta { timings, ..meta })
    }

    /// Updates the golden image to the latest base. Works on a clone of the
    /// current image and swaps it in at the end, so `new` keeps working meanwhile
    /// and existing clones are never affected.
    pub fn refresh(&self, r: &dyn Reporter) -> Result<GoldenMeta> {
        self.refresh_with(&RefreshOptions::default(), r)
    }

    /// Like [`refresh`](Self::refresh). Warm steps whose `watch` paths did not
    /// change since the previous golden commit are skipped unless `full`.
    pub fn refresh_with(&self, opts: &RefreshOptions, r: &dyn Reporter) -> Result<GoldenMeta> {
        let _no_fsmonitor = git::golden_git();
        let _refresh = self.refresh_lock()?;
        let record = self.require_record()?;
        let prev = self.golden_meta()?;
        let cur = self
            .current_generation()?
            .context("golden image missing; run `clove init`")?;
        let next = cur + 1;
        let cur_dir = self.golden_gen_dir(cur);
        let next_dir = self.golden_gen_dir(next);
        let _ = std::fs::remove_dir_all(&next_dir);
        std::fs::create_dir_all(&next_dir)?;
        let mut timings = vec![];
        let store = record.backend.store()?;
        let mount = store.golden_mount(self);
        let wt = mount.join(&self.name);
        store.golden_reset(self)?;

        let result = (|| -> Result<GoldenMeta> {
            timed(r, &mut timings, "clone golden", || {
                store.golden_derive(self, cur, next)
            })?;
            timed(r, &mut timings, "mount", || store.golden_attach(self, next))?;
            let mut guard = GoldenGuard {
                clove: self,
                store,
                armed: true,
            };
            self.write_admin(
                &record.golden_admin,
                &wt,
                &cur_dir,
                "clove golden image (detached between refreshes)",
            )?;
            self.fetch_base(r, &mut timings, &self.config.base, Some(self.config.fetch))?;
            timed(r, &mut timings, "checkout", || {
                git::run_ok(
                    &wt,
                    &["checkout", "--quiet", "--force", "--detach", &self.config.base],
                )
            })?;
            let prev = if opts.full { None } else { prev.as_ref() };
            let meta = self.warm_and_snapshot(r, &mut timings, next, &next_dir, &wt, prev)?;
            timed(r, &mut timings, "unmount", || self.unmount_golden(r, store, next))?;
            guard.armed = false;
            Ok(meta)
        })();

        match result {
            Ok(meta) => {
                let _lock = self.lock()?;
                self.set_current_generation(next)?;
                // Clones are independent files; old generations can go right away.
                self.prune_generations(store, next);
                Ok(GoldenMeta { timings, ..meta })
            }
            Err(e) => {
                let _ = store.golden_drop(self, next);
                let _ = std::fs::remove_dir_all(&next_dir);
                let _ = self.write_admin(
                    &record.golden_admin,
                    &wt,
                    &cur_dir,
                    "clove golden image (detached between refreshes)",
                );
                Err(e)
            }
        }
    }

    /// Unmounts (or seals) the golden volume, stopping daemons the warm steps left behind.
    fn unmount_golden(&self, r: &dyn Reporter, store: &dyn Store, generation: u32) -> Result<()> {
        let mount = store.golden_mount(self);
        // Name them before stopping them: `java` alone says little.
        let names: Vec<String> = backend::users(&mount).iter().map(backend::describe).collect();
        let stopped = backend::stop_users(&mount, std::time::Duration::from_secs(10));
        if !stopped.is_empty() {
            let mut names = names;
            names.dedup();
            r.event(Event::Info {
                message: format!(
                    "stopped background servers the warm steps left running: {}",
                    names.join(", ")
                ),
            });
        }
        store.golden_seal(self, generation)
    }

    fn prune_generations(&self, store: &dyn Store, keep: u32) {
        if let Ok(rd) = std::fs::read_dir(self.golden_dir()) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.starts_with("gen-") && n != format!("gen-{keep}") {
                    if let Ok(g) = n[4..].parse::<u32>() {
                        let _ = store.golden_drop(self, g);
                    }
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
    }

    fn warm_and_snapshot(
        &self,
        r: &dyn Reporter,
        timings: &mut Vec<Timing>,
        generation: u32,
        gen_dir: &Path,
        wt: &Path,
        prev: Option<&GoldenMeta>,
    ) -> Result<GoldenMeta> {
        let mut env = self.step_env(wt, "golden");
        // git run by the steps themselves: no fsmonitor daemon in the image.
        if !env.contains_key("GIT_CONFIG_COUNT") {
            env.insert("GIT_CONFIG_COUNT".into(), "1".into());
            env.insert("GIT_CONFIG_KEY_0".into(), "core.fsmonitor".into());
            env.insert("GIT_CONFIG_VALUE_0".into(), "false".into());
        }
        let copied = self.resolve_copy();
        if !copied.is_empty() {
            timed(r, timings, "copy files", || {
                self.copy_into(wt, &copied).map(|_| ())
            })?;
        }
        let mut hashes = BTreeMap::new();
        for step in &self.config.steps {
            let hash = step_hash(step);
            hashes.insert(step.name.clone(), hash.clone());
            let Some(cmd) = &step.warm else { continue };
            let label = format!("warm {}", step.name);
            // Skip a step when its definition and its watched inputs are unchanged
            // since the previous golden image, whose output this clone carries.
            if let Some(p) = prev
                && !step.watch.is_empty()
                && p.step_hashes.get(&step.name) == Some(&hash)
            {
                let mut args = vec!["diff", "--quiet", p.commit.as_str(), "HEAD", "--"];
                args.extend(step.watch.iter().map(String::as_str));
                if git::ok(wt, &args) {
                    r.event(Event::Skip {
                        step: label,
                        reason: format!("unchanged since gen {}", p.generation),
                    });
                    continue;
                }
            }
            timed(r, timings, &label, || {
                run_step(
                    r,
                    StepRun {
                        label: &label,
                        command: cmd,
                        cwd: wt,
                        exec: self.config.exec,
                        step,
                        env: &env,
                        log: &self.logs_dir().join(format!("golden-{}.log", step.name)),
                    },
                )
            })?;
        }
        // Stat data in the index must match the image so clones skip the rehash.
        timed(r, timings, "index", || {
            let _ = git::out(wt, &["update-index", "-q", "--refresh"]);
            Ok(())
        })?;
        let dirty = git::is_dirty(wt)?;
        if dirty {
            r.event(Event::Warn {
                message: "warm steps modified tracked files in the golden image; clones will start with those changes".into(),
            });
        }
        let record = self.require_record()?;
        let admin = self.admin_dir(&record.golden_admin);
        for f in ["HEAD", "index"] {
            std::fs::copy(admin.join(f), gen_dir.join(f)).with_context(|| format!("snapshot golden {f}"))?;
        }
        let meta = GoldenMeta {
            generation,
            commit: git::head_commit(wt)?,
            base: self.config.base.clone(),
            created_at: now_unix(),
            timings: timings.clone(),
            steps: self.config.steps.iter().map(|s| s.name.clone()).collect(),
            dirty,
            step_hashes: hashes,
            copied,
        };
        write_json(&gen_dir.join("meta.json"), &meta)?;
        self.record_golden(&meta);
        Ok(meta)
    }

    /// Fetches the configured base now (for staleness checks), ignoring the interval.
    pub fn fetch(&self) -> Result<()> {
        let mut t = vec![];
        self.fetch_base(&crate::events::Silent, &mut t, &self.config.base, Some(true))
    }

    /// Fetches the base unless it was fetched within `fetch_interval`. The
    /// background service runs when FETCH_HEAD changes, so a forced fetch there
    /// would retrigger it forever.
    pub fn fetch_if_due(&self) -> Result<()> {
        let mut t = vec![];
        self.fetch_base(&crate::events::Silent, &mut t, &self.config.base, None)
    }

    pub(crate) fn fetch_base(
        &self,
        r: &dyn Reporter,
        timings: &mut Vec<Timing>,
        base: &str,
        enabled: Option<bool>,
    ) -> Result<()> {
        if !enabled.unwrap_or(self.config.fetch) {
            return Ok(());
        }
        let Some((remote, branch)) = git::split_remote_ref(&self.root, base) else {
            return Ok(());
        };
        // A base we do not have yet (a new remote branch) is always fetched.
        if enabled.is_none() && git::ref_exists(&self.root, base) {
            let age = std::fs::metadata(self.common_dir.join("FETCH_HEAD"))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs());
            if let Some(age) = age.filter(|a| *a < self.config.fetch_interval) {
                r.event(Event::Skip {
                    step: "fetch".into(),
                    reason: format!("fetched {} ago (--fetch to force)", crate::util::human_age(age)),
                });
                return Ok(());
            }
        }
        let res = timed(r, timings, "fetch", || {
            git::run_ok(&self.root, &["fetch", "--quiet", &remote, &branch])
        });
        if let Err(e) = res {
            r.event(Event::Warn {
                message: format!("fetch {base} failed, using the local copy: {e:#}"),
            });
        }
        Ok(())
    }

    // ---- clones --------------------------------------------------------

    /// Creates a clone: O(1) image copy, mount, git registration, branch checkout,
    /// then only the setup steps whose inputs changed since the golden commit.
    pub fn new_clone(&self, opts: &NewOptions, r: &dyn Reporter) -> Result<CloneInfo> {
        let started = std::time::Instant::now();
        let record = self.require_record()?;
        let branch = match (&opts.branch, opts.detach) {
            // pr:123 checks out the pull request's head as local branch pr/123.
            (Some(b), _) if b.starts_with("pr:") => {
                let n = b[3..].trim_start_matches('#');
                if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) {
                    bail!("expected pr:<number>, got {b}");
                }
                let local = format!("pr/{n}");
                let mut t = vec![];
                timed(r, &mut t, "fetch pr", || {
                    git::run_ok(
                        &self.root,
                        &[
                            "fetch",
                            "--quiet",
                            "--force",
                            "origin",
                            &format!("pull/{n}/head:refs/heads/{local}"),
                        ],
                    )
                })
                .with_context(|| format!("fetch pull request {n} from origin"))?;
                Some(local)
            }
            (Some(b), _) => Some(b.clone()),
            (None, true) => None,
            (None, false) => Some(crate::names::generate(|n| {
                self.clone_meta(n).ok().flatten().is_none()
                    && !git::ref_exists(&self.root, &format!("refs/heads/{n}"))
            })),
        };
        let name = sanitize_name(opts.name.as_deref().or(branch.as_deref()).unwrap_or("detached"));
        if name.is_empty() {
            bail!("cannot derive a clone name; pass --name");
        }
        if self.clone_meta(&name)?.is_some() || self.clone_dir(&name).exists() {
            bail!("clone `{name}` already exists (`clove rm {name}` to replace it)");
        }
        let mount = self.clone_mount(&name);
        if mount.exists() && std::fs::read_dir(&mount)?.next().is_some() {
            bail!("{} exists and is not empty", mount.display());
        }
        let base = opts.base.clone().unwrap_or_else(|| self.config.base.clone());
        let wt = self.clone_worktree(&name);
        let dir = self.clone_dir(&name);
        let admin = format!("clove-{name}");
        let mut timings = vec![];
        let store = record.backend.store()?;

        // Copy the image and the matching index snapshot under the short lock so a
        // concurrent refresh cannot swap generations in between.
        let (golden, claimed) = {
            let _lock = self.lock()?;
            let generation = self
                .current_generation()?
                .context("golden image missing; run `clove init`")?;
            let gen_dir = self.golden_gen_dir(generation);
            let meta: GoldenMeta = read_json(&gen_dir.join("meta.json"))?.context("golden meta missing")?;
            std::fs::create_dir_all(&dir)?;
            // A spare of this generation is already mounted: rename it into place.
            let start = std::time::Instant::now();
            let claimed = self.claim_spare(&meta, &name, &mount);
            if claimed {
                let step = "claim spare".to_string();
                let took = start.elapsed();
                r.event(Event::Start { step: step.clone() });
                r.event(Event::Done {
                    step: step.clone(),
                    took,
                });
                timings.push(Timing {
                    step,
                    ms: took.as_secs_f64() * 1000.0,
                });
            } else {
                timed(r, &mut timings, "clone image", || {
                    store.clone_create(self, generation, &name, &mount)
                })?;
            }
            for f in ["HEAD", "index"] {
                std::fs::copy(gen_dir.join(f), dir.join(f))?;
            }
            (meta, claimed)
        };

        let cleanup = |e: anyhow::Error| -> anyhow::Error {
            let _ = store.clone_detach(self, &name, &mount, true);
            let _ = store.clone_delete(self, &name, &mount);
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::remove_dir_all(self.admin_dir(&admin));
            let _ = std::fs::remove_dir(&mount);
            e
        };

        // Fetch runs while the image mounts; both take ~1s.
        let fetch_enabled = opts.fetch;
        let (fetch_timings, attach) = std::thread::scope(|s| {
            let fetch = s.spawn(|| {
                let mut t = vec![];
                let _ = self.fetch_base(r, &mut t, &base, fetch_enabled);
                t
            });
            let mut t = vec![];
            let a = if claimed {
                Ok(())
            } else {
                timed(r, &mut t, "mount", || store.clone_attach(self, &name, &mount))
            };
            (fetch.join().unwrap_or_default(), a.map(|_| t))
        });
        timings.extend(attach.map_err(cleanup)?);
        timings.extend(fetch_timings);

        let result = (|| -> Result<()> {
            self.write_admin(&admin, &wt, &dir, &lock_reason(&name))?;
            timed(r, &mut timings, "checkout", || {
                self.checkout(&wt, branch.as_deref(), &base, &golden.commit)
            })?;
            Ok(())
        })();
        result.map_err(cleanup)?;

        if let Some(n) = git::count_commits(&self.root, &format!("{}..{base}", golden.commit))
            && n > 200
        {
            r.event(Event::Warn {
                message: format!(
                    "the golden image is {n} commits behind {base}, so checkout and setup do more work; `clove refresh` (or `clove service install` to do it nightly)"
                ),
            });
        }
        let _ = self.write_clone_env(&wt, &name);
        // Fresh copies of local-only files: the golden ones may be older.
        if !golden.copied.is_empty() {
            let files = golden.copied.clone();
            timed(r, &mut timings, "copy files", || {
                self.copy_into(&wt, &files).map(|_| ())
            })?;
        }
        let mut failed = vec![];
        let mut setup_commit = None;
        if !opts.no_setup {
            failed = self.setup(r, &mut timings, &wt, &name, Some((&golden.commit, "golden")));
            setup_commit = git::head_commit(&wt).ok();
        }

        let meta = CloneMeta {
            name: name.clone(),
            branch: branch.clone(),
            path: wt.clone(),
            mount: mount.clone(),
            base,
            golden_commit: golden.commit.clone(),
            golden_generation: golden.generation,
            created_at: now_unix(),
            timings,
            total_ms: started.elapsed().as_secs_f64() * 1000.0,
            failed_steps: failed,
            setup_commit,
        };
        write_json(&dir.join("meta.json"), &meta)?;
        self.record_created(&meta);
        self.info(&name, false)
    }

    fn checkout(&self, wt: &Path, branch: Option<&str>, base: &str, golden: &str) -> Result<()> {
        let new_branch = branch.filter(|b| {
            !git::ref_exists(wt, &format!("refs/heads/{b}"))
                && !git::ref_exists(wt, &format!("refs/remotes/origin/{b}"))
        });
        // The image is the golden worktree, detached at its commit, with an
        // index that matches it. When the base is that commit, files and index
        // are already right and only HEAD has to move: no `git switch`, which
        // stats every file of the fresh volume first (~0.7 s in a big repo).
        // A post-checkout hook (husky, LFS) only runs under `git switch`.
        if git::commit_of(wt, base).as_deref() == Some(golden)
            && git::head_commit(wt).ok().as_deref() == Some(golden)
            && !git::has_hook(wt, "post-checkout")
        {
            match (branch, new_branch) {
                (None, _) => return Ok(()),
                (Some(_), Some(b)) => {
                    // --no-track: a feature branch must not push to its base by accident.
                    git::run_ok(wt, &["branch", "--no-track", b, golden])?;
                    // The reflog line `git switch` writes, so `git checkout -` works the same.
                    let why = format!("checkout: moving from {golden} to {b}");
                    return git::run_ok(
                        wt,
                        &["symbolic-ref", "-m", &why, "HEAD", &format!("refs/heads/{b}")],
                    );
                }
                _ => {}
            }
        }
        // Otherwise git does the work, preloading the index from threads: on a
        // just-mounted volume that is ~3x faster than one stat at a time.
        let switch = |args: &[&str]| {
            let mut all = vec!["-c", "core.preloadIndex=true", "switch", "--quiet"];
            all.extend(args);
            git::run_ok(wt, &all)
        };
        match (branch, new_branch) {
            (None, _) => switch(&["--detach", base]),
            (Some(b), None) if git::ref_exists(wt, &format!("refs/heads/{b}")) => switch(&[b]),
            (Some(b), None) => switch(&["--track", &format!("origin/{b}")]),
            // --no-track: a feature branch must not push to its base by accident.
            (Some(b), Some(_)) => switch(&["--no-track", "-c", b, base]),
        }
    }

    /// Runs setup steps whose watched paths changed since `since` (all of them
    /// when `since` is None). Returns the steps that failed; a failed step leaves
    /// a usable clone.
    fn setup(
        &self,
        r: &dyn Reporter,
        timings: &mut Vec<Timing>,
        wt: &Path,
        name: &str,
        since: Option<(&str, &str)>,
    ) -> Vec<String> {
        let env = self.step_env(wt, name);
        let mut failed = vec![];
        // As in `changed_steps`: at the same commit nothing moved, and one
        // diff over all watched paths spares the per-step ones when nothing did.
        let moved = since.map(|(since, _)| {
            let watched: Vec<&str> = self
                .config
                .steps
                .iter()
                .filter(|s| s.setup.is_some())
                .flat_map(|s| s.watch.iter().map(String::as_str))
                .collect();
            let head = git::head_commit(wt).ok();
            head.as_deref() != Some(since) && git::any_changed(wt, since, "HEAD", &watched)
        });
        for step in &self.config.steps {
            let Some(cmd) = &step.setup else { continue };
            let label = format!("setup {}", step.name);
            if let Some((since, since_label)) = since
                && !step.watch.is_empty()
            {
                let mut args = vec!["diff", "--quiet", since, "HEAD", "--"];
                args.extend(step.watch.iter().map(String::as_str));
                if moved == Some(false) || git::ok(wt, &args) {
                    r.event(Event::Skip {
                        step: label,
                        reason: format!("unchanged since {since_label}"),
                    });
                    continue;
                }
            }
            let res = timed(r, timings, &label, || {
                run_step(
                    r,
                    StepRun {
                        label: &label,
                        command: cmd,
                        cwd: wt,
                        exec: self.config.exec,
                        step,
                        env: &env,
                        log: &self.logs_dir().join(format!("{name}-{}.log", step.name)),
                    },
                )
            });
            if let Err(e) = res {
                r.event(Event::Warn {
                    message: format!("{e:#}"),
                });
                failed.push(step.name.clone());
            }
        }
        failed
    }

    /// Environment for step commands and `clove run`.
    pub fn step_env(&self, wt: &Path, name: &str) -> BTreeMap<String, String> {
        let mut env = self.config.env.clone();
        env.insert("CLOVE".into(), "1".into());
        env.insert("CLOVE_NAME".into(), name.into());
        env.insert("CLOVE_PATH".into(), wt.to_string_lossy().into());
        env.insert("CLOVE_ROOT".into(), self.root.to_string_lossy().into());
        if self.uses_pnpm() {
            // The image is its own volume; without this pnpm starts an empty store
            // there. pnpm 11+ reads only pnpm_config_store_dir, older only the npm one.
            let set = ["npm_config_store_dir", "pnpm_config_store_dir"]
                .iter()
                .find_map(|k| env.get(*k).cloned());
            if let Some(store) = set.or_else(|| self.pnpm_store()) {
                for k in ["npm_config_store_dir", "pnpm_config_store_dir"] {
                    env.entry(k.into()).or_insert_with(|| store.clone());
                }
            }
        }
        env
    }

    pub fn uses_pnpm(&self) -> bool {
        self.config.steps.iter().any(|s| {
            [&s.warm, &s.setup]
                .iter()
                .any(|c| c.as_deref().is_some_and(|c| c.contains("pnpm")))
        })
    }

    /// pnpm's store on the home volume, as pnpm resolves it in the main checkout.
    pub fn pnpm_store(&self) -> Option<String> {
        let cache = self.state.join("pnpm-store");
        if let Ok(s) = std::fs::read_to_string(&cache) {
            return Some(s.trim().to_string());
        }
        let argv = crate::detect::wrap(self.config.exec, "pnpm store path");
        let store = run(Command::new(&argv[0]).args(&argv[1..]).current_dir(&self.root))
            .ok()
            .and_then(|s| s.lines().last().map(str::to_string))
            .filter(|s| s.starts_with('/'))?;
        let _ = std::fs::create_dir_all(&self.state);
        let _ = std::fs::write(&cache, &store);
        Some(store)
    }

    /// The environment a shell inside the clone should have: what clove's own
    /// steps get (config `env`, pnpm's store, CLOVE_*). Written next to the
    /// worktree, outside git, for the shell integration to load on `cd`.
    /// A clone is its own volume, so tools that keep a cache per disk (pnpm's
    /// store) would otherwise start an empty one there.
    pub fn write_clone_env(&self, wt: &Path, name: &str) -> Result<()> {
        let Some(dir) = wt.parent() else {
            return Ok(());
        };
        let valid = |k: &str| {
            k.chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        };
        let mut body = format!("#clove-env {}\n", crate::repo::env_token()?);
        body += &self
            .step_env(wt, name)
            .into_iter()
            .filter(|(k, v)| valid(k) && !v.contains('\n'))
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect::<String>();
        write_atomic(&dir.join(CLONE_ENV_FILE), body.as_bytes())
    }

    /// Writes a worktree admin dir from a HEAD/index snapshot and links the
    /// worktree's `.git` file to it. This is what `git worktree add` would do,
    /// minus the checkout we already have.
    fn write_admin(&self, admin: &str, wt: &Path, snapshot: &Path, reason: &str) -> Result<()> {
        let dir = self.admin_dir(admin);
        std::fs::create_dir_all(&dir)?;
        for f in ["HEAD", "index"] {
            std::fs::copy(snapshot.join(f), dir.join(f))
                .with_context(|| format!("copy {f} into {}", dir.display()))?;
        }
        std::fs::write(dir.join("commondir"), "../..\n")?;
        std::fs::write(dir.join("gitdir"), format!("{}\n", wt.join(".git").display()))?;
        std::fs::write(dir.join("locked"), reason)?;
        write_atomic(
            &wt.join(".git"),
            format!("gitdir: {}\n", dir.display()).as_bytes(),
        )?;
        Ok(())
    }

    /// Removes any existing worktree registration pointing at `wt`.
    fn drop_registration(&self, wt: &Path) -> Result<()> {
        let want = wt.join(".git");
        if let Ok(rd) = std::fs::read_dir(self.common_dir.join("worktrees")) {
            for e in rd.flatten() {
                if let Ok(g) = std::fs::read_to_string(e.path().join("gitdir"))
                    && Path::new(g.trim()) == want
                {
                    std::fs::remove_dir_all(e.path())?;
                }
            }
        }
        Ok(())
    }

    pub fn remove(&self, name: &str, opts: &RemoveOptions, r: &dyn Reporter) -> Result<()> {
        let meta = self.clone_meta(name)?.ok_or_else(|| self.no_clone(name))?;
        let mut timings = vec![];
        let store = self.store()?;
        if store.clone_attached(self, name, &meta.mount)
            && !opts.force
            && git::is_dirty(&meta.path).unwrap_or(false)
        {
            bail!("`{name}` has uncommitted changes; commit them, or `clove rm --force {name}` to discard");
        }
        let branch = git::current_branch(&meta.path).or(meta.branch.clone());
        let head = git::out(&meta.path, &["rev-parse", "HEAD"])
            .or_else(|_| {
                git::out(
                    &self.root,
                    &[
                        "rev-parse",
                        &format!("refs/heads/{}", branch.as_deref().unwrap_or("-")),
                    ],
                )
            })
            .ok()
            .map(|h| h.trim().to_string())
            .filter(|h| !h.is_empty());
        stop_daemons(&meta.path, &meta.mount);
        timed(r, &mut timings, "unmount", || {
            store.clone_detach(self, name, &meta.mount, opts.force)
        })?;
        let _lock = self.lock()?;
        timed(r, &mut timings, "delete image", || {
            let _ = std::fs::remove_dir_all(self.clone_admin(name));
            store.clone_delete(self, name, &meta.mount)?;
            std::fs::remove_dir_all(self.clone_dir(name))?;
            let _ = std::fs::remove_dir(&meta.mount);
            Ok(())
        })?;
        let mut branch_deleted = false;
        if opts.delete_branch
            && let Some(b) = &branch
        {
            // A branch whose tip is already in its base loses nothing; `-d` would
            // still refuse when the main checkout's branch is behind that base.
            let merged = git::ok(&self.root, &["merge-base", "--is-ancestor", b, &meta.base]);
            let flag = if opts.force || merged || opts.branch_merged {
                "-D"
            } else {
                "-d"
            };
            match git::run_ok(&self.root, &["branch", flag, b]) {
                Ok(_) => branch_deleted = true,
                Err(e) => r.event(Event::Warn {
                    message: format!("kept branch {b}: {e:#}"),
                }),
            }
        }
        self.record_removal(crate::history::Removed {
            at: 0,
            kind: crate::history::Kind::Clone,
            name: name.to_string(),
            path: meta.path.clone(),
            branch,
            head,
            by: opts.by,
            reason: opts.reason.clone(),
            branch_deleted,
        });
        Ok(())
    }

    /// Mounts a clone if needed and returns its worktree path.
    pub fn mount(&self, name: &str) -> Result<PathBuf> {
        let record = self.require_record()?;
        let meta = self.clone_meta(name)?.ok_or_else(|| self.no_clone(name))?;
        record.backend.store()?.clone_attach(self, name, &meta.mount)?;
        // Clones made before these existed get them on their next mount.
        let _ = self.write_clone_env(&meta.path, name);
        let admin = self.clone_admin(name);
        if admin.exists() {
            let _ = std::fs::write(admin.join("locked"), lock_reason(name));
        }
        Ok(meta.path)
    }

    pub fn unmount(&self, name: &str, force: bool) -> Result<()> {
        let meta = self.clone_meta(name)?.ok_or_else(|| self.no_clone(name))?;
        if self.shared_volume() {
            bail!(
                "clones share the repo's volume; `clove unmount` without arguments unmounts it when nothing uses it"
            );
        }
        stop_daemons(&meta.path, &meta.mount);
        self.store()?.clone_detach(self, name, &meta.mount, force)
    }

    /// With `status` in a UI (see [`crate::memo::serve_saved`]), the status
    /// may be the last known one; see [`CloneInfo::status_pending`].
    pub fn info(&self, name: &str, status: bool) -> Result<CloneInfo> {
        self.info_with(name, status, true)
    }

    /// With the status git gives now, for decisions such as pruning.
    pub fn info_live(&self, name: &str) -> Result<CloneInfo> {
        self.info_with(name, true, false)
    }

    fn info_with(&self, name: &str, status: bool, saved_ok: bool) -> Result<CloneInfo> {
        let meta = self.clone_meta(name)?.ok_or_else(|| self.no_clone(name))?;
        let store = self.require_record()?.backend.store()?;
        let mounted = store.clone_attached(self, name, &meta.mount);
        let head_file = std::fs::read_to_string(self.clone_admin(name).join("HEAD")).unwrap_or_default();
        let head_file = head_file.trim();
        let (branch, head) = match head_file.strip_prefix("ref: refs/heads/") {
            Some(b) => {
                let r = format!("refs/heads/{b}");
                let head = crate::memo::ref_sha(&self.common_dir, &r)
                    .or_else(|| git::out(&self.root, &["rev-parse", "--verify", "--quiet", &r]).ok());
                (Some(b.to_string()), head)
            }
            // Mid-rebase HEAD is detached; the branch being rebased is in git's state.
            None if !head_file.is_empty() => {
                let admin = self.clone_admin(name);
                let rebasing = ["rebase-merge", "rebase-apply"].iter().find_map(|d| {
                    let s = std::fs::read_to_string(admin.join(d).join("head-name")).ok()?;
                    s.trim().strip_prefix("refs/heads/").map(str::to_string)
                });
                (rebasing, Some(head_file.to_string()))
            }
            None => (meta.branch.clone(), None),
        };
        let (mut dirty, mut ahead, mut behind) = (None, None, None);
        let mut status_pending = false;
        if status && mounted {
            // Reused until the clone's index, HEAD, branch or base moves, or
            // its tree changes (see `memo`).
            use crate::memo::Dep;
            let admin = self.clone_admin(name);
            let mut deps = vec![
                Dep::file(admin.join("HEAD")),
                Dep::file(admin.join("index")),
                Dep::git_ref(&self.common_dir, &meta.base),
            ];
            if let Some(b) = &branch {
                deps.push(Dep::git_ref(&self.common_dir, &format!("refs/heads/{b}")));
            }
            let key = format!("clone-status:{}", meta.path.display());
            let recheck = |v: &(Option<bool>, Option<u32>, Option<u32>)| {
                if v.0 == Some(true) {
                    crate::memo::DIRTY_RECHECK
                } else {
                    crate::memo::MIN_RECHECK
                }
            };
            let (path, base) = (meta.path.clone(), meta.base.clone());
            let saved = crate::memo::get_tree_saved(&key, &meta.path, &deps, saved_ok, recheck, move || {
                let dirty = git::is_dirty(&path).ok();
                let (mut ahead, mut behind) = (None, None);
                if let Ok(s) = git::out(
                    &path,
                    &["rev-list", "--left-right", "--count", &format!("{base}...HEAD")],
                ) {
                    let mut it = s.split_whitespace().filter_map(|n| n.parse().ok());
                    behind = it.next();
                    ahead = it.next();
                }
                (dirty, ahead, behind)
            });
            (dirty, ahead, behind) = saved.value.unwrap_or_default();
            status_pending = saved.pending;
        }
        let key = format!("clone-bytes:{}:{name}", self.root.display());
        let (image_bytes, own_bytes) = if crate::memo::serving_saved() {
            // Measured in the background, where `self` can't go.
            let (root, owned, mount) = (self.root.clone(), name.to_string(), meta.mount.clone());
            crate::memo::get_aged_saved(&key, &self.clone_dir(name), crate::memo::SIZE_TTL, move || {
                Clove::open(&root).map_or((0, 0), |c| store.clone_bytes(&c, &owned, &mount))
            })
        } else {
            crate::memo::get_aged(&key, crate::memo::SIZE_TTL, || {
                store.clone_bytes(self, name, &meta.mount)
            })
        };
        let issue = self.clone_issue(name, &meta.path, mounted);
        Ok(CloneInfo {
            name: meta.name,
            path: meta.path,
            mounted,
            branch,
            head: head.map(|h| h.chars().take(12).collect()),
            dirty,
            ahead,
            behind,
            image_bytes,
            own_bytes,
            created_at: meta.created_at,
            golden_generation: meta.golden_generation,
            timings: meta.timings,
            total_ms: meta.total_ms,
            failed_steps: meta.failed_steps,
            issue,
            removing_when_free: self.clone_dir(name).join(PENDING_FILE).exists(),
            operation: crate::update::operation(&self.clone_admin(name)),
            status_pending,
        })
    }

    /// Removes the clone now if nothing uses it; otherwise marks it to go as
    /// soon as nothing does (see [`Clove::remove_pending`]) and says who holds
    /// it. Uncommitted changes refuse either way, unless `force`.
    pub fn remove_or_defer(&self, name: &str, opts: &RemoveOptions, r: &dyn Reporter) -> Result<Removal> {
        let meta = self.clone_meta(name)?.ok_or_else(|| self.no_clone(name))?;
        if !opts.force {
            let store = self.store()?;
            if store.clone_attached(self, name, &meta.mount) {
                if git::is_dirty(&meta.path).unwrap_or(false) {
                    bail!(
                        "`{name}` has uncommitted changes; commit them, or `clove rm --force {name}` to discard"
                    );
                }
                stop_daemons(&meta.path, &meta.mount);
                let users = backend::users(&meta.mount);
                if !users.is_empty() {
                    let pending = serde_json::json!({
                        "delete_branch": opts.delete_branch,
                        "branch_merged": opts.branch_merged,
                        "by": opts.by,
                        "reason": opts.reason,
                        "asked_at": now_unix(),
                    });
                    write_json(&self.clone_dir(name).join(PENDING_FILE), &pending)?;
                    let mut names: Vec<String> = users.iter().map(backend::describe).collect();
                    names.sort();
                    names.dedup();
                    return Ok(Removal::WhenFree(names));
                }
            }
        }
        self.remove(name, opts, r)?;
        Ok(Removal::Removed)
    }

    /// Clones marked to go once nothing uses them.
    pub fn pending_removals(&self) -> Vec<String> {
        self.clone_names()
            .unwrap_or_default()
            .into_iter()
            .filter(|n| self.clone_dir(n).join(PENDING_FILE).exists())
            .collect()
    }

    /// Removes the marked clones nothing uses any more. Returns what went and
    /// what failed (a clone that got uncommitted changes since is unmarked
    /// and kept, with the reason).
    pub fn remove_pending(&self, r: &dyn Reporter) -> (Vec<String>, Vec<(String, String)>) {
        let (mut done, mut kept) = (vec![], vec![]);
        for name in self.pending_removals() {
            let Ok(Some(meta)) = self.clone_meta(&name) else {
                continue;
            };
            let marker = self.clone_dir(&name).join(PENDING_FILE);
            let opts: serde_json::Value = read_json(&marker).ok().flatten().unwrap_or_default();
            let opts = RemoveOptions {
                force: false,
                delete_branch: opts["delete_branch"].as_bool().unwrap_or(false),
                branch_merged: opts["branch_merged"].as_bool().unwrap_or(false),
                unlock: false,
                by: serde_json::from_value(opts["by"].clone()).unwrap_or_default(),
                reason: opts["reason"].as_str().map(str::to_string),
            };
            let attached = self
                .store()
                .is_ok_and(|s| s.clone_attached(self, &name, &meta.mount));
            if attached {
                // fsmonitor too: git restarts it whenever anyone runs git there.
                stop_daemons(&meta.path, &meta.mount);
                if !backend::users(&meta.mount).is_empty() {
                    continue;
                }
            }
            match self.remove(&name, &opts, r) {
                Ok(()) => done.push(name),
                Err(e) => {
                    let _ = std::fs::remove_file(&marker);
                    kept.push((name, format!("{e:#}")));
                }
            }
        }
        (done, kept)
    }

    /// Waits for the marked clones to be free and removes them, checking every
    /// few seconds, until none are left or `max` has passed. One waiter per
    /// repo; a second call returns at once.
    pub fn wait_and_remove_pending(&self, max: std::time::Duration, r: &dyn Reporter) -> Result<()> {
        let Ok(_lock) = FileLock::try_acquire(&self.state.join("remove-pending.lock")) else {
            return Ok(());
        };
        let start = std::time::Instant::now();
        // Each check scans every process's open files: back off to a minute.
        let mut wait = std::time::Duration::from_secs(5);
        while !self.pending_removals().is_empty() && start.elapsed() < max {
            self.remove_pending(r);
            std::thread::sleep(wait);
            wait = (wait * 2).min(std::time::Duration::from_secs(60));
        }
        Ok(())
    }

    /// Existing branches worth offering for `new`, newest first: local ones,
    /// then origin-only ones. Leaves out the base, agent scaffolding
    /// (`worktree-agent-*`), branches checked out in a worktree (git refuses a
    /// second checkout), and branches already merged: into the base by
    /// ancestry, or by a PR the last check saw merged (squash merges).
    pub fn branch_candidates(&self) -> Vec<BranchCandidate> {
        let base = &self.config.base;
        let base_short = base.split_once('/').map(|(_, b)| b).unwrap_or(base).to_string();
        let lines = |args: &[&str]| -> Vec<String> {
            git::out(&self.root, args)
                .map(|s| s.lines().map(|l| l.trim().to_string()).collect())
                .unwrap_or_default()
        };
        let checked_out: std::collections::HashSet<String> = lines(&["worktree", "list", "--porcelain"])
            .into_iter()
            .filter_map(|l| l.strip_prefix("branch refs/heads/").map(str::to_string))
            .collect();
        let mut merged: std::collections::HashSet<String> =
            lines(&["branch", "--format=%(refname:short)", "--merged", base])
                .into_iter()
                .collect();
        merged.extend(
            lines(&["branch", "-r", "--format=%(refname:short)", "--merged", base])
                .into_iter()
                .filter_map(|r| r.split_once('/').map(|(_, b)| b.to_string())),
        );
        if let Some(p) = self.prune_cache() {
            merged.extend(p.checks.iter().filter_map(|m| {
                let pr = m.pr.as_ref()?;
                (pr.state == crate::prune::PrState::Merged)
                    .then(|| m.branch.clone())
                    .flatten()
            }));
        }
        let skip = |b: &str| {
            b == base_short
                || b == "HEAD"
                || b.starts_with("worktree-agent-")
                || checked_out.contains(b)
                || merged.contains(b)
        };
        let mut seen = std::collections::HashSet::new();
        let mut out = vec![];
        for (args, remote) in [
            (
                &[
                    "for-each-ref",
                    "--sort=-committerdate",
                    "--format=%(refname:short)",
                    "refs/heads",
                ][..],
                false,
            ),
            (
                &[
                    "for-each-ref",
                    "--sort=-committerdate",
                    "--format=%(refname:short)",
                    "refs/remotes/origin",
                ][..],
                true,
            ),
        ] {
            for r in lines(args) {
                let name = if remote {
                    match r.strip_prefix("origin/") {
                        Some(n) => n.to_string(),
                        None => continue,
                    }
                } else {
                    r
                };
                if name.is_empty() || skip(&name) || !seen.insert(name.clone()) {
                    continue;
                }
                out.push(BranchCandidate { name, remote });
            }
        }
        out
    }

    /// What an outside tool broke, if anything. A clone needs its git
    /// registration (the admin dir) and, when mounted, its worktree.
    fn clone_issue(&self, name: &str, wt: &Path, mounted: bool) -> Option<String> {
        if !self.clone_admin(name).exists() {
            return Some("unregistered from git outside clove".into());
        }
        if mounted && !wt.join(".git").exists() {
            return Some("worktree deleted outside clove".into());
        }
        None
    }

    pub fn list(&self, status: bool) -> Result<Vec<CloneInfo>> {
        if self.record()?.is_none() {
            return Ok(vec![]);
        }
        self.clone_names()?.iter().map(|n| self.info(n, status)).collect()
    }

    /// Resolves a clone from a name, a branch, or a path inside a clone.
    pub fn resolve(&self, query: &str) -> Result<String> {
        let names = self.clone_names()?;
        if names.iter().any(|n| n == query) {
            return Ok(query.to_string());
        }
        let s = sanitize_name(query);
        if names.contains(&s) {
            return Ok(s);
        }
        for n in &names {
            if let Ok(Some(m)) = self.clone_meta(n)
                && m.branch.as_deref() == Some(query)
            {
                return Ok(n.clone());
            }
        }
        let p = if query == "." {
            std::env::current_dir()?
        } else {
            PathBuf::from(query)
        };
        if let Ok(p) = p.canonicalize() {
            for n in &names {
                if let Ok(Some(m)) = self.clone_meta(n)
                    && p.starts_with(&m.mount)
                {
                    return Ok(n.clone());
                }
            }
        }
        // Gone: say who removed it and how the branch comes back.
        let gone = self.history().into_iter().find(|e| {
            e.kind == crate::history::Kind::Clone
                && (e.name == query
                    || e.name == s
                    || e.branch.as_deref() == Some(query)
                    || p.starts_with(&e.path))
        });
        if let Some(e) = gone {
            return Err(self.no_clone(&e.name));
        }
        bail!("no clone matches `{query}` (see `clove ls`)")
    }

    pub fn status(&self) -> Result<RepoStatus> {
        let record = self.record()?;
        let golden = match self.golden_meta()? {
            Some(meta) => {
                let range = format!("{}..{}", meta.commit, meta.base);
                let behind = crate::memo::get(
                    &format!("behind:{}:{range}", self.root.display()),
                    &[crate::memo::Dep::git_ref(&self.common_dir, &meta.base)],
                    || git::count_commits(&self.root, &range),
                );
                let image_bytes = record
                    .as_ref()
                    .and_then(|r| r.backend.store().ok())
                    .map_or(0, |s| {
                        crate::memo::get_aged(
                            &format!("golden-bytes:{}:{}", self.root.display(), meta.generation),
                            crate::memo::SIZE_TTL,
                            || s.golden_bytes(self, meta.generation),
                        )
                    });
                Some(GoldenStatus {
                    age_secs: now_unix().saturating_sub(meta.created_at),
                    behind,
                    image_bytes,
                    meta,
                })
            }
            None => None,
        };
        let clones = self.list(false)?;
        Ok(RepoStatus {
            name: self.name.clone(),
            root: self.root.clone(),
            id: self.id.clone(),
            state: self.state.clone(),
            clone_root: self.clone_root(),
            base: self.config.base.clone(),
            backend: record.map(|r| r.backend),
            golden,
            mounted: clones.iter().filter(|c| c.mounted).count(),
            clones: clones.len(),
            steps: self.config.steps.clone(),
            steps_detected: self.config.steps_detected,
            freshness: self.freshness()?,
            copy: self.golden_meta()?.map(|g| g.copied).unwrap_or_default(),
        })
    }

    /// Finds leftovers from interrupted operations. With `apply`, removes them.
    pub fn gc(&self, apply: bool) -> Result<Vec<String>> {
        let _lock = self.lock()?;
        let mut actions = vec![];
        let names = self.clone_names()?;
        let store = self.store()?;

        if let Ok(rd) = std::fs::read_dir(self.state.join("clones")) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if !names.contains(&n) {
                    actions.push(format!("remove half-created clone {n}"));
                    if apply {
                        let _ = store.clone_detach(self, &n, &self.clone_mount(&n), true);
                        let _ = store.clone_delete(self, &n, &self.clone_mount(&n));
                        std::fs::remove_dir_all(e.path())?;
                    }
                }
            }
        }
        let pending = self.pending_removals();
        for n in &pending {
            actions.push(format!("remove clone {n} once nothing uses it (asked earlier)"));
        }
        // remove() takes the repo lock itself: release ours around it.
        let _lock = if apply && !pending.is_empty() {
            drop(_lock);
            self.remove_pending(&crate::events::Silent);
            self.lock()?
        } else {
            _lock
        };
        // Clones an outside tool broke: worktree deleted, or unregistered.
        for n in &names {
            let Ok(Some(meta)) = self.clone_meta(n) else {
                continue;
            };
            let mounted = store.clone_attached(self, n, &meta.mount);
            if let Some(issue) = self.clone_issue(n, &meta.path, mounted) {
                actions.push(format!("remove clone {n}: {issue}"));
                if apply {
                    let _ = store.clone_detach(self, n, &meta.mount, true);
                    let _ = std::fs::remove_dir_all(self.clone_admin(n));
                    let _ = store.clone_delete(self, n, &meta.mount);
                    let _ = std::fs::remove_dir_all(self.clone_dir(n));
                    let _ = std::fs::remove_dir(&meta.mount);
                }
            }
        }
        if let Ok(rd) = std::fs::read_dir(self.common_dir.join("worktrees")) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if let Some(clone) = n.strip_prefix("clove-")
                    && !names.iter().any(|x| x == clone)
                {
                    actions.push(format!("remove git registration {n}"));
                    if apply {
                        std::fs::remove_dir_all(e.path())?;
                    }
                }
            }
        }
        if let Ok(rd) = std::fs::read_dir(self.clone_root()) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                let empty = std::fs::read_dir(e.path())
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(false);
                // The spare's mount point is handled below, with the spare.
                if !names.contains(&n) && empty && e.path() != self.spare_mount() {
                    actions.push(format!("remove empty mount point {}", e.path().display()));
                    if apply {
                        let _ = std::fs::remove_dir(e.path());
                    }
                }
            }
        }
        // Daemons started from a clone that is gone or unmounted (a forced
        // unmount, a clone removed outside clove): nothing else stops them,
        // and some spin once their files are gone.
        let roots = clone_roots(&self.clone_root());
        for (pid, ppid, exe) in backend::processes() {
            let Some(clone) = clone_of(&exe, &roots) else {
                continue;
            };
            let known = names.contains(&clone);
            if ppid != 1 || (known && store.clone_attached(self, &clone, &self.clone_mount(&clone))) {
                continue;
            }
            let what = exe
                .file_name()
                .map_or_else(|| exe.display().to_string(), |n| n.to_string_lossy().to_string());
            let state = if known { "unmounted" } else { "removed" };
            actions.push(format!(
                "stop {what} (pid {pid}), still running from {state} clone {clone}"
            ));
            if apply {
                backend::stop_processes(&[pid], std::time::Duration::from_secs(5));
            }
        }
        match self.spare_state() {
            crate::spare::SpareState::Stale(why) => {
                actions.push(format!("detach and delete the spare clone: {why}"));
                if apply {
                    self.discard_spare(true)?;
                }
            }
            crate::spare::SpareState::Off if self.spare_exists() => {
                actions.push("detach and delete the spare clone (spare = false)".into());
                if apply {
                    self.discard_spare(false)?;
                }
            }
            _ => {}
        }
        if store.golden_stuck(self)
            && let Ok(lock) = FileLock::try_acquire(&self.state.join("refresh.lock"))
        {
            actions.push("unmount golden image left mounted by an interrupted refresh".into());
            if apply {
                store.golden_reset(self)?;
            }
            drop(lock);
        }
        if let Some(cur) = self.current_generation()?
            && let Ok(rd) = std::fs::read_dir(self.golden_dir())
        {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.starts_with("gen-") && n != format!("gen-{cur}") {
                    actions.push(format!("remove old golden {n}"));
                    if apply {
                        if let Ok(g) = n[4..].parse::<u32>() {
                            store.golden_drop(self, g)?;
                        }
                        std::fs::remove_dir_all(e.path())?;
                    }
                }
            }
        }
        Ok(actions)
    }

    /// Removes the golden image and clove's records. Refuses while clones exist.
    pub fn deinit(&self, r: &dyn Reporter) -> Result<()> {
        let _refresh = self.refresh_lock()?;
        if !self.clone_names()?.is_empty() {
            bail!("remove the clones first (`clove ls`), or `clove rm --all`");
        }
        let mut t = vec![];
        let store = self.store()?;
        timed(r, &mut t, "delete golden", || {
            // A mounted spare lives in the state folder, which goes next.
            self.discard_spare(false)?;
            if backend::is_mounted(&self.spare_mount()) {
                bail!("a spare clone is being made; try again in a few seconds");
            }
            store.golden_reset(self)?;
            if let Some(rec) = self.record()? {
                let _ = std::fs::remove_dir_all(self.admin_dir(&rec.golden_admin));
            }
            store.deinit(self)?;
            std::fs::remove_dir_all(&self.state)?;
            // The clones folder clove made; only removed when empty.
            let _ = std::fs::remove_dir(&self.config.clone_root);
            Ok(())
        })
    }
}

/// Inside a clone's state dir: removal asked while the clone was in use.
pub const PENDING_FILE: &str = "remove-when-free.json";

/// The lock reason git prints when something tries `git worktree remove` on a
/// clone: it names the command that works.
fn lock_reason(name: &str) -> String {
    format!("clove clone: remove it with `clove rm {name}` (git can't unmount or delete its disk image)")
}

/// git's fsmonitor daemon (core.fsmonitor) keeps running in a worktree and
/// holds its volume. It's a cache, so stop it before unmounting.
fn stop_fsmonitor(wt: &Path) {
    if wt.join(".git").exists() {
        let _ = git::cmd(wt).args(["fsmonitor--daemon", "stop"]).output();
    }
}

/// Stops what would hold a clone's volume with nobody to close it: git's
/// fsmonitor and daemons started from the clone (see `stop_orphan_daemons`).
fn stop_daemons(wt: &Path, mount: &Path) {
    stop_fsmonitor(wt);
    backend::stop_orphan_daemons(mount, std::time::Duration::from_secs(5));
}

/// A folder deep enough to hold clones and nothing else: never `/`, an empty
/// path, or a top-level system folder.
fn safe_root(p: &Path) -> bool {
    p.is_absolute() && p.components().count() >= 4
}

/// Where clones of this repo live, as written and resolved. Empty when the
/// folder doesn't exist: a missing one must never become an empty prefix,
/// which every path starts with.
fn clone_roots(root: &Path) -> Vec<PathBuf> {
    let mut v = vec![];
    if let Ok(real) = root.canonicalize() {
        v.push(root.to_path_buf());
        v.push(real);
    }
    v.retain(|r| safe_root(r));
    v
}

/// The clone a process's executable lives in, if it lives under a root.
fn clone_of(exe: &Path, roots: &[PathBuf]) -> Option<String> {
    let rel = roots.iter().find_map(|r| exe.strip_prefix(r).ok())?;
    match rel.components().next()? {
        std::path::Component::Normal(first) => Some(first.to_string_lossy().to_string()),
        _ => None,
    }
}

/// Next to each clone's worktree: KEY=VALUE lines the shell integration loads.
pub const CLONE_ENV_FILE: &str = ".clove-env";

fn admin_name_of(wt: &Path) -> Result<String> {
    let s = std::fs::read_to_string(wt.join(".git"))?;
    let p = s.trim().strip_prefix("gitdir: ").context("worktree .git file")?;
    Ok(Path::new(p)
        .file_name()
        .context("admin dir name")?
        .to_string_lossy()
        .to_string())
}

/// Patterns for files that live only in a developer's checkout.
const DETECT_COPY: &[&str] = &["**/.env", "**/.env.*", "**/.envrc.local", "**/.dev.vars"];

fn step_hash(step: &Step) -> String {
    use sha2::{Digest, Sha256};
    let json = serde_json::to_vec(step).unwrap_or_default();
    Sha256::digest(&json)
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Clove {
    /// Untracked files in the main checkout matching `copy` (or the detected
    /// env-file patterns). Ignored directories are not descended into.
    pub fn resolve_copy(&self) -> Vec<String> {
        let patterns: Vec<String> = match &self.config.copy {
            Some(p) => p.clone(),
            None => DETECT_COPY.iter().map(|s| s.to_string()).collect(),
        };
        if patterns.is_empty() {
            return vec![];
        }
        let specs: Vec<String> = patterns
            .iter()
            .map(|p| {
                if p.contains(['*', '?', '[']) {
                    format!(":(glob){p}")
                } else {
                    p.clone()
                }
            })
            .collect();
        let mut files = std::collections::BTreeSet::new();
        for ignored in [false, true] {
            let mut args = vec!["ls-files", "-z", "--others", "--exclude-standard"];
            if ignored {
                args.extend(["--ignored", "--directory", "--no-empty-directory"]);
            }
            args.push("--");
            args.extend(specs.iter().map(String::as_str));
            if let Ok(out) = crate::util::output(git::cmd(&self.root).args(&args)) {
                for f in String::from_utf8_lossy(&out.stdout).split('\0') {
                    let skip = f.is_empty()
                        || f.ends_with('/')
                        || f.split('/').any(|c| c == "node_modules")
                        || f.contains(".example")
                        || f.contains(".sample")
                        || f.contains(".template");
                    if !skip && self.root.join(f).is_file() {
                        files.insert(f.to_string());
                    }
                }
            }
        }
        files.into_iter().collect()
    }

    /// Copies `files` (relative to the main checkout) into `wt`, overwriting.
    /// Returns how many existed and were copied.
    fn copy_into(&self, wt: &Path, files: &[String]) -> Result<usize> {
        let mut n = 0;
        for f in files {
            let src = self.root.join(f);
            if !src.is_file() {
                continue;
            }
            let dst = wt.join(f);
            if let Some(p) = dst.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(&src, &dst).with_context(|| format!("copy {f}"))?;
            n += 1;
        }
        Ok(n)
    }

    /// Brings an existing clone's environment up to date after its branch moved
    /// (a pull, a rebase, a checkout): copies local files again and reruns the
    /// setup steps whose inputs changed since setup last ran. `all` reruns every step.
    pub fn setup_clone(&self, name: &str, all: bool, r: &dyn Reporter) -> Result<CloneInfo> {
        let mut meta = self.clone_meta(name)?.ok_or_else(|| self.no_clone(name))?;
        let wt = self.mount(name)?;
        let mut timings = vec![];
        let golden = self.golden_meta()?;
        let files = golden.map(|g| g.copied).unwrap_or_default();
        if !files.is_empty() {
            timed(r, &mut timings, "copy files", || {
                self.copy_into(&wt, &files).map(|_| ())
            })?;
        }
        let since = meta
            .setup_commit
            .clone()
            .unwrap_or_else(|| meta.golden_commit.clone());
        let since = (!all).then_some((since.as_str(), "last setup"));
        meta.failed_steps = self.setup(r, &mut timings, &wt, name, since);
        meta.setup_commit = git::head_commit(&wt).ok();
        write_json(&self.clone_dir(name).join("meta.json"), &meta)?;
        self.info(name, false)
    }
}

#[cfg(test)]
mod stray_tests {
    use super::*;

    #[test]
    fn a_missing_clone_folder_claims_no_process() {
        let gone = Path::new("/Users/nobody/src/app.clove");
        assert!(clone_roots(gone).is_empty());
        assert_eq!(clone_of(Path::new("/usr/libexec/logd"), &clone_roots(gone)), None);
        // What 0.1.12 to 0.1.17 did: an empty root matched every process.
        assert_eq!(clone_of(Path::new("/usr/libexec/logd"), &[PathBuf::new()]), None);
    }

    #[test]
    fn only_deep_folders_are_roots() {
        for p in ["", "/", "/Users", "/Users/me", "relative/a/b/c"] {
            assert!(!safe_root(Path::new(p)), "{p:?}");
        }
        assert!(safe_root(Path::new("/Users/me/src/app.clove")));
    }

    #[test]
    fn a_process_belongs_to_the_clone_folder_it_runs_from() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("app.clove");
        std::fs::create_dir_all(&root).unwrap();
        let roots = clone_roots(&root);
        assert!(!roots.is_empty());
        let exe = roots[1].join("feat-x/app/node_modules/.bin/turbo");
        assert_eq!(clone_of(&exe, &roots).as_deref(), Some("feat-x"));
        assert_eq!(clone_of(Path::new("/usr/bin/git"), &roots), None);
    }
}
