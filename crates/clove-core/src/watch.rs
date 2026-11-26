//! Tells a UI when a repo changed on disk, so it reloads right away instead of
//! polling often. It watches a few directories, each non-recursively, and only
//! the entries in them that clove state is made of: `.git/worktrees` (clones
//! added or removed by clove or plain git), `FETCH_HEAD`, and clove's state dir
//! (golden generations, clone records, the PR cache). FSEvents on macOS,
//! inotify on Linux.
//!
//! On macOS it also watches each worktree's tree recursively (FSEvents is
//! recursive at no extra cost), so cached `git status` answers (see `memo`)
//! are reused until something in the tree changes. Build output dirs don't
//! count. inotify needs one watch per directory, so Linux relies on the memo's
//! short expiry instead.

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

/// What a report covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    /// Only edits inside working trees: clone and worktree status may have
    /// changed, nothing else (no clone added, no ref moved). Nothing new to watch.
    pub trees_only: bool,
}

/// What an event means, in rising order of urgency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Hit {
    None,
    /// An edit in a working tree.
    Tree,
    /// Clove or git state: worktrees, refs, golden images.
    State,
}

/// One directory to watch and which of its entries count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub dir: PathBuf,
    /// Entry names that count; `None`: all of them.
    pub only: Option<&'static [&'static str]>,
    /// Watch everything below, not just the entries (macOS only).
    pub tree: bool,
}

/// Path components whose changes never show in `git status` in practice.
const NOISE: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".turbo",
    ".next",
    ".nuxt",
    ".cache",
    ".parcel-cache",
    ".venv",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".gradle",
    ".DS_Store",
];

/// Whether recursive tree watching is used here.
pub const TREES: bool = cfg!(target_os = "macos");

impl Target {
    fn any(dir: PathBuf) -> Self {
        Self {
            dir,
            only: None,
            tree: false,
        }
    }
    fn only(dir: PathBuf, names: &'static [&'static str]) -> Self {
        Self {
            dir,
            only: Some(names),
            tree: false,
        }
    }
    /// A worktree: any change below it, outside build output.
    pub fn tree(dir: PathBuf) -> Self {
        Self {
            dir,
            only: None,
            tree: true,
        }
    }
    fn counts(&self, path: &Path) -> bool {
        if self.tree {
            return path.strip_prefix(&self.dir).is_ok_and(|rel| {
                !rel.components()
                    .any(|c| NOISE.iter().any(|n| c.as_os_str() == *n))
            });
        }
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
        t.extend(self.worktree_trees().into_iter().map(Target::tree));
        t
    }

    /// Working trees of the repo's linked worktrees, clones included, from
    /// their admin dirs (no git call). Not the golden one: refreshes build there.
    fn worktree_trees(&self) -> Vec<PathBuf> {
        let golden = self.record().ok().flatten().map(|r| r.golden_admin);
        let Ok(rd) = std::fs::read_dir(self.common_dir.join("worktrees")) else {
            return vec![];
        };
        rd.flatten()
            .filter(|e| golden.as_deref() != Some(&*e.file_name().to_string_lossy()))
            .filter_map(|e| {
                let g = std::fs::read_to_string(e.path().join("gitdir")).ok()?;
                Some(Path::new(g.trim()).parent()?.to_path_buf())
            })
            .collect()
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
    pub fn new(on_change: impl Fn(Change) + Send + 'static) -> Result<Self> {
        let (tx, rx) = channel::<notify::Result<notify::Event>>();
        let w = notify::recommended_watcher(tx)?;
        let targets: Arc<Mutex<Vec<Target>>> = Arc::default();
        let seen = targets.clone();
        let relevant = move |ev: &notify::Result<notify::Event>| {
            let Ok(ev) = ev else { return Hit::State };
            if matches!(
                ev.kind,
                EventKind::Access(_) | EventKind::Modify(notify::event::ModifyKind::Metadata(_))
            ) {
                return Hit::None;
            }
            let t = seen.lock().unwrap_or_else(|e| e.into_inner());
            let mut hit = Hit::None;
            let mut first = None;
            for p in &ev.paths {
                for t in t.iter().filter(|t| t.counts(p)) {
                    if t.tree {
                        crate::memo::invalidate_under(p);
                        hit = hit.max(Hit::Tree);
                    } else {
                        hit = Hit::State;
                    }
                    first.get_or_insert(p);
                }
            }
            if let (Some(p), Some(log)) = (first, std::env::var_os("CLOVE_WATCH_LOG")) {
                // Debug aid: which change woke the watcher.
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log) {
                    let _ = writeln!(f, "{} {:?} {}", crate::util::now_unix(), ev.kind, p.display());
                }
            }
            hit
        };
        std::thread::Builder::new()
            .name("clove-watch".into())
            .spawn(move || {
                let mut last_fire: Option<Instant> = None;
                // Edits held back (see below) are reported then.
                let mut trailing: Option<Instant> = None;
                loop {
                    let ev = match trailing {
                        Some(at) => match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                            Ok(ev) => ev,
                            Err(RecvTimeoutError::Timeout) => {
                                trailing = None;
                                last_fire = Some(Instant::now());
                                on_change(Change { trees_only: true });
                                continue;
                            }
                            Err(RecvTimeoutError::Disconnected) => return,
                        },
                        None => match rx.recv() {
                            Ok(ev) => ev,
                            Err(_) => return,
                        },
                    };
                    let mut hit = relevant(&ev);
                    if hit == Hit::None {
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
                            Ok(ev) => {
                                let h = relevant(&ev);
                                if h != Hit::None {
                                    hit = hit.max(h);
                                    quiet_until = Instant::now() + DEBOUNCE;
                                }
                            }
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    // Edits in a working tree: their status is recomputed at
                    // most every MIN_RECHECK, so reporting them more often
                    // only makes UIs reload for nothing. Hold them till then.
                    if hit == Hit::Tree
                        && let Some(t) = last_fire
                        && t.elapsed() < crate::memo::MIN_RECHECK
                    {
                        trailing.get_or_insert(t + crate::memo::MIN_RECHECK);
                        continue;
                    }
                    if hit == Hit::State {
                        trailing = None;
                    }
                    last_fire = Some(Instant::now());
                    on_change(Change {
                        trees_only: hit == Hit::Tree,
                    });
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
            .filter(|t| TREES || !t.tree)
            .map(|t| Target {
                dir: t.dir.canonicalize().unwrap_or(t.dir),
                ..t
            })
            .collect();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Inner { w, watched } = &mut *inner;
        let want: HashMap<&Path, ((u64, u64), bool)> = targets
            .iter()
            .filter_map(|t| {
                let m = std::fs::metadata(&t.dir).ok().filter(|m| m.is_dir())?;
                Some((t.dir.as_path(), ((m.dev(), m.ino()), t.tree)))
            })
            .collect();
        watched.retain(|dir, id| {
            if want.get(dir.as_path()).map(|(i, _)| i) == Some(id) {
                return true;
            }
            let _ = w.unwatch(dir);
            false
        });
        let mut trees = vec![];
        for (dir, (id, tree)) in want {
            let mode = if tree {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if watched.contains_key(dir) || w.watch(dir, mode).is_ok() {
                watched.entry(dir.to_path_buf()).or_insert(id);
                if tree {
                    trees.push(dir.to_path_buf());
                }
            }
        }
        crate::memo::set_watched(trees);
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
            let w = Watcher::new(move |_| {
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
        let w = Watcher::new(move |_| {
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
        w.sync(c.watch_targets());
        // Edits right after a report are held until the memo would recheck.
        std::thread::sleep(crate::memo::MIN_RECHECK);
        // An edit in a worktree is news on macOS, where its tree is watched.
        let n = after(&|| std::fs::write(wt.join("f"), "x").unwrap());
        assert_eq!(n >= 1, TREES, "edit in a worktree: {n}");
        // Status (which rewrites the index) and staging touch no tree: no reloads.
        let n = after(&|| {
            git(&wt, &["add", "f"]);
            git(&wt, &["status", "-s"]);
            git(&root, &["status", "-s"]);
        });
        assert_eq!(n, 0, "status and staging are not repo changes");
        // Build output inside a worktree is not either.
        let n = after(&|| {
            std::fs::create_dir_all(wt.join("node_modules/x")).unwrap();
            std::fs::write(wt.join("node_modules/x/a.js"), "x").unwrap();
        });
        assert_eq!(n, 0, "build output is not a change");
    }

    /// A burst of edits reports once at once and once when the memo would
    /// recompute, not every MAX_WAIT.
    #[test]
    fn edit_bursts_report_at_most_every_recheck() {
        if !TREES {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let tree = tmp.path().canonicalize().unwrap().join("wt");
        std::fs::create_dir_all(&tree).unwrap();
        let fired: Arc<Mutex<Vec<Instant>>> = Arc::default();
        let f = fired.clone();
        let w = Watcher::new(move |_| f.lock().unwrap().push(Instant::now())).unwrap();
        w.sync(vec![Target::tree(tree.clone())]);
        std::thread::sleep(Duration::from_millis(500));
        fired.lock().unwrap().clear();
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(4) {
            std::fs::write(tree.join("a.ts"), format!("{:?}", Instant::now())).unwrap();
            std::thread::sleep(Duration::from_millis(100));
        }
        let edits_done = Instant::now();
        std::thread::sleep(crate::memo::MIN_RECHECK + Duration::from_secs(2));
        let fired = fired.lock().unwrap();
        // Unthrottled, 4s of edits would report every MAX_WAIT.
        assert!((1..=2).contains(&fired.len()), "{} reports", fired.len());
        // The last edits are still reported once the recheck is due.
        assert!(
            fired.last().is_some_and(|t| *t > edits_done),
            "no trailing report"
        );
    }

    #[test]
    fn tree_targets_skip_build_output() {
        let t = Target::tree(PathBuf::from("/w"));
        assert!(t.counts(Path::new("/w/src/a.ts")));
        assert!(t.counts(Path::new("/w/README.md")));
        assert!(!t.counts(Path::new("/w/node_modules/x/a.js")));
        assert!(!t.counts(Path::new("/w/pkg/target/debug/x")));
        assert!(!t.counts(Path::new("/w/.turbo/cache")));
        assert!(!t.counts(Path::new("/elsewhere/a")));
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
