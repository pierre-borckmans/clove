//! Tells a UI when a repo changed on disk, so it reloads right away instead of
//! polling often. It watches a few directories, each non-recursively, and only
//! the entries in them that clove state is made of: `.git/worktrees` (clones
//! added or removed by clove or plain git), `FETCH_HEAD`, and clove's state dir
//! (golden generations, clone records, the PR cache). Clone working trees are
//! never watched: builds there would flood it. FSEvents on macOS, inotify on Linux.

use crate::repo::{Clove, state_home};
use anyhow::Result;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Quiet time after the last relevant event before `on_change` runs, so one
/// `git worktree add` (several files) means one reload.
const DEBOUNCE: Duration = Duration::from_millis(150);
/// A steady stream of events still reloads this often.
const MAX_WAIT: Duration = Duration::from_secs(1);

/// One directory to watch and which of its entries count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub dir: PathBuf,
    /// Entry names that count; `None`: all of them.
    pub only: Option<&'static [&'static str]>,
}

impl Target {
    fn any(dir: PathBuf) -> Self {
        Self { dir, only: None }
    }
    fn only(dir: PathBuf, names: &'static [&'static str]) -> Self {
        Self {
            dir,
            only: Some(names),
        }
    }
    fn counts(&self, path: &Path) -> bool {
        if path == self.dir {
            return true;
        }
        if path.parent() != Some(self.dir.as_path()) {
            return false;
        }
        match (self.only, path.file_name()) {
            (None, _) => true,
            (Some(names), Some(n)) => names.iter().any(|w| n == *w),
            (Some(_), None) => false,
        }
    }
}

impl Clove {
    /// What to watch to see this repo change. Directories that don't exist yet
    /// are listed too; `sync` picks them up once they appear.
    pub fn watch_targets(&self) -> Vec<Target> {
        let mut t = vec![
            Target::only(
                self.common_dir.clone(),
                &["FETCH_HEAD", "packed-refs", "HEAD", "worktrees"],
            ),
            Target::any(self.common_dir.join("worktrees")),
            Target::only(
                self.state.clone(),
                &[
                    "repo.json",
                    "prune.json",
                    "refresh.lock",
                    "golden",
                    "clones",
                    "mount",
                ],
            ),
            Target::any(self.golden_dir()),
            Target::any(self.state.join("clones")),
        ];
        if let Ok(rd) = std::fs::read_dir(self.state.join("clones")) {
            for e in rd.flatten() {
                t.push(Target::only(e.path(), &["meta.json", crate::ops::PENDING_FILE]));
            }
        }
        t
    }
}

/// Repos set up or removed anywhere.
pub fn repos_target() -> Target {
    Target::any(state_home().join("repos"))
}

/// Every known repo's targets plus `repos_target`: for UIs that show all repos.
pub fn all_targets() -> Vec<Target> {
    let mut t = vec![repos_target()];
    for r in crate::repo::known_repos() {
        if let Ok(c) = Clove::open(&r.root) {
            t.extend(c.watch_targets());
        }
    }
    t
}

pub struct Watcher {
    inner: Mutex<Inner>,
    targets: Arc<Mutex<Vec<Target>>>,
}

struct Inner {
    w: RecommendedWatcher,
    /// Watched dir → its (device, inode), to rewatch a dir deleted and made again.
    watched: HashMap<PathBuf, (u64, u64)>,
}

impl Watcher {
    /// `on_change` runs on a background thread after relevant changes settle.
    pub fn new(on_change: impl Fn() + Send + 'static) -> Result<Self> {
        let (tx, rx) = channel::<notify::Result<notify::Event>>();
        let w = notify::recommended_watcher(tx)?;
        let targets: Arc<Mutex<Vec<Target>>> = Arc::default();
        let seen = targets.clone();
        let relevant = move |ev: &notify::Result<notify::Event>| {
            let Ok(ev) = ev else { return true };
            if matches!(
                ev.kind,
                EventKind::Access(_) | EventKind::Modify(notify::event::ModifyKind::Metadata(_))
            ) {
                return false;
            }
            let t = seen.lock().unwrap_or_else(|e| e.into_inner());
            ev.paths.iter().any(|p| t.iter().any(|t| t.counts(p)))
        };
        std::thread::Builder::new()
            .name("clove-watch".into())
            .spawn(move || {
                while let Ok(ev) = rx.recv() {
                    if !relevant(&ev) {
                        continue;
                    }
                    let start = Instant::now();
                    let mut quiet_until = start + DEBOUNCE;
                    loop {
                        let now = Instant::now();
                        let until = quiet_until.min(start + MAX_WAIT);
                        if now >= until {
                            break;
                        }
                        match rx.recv_timeout(until - now) {
                            Ok(ev) if relevant(&ev) => quiet_until = Instant::now() + DEBOUNCE,
                            Ok(_) => {}
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    on_change();
                }
            })?;
        Ok(Self {
            inner: Mutex::new(Inner {
                w,
                watched: HashMap::new(),
            }),
            targets,
        })
    }

    /// Watches exactly `targets`: adds dirs that appeared, rewatches ones made
    /// again, drops the rest. Cheap when nothing changed; call it after reloads.
    pub fn sync(&self, targets: Vec<Target>) {
        let targets: Vec<Target> = targets
            .into_iter()
            .map(|t| Target {
                dir: t.dir.canonicalize().unwrap_or(t.dir),
                only: t.only,
            })
            .collect();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Inner { w, watched } = &mut *inner;
        let want: HashMap<&Path, (u64, u64)> = targets
            .iter()
            .filter_map(|t| {
                let m = std::fs::metadata(&t.dir).ok().filter(|m| m.is_dir())?;
                Some((t.dir.as_path(), (m.dev(), m.ino())))
            })
            .collect();
        watched.retain(|dir, id| {
            if want.get(dir.as_path()) == Some(id) {
                return true;
            }
            let _ = w.unwatch(dir);
            false
        });
        for (dir, id) in want {
            if !watched.contains_key(dir) && w.watch(dir, RecursiveMode::NonRecursive).is_ok() {
                watched.insert(dir.to_path_buf(), id);
            }
        }
        *self.targets.lock().unwrap_or_else(|e| e.into_inner()) = targets;
    }

    /// Directories watched right now.
    pub fn watched(&self) -> Vec<PathBuf> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut v: Vec<PathBuf> = inner.watched.keys().cloned().collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Rig {
        _tmp: tempfile::TempDir,
        git: PathBuf,
        state: PathBuf,
        hits: Arc<AtomicUsize>,
        w: Watcher,
    }

    impl Rig {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let base = tmp.path().canonicalize().unwrap();
            let git = base.join("repo/.git");
            let state = base.join("state");
            std::fs::create_dir_all(git.join("worktrees")).unwrap();
            std::fs::create_dir_all(state.join("clones")).unwrap();
            let hits = Arc::new(AtomicUsize::new(0));
            let h = hits.clone();
            let w = Watcher::new(move || {
                h.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
            let r = Self {
                _tmp: tmp,
                git,
                state,
                hits,
                w,
            };
            r.sync();
            // FSEvents can deliver events from just before the stream started.
            std::thread::sleep(Duration::from_millis(300));
            r.hits.store(0, Ordering::SeqCst);
            r
        }
        fn targets(&self) -> Vec<Target> {
            vec![
                Target::only(self.git.clone(), &["FETCH_HEAD", "worktrees"]),
                Target::any(self.git.join("worktrees")),
                Target::only(self.state.clone(), &["repo.json", "clones"]),
                Target::any(self.state.join("clones")),
            ]
        }
        fn sync(&self) {
            self.w.sync(self.targets());
        }
        /// Runs `f`, then reports how many times `on_change` ran.
        fn after(&self, f: impl FnOnce()) -> usize {
            settle(&self.hits, f)
        }
    }

    /// Runs `f`, waits up to 3s for a first `on_change` (CI runners deliver
    /// FSEvents slowly), lets any extra ones land, and returns the count.
    fn settle(hits: &AtomicUsize, f: impl FnOnce()) -> usize {
        hits.store(0, Ordering::SeqCst);
        f();
        let start = std::time::Instant::now();
        while hits.load(Ordering::SeqCst) == 0 && start.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(MAX_WAIT);
        hits.load(Ordering::SeqCst)
    }

    #[test]
    fn sees_a_worktree_added_and_removed() {
        let r = Rig::new();
        let wt = r.git.join("worktrees/feat");
        let n = r.after(|| {
            std::fs::create_dir(&wt).unwrap();
            std::fs::write(wt.join("HEAD"), "ref: refs/heads/feat\n").unwrap();
            std::fs::write(wt.join("gitdir"), "/x/.git\n").unwrap();
        });
        // One add is several writes; the debounce makes it one or two reloads
        // (CI's FSEvents can split a burst), never one per file.
        assert!((1..=2).contains(&n), "{n}");
        assert!(r.after(|| std::fs::remove_dir_all(&wt).unwrap()) >= 1);
    }

    #[test]
    fn sees_fetch_and_state_files() {
        let r = Rig::new();
        // At least one each: CI runners can deliver an earlier event late.
        assert!(r.after(|| std::fs::write(r.git.join("FETCH_HEAD"), "x").unwrap()) >= 1);
        assert!(r.after(|| std::fs::write(r.state.join("repo.json"), "{}").unwrap()) >= 1);
        assert!(r.after(|| std::fs::create_dir(r.state.join("clones/a")).unwrap()) >= 1);
    }

    #[test]
    fn ignores_churn_that_does_not_matter() {
        let r = Rig::new();
        let n = r.after(|| {
            // git touches these on every status; locks are probed on every load.
            std::fs::write(r.git.join("index"), "x").unwrap();
            std::fs::write(r.git.join("index.lock"), "x").unwrap();
            std::fs::write(r.state.join("lock"), "").unwrap();
            // A clone's own git dir: not a direct entry of `worktrees`.
            std::fs::create_dir_all(r.git.join("worktrees/feat-x")).ok();
        });
        // Creating worktrees/feat-x counts; nothing else does.
        assert!(n >= 1);
        // Let late events from the writes above land before counting again.
        std::thread::sleep(Duration::from_secs(1));
        let n = r.after(|| {
            std::fs::write(r.git.join("worktrees/feat-x/index"), "x").unwrap();
            std::fs::write(r.git.join("worktrees/feat-x/HEAD"), "x").unwrap();
            let _ = std::fs::read_to_string(r.state.join("lock"));
        });
        assert_eq!(n, 0, "nested and read-only activity is ignored");
    }

    #[test]
    fn picks_up_dirs_that_appear_or_are_made_again() {
        let r = Rig::new();
        std::fs::remove_dir_all(&r.state).unwrap();
        r.sync();
        assert!(!r.w.watched().contains(&r.state));
        std::fs::create_dir_all(r.state.join("clones")).unwrap();
        r.sync();
        assert!(r.w.watched().contains(&r.state.join("clones")));
        // A fresh watch can replay the dir's own creation too: at least one.
        let n = r.after(|| std::fs::write(r.state.join("repo.json"), "{}").unwrap());
        assert!(n >= 1, "{n}");
    }

    /// Real git against `Clove::watch_targets`: what agents and users actually run.
    #[test]
    fn sees_real_git_worktrees_and_fetches() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let git = |dir: &Path, args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "init.defaultBranch=main",
                ])
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        let remote = base.join("remote.git");
        std::fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "-q", "--bare"]);
        let root = base.join("repo");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q"]);
        git(&root, &["commit", "-q", "--allow-empty", "-m", "a"]);
        git(&root, &["remote", "add", "origin", remote.to_str().unwrap()]);
        git(&root, &["push", "-q", "origin", "main"]);

        let c = Clove::open(&root).unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        let w = Watcher::new(move || {
            h.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        w.sync(c.watch_targets());
        std::thread::sleep(Duration::from_millis(300));
        let after = |f: &dyn Fn()| settle(&hits, f);
        let wt = base.join("wt");
        let wt_s = wt.to_str().unwrap().to_string();
        // The first worktree creates `.git/worktrees`, which then gets watched.
        assert!(after(&|| git(&root, &["worktree", "add", "-q", "-b", "x", &wt_s])) >= 1);
        w.sync(c.watch_targets());
        assert!(after(&|| git(&root, &["worktree", "remove", &wt_s])) >= 1);
        assert!(after(&|| git(&root, &["worktree", "add", "-q", "-b", "y", &wt_s])) >= 1);
        assert!(after(&|| git(&root, &["fetch", "-q", "origin"])) >= 1);
        // Work inside the worktree and plain status in the main checkout: no reloads.
        let n = after(&|| {
            std::fs::write(wt.join("f"), "x").unwrap();
            git(&wt, &["add", "f"]);
            git(&wt, &["status", "-s"]);
            git(&root, &["status", "-s"]);
        });
        assert_eq!(n, 0, "edits and status are not repo changes");
    }

    #[test]
    fn target_matching() {
        let t = Target::only(PathBuf::from("/r/.git"), &["FETCH_HEAD"]);
        assert!(t.counts(Path::new("/r/.git/FETCH_HEAD")));
        assert!(t.counts(Path::new("/r/.git")));
        assert!(!t.counts(Path::new("/r/.git/index")));
        assert!(!t.counts(Path::new("/r/.git/x/FETCH_HEAD")));
        assert!(Target::any(PathBuf::from("/s")).counts(Path::new("/s/anything")));
    }
}
