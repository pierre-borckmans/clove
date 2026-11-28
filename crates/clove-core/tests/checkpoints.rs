//! Checkpoints and forks end to end, on real disk images. Its own test
//! binary (and so its own process), because clove reads its state dir from
//! process-wide env vars. macOS only, like e2e.rs.

#![cfg(target_os = "macos")]

use clove_core::{Clove, ForkOptions, NewOptions, RemoveOptions, RewindOptions, Silent};
use std::path::Path;
use std::process::Command;

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

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

/// Ejects every volume mounted under the test dir, even when an assertion fails.
struct UnmountOnDrop(std::path::PathBuf);

impl Drop for UnmountOnDrop {
    fn drop(&mut self) {
        let out = Command::new("mount").output().unwrap();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some((dev, rest)) = line.split_once(" on ")
                && let Some((point, _)) = rest.split_once(" (")
                && point.starts_with(&*self.0.to_string_lossy())
            {
                let _ = Command::new("diskutil")
                    .args(["unmount", "force", point])
                    .output();
                let _ = Command::new("diskutil").args(["eject", dev]).output();
            }
        }
    }
}

const CONFIG: &str = r#"
image_size = "2G"
spare = false
fetch = false
[refresh]
auto = false
[[step]]
name = "deps"
warm = "mkdir -p deps && cp lock.txt deps/installed"
setup = "cp lock.txt deps/installed"
watch = ["lock.txt"]
"#;

#[test]
fn checkpoint_rewind_fork() {
    let tmp = tempfile::Builder::new()
        .prefix("clove-cp")
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    let t = tmp.path().canonicalize().unwrap();
    let _unmount = UnmountOnDrop(t.clone());
    // SAFETY: single-threaded at this point; the only test in this binary.
    unsafe {
        std::env::set_var("CLOVE_HOME", t.join("state"));
        std::env::set_var("CLOVE_CLONE_ROOT", t.join("app/clones"));
        std::env::set_var("CLOVE_CONFIG", t.join("no-user-config.toml"));
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
        std::env::set_var("GIT_DISCOVERY_ACROSS_FILESYSTEM", "1");
        std::env::set_var("GIT_AUTHOR_NAME", "t");
        std::env::set_var("GIT_AUTHOR_EMAIL", "t@t");
        std::env::set_var("GIT_COMMITTER_NAME", "t");
        std::env::set_var("GIT_COMMITTER_EMAIL", "t@t");
    }
    let remote = t.join("remote.git");
    sh(&t, &format!("git init -q --bare -b main {}", remote.display()));
    let seed = t.join("seed");
    sh(&t, &format!("git clone -q {} seed", remote.display()));
    std::fs::write(seed.join(".clove.toml"), CONFIG).unwrap();
    std::fs::write(seed.join(".gitignore"), "deps/\nclones/\nbuild/\n").unwrap();
    std::fs::write(seed.join("lock.txt"), "v1\n").unwrap();
    std::fs::write(seed.join("app.txt"), "hello\n").unwrap();
    sh(
        &seed,
        "git add -A && git commit -qm init && git push -q origin main",
    );
    let root = t.join("app");
    sh(&t, &format!("git clone -q {} app", remote.display()));
    let c = Clove::discover(&root).unwrap();
    c.init(&Silent).expect("init");
    let x = c
        .new_clone(
            &NewOptions {
                branch: Some("feat/x".into()),
                ..Default::default()
            },
            &Silent,
        )
        .expect("new");
    let wt = x.path.clone();

    // Mid-work state: a commit, a staged and an unstaged edit, an untracked
    // file, and ignored build output.
    std::fs::write(wt.join("app.txt"), "hello\nx1\n").unwrap();
    git(&wt, &["commit", "-qam", "x1"]);
    let good_head = git(&wt, &["rev-parse", "HEAD"]);
    std::fs::write(wt.join("staged.txt"), "staged\n").unwrap();
    git(&wt, &["add", "staged.txt"]);
    std::fs::write(wt.join("app.txt"), "hello\nx1\nwip\n").unwrap();
    std::fs::write(wt.join("untracked.txt"), "u\n").unwrap();
    std::fs::create_dir_all(wt.join("build")).unwrap();
    std::fs::write(wt.join("build/out"), "built\n").unwrap();
    let status = git(&wt, &["status", "--porcelain"]);

    // Taken right after the writes, with the volume mounted: the flush must
    // carry them into the copy.
    let cp = c.checkpoint(&x.name, Some("good"), &Silent).expect("checkpoint");
    assert_eq!(cp.meta.branch.as_deref(), Some("feat/x"));
    assert_eq!(cp.meta.head.as_deref(), Some(good_head.as_str()));
    assert!(
        c.checkpoint(&x.name, Some("good"), &Silent).is_err(),
        "names are unique"
    );
    assert!(
        c.checkpoint(&x.name, Some("before-rewind"), &Silent).is_err(),
        "reserved"
    );
    assert!(c.checkpoint(&x.name, Some("a/b"), &Silent).is_err());
    let auto = c.checkpoint(&x.name, None, &Silent).unwrap();
    assert_eq!(auto.meta.id, "cp-1");
    assert_eq!(
        git(&root, &["rev-parse", "refs/clove/checkpoints/feat-x/good"]),
        good_head,
        "a ref keeps the commit from gc"
    );

    // Wreck it: ignored files gone, tracked file broken, a bad commit, the
    // index changed.
    std::fs::remove_dir_all(wt.join("deps")).unwrap();
    std::fs::remove_dir_all(wt.join("build")).unwrap();
    std::fs::write(wt.join("app.txt"), "broken\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "oops"]);
    let bad_head = git(&wt, &["rev-parse", "HEAD"]);

    // A process inside the clone blocks the rewind, which then changes nothing.
    let mut holder = Command::new("sleep").arg("30").current_dir(&wt).spawn().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let busy = c.rewind(
        &x.name,
        &RewindOptions {
            id: Some("good".into()),
            ..Default::default()
        },
        &Silent,
    );
    holder.kill().unwrap();
    let _ = holder.wait();
    assert!(busy.unwrap_err().to_string().contains("busy"));
    assert!(
        !c.checkpoint_dir(&x.name, "before-rewind").exists(),
        "a refused rewind saves nothing"
    );
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), bad_head);

    let done = c
        .rewind(
            &x.name,
            &RewindOptions {
                id: Some("good".into()),
                ..Default::default()
            },
            &Silent,
        )
        .expect("rewind");
    eprintln!("rewind took {:.0}ms: {:?}", done.total_ms, done.timings);
    assert_eq!(done.branch_moved, Some(("feat/x".into(), bad_head.clone())));
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), good_head);
    assert_eq!(git(&wt, &["branch", "--show-current"]), "feat/x");
    assert_eq!(
        git(&wt, &["status", "--porcelain"]),
        status,
        "index and files are back"
    );
    assert_eq!(read(&wt.join("app.txt")), "hello\nx1\nwip\n");
    assert_eq!(read(&wt.join("build/out")), "built\n");
    assert_eq!(read(&wt.join("deps/installed")), "v1\n");
    assert_eq!(git(&wt, &["diff", "--cached", "--name-only"]), "staged.txt");

    // The replaced state is kept, so the rewind can be undone.
    let list = c.checkpoints(&x.name).unwrap();
    let ids: Vec<&str> = list.iter().map(|i| i.meta.id.as_str()).collect();
    assert_eq!(ids, ["good", "cp-1", "before-rewind"]);
    assert_eq!(c.checkpoint_count(&x.name), 3);
    assert!(list[2].meta.auto);
    c.rewind(
        &x.name,
        &RewindOptions {
            id: Some("before-rewind".into()),
            ..Default::default()
        },
        &Silent,
    )
    .expect("undo");
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), bad_head);
    assert_eq!(read(&wt.join("app.txt")), "broken\n");
    assert!(!wt.join("deps").exists());
    // No id: the newest checkpoint made by hand, never an automatic one.
    let latest = c.rewind(&x.name, &RewindOptions::default(), &Silent).unwrap();
    assert_eq!(latest.to, "cp-1");
    assert_eq!(read(&wt.join("app.txt")), "hello\nx1\nwip\n");

    // Fork three times from the mid-work state.
    let forks = c
        .fork(
            &x.name,
            &ForkOptions {
                names: vec!["a".into(), "b".into(), "c".into()],
                ..Default::default()
            },
            &Silent,
        )
        .expect("fork");
    assert_eq!(forks.len(), 3);
    for f in &forks {
        let fw = &f.path;
        assert!(f.mounted);
        assert_eq!(f.forked_from.as_deref(), Some(x.name.as_str()));
        assert_eq!(f.branch.as_deref(), Some(format!("feat/x-{}", f.name).as_str()));
        assert_eq!(
            git(fw, &["branch", "--show-current"]),
            format!("feat/x-{}", f.name)
        );
        assert_eq!(git(fw, &["rev-parse", "HEAD"]), good_head);
        assert_eq!(
            git(fw, &["status", "--porcelain"]),
            status,
            "uncommitted work came along"
        );
        assert_eq!(read(&fw.join("build/out")), "built\n");
        assert_eq!(
            Path::new(&git(fw, &["rev-parse", "--absolute-git-dir"])),
            c.clone_admin(&f.name),
            "the fork has its own registration"
        );
        std::fs::write(fw.join("app.txt"), format!("approach {}\n", f.name)).unwrap();
        git(fw, &["commit", "-qam", &f.name]);
    }
    // They diverged; the source didn't move.
    assert_eq!(read(&wt.join("app.txt")), "hello\nx1\nwip\n");
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), good_head);
    assert_eq!(git(&wt, &["branch", "--show-current"]), "feat/x");
    assert_eq!(read(&forks[1].path.join("app.txt")), "approach b\n");
    // Name and branch clashes are refused up front.
    let again = c.fork(
        &x.name,
        &ForkOptions {
            names: vec!["a".into()],
            ..Default::default()
        },
        &Silent,
    );
    assert!(again.unwrap_err().to_string().contains("already exists"));
    let named = c
        .fork(
            "b",
            &ForkOptions {
                names: vec!["b2".into()],
                branch: Some("winner".into()),
                ..Default::default()
            },
            &Silent,
        )
        .unwrap();
    assert_eq!(git(&named[0].path, &["log", "-1", "--format=%s"]), "b");
    assert_eq!(git(&named[0].path, &["branch", "--show-current"]), "winner");
    assert_eq!(named[0].forked_from.as_deref(), Some("b"));
    // A generated name: the first letter free for clone and branch alike.
    let auto = c.fork_auto(&x.name, &Silent).unwrap();
    assert_eq!(
        (auto.name.as_str(), auto.branch.as_deref()),
        ("feat-x-d", Some("feat/x-d"))
    );

    // Removing a clone takes its checkpoints and their refs with it.
    c.remove(
        &x.name,
        &RemoveOptions {
            force: true,
            ..Default::default()
        },
        &Silent,
    )
    .unwrap();
    assert!(git(&root, &["for-each-ref", "refs/clove/checkpoints/feat-x"]).is_empty());
    for n in ["a", "b", "c", "b2", "feat-x-d"] {
        c.remove(
            n,
            &RemoveOptions {
                force: true,
                ..Default::default()
            },
            &Silent,
        )
        .unwrap();
    }
    assert!(c.gc(false).unwrap().is_empty(), "nothing left behind");
}
