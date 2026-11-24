//! The operations: init, refresh, new, remove, mount, list, gc.
//!
//! Layout under `~/.clove/repos/<id>/`:
//!   repo.json                 RepoRecord
//!   golden/current -> gen-N   symlink, swapped atomically by refresh
//!   golden/gen-N/             image + HEAD + index + meta.json (one snapshot)
//!   clones/<name>/            image (clonefile of a golden image) + meta.json
//!   mnt/golden                golden mount point (only while warming)
//! Clones mount at `<clone_root>/<name>` and the worktree is `<mount>/<repo>`.

use crate::backend::{self, Backend};
use crate::config::Step;
use crate::events::{Event, Reporter, Timing, timed};
use crate::git;
use crate::repo::{
    CloneInfo, CloneMeta, Clove, GoldenMeta, RepoRecord, read_json, sanitize_name, write_json,
};
use crate::steps::{StepRun, run_step};
use crate::util::{FileLock, allocated_bytes, clonefile, now_unix, run, write_atomic};
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
pub struct RemoveOptions {
    /// Remove even with uncommitted changes, and force-unmount busy volumes.
    pub force: bool,
    /// Also delete the branch (`git branch -d`, or `-D` with force).
    pub delete_branch: bool,
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
}

/// Unmounts the golden image when an operation bails halfway.
struct GoldenGuard<'a> {
    mount: &'a Path,
    armed: bool,
}

impl Drop for GoldenGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = backend::detach(self.mount, true);
        }
    }
}

impl Clove {
    fn refresh_lock(&self) -> Result<FileLock> {
        std::fs::create_dir_all(&self.state)?;
        FileLock::acquire(&self.state.join("refresh.lock"))
    }

    fn lock(&self) -> Result<FileLock> {
        std::fs::create_dir_all(&self.state)?;
        FileLock::acquire(&self.lock_path())
    }

    // ---- golden --------------------------------------------------------

    /// Builds the golden image: a checkout of `base` with every warm step run.
    pub fn init(&self, r: &dyn Reporter) -> Result<GoldenMeta> {
        let _refresh = self.refresh_lock()?;
        if self.current_generation()?.is_some() {
            bail!(
                "{} already has a golden image; use `clove refresh` to update it",
                self.name
            );
        }
        let backend = Backend::detect()?;
        let mut timings = vec![];
        std::fs::create_dir_all(&self.state)?;
        // Time Machine would back up every image; they are caches.
        let _ = Command::new("tmutil")
            .arg("addexclusion")
            .arg(&self.state)
            .output();

        let generation = 1;
        let gen_dir = self.golden_gen_dir(generation);
        let _ = std::fs::remove_dir_all(self.golden_dir());
        std::fs::create_dir_all(&gen_dir)?;
        let image = gen_dir.join(backend.file_name());
        let mount = self.golden_mount();
        backend::detach(&mount, true)?;

        timed(r, &mut timings, "create image", || {
            backend.create(&image, &self.config.image_size, &format!("{}-clove", self.name))
        })?;
        timed(r, &mut timings, "mount", || backend.attach(&image, &mount))?;
        let mut guard = GoldenGuard {
            mount: &mount,
            armed: true,
        };
        // Keep Spotlight off the volume; clones inherit the marker.
        let _ = std::fs::write(mount.join(".metadata_never_index"), "");

        self.fetch_base(r, &mut timings, &self.config.base, Some(self.config.fetch))?;
        let wt = self.golden_worktree();
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

        let meta = self.warm_and_snapshot(r, &mut timings, generation, &gen_dir)?;
        timed(r, &mut timings, "unmount", || self.unmount_golden(r, &mount))?;
        guard.armed = false;
        self.set_current_generation(generation)?;
        Ok(GoldenMeta { timings, ..meta })
    }

    /// Updates the golden image to the latest base. Works on a clone of the
    /// current image and swaps it in at the end, so `new` keeps working meanwhile
    /// and existing clones are never affected.
    pub fn refresh(&self, r: &dyn Reporter) -> Result<GoldenMeta> {
        let _refresh = self.refresh_lock()?;
        let record = self.require_record()?;
        let cur = self
            .current_generation()?
            .context("golden image missing; run `clove init`")?;
        let next = cur + 1;
        let cur_dir = self.golden_gen_dir(cur);
        let next_dir = self.golden_gen_dir(next);
        let _ = std::fs::remove_dir_all(&next_dir);
        std::fs::create_dir_all(&next_dir)?;
        let mut timings = vec![];
        let file = record.backend.file_name();
        let mount = self.golden_mount();
        backend::detach(&mount, true)?;

        let result = (|| -> Result<GoldenMeta> {
            timed(r, &mut timings, "clone golden", || {
                clonefile(&cur_dir.join(file), &next_dir.join(file))
            })?;
            timed(r, &mut timings, "mount", || {
                record.backend.attach(&next_dir.join(file), &mount)
            })?;
            let mut guard = GoldenGuard {
                mount: &mount,
                armed: true,
            };
            let wt = self.golden_worktree();
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
            let meta = self.warm_and_snapshot(r, &mut timings, next, &next_dir)?;
            timed(r, &mut timings, "unmount", || self.unmount_golden(r, &mount))?;
            guard.armed = false;
            Ok(meta)
        })();

        match result {
            Ok(meta) => {
                let _lock = self.lock()?;
                self.set_current_generation(next)?;
                // Clones are independent files; old generations can go right away.
                self.prune_generations(next);
                Ok(GoldenMeta { timings, ..meta })
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&next_dir);
                let _ = self.write_admin(
                    &record.golden_admin,
                    &self.golden_worktree(),
                    &cur_dir,
                    "clove golden image (detached between refreshes)",
                );
                Err(e)
            }
        }
    }

    /// Unmounts the golden volume, stopping daemons the warm steps left behind.
    fn unmount_golden(&self, r: &dyn Reporter, mount: &Path) -> Result<()> {
        let stopped = backend::stop_users(mount, std::time::Duration::from_secs(10));
        if !stopped.is_empty() {
            let names: Vec<_> = stopped.iter().map(|u| u.command.as_str()).collect();
            r.event(Event::Warn {
                message: format!("stopped processes a warm step left running: {}", names.join(", ")),
            });
        }
        backend::detach(mount, false)
    }

    fn prune_generations(&self, keep: u32) {
        if let Ok(rd) = std::fs::read_dir(self.golden_dir()) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.starts_with("gen-") && n != format!("gen-{keep}") {
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
    ) -> Result<GoldenMeta> {
        let wt = self.golden_worktree();
        let env = self.step_env(&wt, "golden");
        for step in &self.config.steps {
            let Some(cmd) = &step.warm else { continue };
            let label = format!("warm {}", step.name);
            timed(r, timings, &label, || {
                run_step(
                    r,
                    StepRun {
                        label: &label,
                        command: cmd,
                        cwd: &wt,
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
            let _ = git::out(&wt, &["update-index", "-q", "--refresh"]);
            Ok(())
        })?;
        let dirty = git::is_dirty(&wt)?;
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
            commit: git::head_commit(&wt)?,
            base: self.config.base.clone(),
            created_at: now_unix(),
            timings: timings.clone(),
            steps: self.config.steps.iter().map(|s| s.name.clone()).collect(),
            dirty,
        };
        write_json(&gen_dir.join("meta.json"), &meta)?;
        Ok(meta)
    }

    fn fetch_base(
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
        let file = record.backend.file_name();

        // Copy the image and the matching index snapshot under the short lock so a
        // concurrent refresh cannot swap generations in between.
        let golden = {
            let _lock = self.lock()?;
            let generation = self
                .current_generation()?
                .context("golden image missing; run `clove init`")?;
            let gen_dir = self.golden_gen_dir(generation);
            let meta: GoldenMeta = read_json(&gen_dir.join("meta.json"))?.context("golden meta missing")?;
            std::fs::create_dir_all(&dir)?;
            timed(r, &mut timings, "clone image", || {
                clonefile(&gen_dir.join(file), &dir.join(file))
            })?;
            for f in ["HEAD", "index"] {
                std::fs::copy(gen_dir.join(f), dir.join(f))?;
            }
            meta
        };

        let cleanup = |e: anyhow::Error| -> anyhow::Error {
            let _ = backend::detach(&mount, true);
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
            let a = timed(r, &mut t, "mount", || {
                record.backend.attach(&dir.join(file), &mount)
            });
            (fetch.join().unwrap_or_default(), a.map(|_| t))
        });
        timings.extend(attach.map_err(cleanup)?);
        timings.extend(fetch_timings);

        let result = (|| -> Result<()> {
            self.write_admin(
                &admin,
                &wt,
                &dir,
                &format!("clove clone {name} (image may be detached)"),
            )?;
            timed(r, &mut timings, "checkout", || {
                self.checkout(&wt, branch.as_deref(), &base)
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
        let mut failed = vec![];
        if !opts.no_setup {
            failed = self.setup(r, &mut timings, &wt, &name, &golden.commit);
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
        };
        write_json(&dir.join("meta.json"), &meta)?;
        self.info(&name, false)
    }

    fn checkout(&self, wt: &Path, branch: Option<&str>, base: &str) -> Result<()> {
        let Some(b) = branch else {
            return git::run_ok(wt, &["switch", "--quiet", "--detach", base]);
        };
        if git::ref_exists(wt, &format!("refs/heads/{b}")) {
            git::run_ok(wt, &["switch", "--quiet", b])
        } else if git::ref_exists(wt, &format!("refs/remotes/origin/{b}")) {
            git::run_ok(wt, &["switch", "--quiet", "--track", &format!("origin/{b}")])
        } else {
            // --no-track: a feature branch must not push to its base by accident.
            git::run_ok(wt, &["switch", "--quiet", "--no-track", "-c", b, base])
        }
    }

    /// Runs setup steps whose watched paths changed since the golden commit.
    /// Returns the steps that failed; a failed step leaves a usable clone.
    fn setup(
        &self,
        r: &dyn Reporter,
        timings: &mut Vec<Timing>,
        wt: &Path,
        name: &str,
        golden_commit: &str,
    ) -> Vec<String> {
        let env = self.step_env(wt, name);
        let mut failed = vec![];
        for step in &self.config.steps {
            let Some(cmd) = &step.setup else { continue };
            let label = format!("setup {}", step.name);
            if !step.watch.is_empty() {
                let mut args = vec!["diff", "--quiet", golden_commit, "HEAD", "--"];
                args.extend(step.watch.iter().map(String::as_str));
                if git::ok(wt, &args) {
                    r.event(Event::Skip {
                        step: label,
                        reason: "unchanged since golden".into(),
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
        if self.uses_pnpm() && !env.contains_key("npm_config_store_dir") {
            // The image is its own volume; without this pnpm starts an empty store there.
            if let Some(store) = self.pnpm_store() {
                env.insert("npm_config_store_dir".into(), store);
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
        let meta = self
            .clone_meta(name)?
            .with_context(|| format!("no clone named `{name}`"))?;
        let mut timings = vec![];
        if backend::is_mounted(&meta.mount) && !opts.force && git::is_dirty(&meta.path).unwrap_or(false) {
            bail!("`{name}` has uncommitted changes; commit them, or `clove rm --force {name}` to discard");
        }
        let branch = git::current_branch(&meta.path).or(meta.branch.clone());
        timed(r, &mut timings, "unmount", || {
            backend::detach(&meta.mount, opts.force)
        })?;
        let _lock = self.lock()?;
        timed(r, &mut timings, "delete image", || {
            let _ = std::fs::remove_dir_all(self.clone_admin(name));
            std::fs::remove_dir_all(self.clone_dir(name))?;
            let _ = std::fs::remove_dir(&meta.mount);
            Ok(())
        })?;
        if opts.delete_branch
            && let Some(b) = branch
        {
            // A branch whose tip is already in its base loses nothing; `-d` would
            // still refuse when the main checkout's branch is behind that base.
            let merged = git::ok(&self.root, &["merge-base", "--is-ancestor", &b, &meta.base]);
            let flag = if opts.force || merged { "-D" } else { "-d" };
            if let Err(e) = git::run_ok(&self.root, &["branch", flag, &b]) {
                r.event(Event::Warn {
                    message: format!("kept branch {b}: {e:#}"),
                });
            }
        }
        Ok(())
    }

    /// Mounts a clone if needed and returns its worktree path.
    pub fn mount(&self, name: &str) -> Result<PathBuf> {
        let record = self.require_record()?;
        let meta = self
            .clone_meta(name)?
            .with_context(|| format!("no clone named `{name}`"))?;
        record.backend.attach(
            &self.clone_dir(name).join(record.backend.file_name()),
            &meta.mount,
        )?;
        Ok(meta.path)
    }

    pub fn unmount(&self, name: &str, force: bool) -> Result<()> {
        let meta = self
            .clone_meta(name)?
            .with_context(|| format!("no clone named `{name}`"))?;
        backend::detach(&meta.mount, force)
    }

    pub fn info(&self, name: &str, status: bool) -> Result<CloneInfo> {
        let meta = self
            .clone_meta(name)?
            .with_context(|| format!("no clone named `{name}`"))?;
        let mounted = backend::is_mounted(&meta.mount);
        let head_file = std::fs::read_to_string(self.clone_admin(name).join("HEAD")).unwrap_or_default();
        let head_file = head_file.trim();
        let (branch, head) = match head_file.strip_prefix("ref: refs/heads/") {
            Some(b) => (
                Some(b.to_string()),
                git::out(
                    &self.root,
                    &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{b}")],
                )
                .ok(),
            ),
            None if !head_file.is_empty() => (None, Some(head_file.to_string())),
            None => (meta.branch.clone(), None),
        };
        let (mut dirty, mut ahead, mut behind) = (None, None, None);
        if status && mounted {
            dirty = git::is_dirty(&meta.path).ok();
            if let Ok(s) = git::out(
                &meta.path,
                &[
                    "rev-list",
                    "--left-right",
                    "--count",
                    &format!("{}...HEAD", meta.base),
                ],
            ) {
                let mut it = s.split_whitespace().filter_map(|n| n.parse().ok());
                behind = it.next();
                ahead = it.next();
            }
        }
        let image = self
            .clone_dir(name)
            .join(self.require_record()?.backend.file_name());
        Ok(CloneInfo {
            name: meta.name,
            path: meta.path,
            mounted,
            branch,
            head: head.map(|h| h.chars().take(12).collect()),
            dirty,
            ahead,
            behind,
            image_bytes: allocated_bytes(&image),
            own_bytes: crate::util::private_bytes(&image),
            created_at: meta.created_at,
            golden_generation: meta.golden_generation,
            timings: meta.timings,
            total_ms: meta.total_ms,
            failed_steps: meta.failed_steps,
        })
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
        bail!("no clone matches `{query}` (see `clove ls`)")
    }

    pub fn status(&self) -> Result<RepoStatus> {
        let record = self.record()?;
        let golden = match self.golden_meta()? {
            Some(meta) => {
                let behind = git::count_commits(&self.root, &format!("{}..{}", meta.commit, meta.base));
                let image = record
                    .as_ref()
                    .map(|r| self.golden_gen_dir(meta.generation).join(r.backend.file_name()));
                Some(GoldenStatus {
                    age_secs: now_unix().saturating_sub(meta.created_at),
                    behind,
                    image_bytes: image.map(|p| allocated_bytes(&p)).unwrap_or(0),
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
            clone_root: self.config.clone_root.clone(),
            base: self.config.base.clone(),
            backend: record.map(|r| r.backend),
            golden,
            mounted: clones.iter().filter(|c| c.mounted).count(),
            clones: clones.len(),
            steps: self.config.steps.clone(),
            steps_detected: self.config.steps_detected,
        })
    }

    /// Finds leftovers from interrupted operations. With `apply`, removes them.
    pub fn gc(&self, apply: bool) -> Result<Vec<String>> {
        let _lock = self.lock()?;
        let mut actions = vec![];
        let names = self.clone_names()?;

        if let Ok(rd) = std::fs::read_dir(self.state.join("clones")) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if !names.contains(&n) {
                    actions.push(format!("remove half-created clone {n}"));
                    if apply {
                        let _ = backend::detach(&self.clone_mount(&n), true);
                        std::fs::remove_dir_all(e.path())?;
                    }
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
        if let Ok(rd) = std::fs::read_dir(&self.config.clone_root) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                let empty = std::fs::read_dir(e.path())
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(false);
                if !names.contains(&n) && empty {
                    actions.push(format!("remove empty mount point {}", e.path().display()));
                    if apply {
                        let _ = std::fs::remove_dir(e.path());
                    }
                }
            }
        }
        if backend::is_mounted(&self.golden_mount())
            && let Ok(lock) = FileLock::try_acquire(&self.state.join("refresh.lock"))
        {
            actions.push("unmount golden image left mounted by an interrupted refresh".into());
            if apply {
                backend::detach(&self.golden_mount(), true)?;
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
        timed(r, &mut t, "delete golden", || {
            backend::detach(&self.golden_mount(), true)?;
            if let Some(rec) = self.record()? {
                let _ = std::fs::remove_dir_all(self.admin_dir(&rec.golden_admin));
            }
            std::fs::remove_dir_all(&self.state)?;
            Ok(())
        })
    }
}

fn admin_name_of(wt: &Path) -> Result<String> {
    let s = std::fs::read_to_string(wt.join(".git"))?;
    let p = s.trim().strip_prefix("gitdir: ").context("worktree .git file")?;
    Ok(Path::new(p)
        .file_name()
        .context("admin dir name")?
        .to_string_lossy()
        .to_string())
}
