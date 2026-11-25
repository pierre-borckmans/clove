//! End-to-end on real disk images. One scenario, because the env vars that
//! point clove at a temp state dir are process-wide. macOS only for now: it
//! drives diskutil, and the btrfs backend gets its own once its design settles.

#![cfg(target_os = "macos")]

use clove_core::{Clove, Event, NewOptions, RefreshOptions, RemoveOptions, Silent, Verdict};
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

fn sh(dir: &Path, script: &str) {
    let out = Command::new("sh")
        .arg("-c")
        .arg(script)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{script}\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Ejects every volume mounted under the test dir, so a failing assertion
/// does not leave disk images attached.
struct UnmountOnDrop(std::path::PathBuf);

impl Drop for UnmountOnDrop {
    fn drop(&mut self) {
        let out = Command::new("mount").output().unwrap();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some((_, rest)) = line.split_once(" on ")
                && let Some((point, _)) = rest.split_once(" (")
                && point.starts_with(&*self.0.to_string_lossy())
            {
                let _ = Command::new("diskutil")
                    .args(["unmount", "force", point])
                    .output();
                let _ = Command::new("diskutil").args(["eject", point]).output();
            }
        }
    }
}

const CONFIG: &str = r#"
image_size = "2G"
[refresh]
auto = false
max_behind = 0
[[step]]
name = "deps"
warm = "mkdir -p deps && cp lock.txt deps/installed && echo warmed > deps/warm-ran"
setup = "cp lock.txt deps/installed && echo ran >> deps/setup-ran"
watch = ["lock.txt"]
"#;

#[test]
fn full_lifecycle() {
    let tmp = tempfile::Builder::new()
        .prefix("clove-e2e")
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    let t = tmp.path().canonicalize().unwrap();
    let _unmount = UnmountOnDrop(t.clone());
    // SAFETY: single-threaded at this point; the only test in this binary.
    unsafe {
        std::env::set_var("CLOVE_HOME", t.join("state"));
        std::env::set_var("CLOVE_CLONE_ROOT", t.join("clones"));
        std::env::set_var("CLOVE_CONFIG", t.join("no-user-config.toml"));
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
        std::env::set_var("GIT_AUTHOR_NAME", "t");
        std::env::set_var("GIT_AUTHOR_EMAIL", "t@t");
        std::env::set_var("GIT_COMMITTER_NAME", "t");
        std::env::set_var("GIT_COMMITTER_EMAIL", "t@t");
    }

    // origin + a checkout of it
    let remote = t.join("remote.git");
    sh(&t, &format!("git init -q --bare -b main {}", remote.display()));
    let seed = t.join("seed");
    sh(&t, &format!("git clone -q {} seed", remote.display()));
    std::fs::write(seed.join(".clove.toml"), CONFIG).unwrap();
    std::fs::write(seed.join(".gitignore"), "deps/\n.env\n").unwrap();
    std::fs::write(seed.join("lock.txt"), "v1\n").unwrap();
    std::fs::write(seed.join("app.txt"), "hello\n").unwrap();
    sh(
        &seed,
        "git add -A && git commit -qm init && git push -q origin main",
    );
    let root = t.join("app");
    sh(&t, &format!("git clone -q {} app", remote.display()));

    // a local-only file, ignored by git: clones should get it
    std::fs::write(root.join(".env"), "SECRET=1\n").unwrap();
    let c = Clove::discover(&root).unwrap();
    assert_eq!(c.config.base, "origin/main");

    // init
    let g = c.init(&Silent).expect("init");
    assert_eq!(g.generation, 1);
    assert!(!g.dirty, "golden should be clean");
    assert_eq!(g.copied, vec![".env".to_string()]);
    assert!(!c.freshness().unwrap().unwrap().stale, "fresh right after init");

    // new: O(1) clone, setup skipped because lock.txt is unchanged
    let start = Instant::now();
    let a = c
        .new_clone(
            &NewOptions {
                branch: Some("feat/a".into()),
                ..Default::default()
            },
            &Silent,
        )
        .expect("new a");
    let took = start.elapsed();
    eprintln!("new feat/a took {took:?}: {:?}", a.timings);
    assert_eq!(a.name, "feat-a");
    assert!(a.mounted);
    assert_eq!(a.branch.as_deref(), Some("feat/a"));
    assert_eq!(
        std::fs::read_to_string(a.path.join("deps/installed")).unwrap(),
        "v1\n"
    );
    assert!(!a.path.join("deps/setup-ran").exists(), "setup should be skipped");
    assert_eq!(git(&a.path, &["status", "--porcelain"]), "", "clone starts clean");
    assert!(git(&root, &["worktree", "list"]).contains(&a.path.display().to_string()));
    let clone_ms = a.timings.iter().find(|t| t.step == "clone image").unwrap().ms;
    assert!(clone_ms < 1000.0, "clone image took {clone_ms}ms");

    // a commit in the clone is visible from the main checkout
    std::fs::write(a.path.join("app.txt"), "changed\n").unwrap();
    sh(&a.path, "git commit -qam change");
    assert!(git(&root, &["log", "--oneline", "feat/a"]).contains("change"));

    // second clone from a base where lock.txt changed: setup runs
    sh(
        &seed,
        "echo v2 > lock.txt && git commit -qam bump && git push -q origin main:bump",
    );
    let b = c
        .new_clone(
            &NewOptions {
                branch: Some("feat-b".into()),
                base: Some("origin/bump".into()),
                ..Default::default()
            },
            &Silent,
        )
        .expect("new b");
    assert_eq!(
        std::fs::read_to_string(b.path.join("deps/installed")).unwrap(),
        "v2\n"
    );
    assert!(b.path.join("deps/setup-ran").exists(), "setup should run");

    // pr:N fetches the pull request head into pr/N
    sh(
        &seed,
        "echo pr >> app.txt && git commit -qam pr && git push -q origin HEAD:refs/pull/7/head && git reset -q --hard HEAD~1",
    );
    let pr = c
        .new_clone(
            &NewOptions {
                branch: Some("pr:7".into()),
                ..Default::default()
            },
            &Silent,
        )
        .expect("new pr:7");
    assert_eq!(pr.branch.as_deref(), Some("pr/7"));
    assert!(
        std::fs::read_to_string(pr.path.join("app.txt"))
            .unwrap()
            .contains("pr")
    );
    c.remove(
        "pr-7",
        &RemoveOptions {
            force: false,
            delete_branch: true,
            branch_merged: false,
        },
        &Silent,
    )
    .unwrap();

    // isolation between clones
    std::fs::write(a.path.join("deps/only-a"), "x").unwrap();
    assert!(!b.path.join("deps/only-a").exists());

    // clones get the current copy of local-only files
    assert_eq!(
        std::fs::read_to_string(a.path.join(".env")).unwrap(),
        "SECRET=1\n"
    );
    std::fs::write(root.join(".env"), "SECRET=2\n").unwrap();
    let e = c
        .new_clone(
            &NewOptions {
                branch: Some("env-check".into()),
                ..Default::default()
            },
            &Silent,
        )
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(e.path.join(".env")).unwrap(),
        "SECRET=2\n"
    );
    c.remove(
        "env-check",
        &RemoveOptions {
            force: false,
            delete_branch: true,
            branch_merged: false,
        },
        &Silent,
    )
    .unwrap();

    // `setup` after the clone's branch moved reruns only the changed steps
    std::fs::remove_file(a.path.join("deps/setup-ran")).ok();
    sh(&a.path, "echo v3 > lock.txt && git commit -qam lock3");
    c.setup_clone("feat-a", false, &Silent).unwrap();
    assert_eq!(
        std::fs::read_to_string(a.path.join("deps/installed")).unwrap(),
        "v3\n"
    );
    assert!(a.path.join("deps/setup-ran").exists());
    std::fs::remove_file(a.path.join("deps/setup-ran")).unwrap();
    c.setup_clone("feat-a", false, &Silent).unwrap();
    assert!(
        !a.path.join("deps/setup-ran").exists(),
        "nothing changed since last setup"
    );

    // unmount / mount round trip keeps state
    c.unmount("feat-a", false).unwrap();
    assert!(!c.info("feat-a", false).unwrap().mounted);
    let p = c.mount("feat-a").unwrap();
    assert!(p.join("deps/only-a").exists());

    // refresh picks up the new base; clones are unaffected
    sh(
        &seed,
        "git push -q origin main:main --force && echo more >> app.txt && git commit -qam more && git push -q origin HEAD:main",
    );
    c.fetch().unwrap();
    let f = c.freshness().unwrap().unwrap();
    assert!(
        f.stale && f.behind.unwrap_or(0) >= 1,
        "behind with max_behind = 0: {f:?}"
    );
    // lock.txt changed on the base, so the deps step makes it stale on its own
    assert_eq!(f.changed_steps, ["deps"], "{f:?}");
    assert!(
        f.reason
            .as_deref()
            .unwrap_or("")
            .starts_with("deps inputs changed"),
        "{f:?}"
    );
    // lock.txt changed on main (the bump commit), so the deps warm step runs
    let g2 = c.refresh(&Silent).expect("refresh");
    assert!(g2.timings.iter().any(|t| t.step == "warm deps"));
    assert!(!c.freshness().unwrap().unwrap().stale);
    assert_eq!(g2.generation, 2);
    assert_eq!(g2.commit, git(&root, &["rev-parse", "origin/main"]));
    assert!(c.info("feat-a", true).unwrap().mounted);
    // incremental: a commit that leaves lock.txt alone skips the deps warm step
    sh(
        &seed,
        "echo again >> app.txt && git commit -qam again && git push -q origin HEAD:main",
    );
    c.fetch().unwrap();
    let f = c.freshness().unwrap().unwrap();
    assert!(f.changed_steps.is_empty(), "lock.txt untouched: {f:?}");
    assert!(f.stale, "still stale by commit count with max_behind = 0: {f:?}");
    let events = Mutex::new(vec![]);
    let collect = |e: Event| events.lock().unwrap().push(e);
    c.refresh(&collect).expect("incremental refresh");
    let skipped = events
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, Event::Skip { step, .. } if step == "warm deps"));
    assert!(skipped, "warm deps should be skipped when lock.txt is unchanged");
    // --full reruns everything
    let g3 = c.refresh_with(&RefreshOptions { full: true }, &Silent).unwrap();
    assert!(g3.timings.iter().any(|t| t.step == "warm deps"));
    let n = c
        .new_clone(
            &NewOptions {
                detach: true,
                name: Some("peek".into()),
                ..Default::default()
            },
            &Silent,
        )
        .unwrap();
    assert!(
        std::fs::read_to_string(n.path.join("app.txt"))
            .unwrap()
            .contains("more")
    );

    // prune without GitHub: only local proofs count. peek is a detached checkout
    // of base history, so it can go; branches with no PR stay.
    let (checks, warning) = c.merge_checks(true).unwrap();
    assert!(warning.is_some(), "no GitHub remote here");
    for m in &checks {
        let want = if m.clone == "peek" {
            Verdict::Prune
        } else {
            Verdict::Keep
        };
        assert_eq!(m.verdict, want, "{m:?}");
    }
    assert!(c.prune_cache().is_some_and(|p| p.checks.len() == checks.len()));

    // remove refuses dirty clones without --force
    std::fs::write(a.path.join("app.txt"), "dirty\n").unwrap();
    assert!(c.remove("feat-a", &RemoveOptions::default(), &Silent).is_err());
    c.remove(
        "feat-a",
        &RemoveOptions {
            force: true,
            delete_branch: true,
            branch_merged: false,
        },
        &Silent,
    )
    .unwrap();
    assert!(!git(&root, &["branch", "--list", "feat/a"]).contains("feat/a"));
    c.remove("feat-b", &RemoveOptions::default(), &Silent).unwrap();
    c.remove("peek", &RemoveOptions::default(), &Silent).unwrap();
    assert!(c.list(false).unwrap().is_empty());
    assert!(
        c.gc(false).unwrap().is_empty(),
        "nothing left behind: {:?}",
        c.gc(false).unwrap()
    );
    assert!(!git(&root, &["worktree", "list"]).contains("clones"));

    let clone_root = c.config.clone_root.clone();
    c.deinit(&Silent).unwrap();
    assert!(!t.join("state/repos").join(&c.id).exists());
    assert!(!clone_root.exists(), "empty clones folder removed");
}
