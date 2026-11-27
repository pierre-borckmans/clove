//! Process-wide memo of git answers, so a UI that reloads every few seconds
//! does not respawn git for things that did not change.
//!
//! An entry is reused while what it depends on is unchanged: files by mtime
//! and size (a worktree's index and HEAD), refs by the commit they point at,
//! read from the ref files without calling git. Refs go by value because a
//! fetch rewrites `packed-refs` even when nothing moved.
//!
//! Working-tree edits change none of those, so answers that read a working
//! tree also expire: quickly, unless a recursive watcher covers the tree and
//! calls [`invalidate_under`] on changes. One-shot CLI commands start with an
//! empty memo, so they always ask git.
//!
//! Some answers are also saved to disk ([`get_tree_saved`]), so a UI that
//! just started can show the last known answer while it asks git again.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// What an answer depends on.
#[derive(Debug, Clone)]
pub enum Dep {
    /// A file, by mtime and size.
    File(PathBuf),
    /// A ref (`refs/heads/x`, `origin/main`, `HEAD`) of the repo whose git
    /// dir is the path, by the commit it points at.
    Ref(PathBuf, String),
}

impl Dep {
    pub fn file(p: impl Into<PathBuf>) -> Self {
        Dep::File(p.into())
    }
    pub fn git_ref(common_dir: &Path, name: &str) -> Self {
        Dep::Ref(common_dir.to_path_buf(), name.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Part {
    File(Option<(SystemTime, u64)>),
    Ref(Option<String>),
}

type Stamp = Vec<Part>;

fn file_stamp(p: &Path) -> Option<(SystemTime, u64)> {
    std::fs::metadata(p)
        .ok()
        .map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()))
}

fn stamp(deps: &[Dep]) -> Stamp {
    deps.iter()
        .map(|d| match d {
            Dep::File(p) => Part::File(file_stamp(p)),
            Dep::Ref(common, name) => Part::Ref(ref_value(common, name)),
        })
        .collect()
}

/// How long a working-tree answer lasts without a watcher on the tree.
pub const UNWATCHED_TTL: Duration = Duration::from_secs(30);
/// With one, as a safety net for events the watcher could not attribute.
pub const WATCHED_TTL: Duration = Duration::from_secs(300);
/// A tree that keeps changing (a build writing outside ignored dirs) is still
/// asked about at most this often.
pub const MIN_RECHECK: Duration = Duration::from_secs(5);
/// Same, for a tree already known to have changes: more edits can't make it
/// look any different (going clean takes a commit, stash or reset, which the
/// index stamp catches), so agents editing in many clones stay cheap.
pub const DIRTY_RECHECK: Duration = Duration::from_secs(60);

struct Entry {
    /// Working tree this answer reads, if any: what invalidation matches on.
    tree: Option<PathBuf>,
    stamp: Stamp,
    at: Instant,
    /// Its tree changed since: recompute once `recheck` has passed.
    stale: bool,
    recheck: Duration,
    value: Box<dyn std::any::Any + Send>,
}

static MEMO: LazyLock<Mutex<HashMap<String, Entry>>> = LazyLock::new(Default::default);
/// Per-key locks, so one key is computed by one caller at a time.
static GATES: LazyLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = LazyLock::new(Default::default);
/// Trees a live recursive watcher reports on.
static WATCHED: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(Default::default);

/// `compute`'s answer for `key`, reused while `deps` are unchanged. Files are
/// stamped after computing, so git's own index refresh does not make the
/// next call miss.
pub fn get<T: Clone + Send + 'static>(key: &str, deps: &[Dep], compute: impl FnOnce() -> T) -> T {
    lookup(key, None, deps, |_| MIN_RECHECK, compute)
}

/// `compute`'s answer for `key`, reused for `max_age`: for numbers that drift
/// slowly and are only shown, like disk usage.
pub fn get_aged<T: Clone + Send + 'static>(key: &str, max_age: Duration, compute: impl FnOnce() -> T) -> T {
    let hit = || aged_hit::<T>(key, max_age);
    if let Some(v) = hit() {
        return v;
    }
    // One computation per key, as in `lookup`: the repo page asks for status
    // and clones at once, and both want every clone's size.
    let gate = {
        let mut g = GATES.lock().unwrap_or_else(|e| e.into_inner());
        g.entry(key.to_string()).or_default().clone()
    };
    let _turn = gate.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = hit() {
        return v;
    }
    let v = compute();
    let entry = Entry {
        tree: None,
        stamp: vec![],
        at: Instant::now(),
        stale: false,
        recheck: MIN_RECHECK,
        value: Box::new(v.clone()),
    };
    MEMO.lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key.to_string(), entry);
    v
}

/// Like [`get_aged`], and the answer is saved to disk. With [`serve_saved`]
/// on, an expired or missing answer is the saved one, if any, while a new one
/// is worked out in the background: sizes are only shown, and measuring a
/// dozen images takes a few hundred milliseconds.
pub fn get_aged_saved<T>(
    key: &str,
    tree: &Path,
    max_age: Duration,
    compute: impl FnOnce() -> T + Send + 'static,
) -> T
where
    T: Clone + Send + Serialize + DeserializeOwned + 'static,
{
    if !serving_saved() {
        return get_aged(key, max_age, compute);
    }
    if let Some(v) = aged_hit(key, max_age) {
        return v;
    }
    if let Some(v) = disk::load::<T>(key, &vec![]) {
        let (key, tree) = (key.to_string(), tree.to_path_buf());
        background::queue(key.clone(), move || {
            let v = get_aged(&key, max_age, compute);
            disk::save(&key, &tree, &vec![], &v);
        });
        return v;
    }
    let v = get_aged(key, max_age, compute);
    disk::save(key, tree, &vec![], &v);
    v
}

fn aged_hit<T: Clone + 'static>(key: &str, max_age: Duration) -> Option<T> {
    let m = MEMO.lock().unwrap_or_else(|e| e.into_inner());
    let e = m.get(key)?;
    (e.at.elapsed() < max_age)
        .then(|| e.value.downcast_ref::<T>().cloned())
        .flatten()
}

/// How long disk usage figures are reused.
pub const SIZE_TTL: Duration = Duration::from_secs(60);

/// Like [`get`] for answers that read the working tree at `tree`.
pub fn get_tree<T: Clone + Send + 'static>(
    key: &str,
    tree: &Path,
    deps: &[Dep],
    compute: impl FnOnce() -> T,
) -> T {
    lookup(key, Some(tree), deps, |_| MIN_RECHECK, compute)
}

/// [`get_tree`] where the answer says how soon edits are worth a recheck.
pub fn get_tree_by<T: Clone + Send + 'static>(
    key: &str,
    tree: &Path,
    deps: &[Dep],
    recheck: impl Fn(&T) -> Duration,
    compute: impl FnOnce() -> T,
) -> T {
    lookup(key, Some(tree), deps, recheck, compute)
}

/// The memo's answer for `key`, if still good.
fn hit<T: Clone + 'static>(key: &str, tree: Option<&Path>, deps: &[Dep]) -> Option<T> {
    let ttl = tree.map(|t| if is_watched(t) { WATCHED_TTL } else { UNWATCHED_TTL });
    let now = stamp(deps);
    let m = MEMO.lock().unwrap_or_else(|e| e.into_inner());
    let e = m.get(key)?;
    let fresh =
        e.stamp == now && ttl.is_none_or(|t| e.at.elapsed() < t) && !(e.stale && e.at.elapsed() >= e.recheck);
    fresh.then(|| e.value.downcast_ref::<T>().cloned()).flatten()
}

fn lookup<T: Clone + Send + 'static>(
    key: &str,
    tree: Option<&Path>,
    deps: &[Dep],
    recheck: impl Fn(&T) -> Duration,
    compute: impl FnOnce() -> T,
) -> T {
    let hit = || hit::<T>(key, tree, deps);
    if let Some(v) = hit() {
        return v;
    }
    // One computation per key at a time: callers that arrive meanwhile (a UI
    // asking from several places at once) wait for it and reuse its answer.
    let gate = {
        let mut g = GATES.lock().unwrap_or_else(|e| e.into_inner());
        g.entry(key.to_string()).or_default().clone()
    };
    let _turn = gate.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = hit() {
        return v;
    }
    let v = compute();
    let entry = Entry {
        tree: tree.map(Path::to_path_buf),
        stamp: stamp(deps),
        at: Instant::now(),
        stale: false,
        recheck: recheck(&v),
        value: Box::new(v.clone()),
    };
    MEMO.lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key.to_string(), entry);
    v
}

/// An answer from [`get_tree_saved`].
#[derive(Debug, Clone)]
pub struct Saved<T> {
    /// None while the first answer is being worked out.
    pub value: Option<T>,
    /// Being asked again in the background: `value` may be out of date.
    pub pending: bool,
}

static SERVE_SAVED: AtomicBool = AtomicBool::new(false);

/// For long-running UIs: [`get_tree_saved`] answers at once from the saved
/// copy and asks git in the background, one tree at a time. Off, it asks git
/// before answering, as one-shot commands must.
pub fn serve_saved(on: bool) {
    SERVE_SAVED.store(on, Ordering::Relaxed);
}

pub fn serving_saved() -> bool {
    SERVE_SAVED.load(Ordering::Relaxed)
}

/// Like [`get_tree_by`], and the answer is saved to disk. With
/// [`serve_saved`] on and `saved_ok`, a miss returns the saved answer, if its
/// deps still match, and works out a new one in the background.
pub fn get_tree_saved<T>(
    key: &str,
    tree: &Path,
    deps: &[Dep],
    saved_ok: bool,
    recheck: impl Fn(&T) -> Duration + Send + 'static,
    compute: impl FnOnce() -> T + Send + 'static,
) -> Saved<T>
where
    T: Clone + Send + Serialize + DeserializeOwned + 'static,
{
    saved_lookup(
        SERVE_SAVED.load(Ordering::Relaxed),
        key,
        tree,
        deps,
        saved_ok,
        recheck,
        compute,
    )
}

fn saved_lookup<T>(
    serve: bool,
    key: &str,
    tree: &Path,
    deps: &[Dep],
    saved_ok: bool,
    recheck: impl Fn(&T) -> Duration + Send + 'static,
    compute: impl FnOnce() -> T + Send + 'static,
) -> Saved<T>
where
    T: Clone + Send + Serialize + DeserializeOwned + 'static,
{
    // One-shot commands neither read nor write the saved copy.
    if !serve {
        return Saved {
            value: Some(lookup(key, Some(tree), deps, &recheck, compute)),
            pending: false,
        };
    }
    if !saved_ok {
        let v = lookup(key, Some(tree), deps, &recheck, compute);
        disk::save(key, tree, &stamp(deps), &v);
        return Saved {
            value: Some(v),
            pending: false,
        };
    }
    if let Some(v) = hit::<T>(key, Some(tree), deps) {
        return Saved {
            value: Some(v),
            pending: false,
        };
    }
    let saved = disk::load::<T>(key, &stamp(deps));
    let (key, tree, deps) = (key.to_string(), tree.to_path_buf(), deps.to_vec());
    background::queue(key.clone(), move || {
        let v = lookup(&key, Some(&tree), &deps, &recheck, compute);
        disk::save(&key, &tree, &stamp(&deps), &v);
    });
    Saved {
        value: saved,
        pending: true,
    }
}

/// Saved answers, one file for every repo. Losing it costs one slow start.
mod disk {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct Item {
        tree: PathBuf,
        stamp: Stamp,
        value: serde_json::Value,
    }

    type Items = HashMap<String, Item>;

    static ITEMS: LazyLock<Mutex<Option<Items>>> = LazyLock::new(Default::default);

    fn path() -> PathBuf {
        if cfg!(test) {
            return std::env::temp_dir().join(format!(
                "clove-memo-test-{}/status-cache.json",
                std::process::id()
            ));
        }
        crate::repo::state_home().join("status-cache.json")
    }

    fn read() -> Items {
        std::fs::read(path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub(super) fn load<T: DeserializeOwned>(key: &str, now: &Stamp) -> Option<T> {
        let mut items = ITEMS.lock().unwrap_or_else(|e| e.into_inner());
        let item = items.get_or_insert_with(read).get(key)?;
        // A moved index, HEAD or ref means the saved answer is wrong; only
        // working-tree edits since can't be told apart, as in memory.
        (item.stamp == *now)
            .then(|| serde_json::from_value(item.value.clone()).ok())
            .flatten()
    }

    pub(super) fn save<T: Serialize>(key: &str, tree: &Path, stamp: &Stamp, v: &T) {
        let Ok(value) = serde_json::to_value(v) else {
            return;
        };
        let mut items = ITEMS.lock().unwrap_or_else(|e| e.into_inner());
        let items = items.get_or_insert_with(read);
        if items
            .get(key)
            .is_some_and(|i| i.stamp == *stamp && i.value == value)
        {
            return;
        }
        let item = Item {
            tree: tree.to_path_buf(),
            stamp: stamp.clone(),
            value,
        };
        items.insert(key.to_string(), item);
        let path = path();
        let Some(dir) = path.parent() else { return };
        let _ = std::fs::create_dir_all(dir);
        // Other clove processes save here too: merge under a lock, keep
        // their entries, drop ones for trees that are gone.
        let Ok(_lock) = crate::util::FileLock::acquire(&dir.join("status-cache.lock")) else {
            return;
        };
        let mut merged = read();
        for (k, i) in items.drain() {
            merged.insert(k, i);
        }
        merged.retain(|_, i| i.tree.exists());
        if let Ok(b) = serde_json::to_vec(&merged) {
            let _ = crate::util::write_atomic(&path, &b);
        }
        *items = merged;
    }
}

/// One thread working through queued recomputations, oldest first: running
/// them side by side makes git fight over the disk (see `serve_saved`).
mod background {
    use super::*;

    type Job = Box<dyn FnOnce() + Send>;

    struct Queue {
        jobs: std::collections::VecDeque<(String, Job)>,
        /// Queued: asking again meanwhile adds nothing. A key asked again
        /// while it runs is queued once more; that run finds the memo filled.
        keys: HashSet<String>,
        started: bool,
    }

    static QUEUE: LazyLock<(Mutex<Queue>, Condvar)> = LazyLock::new(|| {
        (
            Mutex::new(Queue {
                jobs: Default::default(),
                keys: Default::default(),
                started: false,
            }),
            Condvar::new(),
        )
    });

    pub(super) fn queue(key: String, job: impl FnOnce() + Send + 'static) {
        let (lock, cv) = &*QUEUE;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        if !q.keys.insert(key.clone()) {
            return;
        }
        q.jobs.push_back((key, Box::new(job)));
        if !q.started {
            q.started = true;
            std::thread::Builder::new()
                .name("clove-memo".into())
                .spawn(work)
                .expect("spawn memo thread");
        }
        cv.notify_one();
    }

    fn work() {
        let (lock, cv) = &*QUEUE;
        loop {
            let job = {
                let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    if let Some((key, job)) = q.jobs.pop_front() {
                        q.keys.remove(&key);
                        break job;
                    }
                    q = cv.wait(q).unwrap_or_else(|e| e.into_inner());
                }
            };
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
        }
    }
}

/// Something changed at `path`: answers about a tree holding it are recomputed
/// on their next use (no sooner than `MIN_RECHECK` after the last time).
pub fn invalidate_under(path: &Path) {
    let mut m = MEMO.lock().unwrap_or_else(|e| e.into_inner());
    for e in m.values_mut() {
        if e.tree.as_deref().is_some_and(|t| path.starts_with(t)) {
            e.stale = true;
        }
    }
}

/// Forgets answers about the tree at `path`: before acting on it, ask git.
pub fn forget_tree(path: &Path) {
    let mut m = MEMO.lock().unwrap_or_else(|e| e.into_inner());
    m.retain(|_, e| e.tree.as_deref() != Some(path));
}

/// Forgets everything: an explicit refresh.
pub fn clear() {
    MEMO.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

pub fn set_watched(trees: impl IntoIterator<Item = PathBuf>) {
    *WATCHED.lock().unwrap_or_else(|e| e.into_inner()) = trees.into_iter().collect();
}

fn is_watched(tree: &Path) -> bool {
    WATCHED.lock().unwrap_or_else(|e| e.into_inner()).contains(tree)
}

/// The commit (or symbolic target) `name` points at, from the ref files.
/// `origin/main` means `refs/remotes/origin/main`, `main` `refs/heads/main`.
/// Repos on the reftable backend can't be read this way: the table list's
/// stamp stands in, which changes whenever any ref does.
pub fn ref_value(common_dir: &Path, name: &str) -> Option<String> {
    let full = if name == "HEAD" || name.starts_with("refs/") {
        name.to_string()
    } else if name.contains('/') {
        format!("refs/remotes/{name}")
    } else {
        format!("refs/heads/{name}")
    };
    if let Ok(s) = std::fs::read_to_string(common_dir.join(&full)) {
        let s = s.trim();
        return match s.strip_prefix("ref: ") {
            // One level is all git writes (HEAD -> a branch).
            Some(target) => Some(format!(
                "{target}={}",
                ref_value(common_dir, target).unwrap_or_default()
            )),
            None => Some(s.to_string()),
        };
    }
    let reftable = common_dir.join("reftable/tables.list");
    if reftable.exists() {
        return file_stamp(&reftable).map(|(t, n)| format!("reftable:{t:?}:{n}"));
    }
    packed_refs(common_dir).get(&full).cloned()
}

/// The commit `name` points at, when the ref files say it plainly (not on
/// reftable, not symbolic).
pub fn ref_sha(common_dir: &Path, name: &str) -> Option<String> {
    ref_value(common_dir, name).filter(|v| v.len() >= 40 && v.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// `packed-refs` parsed, reparsed only when the file changes.
fn packed_refs(common_dir: &Path) -> Arc<HashMap<String, String>> {
    type Cache = HashMap<PathBuf, (Option<(SystemTime, u64)>, Arc<HashMap<String, String>>)>;
    static PACKED: LazyLock<Mutex<Cache>> = LazyLock::new(Default::default);
    let path = common_dir.join("packed-refs");
    let st = file_stamp(&path);
    let mut c = PACKED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((s, refs)) = c.get(&path)
        && *s == st
    {
        return refs.clone();
    }
    let refs: HashMap<String, String> = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with('^'))
        .filter_map(|l| {
            let (sha, name) = l.split_once(' ')?;
            Some((name.to_string(), sha.to_string()))
        })
        .collect();
    let refs = Arc::new(refs);
    c.insert(path, (st, refs.clone()));
    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn reuses_until_a_file_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("index");
        std::fs::write(&f, "a").unwrap();
        let calls = AtomicUsize::new(0);
        let key = format!("t1:{}", tmp.path().display());
        let ask = || {
            get(&key, &[Dep::file(&f)], || {
                calls.fetch_add(1, Ordering::SeqCst);
                1
            })
        };
        ask();
        ask();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        std::fs::write(&f, "bb").unwrap();
        ask();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn refs_count_by_value_not_by_file_writes() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path();
        std::fs::create_dir_all(g.join("refs/heads")).unwrap();
        std::fs::write(
            g.join("packed-refs"),
            "# pack-refs\naaa refs/remotes/origin/main\n",
        )
        .unwrap();
        let calls = AtomicUsize::new(0);
        let key = format!("t3:{}", g.display());
        let ask = || {
            get(&key, &[Dep::git_ref(g, "origin/main")], || {
                calls.fetch_add(1, Ordering::SeqCst);
            })
        };
        ask();
        // A fetch rewrites packed-refs with the same values: still a hit.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(
            g.join("packed-refs"),
            "# pack-refs \naaa refs/remotes/origin/main\n",
        )
        .unwrap();
        ask();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // A loose ref takes over from the packed one.
        std::fs::create_dir_all(g.join("refs/remotes/origin")).unwrap();
        std::fs::write(g.join("refs/remotes/origin/main"), "bbb\n").unwrap();
        ask();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(ref_value(g, "origin/main").as_deref(), Some("bbb"));
        std::fs::write(g.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(g.join("refs/heads/main"), "ccc\n").unwrap();
        assert_eq!(ref_value(g, "HEAD").as_deref(), Some("refs/heads/main=ccc"));
        assert_eq!(ref_value(g, "main").as_deref(), Some("ccc"));
    }

    fn backdate(key: &str, by: Duration) {
        let mut m = MEMO.lock().unwrap();
        let e = m.get_mut(key).unwrap();
        e.at -= by;
    }

    #[test]
    fn concurrent_callers_share_one_computation() {
        let calls = Arc::new(AtomicUsize::new(0));
        let key = format!("t5:{:?}", std::thread::current().id());
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let (calls, key) = (calls.clone(), key.clone());
                std::thread::spawn(move || {
                    get(&key, &[], || {
                        calls.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(100));
                        7
                    })
                })
            })
            .collect();
        for t in threads {
            assert_eq!(t.join().unwrap(), 7);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_dirty_tree_is_rechecked_less_often() {
        let tmp = tempfile::tempdir().unwrap();
        let tree = tmp.path().join("wt");
        let calls = AtomicUsize::new(0);
        let key = format!("t4:{}", tree.display());
        let ask = || {
            get_tree_by(
                &key,
                &tree,
                &[],
                |dirty: &bool| if *dirty { DIRTY_RECHECK } else { MIN_RECHECK },
                || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    true
                },
            )
        };
        ask();
        invalidate_under(&tree.join("a"));
        backdate(&key, MIN_RECHECK);
        ask();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "dirty: edits wait for DIRTY_RECHECK"
        );
        backdate(&key, DIRTY_RECHECK);
        ask();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn saved_answers_show_at_once_and_refresh_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let tree = tmp.path().join("wt");
        std::fs::create_dir_all(&tree).unwrap();
        let index = tree.join("index");
        std::fs::write(&index, "a").unwrap();
        let key = format!("t6:{}", tree.display());
        let deps = [Dep::file(&index)];
        let ask = |v: u32| saved_lookup(true, &key, &tree, &deps, true, |_| MIN_RECHECK, move || v);
        let wait = || {
            for _ in 0..200 {
                if hit::<u32>(&key, Some(&tree), &deps).is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("background answer never landed");
        };

        // Nothing saved yet: no value, asked in the background.
        let s = ask(1);
        assert_eq!((s.value, s.pending), (None, true));
        wait();
        let s = ask(2);
        assert_eq!((s.value, s.pending), (Some(1), false));

        // A new process: the memo is empty, the saved answer shows at once.
        MEMO.lock().unwrap().remove(&key);
        let s = ask(3);
        assert_eq!((s.value, s.pending), (Some(1), true));
        wait();
        assert_eq!(ask(4).value, Some(3));

        // Deps moved: the saved answer is wrong, so none is shown.
        MEMO.lock().unwrap().remove(&key);
        std::fs::write(&index, "bb").unwrap();
        let s = ask(5);
        assert_eq!((s.value, s.pending), (None, true));
        wait();

        // Live asks wait for git, and one-shot commands never see the copy.
        assert_eq!(
            saved_lookup(true, &key, &tree, &deps, false, |_| MIN_RECHECK, || 6u32).value,
            Some(5)
        );
        MEMO.lock().unwrap().remove(&key);
        let s = saved_lookup(false, &key, &tree, &deps, true, |_| MIN_RECHECK, || 7u32);
        assert_eq!((s.value, s.pending), (Some(7), false));
    }

    #[test]
    fn tree_answers_drop_on_a_change_inside() {
        let tmp = tempfile::tempdir().unwrap();
        let tree = tmp.path().join("wt");
        let calls = AtomicUsize::new(0);
        let key = format!("t2:{}", tree.display());
        let ask = || {
            get_tree(&key, &tree, &[], || {
                calls.fetch_add(1, Ordering::SeqCst);
                true
            })
        };
        ask();
        ask();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        invalidate_under(&tmp.path().join("elsewhere/x"));
        ask();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        invalidate_under(&tree.join("src/a.ts"));
        // Fresh answers survive a change for MIN_RECHECK...
        ask();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // ...then the next use recomputes.
        backdate(&key, MIN_RECHECK);
        ask();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        ask();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
