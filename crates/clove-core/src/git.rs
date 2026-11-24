use crate::util::{output, run};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `git -C <dir>` with locale and pager pinned so output is parseable.
pub fn cmd(dir: &Path) -> Command {
    let mut c = Command::new("git");
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
