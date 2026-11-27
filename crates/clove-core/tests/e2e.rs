//! End-to-end on real disk images. One scenario, because the env vars that
//! point clove at a temp state dir are process-wide. macOS only for now: it
//! drives diskutil, and the btrfs backend gets its own once its design settles.

#![cfg(target_os = "macos")]

use clove_core::opencode::{WorktreeEntry, list_worktrees};
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
        std::env::set_var("CLOVE_CLONE_ROOT", t.join("app/clones"));
        std::env::set_var("CLOVE_CONFIG", t.join("no-user-config.toml"));
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
        std::env::set_var("GIT_DISCOVERY_ACROSS_FILESYSTEM", "1");
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
    std::fs::write(seed.join(".gitignore"), "deps/\n.env\nclones/\n").unwrap();
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

    // the preview reads only
    let p = clove_core::preview::init_preview(&root).unwrap();
    assert!(!p.initialized);
    assert_eq!(p.base, "origin/main");
    assert!(p.base_found);
    assert_eq!(p.steps.len(), 1);
    assert!(!p.steps_detected);
    assert_eq!(p.copy, vec![".env".to_string()]);
    assert_eq!(p.image_size, "2G");
    assert!(!c.state.exists());
    let offered = |exclude| {
        clove_core::candidates::scan(&clove_core::candidates::ScanOptions {
            roots: vec![t.clone()],
            max_depth: 1,
            budget: std::time::Duration::from_secs(10),
            exclude,
        })
        .into_iter()
        .map(|c| c.name)
        .collect::<Vec<_>>()
    };
    let managed = || {
        clove_core::known_repos()
            .into_iter()
            .map(|r| r.root)
            .collect::<std::collections::BTreeSet<_>>()
    };
    let mut before = offered(managed());
    before.sort();
    assert_eq!(before, ["app", "seed"], "remote.git is bare");

    // init
    let g = c.init(&Silent).expect("init");
    assert!(clove_core::preview::init_preview(&root).unwrap().initialized);
    assert_eq!(offered(managed()), ["seed"], "managed repos aren't offered");
    assert_eq!(g.generation, 1);
    assert!(!g.dirty, "golden should be clean");
    assert_eq!(g.copied, vec![".env".to_string()]);
    assert!(!c.freshness().unwrap().unwrap().stale, "fresh right after init");

    // A broken snapshot fails after reserving the clone and copying its image.
    let snapshot_head = c.golden_gen_dir(g.generation).join("HEAD");
    let saved_head = snapshot_head.with_extension("saved");
    std::fs::rename(&snapshot_head, &saved_head).unwrap();
    let result = c.new_clone_with_request(
        &NewOptions {
            name: Some("broken-snapshot".into()),
            ..Default::default()
        },
        Some("broken-snapshot-request"),
        &Silent,
    );
    std::fs::rename(&saved_head, &snapshot_head).unwrap();
    assert!(result.unwrap_err().to_string().contains("copy HEAD from"));
    assert!(!c.clone_dir("broken-snapshot").exists());
    assert!(!c.clone_mount("broken-snapshot").exists());

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
    assert!(
        c.new_clone(
            &NewOptions {
                branch: Some("feat/a".into()),
                ..Default::default()
            },
            &Silent,
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(a.path.join("deps/installed")).unwrap(),
        "v1\n"
    );
    assert!(!a.path.join("deps/setup-ran").exists(), "setup should be skipped");
    assert_eq!(git(&a.path, &["status", "--porcelain"]), "", "clone starts clean");
    let root_entry = WorktreeEntry {
        directory: root.clone(),
        kind: "root",
    };
    assert_eq!(
        list_worktrees(&root).unwrap(),
        [
            root_entry.clone(),
            WorktreeEntry {
                directory: a.path.clone(),
                kind: "worktree",
            },
        ]
    );
    c.require_clean_clone(&a.name).unwrap();
    let link = a.path.join(".git");
    let gitfile = std::fs::read(&link).unwrap();
    std::fs::remove_file(&link).unwrap();
    // Git finds the clean parent repo, even though this is a different checkout.
    assert_eq!(
        git(&a.path, &["rev-parse", "--show-toplevel"]),
        root.to_str().unwrap()
    );
    assert_eq!(git(&a.path, &["status", "--porcelain"]), "");
    let err = c.require_clean_clone(&a.name).unwrap_err();
    assert!(err.to_string().contains("cannot safely inspect"));
    assert!(!format!("{err:#}").contains("uncommitted changes"));
    std::fs::write(&link, "gitdir: missing\n").unwrap();
    let err = c.require_clean_clone(&a.name).unwrap_err();
    assert!(err.to_string().contains("cannot safely inspect"));
    assert!(!format!("{err:#}").contains("uncommitted changes"));
    std::fs::write(&link, gitfile).unwrap();
    c.unmount(&a.name, false).unwrap();
    assert_eq!(list_worktrees(&root).unwrap(), [root_entry]);
    c.mount(&a.name).unwrap();
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
            unlock: false,
            ..Default::default()
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
            unlock: false,
            ..Default::default()
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

    // Branch suggestions for `new`: not the base, not one a clone has.
    let cands: Vec<String> = c.branch_candidates().into_iter().map(|b| b.name).collect();
    assert!(!cands.iter().any(|b| b == "main" || b == "feat/a"), "{cands:?}");

    // Broken outside clove: registration deleted (git worktree remove -f -f,
    // prune), or the worktree deleted while mounted. Both are flagged, and gc
    // finishes removing them.
    for (name, how) in [("gone-admin", "unregistered"), ("gone-tree", "worktree deleted")] {
        let b = c
            .new_clone(
                &NewOptions {
                    detach: true,
                    name: Some(name.into()),
                    ..Default::default()
                },
                &Silent,
            )
            .unwrap();
        if name == "gone-admin" {
            std::fs::remove_dir_all(root.join(".git/worktrees").join(format!("clove-{name}"))).unwrap();
        } else {
            std::fs::remove_dir_all(&b.path).unwrap();
        }
        let issue = c.info(name, false).unwrap().issue.unwrap_or_default();
        assert!(issue.contains(how), "{name}: {issue}");
    }
    let plan = c.gc(false).unwrap();
    assert!(
        plan.iter().any(|a| a.contains("remove clone gone-admin")),
        "{plan:?}"
    );
    assert!(
        plan.iter().any(|a| a.contains("remove clone gone-tree")),
        "{plan:?}"
    );
    c.gc(true).unwrap();
    let names = c.clone_names().unwrap();
    assert!(!names.iter().any(|n| n.starts_with("gone-")), "{names:?}");
    // A healthy clone's lock tells git users what to run instead.
    let lock = std::fs::read_to_string(root.join(".git/worktrees/clove-peek/locked")).unwrap();
    assert!(lock.contains("clove rm peek"), "{lock}");

    // Auto-prune: a merged, clean clone goes once git has been quiet in it
    // for an hour; a just-touched one stays. Each removal is in the history.
    for b in ["done/old", "done/fresh"] {
        let info = c
            .new_clone(
                &NewOptions {
                    branch: Some(b.into()),
                    ..Default::default()
                },
                &Silent,
            )
            .unwrap();
        sh(
            &info.path,
            &format!("echo {b} > done.txt && git add done.txt && git commit -qm {b}"),
        );
    }
    let checks = c
        .merge_checks_with(false, |asks| {
            let prs = asks
                .iter()
                .filter(|(_, b)| b.starts_with("done/"))
                .map(|(k, b)| {
                    let pr = clove_core::Pr {
                        number: 7,
                        state: clove_core::PrState::Merged,
                        head_oid: git(&root, &["rev-parse", b]),
                        title: String::new(),
                        url: String::new(),
                    };
                    (k.clone(), pr)
                })
                .collect();
            (prs, None)
        })
        .unwrap()
        .clones;
    sh(
        &root,
        &format!(
            "touch -t 202001010000 {d} {a}/index {a}/HEAD {a}/logs/HEAD",
            d = c.clone_dir("done-old").display(),
            a = c.clone_admin("done-old").display()
        ),
    );
    let gone = c.auto_prune_checks(&checks, &Silent);
    assert_eq!(gone, ["done-old"], "{checks:?}");
    let h = c.history();
    assert_eq!(
        (h[0].name.as_str(), h[0].by),
        ("done-old", clove_core::history::By::AutoPrune)
    );
    assert!(h[0].branch_deleted && h[0].restore().is_some(), "{:?}", h[0]);
    let why = c.resolve("done/old").unwrap_err().to_string();
    assert!(
        why.contains("auto-pruned done-old") && why.contains("git branch done/old"),
        "{why}"
    );
    c.remove("done-fresh", &RemoveOptions::default(), &Silent)
        .unwrap();

    // Update: the clone's branch comes onto the new base and setup reruns
    // for the changed lockfile. A conflicting edit stops with the file
    // listed; abort puts the branch back; continue finishes once fixed.
    {
        use clove_core::update::{Strategy, Update, UpdateOptions};
        let read = |p: std::path::PathBuf| std::fs::read_to_string(p).unwrap();
        let up = |opts: &UpdateOptions| c.update("feat-up", opts, &Silent);
        let u = c
            .new_clone(
                &NewOptions {
                    branch: Some("feat/up".into()),
                    ..Default::default()
                },
                &Silent,
            )
            .unwrap();
        sh(
            &u.path,
            "echo mine > mine.txt && git add mine.txt && git commit -qm mine",
        );
        sh(
            &seed,
            "git pull -q --rebase origin main && echo v-update > lock.txt && git commit -qam lock && git push -q origin main",
        );
        let r = up(&UpdateOptions::default()).unwrap();
        assert!(matches!(r, Update::Updated { commits: Some(1), .. }), "{r:?}");
        assert_eq!(
            read(u.path.join("deps/installed")),
            "v-update\n",
            "setup reran for the new lockfile"
        );
        assert!(matches!(
            up(&UpdateOptions::default()).unwrap(),
            Update::UpToDate { .. }
        ));

        sh(&u.path, "echo clone > app.txt && git commit -qam clone-edit");
        sh(
            &seed,
            "echo base > app.txt && git commit -qam base && git push -q origin main",
        );
        match up(&UpdateOptions::default()).unwrap() {
            Update::Conflict { files, strategy, .. } => {
                assert_eq!(files, ["app.txt"]);
                assert_eq!(strategy, Strategy::Rebase);
            }
            other => panic!("expected a conflict: {other:?}"),
        }
        assert_eq!(
            c.info("feat-up", false).unwrap().operation,
            Some(Strategy::Rebase)
        );
        assert!(
            up(&UpdateOptions::default()).is_err(),
            "mid-rebase: update refuses"
        );
        c.update_abort("feat-up").unwrap();
        assert_eq!(c.info("feat-up", false).unwrap().operation, None);
        assert_eq!(
            read(u.path.join("app.txt")),
            "clone\n",
            "abort puts the branch back"
        );

        assert!(matches!(
            up(&UpdateOptions::default()).unwrap(),
            Update::Conflict { .. }
        ));
        // The resolver runs in the clone with the prompt as its last argument.
        let prompt = c.resolve_prompt("feat-up").unwrap();
        assert!(
            prompt.contains("app.txt") && prompt.contains("clove update --continue feat-up"),
            "{prompt}"
        );
        let mut with_resolver = Clove::open(&root).unwrap();
        with_resolver.config.update.resolver = Some("printf %s > resolver-saw.txt".into());
        let (who, mut cmd) = with_resolver.resolver_command("feat-up").unwrap();
        assert_eq!(who, "Printf");
        assert!(cmd.status().unwrap().success());
        assert_eq!(read(u.path.join("resolver-saw.txt")), prompt);
        std::fs::remove_file(u.path.join("resolver-saw.txt")).unwrap();
        std::fs::write(u.path.join("app.txt"), "both\n").unwrap();
        assert!(c.update_continue("feat-up", &Silent).is_err(), "not added yet");
        sh(&u.path, "git add app.txt");
        let r = c.update_continue("feat-up", &Silent).unwrap();
        assert!(matches!(r, Update::Updated { commits: None, .. }), "{r:?}");
        assert_eq!(git(&u.path, &["log", "-1", "--format=%s"]), "clone-edit");
        assert_eq!(c.info("feat-up", false).unwrap().operation, None);

        // Uncommitted changes: refused, unless set aside and reapplied.
        sh(
            &seed,
            "echo x > other.txt && git add other.txt && git commit -qm other && git push -q origin main",
        );
        std::fs::write(u.path.join("mine.txt"), "dirty\n").unwrap();
        let e = up(&UpdateOptions::default()).unwrap_err().to_string();
        assert!(e.contains("uncommitted"), "{e}");
        let r = up(&UpdateOptions {
            autostash: true,
            ..Default::default()
        })
        .unwrap();
        assert!(matches!(r, Update::Updated { .. }), "{r:?}");
        assert_eq!(read(u.path.join("mine.txt")), "dirty\n");
        assert!(u.path.join("other.txt").exists());
        c.remove(
            "feat-up",
            &RemoveOptions {
                force: true,
                delete_branch: true,
                ..Default::default()
            },
            &Silent,
        )
        .unwrap();
    }

    // remove refuses dirty clones without --force
    std::fs::write(a.path.join("app.txt"), "dirty\n").unwrap();
    assert!(
        c.require_clean_clone(&a.name)
            .unwrap_err()
            .to_string()
            .contains("has uncommitted changes")
    );
    assert!(c.remove("feat-a", &RemoveOptions::default(), &Silent).is_err());
    c.remove(
        "feat-a",
        &RemoveOptions {
            force: true,
            delete_branch: true,
            branch_merged: false,
            unlock: false,
            ..Default::default()
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
