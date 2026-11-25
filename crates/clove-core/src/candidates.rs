//! Finds git repositories clove could start managing: a bounded scan of the
//! places people keep code, most recently used first.

use crate::repo::{known_repos, state_home};
use crate::util::home;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

/// A git repository clove does not manage yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepoCandidate {
    pub root: PathBuf,
    pub name: String,
    /// Checked-out branch, read from `.git/HEAD`; None when detached.
    pub branch: Option<String>,
    /// Last git activity (unix secs): newest of HEAD, index, logs/HEAD, FETCH_HEAD.
    pub active: u64,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub roots: Vec<PathBuf>,
    /// Levels below each root to look at (the root itself is level 0).
    pub max_depth: usize,
    /// The scan stops and returns what it has after this long.
    pub budget: Duration,
    /// Repos to leave out, e.g. those clove already manages.
    pub exclude: BTreeSet<PathBuf>,
}

/// Dirs never descended into: dependencies and build output.
const SKIP: [&str; 6] = ["node_modules", "target", "vendor", "Library", "dist", "build"];

/// Where people keep code, plus the folders of repos clove already manages.
pub fn default_roots() -> Vec<PathBuf> {
    let h = home();
    let mut roots: Vec<PathBuf> = vec![];
    for r in known_repos() {
        if let Some(p) = r.root.parent() {
            roots.push(p.to_path_buf());
        }
    }
    for d in ["Documents/Dev", "dev", "code", "src", "projects", "Developer"] {
        roots.push(h.join(d));
    }
    let mut seen = BTreeSet::new();
    roots
        .into_iter()
        .filter(|r| r.is_dir())
        .filter_map(|r| r.canonicalize().ok())
        .filter(|r| *r != h && seen.insert(r.clone()))
        .collect()
}

/// Candidates under the default roots, leaving out managed repos.
pub fn find() -> Vec<RepoCandidate> {
    let exclude = known_repos()
        .into_iter()
        .map(|r| r.root.canonicalize().unwrap_or(r.root))
        .collect();
    scan(&ScanOptions {
        roots: default_roots(),
        max_depth: 3,
        budget: Duration::from_millis(1500),
        exclude,
    })
}

pub fn scan(opts: &ScanOptions) -> Vec<RepoCandidate> {
    let deadline = Instant::now() + opts.budget;
    let state = state_home();
    let mut found = BTreeSet::new();
    let mut out = vec![];
    // Breadth first, so a time-out leaves the shallow (likelier) repos found.
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> =
        opts.roots.iter().map(|r| (r.clone(), 0)).collect();
    while let Some((dir, depth)) = queue.pop_front() {
        if Instant::now() > deadline {
            break;
        }
        if dir.starts_with(&state) {
            continue;
        }
        let git = dir.join(".git");
        match std::fs::symlink_metadata(&git) {
            Ok(m) if m.is_dir() => {
                if !opts.exclude.contains(&dir) && !is_bare(&git) && found.insert(dir.clone()) {
                    out.push(candidate(&dir, &git));
                }
                // Repos inside a repo are vendored or submodules: not offered.
                continue;
            }
            // A `.git` file: a worktree (clove clones, golden images) or a submodule.
            Ok(_) => continue,
            Err(_) => {}
        }
        if depth >= opts.max_depth {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            // DirEntry::file_type doesn't follow symlinks, so links aren't walked.
            if !e.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name.ends_with(".clove") || SKIP.contains(&name.as_ref()) {
                continue;
            }
            // A bare repo's internals.
            if name.ends_with(".git") && e.path().join("HEAD").is_file() {
                continue;
            }
            queue.push_back((e.path(), depth + 1));
        }
    }
    out.sort_by(|a, b| b.active.cmp(&a.active).then_with(|| a.root.cmp(&b.root)));
    out
}

fn is_bare(git: &Path) -> bool {
    std::fs::read_to_string(git.join("config")).is_ok_and(|c| {
        c.lines()
            .map(|l| l.split_whitespace().collect::<String>())
            .any(|l| l == "bare=true")
    })
}

fn candidate(root: &Path, git: &Path) -> RepoCandidate {
    let active = ["HEAD", "index", "logs/HEAD", "FETCH_HEAD"]
        .iter()
        .filter_map(|f| std::fs::metadata(git.join(f)).and_then(|m| m.modified()).ok())
        .filter_map(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .max()
        .unwrap_or(0);
    let branch = std::fs::read_to_string(git.join("HEAD"))
        .ok()
        .and_then(|h| h.trim().strip_prefix("ref: refs/heads/").map(str::to_string));
    RepoCandidate {
        root: root.to_path_buf(),
        name: root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        branch,
        active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    fn repo(p: &Path) {
        std::fs::create_dir_all(p).unwrap();
        git(p, &["init", "-q"]);
    }

    fn names(c: &[RepoCandidate]) -> Vec<String> {
        let mut v: Vec<String> = c.iter().map(|c| c.name.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn finds_plain_repos_and_skips_the_rest() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path().canonicalize().unwrap();
        repo(&r.join("app"));
        repo(&r.join("work/api"));
        repo(&r.join("a/b/c/deep")); // depth 4: past the bound
        repo(&r.join("node_modules/dep"));
        repo(&r.join(".hidden/x"));
        repo(&r.join("managed"));
        repo(&r.join("app/vendored")); // inside a repo
        // A bare repo, inside and outside a `.git`-suffixed folder.
        git(&r, &["init", "-q", "--bare", "bare.git"]);
        git(&r, &["init", "-q", "--bare", "plainbare"]);
        // A clove clone root, and a worktree (what a clone's checkout is).
        repo(&r.join("app.clove/feat/app"));
        git(
            &r.join("app"),
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "x",
            ],
        );
        git(
            &r.join("app"),
            &["worktree", "add", "-q", &r.join("wt").to_string_lossy()],
        );

        let found = scan(&ScanOptions {
            roots: vec![r.clone()],
            max_depth: 3,
            budget: Duration::from_secs(10),
            exclude: [r.join("managed")].into_iter().collect(),
        });
        assert_eq!(names(&found), ["api", "app"]);
        let app = found.iter().find(|c| c.name == "app").unwrap();
        assert_eq!(app.root, r.join("app"));
        assert!(app.active > 0);
        assert!(app.branch.is_some());
    }

    #[test]
    fn newest_activity_first_and_depth_bound() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path().canonicalize().unwrap();
        repo(&r.join("old"));
        repo(&r.join("x/new"));
        // Age `old` by an hour.
        let past = std::time::SystemTime::now() - Duration::from_secs(3600);
        for f in ["HEAD", "index", "logs/HEAD", "FETCH_HEAD"] {
            if let Ok(file) = std::fs::File::options()
                .write(true)
                .open(r.join("old/.git").join(f))
            {
                file.set_modified(past).unwrap();
            }
        }
        let opts = |depth| ScanOptions {
            roots: vec![r.clone()],
            max_depth: depth,
            budget: Duration::from_secs(10),
            exclude: BTreeSet::new(),
        };
        let found = scan(&opts(3));
        let order: Vec<_> = found.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(order, ["new", "old"]);
        assert_eq!(names(&scan(&opts(1))), ["old"]);
        // A root that is itself a repo is offered.
        assert_eq!(
            names(&scan(&ScanOptions {
                roots: vec![r.join("old")],
                ..opts(0)
            })),
            ["old"]
        );
    }

    #[test]
    fn an_exhausted_budget_returns_early() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path().canonicalize().unwrap();
        repo(&r.join("a"));
        let found = scan(&ScanOptions {
            roots: vec![r],
            max_depth: 3,
            budget: Duration::ZERO,
            exclude: BTreeSet::new(),
        });
        assert!(found.is_empty());
    }
}
