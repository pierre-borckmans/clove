use crate::util::{output, run};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `git -C <dir>` with locale and pager pinned so output is parseable.
/// Set while a golden image is built: git's fsmonitor daemon would start in
/// the image and keep it from unmounting.
static GOLDEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Turns off fsmonitor for clove's git commands until dropped.
pub struct GoldenGit(());

pub fn golden_git() -> GoldenGit {
    GOLDEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    GoldenGit(())
}

impl Drop for GoldenGit {
    fn drop(&mut self) {
        GOLDEN.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

pub fn cmd(dir: &Path) -> Command {
    let mut c = Command::new("git");
    // Preloading stats the index from up to 20 threads; on APFS they contend
    // in the kernel and a status costs ~10x the CPU for the same wall time.
    c.args(["-c", "core.preloadIndex=false"]);
    if GOLDEN.load(std::sync::atomic::Ordering::SeqCst) > 0 {
        c.args(["-c", "core.fsmonitor=false"]);
    }
    c.arg("-C").arg(dir);
    c.env("LC_ALL", "C").env("GIT_TERMINAL_PROMPT", "0");
    c
}

pub fn out(dir: &Path, args: &[&str]) -> Result<String> {
    run(cmd(dir).args(args))
}

pub fn ok(dir: &Path, args: &[&str]) -> bool {
    cmd(dir)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn run_ok(dir: &Path, args: &[&str]) -> Result<()> {
    output(cmd(dir).args(args)).map(|_| ())
}

/// Absolute path of the shared `.git` directory for any worktree of a repo.
pub fn common_dir(dir: &Path) -> Result<PathBuf> {
    let s = out(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    Ok(PathBuf::from(s))
}

pub fn head_commit(dir: &Path) -> Result<String> {
    out(dir, &["rev-parse", "HEAD"])
}

pub fn current_branch(dir: &Path) -> Option<String> {
    out(dir, &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok()
}

pub fn ref_exists(dir: &Path, r: &str) -> bool {
    ok(
        dir,
        &["rev-parse", "--verify", "--quiet", &format!("{r}^{{commit}}")],
    )
}

pub fn has_remote(dir: &Path, remote: &str) -> bool {
    ok(dir, &["remote", "get-url", remote])
}

/// `origin/main` style default base: the remote's HEAD, else the main worktree's branch.
pub fn default_base(dir: &Path) -> String {
    use crate::memo::Dep;
    let g = dir.join(".git");
    let deps = [
        Dep::file(g.join("refs/remotes/origin/HEAD")),
        Dep::git_ref(&g, "origin/main"),
        Dep::git_ref(&g, "origin/master"),
        Dep::git_ref(&g, "HEAD"),
    ];
    crate::memo::get(&format!("default-base:{}", dir.display()), &deps, || {
        find_default_base(dir)
    })
}

fn find_default_base(dir: &Path) -> String {
    if let Ok(r) = out(
        dir,
        &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"],
    ) {
        return r;
    }
    for b in ["origin/main", "origin/master"] {
        if ref_exists(dir, b) {
            return b.to_string();
        }
    }
    current_branch(dir).unwrap_or_else(|| "HEAD".into())
}

pub fn is_dirty(dir: &Path) -> Result<bool> {
    Ok(!out(dir, &["status", "--porcelain", "--untracked-files=normal"])?.is_empty())
}

pub fn count_commits(dir: &Path, range: &str) -> Option<u32> {
    out(dir, &["rev-list", "--count", range]).ok()?.parse().ok()
}

/// Splits `origin/main` into (`origin`, `main`) when the prefix is a remote.
pub fn split_remote_ref(dir: &Path, r: &str) -> Option<(String, String)> {
    let (remote, branch) = r.split_once('/')?;
    has_remote(dir, remote).then(|| (remote.to_string(), branch.to_string()))
}
