//! Process-wide memo of git answers, so a UI that reloads every few seconds
//! does not respawn git for things that did not change.
//!
//! An entry is reused while the files it depends on keep their mtime and size
//! (a "stamp": index, HEAD, refs). Working-tree edits change none of those, so
//! entries that read the working tree also expire: quickly, unless a recursive
//! watcher covers the tree and calls [`invalidate_under`] on changes.
//! One-shot CLI commands start with an empty memo, so they always ask git.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// mtime and size of each file, `None` when missing.
pub type Stamp = Vec<Option<(SystemTime, u64)>>;

pub fn stamp(paths: &[PathBuf]) -> Stamp {
    paths
        .iter()
        .map(|p| {
            std::fs::metadata(p)
                .ok()
                .map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()))
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

struct Entry {
    /// Working tree this answer reads, if any: what invalidation matches on.
    tree: Option<PathBuf>,
    stamp: Stamp,
    at: Instant,
    /// Its tree changed since: recompute once `MIN_RECHECK` has passed.
    stale: bool,
    value: Box<dyn std::any::Any + Send>,
}

static MEMO: LazyLock<Mutex<HashMap<String, Entry>>> = LazyLock::new(Default::default);
/// Trees a live recursive watcher reports on.
static WATCHED: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(Default::default);

/// `compute`'s answer for `key`, reused while `stamp_paths` keep their stamp.
/// The stamp is taken after computing, so git's own index refresh does not
/// make the next call miss.
pub fn get<T: Clone + Send + 'static>(key: &str, stamp_paths: &[PathBuf], compute: impl FnOnce() -> T) -> T {
    lookup(key, None, stamp_paths, compute)
}

/// Like [`get`] for answers that read the working tree at `tree`.
pub fn get_tree<T: Clone + Send + 'static>(
    key: &str,
    tree: &Path,
    stamp_paths: &[PathBuf],
    compute: impl FnOnce() -> T,
) -> T {
    lookup(key, Some(tree), stamp_paths, compute)
}

fn lookup<T: Clone + Send + 'static>(
    key: &str,
    tree: Option<&Path>,
    stamp_paths: &[PathBuf],
    compute: impl FnOnce() -> T,
) -> T {
    let now = stamp(stamp_paths);
    let ttl = tree.map(|t| if is_watched(t) { WATCHED_TTL } else { UNWATCHED_TTL });
    {
        let m = MEMO.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = m.get(key)
            && e.stamp == now
            && ttl.is_none_or(|t| e.at.elapsed() < t)
            && !(e.stale && e.at.elapsed() >= MIN_RECHECK)
            && let Some(v) = e.value.downcast_ref::<T>()
        {
            return v.clone();
        }
    }
    let v = compute();
    let entry = Entry {
        tree: tree.map(Path::to_path_buf),
        stamp: stamp(stamp_paths),
        at: Instant::now(),
        stale: false,
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

/// Files whose stamp changes when `refname` (`refs/heads/x`, `origin/main`)
/// moves: its loose file and packed-refs.
pub fn ref_files(common_dir: &Path, refname: &str) -> Vec<PathBuf> {
    if refname == "HEAD" {
        let mut v = vec![common_dir.join("HEAD"), common_dir.join("packed-refs")];
        if let Ok(h) = std::fs::read_to_string(common_dir.join("HEAD"))
            && let Some(r) = h.trim().strip_prefix("ref: ")
        {
            v.push(common_dir.join(r));
        }
        return v;
    }
    let full = if refname.starts_with("refs/") {
        refname.to_string()
    } else if refname.contains('/') {
        format!("refs/remotes/{refname}")
    } else {
        format!("refs/heads/{refname}")
    };
    vec![common_dir.join(full), common_dir.join("packed-refs")]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn reuses_until_a_stamped_file_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("index");
        std::fs::write(&f, "a").unwrap();
        let calls = AtomicUsize::new(0);
        let key = format!("t1:{}", tmp.path().display());
        let ask = || {
            get(&key, std::slice::from_ref(&f), || {
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

    fn backdate(key: &str, by: Duration) {
        let mut m = MEMO.lock().unwrap();
        let e = m.get_mut(key).unwrap();
        e.at -= by;
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

    #[test]
    fn ref_files_cover_loose_and_packed() {
        let c = Path::new("/r/.git");
        assert_eq!(
            ref_files(c, "origin/main"),
            vec![c.join("refs/remotes/origin/main"), c.join("packed-refs")]
        );
        assert_eq!(ref_files(c, "main")[0], c.join("refs/heads/main"));
    }
}
