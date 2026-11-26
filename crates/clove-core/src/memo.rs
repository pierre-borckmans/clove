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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

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

#[derive(Debug, Clone, PartialEq, Eq)]
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
    {
        let m = MEMO.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = m.get(key)
            && e.at.elapsed() < max_age
            && let Some(v) = e.value.downcast_ref::<T>()
        {
            return v.clone();
        }
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

fn lookup<T: Clone + Send + 'static>(
    key: &str,
    tree: Option<&Path>,
    deps: &[Dep],
    recheck: impl Fn(&T) -> Duration,
    compute: impl FnOnce() -> T,
) -> T {
    let ttl = tree.map(|t| if is_watched(t) { WATCHED_TTL } else { UNWATCHED_TTL });
    let hit = || {
        let now = stamp(deps);
        let m = MEMO.lock().unwrap_or_else(|e| e.into_inner());
        let e = m.get(key)?;
        let fresh = e.stamp == now
            && ttl.is_none_or(|t| e.at.elapsed() < t)
            && !(e.stale && e.at.elapsed() >= e.recheck);
        fresh.then(|| e.value.downcast_ref::<T>().cloned()).flatten()
    };
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
