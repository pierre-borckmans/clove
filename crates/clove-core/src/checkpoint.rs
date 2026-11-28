//! Checkpoints and forks: O(1) copies of a clone as it is right now.
//!
//! A checkpoint has two halves:
//! - the clone's storage, frozen by the backend (a `clonefile` of the image
//!   on macOS, a read-only btrfs snapshot on Linux): tracked, untracked and
//!   ignored files alike;
//! - the clone's git state, which lives outside the image, in the main
//!   repo's `.git/worktrees/clove-<name>/` (HEAD, index, reflog, rebase or
//!   merge state). It is copied file by file; it's small.
//!
//! The branch ref lives in the main repo too, so a checkpoint records HEAD's
//! commit, and a rewind moves the branch back to it. A ref
//! `refs/clove/checkpoints/<clone>/<id>` keeps that commit from `git gc`.
//!
//! A fork is a new clone whose image is a copy of another clone's, with a
//! copy of its index and a new branch at its HEAD.
//!
//! Layout: `clones/<name>/checkpoints/<id>/{meta.json, git/, image (macOS)}`.

use crate::events::{Reporter, Timing, timed};
use crate::git;
use crate::ops::{drop_admin_registration, lock_reason, stop_daemons};
use crate::repo::{CloneInfo, CloneMeta, Clove, read_json, sanitize_name, write_json};
use crate::util::{now_unix, write_atomic};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where a rewind keeps the state it replaced, so the rewind can be undone.
pub const BEFORE_REWIND: &str = "before-rewind";
const SAVING: &str = "before-rewind-saving";

/// Files of a worktree admin dir that name the worktree, not its state.
const ADMIN_IDENTITY: &[&str] = &["gitdir", "commondir", "locked"];
/// State a fork starts without: the source's reflog and lock files.
const FORK_SKIP: &[&str] = &["logs", "index.lock", "HEAD.lock", "FETCH_HEAD"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMeta {
    pub id: String,
    pub clone: String,
    pub created_at: u64,
    /// Orders checkpoints made within the same second.
    #[serde(default)]
    pub created_ms: u64,
    /// The commit HEAD was at (None on an unborn branch).
    pub head: Option<String>,
    /// The branch HEAD was on (None when detached).
    pub branch: Option<String>,
    /// Made by clove before a rewind, not asked for.
    #[serde(default)]
    pub auto: bool,
    pub took_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckpointInfo {
    #[serde(flatten)]
    pub meta: CheckpointMeta,
    /// Bytes only this checkpoint holds: what dropping it frees. Starts near
    /// zero and grows as the clone moves away from it.
    pub own_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct RewindOptions {
    /// Checkpoint id; None: the newest one you made.
    pub id: Option<String>,
    /// Unmount even if processes have files open in the clone.
    pub force: bool,
    /// Don't keep the replaced state as `before-rewind`.
    pub no_save: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rewound {
    pub clone: String,
    pub to: String,
    /// `before-rewind` when the replaced state was kept.
    pub saved: Option<String>,
    /// The branch, and the commit it moved from, when the rewind moved it.
    pub branch_moved: Option<(String, String)>,
    pub timings: Vec<Timing>,
    pub total_ms: f64,
}

#[derive(Debug, Clone, Default)]
pub struct ForkOptions {
    /// New clone names, one fork each.
    pub names: Vec<String>,
    /// The fork's branch (one fork only). Default `<source branch>-<name>`.
    pub branch: Option<String>,
    /// Detached HEAD at the source's commit instead of a branch.
    pub detach: bool,
}

/// What `fork` will make for one name.
struct Plan {
    name: String,
    branch: Option<String>,
    mount: PathBuf,
    wt: PathBuf,
}

impl Clove {
    /// Freezes clone `name` as it is now: files (ignored ones too) and git
    /// state. `label` names it; default `cp-N`. Works on a mounted clone in
    /// use: what processes wrote is flushed first, and the copy is what a
    /// power cut at that instant would leave (buffers still inside a
    /// process are not on disk yet, so aren't in it).
    pub fn checkpoint(&self, name: &str, label: Option<&str>, r: &dyn Reporter) -> Result<CheckpointInfo> {
        let meta = self.clone_meta(name)?.ok_or_else(|| self.no_clone(name))?;
        let id = match label {
            Some(l) => {
                let id = sanitize_name(l);
                if id.is_empty() || id != l {
                    bail!("checkpoint names take letters, digits, `.`, `_` and `-` (try `{id}`)");
                }
                if id == BEFORE_REWIND || id == SAVING {
                    bail!("`{id}` is kept for the state a rewind replaces");
                }
                id
            }
            None => self.next_checkpoint_id(name),
        };
        if self.checkpoint_dir(name, &id).exists() {
            bail!("`{name}` already has a checkpoint `{id}`; `clove checkpoint {name} {id} --drop` first");
        }
        let _lock = self.lock()?;
        let cp = self.make_checkpoint(name, &meta, &id, false, r, &mut vec![])?;
        Ok(self.checkpoint_info(cp))
    }

    /// Checkpoints of clone `name`, oldest first.
    pub fn checkpoints(&self, name: &str) -> Result<Vec<CheckpointInfo>> {
        if self.clone_meta(name)?.is_none() {
            return Err(self.no_clone(name));
        }
        Ok(self
            .checkpoint_metas(name)
            .into_iter()
            .map(|m| self.checkpoint_info(m))
            .collect())
    }

    /// Deletes checkpoint `id` of clone `name`.
    pub fn drop_checkpoint(&self, name: &str, id: &str) -> Result<()> {
        let dir = self.checkpoint_dir(name, id);
        if sanitize_name(id) != id || !dir.join("meta.json").exists() {
            bail!("`{name}` has no checkpoint `{id}` (`clove checkpoints {name}`)");
        }
        let _lock = self.lock()?;
        self.store()?.checkpoint_delete(self, name, id)?;
        std::fs::remove_dir_all(&dir).with_context(|| format!("remove {}", dir.display()))?;
        self.delete_checkpoint_ref(name, id);
        Ok(())
    }

    /// Restores clone `name` to a checkpoint: files, index, HEAD, and the
    /// branch's commit. The clone is unmounted and mounted again, so nothing
    /// may be using it, unless `force` (they lose their open files; a shell
    /// inside needs `cd $PWD`). The state it replaces is kept as
    /// `before-rewind`, so `clove rewind <clone> before-rewind` undoes it.
    pub fn rewind(&self, name: &str, opts: &RewindOptions, r: &dyn Reporter) -> Result<Rewound> {
        let started = std::time::Instant::now();
        let meta = self.clone_meta(name)?.ok_or_else(|| self.no_clone(name))?;
        let target = match &opts.id {
            Some(id) => self
                .checkpoint_metas(name)
                .into_iter()
                .find(|m| &m.id == id)
                .with_context(|| format!("`{name}` has no checkpoint `{id}` (`clove checkpoints {name}`)"))?,
            None => self
                .checkpoint_metas(name)
                .into_iter()
                .rfind(|m| !m.auto)
                .with_context(|| {
                    format!("`{name}` has no checkpoints; `clove checkpoint {name}` makes one")
                })?,
        };
        let store = self.store()?;
        let mut timings = vec![];
        let _lock = self.lock()?;
        let admin = self.clone_admin(name);
        if admin.join("index.lock").exists() {
            bail!("a git command is running in `{name}` (index.lock); try again when it's done");
        }
        // Before saving anything: a busy clone would fail the unmount anyway.
        if !opts.force && store.clone_attached(self, name, &meta.mount) {
            stop_daemons(&meta.path, &meta.mount);
            let who = crate::backend::users(&meta.mount);
            if !who.is_empty() {
                bail!("{}", crate::backend::busy_error(&who));
            }
        }
        let save = !opts.no_save;
        if save {
            let _ = self.forget_checkpoint(name, SAVING);
            self.make_checkpoint(name, &meta, SAVING, true, r, &mut timings)?;
        }
        stop_daemons(&meta.path, &meta.mount);
        let restored = timed(r, &mut timings, "restore image", || {
            store.checkpoint_restore(self, name, &meta.mount, &target.id, opts.force)
        });
        if let Err(e) = restored {
            if save {
                let _ = self.forget_checkpoint(name, SAVING);
            }
            return Err(e);
        }
        let branch_moved = timed(r, &mut timings, "restore git", || self.restore_git(name, &target))?;
        if save {
            let _ = self.forget_checkpoint(name, BEFORE_REWIND);
            store.checkpoint_rename(self, name, SAVING, BEFORE_REWIND)?;
            let (from, to) = (
                self.checkpoint_dir(name, SAVING),
                self.checkpoint_dir(name, BEFORE_REWIND),
            );
            std::fs::rename(&from, &to)?;
            if let Some(mut m) = read_json::<CheckpointMeta>(&to.join("meta.json"))? {
                m.id = BEFORE_REWIND.into();
                write_json(&to.join("meta.json"), &m)?;
                if let Some(h) = &m.head {
                    self.write_checkpoint_ref(name, BEFORE_REWIND, h);
                }
            }
            self.delete_checkpoint_ref(name, SAVING);
        }
        let _ = self.write_clone_env(&meta.path, name);
        Ok(Rewound {
            clone: name.to_string(),
            to: target.id,
            saved: save.then(|| BEFORE_REWIND.to_string()),
            branch_moved,
            timings,
            total_ms: started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// New clones copied from clone `from` as it is now: uncommitted and
    /// ignored files, index and all, each on its own new branch at `from`'s
    /// HEAD (default `<from's branch>-<suffix>`, see [`fork_suffix`]). Several names share one flush
    /// of the source, so they start identical.
    pub fn fork(&self, from: &str, opts: &ForkOptions, r: &dyn Reporter) -> Result<Vec<CloneInfo>> {
        let started = std::time::Instant::now();
        let record = self.require_record()?;
        let store = record.backend.store()?;
        let src = self.clone_meta(from)?.ok_or_else(|| self.no_clone(from))?;
        if opts.names.is_empty() {
            bail!("name at least one fork");
        }
        if opts.branch.is_some() && opts.names.len() > 1 {
            bail!("--branch names one fork's branch; several forks get one branch each");
        }
        let src_admin = self.clone_admin(from);
        if let Some(op) = crate::update::operation(&src_admin) {
            bail!("`{from}` is stopped in a {op:?}; finish or abort it before forking");
        }
        if src_admin.join("index.lock").exists() {
            bail!("a git command is running in `{from}` (index.lock); try again when it's done");
        }
        let head_file = std::fs::read_to_string(src_admin.join("HEAD"))
            .with_context(|| format!("read {}", src_admin.join("HEAD").display()))?;
        let src_branch = head_file
            .trim()
            .strip_prefix("ref: refs/heads/")
            .map(str::to_string);
        let head = match &src_branch {
            Some(b) => git::commit_of(&self.root, &format!("refs/heads/{b}")),
            None => Some(head_file.trim().to_string()),
        }
        .with_context(|| format!("`{from}` has no commit yet to fork from"))?;

        let mut plans: Vec<Plan> = vec![];
        for n in &opts.names {
            let name = sanitize_name(n);
            if name.is_empty() || name != *n {
                bail!("clone names take letters, digits, `.`, `_` and `-` (try `{name}`)");
            }
            if plans.iter().any(|p| p.name == name) {
                bail!("`{name}` is named twice");
            }
            if self.clone_meta(&name)?.is_some() || self.clone_dir(&name).exists() {
                bail!("clone `{name}` already exists");
            }
            if self.clone_admin(&name).exists() {
                bail!("Git worktree registration `clove-{name}` already exists");
            }
            let mount = self.clone_mount(&name);
            if mount.exists() && std::fs::read_dir(&mount)?.next().is_some() {
                bail!("{} exists and is not empty", mount.display());
            }
            let branch = (!opts.detach).then(|| {
                opts.branch.clone().unwrap_or_else(|| match &src_branch {
                    Some(b) => format!("{b}-{}", fork_suffix(from, &name)),
                    None => name.clone(),
                })
            });
            if let Some(b) = &branch {
                if !git::ok(&self.root, &["check-ref-format", "--branch", b]) {
                    bail!("`{b}` is not a valid branch name");
                }
                if git::ref_exists(&self.root, &format!("refs/heads/{b}")) {
                    bail!("branch `{b}` already exists; pass --branch");
                }
            }
            plans.push(Plan {
                wt: self.clone_worktree(&name),
                mount,
                branch,
                name,
            });
        }

        let undo = |p: &Plan| {
            let _ = store.clone_detach(self, &p.name, &p.mount, true);
            let _ = store.clone_delete(self, &p.name, &p.mount);
            let _ = drop_admin_registration(&self.clone_admin(&p.name), &p.wt);
            let _ = std::fs::remove_dir(&p.mount);
            let _ = std::fs::remove_dir_all(self.clone_dir(&p.name));
            if let Some(b) = &p.branch
                && git::commit_of(&self.root, &format!("refs/heads/{b}")).as_deref() == Some(head.as_str())
            {
                let _ = git::run_ok(&self.root, &["branch", "-D", b]);
            }
        };
        let undo_all = |e: anyhow::Error| {
            plans.iter().for_each(undo);
            e
        };

        let mut timings = vec![];
        // The source's image and index, copied under the lock so two forks
        // and a checkpoint can't interleave.
        {
            let _lock = self.lock()?;
            timed(r, &mut timings, "copy image", || {
                for p in &plans {
                    self.reserve_clone(&p.name, None)?;
                    store.clone_fork(self, from, &src.mount, &p.name, &p.mount)?;
                }
                Ok(())
            })
            .map_err(undo_all)?;
            timed(r, &mut timings, "copy git state", || {
                for p in &plans {
                    copy_admin_state(&src_admin, &self.clone_admin(&p.name), FORK_SKIP)?;
                }
                Ok(())
            })
            .map_err(undo_all)?;
        }
        // ~0.7s each on macOS; side by side, N forks take about as long as one.
        timed(r, &mut timings, "mount", || {
            std::thread::scope(|s| {
                let handles: Vec<_> = plans
                    .iter()
                    .map(|p| s.spawn(|| store.clone_attach(self, &p.name, &p.mount)))
                    .collect();
                handles
                    .into_iter()
                    .map(|h| {
                        h.join()
                            .unwrap_or_else(|_| Err(anyhow::anyhow!("mount panicked")))
                    })
                    .collect::<Result<Vec<()>>>()
            })
        })
        .map_err(undo_all)?;
        timed(r, &mut timings, "register", || {
            for p in &plans {
                self.register_fork(p, &head)?;
            }
            Ok(())
        })
        .map_err(undo_all)?;
        // As after `new`: paths baked into ignored files (venvs) name the source.
        let with_setup = self.config.steps.iter().filter(|s| s.setup.is_some());
        for t in crate::toolchains::used(with_setup) {
            if let Some(f) = t.before_setup {
                let _ = timed(r, &mut timings, f.label, || {
                    plans.iter().for_each(|p| {
                        (f.run)(&p.wt, t.files);
                    });
                    Ok(())
                });
            }
        }
        let total_ms = started.elapsed().as_secs_f64() * 1000.0;
        let mut out = vec![];
        for p in &plans {
            let _ = self.write_clone_env(&p.wt, &p.name);
            let meta = CloneMeta {
                name: p.name.clone(),
                branch: p.branch.clone(),
                path: p.wt.clone(),
                mount: p.mount.clone(),
                created_at: now_unix(),
                timings: timings.clone(),
                total_ms,
                forked_from: Some(from.to_string()),
                ..src.clone()
            };
            write_json(&self.clone_dir(&p.name).join("meta.json"), &meta).map_err(undo_all)?;
            self.record_created(&meta);
            out.push(self.info(&p.name, false)?);
        }
        Ok(out)
    }

    /// One fork of `from` on a generated name: `<from>-a`, `-b`, ... on
    /// branch `<from's branch>-a`, the first letter free for both.
    pub fn fork_auto(&self, from: &str, r: &dyn Reporter) -> Result<CloneInfo> {
        if self.clone_meta(from)?.is_none() {
            return Err(self.no_clone(from));
        }
        let head = std::fs::read_to_string(self.clone_admin(from).join("HEAD")).unwrap_or_default();
        let src_branch = head.trim().strip_prefix("ref: refs/heads/").map(str::to_string);
        let suffixes = (b'a'..=b'z')
            .map(|c| (c as char).to_string())
            .chain((2..100).map(|n| n.to_string()));
        for s in suffixes {
            let name = format!("{from}-{s}");
            let branch = match &src_branch {
                Some(b) => format!("{b}-{s}"),
                None => name.clone(),
            };
            let taken = self.clone_meta(&name)?.is_some()
                || self.clone_dir(&name).exists()
                || self.clone_admin(&name).exists()
                || git::ref_exists(&self.root, &format!("refs/heads/{branch}"));
            if taken {
                continue;
            }
            let opts = ForkOptions {
                names: vec![name],
                branch: Some(branch),
                detach: false,
            };
            return self.fork(from, &opts, r)?.pop().context("fork made nothing");
        }
        bail!("no free fork name for `{from}`")
    }

    /// Admin dir for a fork whose state was copied in: its own identity, its
    /// own branch, and the worktree's `.git` file pointing at it (the copied
    /// image still points at the source's).
    fn register_fork(&self, p: &Plan, head: &str) -> Result<()> {
        let dir = self.clone_admin(&p.name);
        std::fs::write(dir.join("gitdir"), format!("{}\n", p.wt.join(".git").display()))?;
        std::fs::write(dir.join("commondir"), "../..\n")?;
        std::fs::write(dir.join("locked"), lock_reason(&p.name))?;
        write_atomic(
            &p.wt.join(".git"),
            format!("gitdir: {}\n", dir.display()).as_bytes(),
        )?;
        match &p.branch {
            Some(b) => {
                git::run_ok(&self.root, &["branch", "--no-track", b, head])?;
                let why = format!("clove fork: {head} as {b}");
                git::run_ok(
                    &p.wt,
                    &["symbolic-ref", "-m", &why, "HEAD", &format!("refs/heads/{b}")],
                )
            }
            None => Ok(std::fs::write(dir.join("HEAD"), format!("{head}\n"))?),
        }
    }

    /// Storage, git state and meta for checkpoint `id`. Called under the lock.
    fn make_checkpoint(
        &self,
        name: &str,
        meta: &CloneMeta,
        id: &str,
        auto: bool,
        r: &dyn Reporter,
        timings: &mut Vec<Timing>,
    ) -> Result<CheckpointMeta> {
        let started = std::time::Instant::now();
        let admin = self.clone_admin(name);
        if admin.join("index.lock").exists() {
            bail!("a git command is running in `{name}` (index.lock); try again when it's done");
        }
        let dir = self.checkpoint_dir(name, id);
        std::fs::create_dir_all(&dir)?;
        let store = self.store()?;
        let made = (|| -> Result<CheckpointMeta> {
            // Git first: a commit landing in between leaves the image newer
            // than HEAD, which reads as uncommitted changes, never lost ones.
            timed(r, timings, "save git state", || {
                copy_admin_state(&admin, &dir.join("git"), &["index.lock", "HEAD.lock"])
            })?;
            let head_file = std::fs::read_to_string(dir.join("git/HEAD")).unwrap_or_default();
            let branch = head_file
                .trim()
                .strip_prefix("ref: refs/heads/")
                .map(str::to_string);
            let head = match &branch {
                Some(b) => git::commit_of(&self.root, &format!("refs/heads/{b}")),
                None => Some(head_file.trim().to_string()).filter(|h| !h.is_empty()),
            };
            timed(r, timings, "freeze image", || {
                store.checkpoint_create(self, name, &meta.mount, id)
            })?;
            let cp = CheckpointMeta {
                id: id.to_string(),
                clone: name.to_string(),
                created_at: now_unix(),
                created_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as u64),
                head,
                branch,
                auto,
                took_ms: started.elapsed().as_secs_f64() * 1000.0,
            };
            write_json(&dir.join("meta.json"), &cp)?;
            if let Some(h) = &cp.head {
                self.write_checkpoint_ref(name, id, h);
            }
            Ok(cp)
        })();
        if made.is_err() {
            let _ = store.checkpoint_delete(self, name, id);
            let _ = std::fs::remove_dir_all(&dir);
        }
        made
    }

    /// Puts the checkpoint's git state back, and its branch at its commit.
    /// Returns the branch and where it was, when it moved.
    fn restore_git(&self, name: &str, cp: &CheckpointMeta) -> Result<Option<(String, String)>> {
        let admin = self.clone_admin(name);
        for e in std::fs::read_dir(&admin)?.flatten() {
            let n = e.file_name();
            if ADMIN_IDENTITY.iter().any(|k| n == *k) {
                continue;
            }
            let p = e.path();
            if p.is_dir() && !p.is_symlink() {
                std::fs::remove_dir_all(&p)?;
            } else {
                std::fs::remove_file(&p)?;
            }
        }
        copy_admin_state(&self.checkpoint_dir(name, &cp.id).join("git"), &admin, &[])?;
        let (Some(b), Some(h)) = (&cp.branch, &cp.head) else {
            return Ok(None);
        };
        let r = format!("refs/heads/{b}");
        let now = git::commit_of(&self.root, &r);
        if now.as_deref() == Some(h.as_str()) {
            return Ok(None);
        }
        let why = format!("clove rewind {name} to {}", cp.id);
        git::run_ok(&self.root, &["update-ref", "-m", &why, &r, h])?;
        Ok(Some((b.clone(), now.unwrap_or_default())))
    }

    fn checkpoint_metas(&self, name: &str) -> Vec<CheckpointMeta> {
        let mut v: Vec<CheckpointMeta> = std::fs::read_dir(self.checkpoints_dir(name))
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| read_json(&e.path().join("meta.json")).ok().flatten())
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| (a.created_at, a.created_ms, &a.id).cmp(&(b.created_at, b.created_ms, &b.id)));
        v
    }

    /// How many checkpoints clone `name` has, without reading them.
    pub fn checkpoint_count(&self, name: &str) -> u32 {
        std::fs::read_dir(self.checkpoints_dir(name)).map_or(0, |rd| {
            rd.flatten()
                .filter(|e| e.file_name() != SAVING && e.path().join("meta.json").exists())
                .count() as u32
        })
    }

    fn checkpoint_info(&self, meta: CheckpointMeta) -> CheckpointInfo {
        let own_bytes = self
            .store()
            .map_or(0, |s| s.checkpoint_bytes(self, &meta.clone, &meta.id));
        CheckpointInfo { meta, own_bytes }
    }

    fn next_checkpoint_id(&self, name: &str) -> String {
        let n = self
            .checkpoint_metas(name)
            .iter()
            .filter_map(|m| m.id.strip_prefix("cp-")?.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        format!("cp-{}", n + 1)
    }

    /// Deletes checkpoint `id`'s storage, state dir and ref, if there.
    fn forget_checkpoint(&self, name: &str, id: &str) -> Result<()> {
        let dir = self.checkpoint_dir(name, id);
        if !dir.exists() {
            return Ok(());
        }
        self.store()?.checkpoint_delete(self, name, id)?;
        std::fs::remove_dir_all(&dir)?;
        self.delete_checkpoint_ref(name, id);
        Ok(())
    }

    /// Before a clone goes: its checkpoints' storage outside the state dir,
    /// and their refs. Best effort.
    pub(crate) fn forget_checkpoints(&self, name: &str) {
        let Ok(rd) = std::fs::read_dir(self.checkpoints_dir(name)) else {
            return;
        };
        for e in rd.flatten() {
            let _ = self.forget_checkpoint(name, &e.file_name().to_string_lossy());
        }
    }

    fn checkpoint_ref(name: &str, id: &str) -> String {
        format!("refs/clove/checkpoints/{name}/{id}")
    }

    fn write_checkpoint_ref(&self, name: &str, id: &str, commit: &str) {
        let _ = git::run_ok(
            &self.root,
            &["update-ref", &Self::checkpoint_ref(name, id), commit],
        );
    }

    fn delete_checkpoint_ref(&self, name: &str, id: &str) {
        let r = Self::checkpoint_ref(name, id);
        if git::ref_exists(&self.root, &r) {
            let _ = git::run_ok(&self.root, &["update-ref", "-d", &r]);
        }
    }
}

/// Copies a worktree admin dir's state (everything but its identity files
/// and `skip`) into `to`, recursively. Clones blocks on APFS.
fn copy_admin_state(from: &Path, to: &Path, skip: &[&str]) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from).with_context(|| format!("read {}", from.display()))? {
        let e = e?;
        let n = e.file_name();
        if ADMIN_IDENTITY.iter().chain(skip).any(|k| n == *k) {
            continue;
        }
        copy_tree(&e.path(), &to.join(&n))?;
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let m = from.symlink_metadata()?;
    if m.is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(from)?, to)?;
    } else if m.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy_tree(&e.path(), &to.join(e.file_name()))?;
        }
    } else {
        std::fs::copy(from, to).with_context(|| format!("copy {}", from.display()))?;
    }
    Ok(())
}

/// The part of a fork's name its default branch adds to the source branch:
/// the name minus the source's `<from>-` prefix (`feat-x-b` from `feat-x`
/// gives `b`), or the whole name when it doesn't start with it.
pub fn fork_suffix<'a>(from: &str, name: &'a str) -> &'a str {
    name.strip_prefix(from)
        .and_then(|s| s.strip_prefix('-'))
        .filter(|s| !s.is_empty())
        .unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_branch_suffix_drops_the_source_name() {
        assert_eq!(fork_suffix("glad-juniper", "glad-juniper-a"), "a");
        assert_eq!(fork_suffix("feat-search", "feat-search-b"), "b");
        assert_eq!(fork_suffix("glad-juniper-a", "glad-juniper-a-a"), "a");
        // No shared prefix, or nothing after it: the whole name.
        assert_eq!(fork_suffix("feat-x", "spike"), "spike");
        assert_eq!(fork_suffix("feat-x", "feat-xy"), "feat-xy");
        assert_eq!(fork_suffix("feat-x", "feat-x-"), "feat-x-");
    }

    #[test]
    fn copies_admin_state_but_not_identity() {
        let t = tempfile::tempdir().unwrap();
        let (a, b) = (t.path().join("a"), t.path().join("b"));
        std::fs::create_dir_all(a.join("logs")).unwrap();
        for f in [
            "HEAD",
            "index",
            "gitdir",
            "commondir",
            "locked",
            "logs/HEAD",
            "index.lock",
        ] {
            std::fs::write(a.join(f), f).unwrap();
        }
        copy_admin_state(&a, &b, FORK_SKIP).unwrap();
        let mut got: Vec<String> = std::fs::read_dir(&b)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        got.sort();
        assert_eq!(got, ["HEAD", "index"]);
    }
}
