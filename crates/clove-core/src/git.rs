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

/// The git binary to run. On macOS, `/usr/bin/git` is a launcher that asks
/// xcrun for the developer directory on every call: ~8 ms of a ~12 ms git
/// call, and `clove new` makes dozens. Its exec path holds the real binary,
/// so ask once and run that. Any other git (Homebrew, nix) runs as found.
pub fn bin() -> &'static Path {
    static BIN: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    BIN.get_or_init(|| resolve_bin().unwrap_or_else(|| PathBuf::from("git")))
}

fn resolve_bin() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let first = std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join("git"))
        .find(|p| p.is_file())?;
    if first != Path::new("/usr/bin/git") {
        return None;
    }
    let out = Command::new(&first).arg("--exec-path").output().ok()?;
    let dir = String::from_utf8(out.stdout).ok()?;
    let real = Path::new(dir.trim()).join("git").canonicalize().ok()?;
    real.is_file().then_some(real)
}

pub fn cmd(dir: &Path) -> Command {
    let mut c = Command::new(bin());
    // Preloading stats the index from up to 20 threads; on APFS they contend
    // in the kernel and a status costs ~10x the CPU for the same wall time.
    // A repo with core.fsmonitor on would start a daemon in every clone
    // clove looks at; it holds the volume open, so the clone can't go.
    c.args(["-c", "core.preloadIndex=false", "-c", "core.fsmonitor=false"]);
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

/// The commit a revision names, or None when it names none.
pub fn commit_of(dir: &Path, rev: &str) -> Option<String> {
    out(
        dir,
        &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")],
    )
    .ok()
}

/// Whether anything under any of `paths` differs between two commits: one
/// git call that lets the per-path checks after it be skipped. Same commit,
/// no call at all.
pub fn any_changed(dir: &Path, a: &str, b: &str, paths: &[&str]) -> bool {
    if paths.is_empty() {
        return false;
    }
    let mut args = vec!["diff", "--quiet", a, b, "--"];
    args.extend(paths);
    !ok(dir, &args)
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
