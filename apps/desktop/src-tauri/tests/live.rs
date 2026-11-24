//! Drives the desktop commands against real disk images in a temp state dir,
//! and checks the JSON the frontend reads. Creates and mounts APFS images, so
//! it only runs on request: `cargo test -- --ignored`.

#![cfg(target_os = "macos")]

use clove_core::Event;
use clove_desktop_lib::{commands, ops};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use tauri::async_runtime::block_on;

fn sh(dir: &Path, script: &str) {
    let out = Command::new("sh")
        .arg("-c")
        .arg(script)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{script}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn keys(v: &Value) -> Vec<String> {
    v.as_object().unwrap().keys().cloned().collect()
}

#[test]
#[ignore = "creates and mounts disk images"]
fn commands_against_a_fixture_repo() {
    let tmp = tempfile::Builder::new()
        .prefix("clove-desktop")
        .tempdir()
        .unwrap();
    let t = tmp.path().canonicalize().unwrap();
    // SAFETY: the only test in this binary, before any threads start.
    unsafe {
        std::env::set_var("CLOVE_HOME", t.join("state"));
        std::env::set_var("CLOVE_CLONE_ROOT", t.join("clones"));
        std::env::set_var("CLOVE_CONFIG", t.join("no-user-config.toml"));
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
        for k in ["GIT_AUTHOR_NAME", "GIT_COMMITTER_NAME"] {
            std::env::set_var(k, "t");
        }
        for k in ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"] {
            std::env::set_var(k, "t@t");
        }
    }
    sh(
        &t,
        "git init -q --bare -b main remote.git && git clone -q remote.git seed",
    );
    let seed = t.join("seed");
    std::fs::write(
        seed.join(".clove.toml"),
        "image_size = \"2G\"\n[[step]]\nname = \"deps\"\nwarm = \"mkdir -p deps && echo warm && cp lock.txt deps/\"\nsetup = \"cp lock.txt deps/\"\nwatch = [\"lock.txt\"]\n",
    )
    .unwrap();
    std::fs::write(seed.join(".gitignore"), "deps/\n").unwrap();
    std::fs::write(seed.join("lock.txt"), "v1\n").unwrap();
    sh(
        &seed,
        "git add -A && git commit -qm init && git push -q origin main",
    );
    sh(&t, "git clone -q remote.git app");
    let root = t.join("app");

    let events = Mutex::new(Vec::<Event>::new());
    let r = |e: Event| events.lock().unwrap().push(e);

    let found = block_on(commands::discover(root.clone())).unwrap();
    assert!(!found.initialized);
    assert_eq!(found.steps, vec!["deps"]);

    ops::run(
        &serde_json::from_value(serde_json::json!({"kind": "init", "root": root})).unwrap(),
        &r,
    )
    .expect("init");
    let steps: Vec<String> = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            Event::Done { step, .. } => Some(step.clone()),
            _ => None,
        })
        .collect();
    assert!(steps.contains(&"warm deps".to_string()), "{steps:?}");

    let repos = serde_json::to_value(block_on(commands::repos()).unwrap()).unwrap();
    assert_eq!(repos[0]["name"], "app");
    assert_eq!(repos[0]["generation"], 1);

    let status = serde_json::to_value(block_on(commands::repo_status(root.clone())).unwrap()).unwrap();
    for k in [
        "generation",
        "commit",
        "age_secs",
        "behind",
        "image_bytes",
        "timings",
    ] {
        assert!(keys(&status["golden"]).contains(&k.to_string()), "golden.{k}");
    }

    let new = ops::run(
        &serde_json::from_value(serde_json::json!({"kind": "new", "root": root, "branch": "feat/x"}))
            .unwrap(),
        &r,
    )
    .expect("new");
    assert_eq!(new["name"], "feat-x");
    assert_eq!(new["mounted"], true);

    let refs = block_on(commands::refs(root.clone())).unwrap();
    assert!(refs.refs.iter().any(|x| x.name == "feat/x" && x.kind == "local"));
    assert_eq!(refs.base, "origin/main");

    let clones = serde_json::to_value(block_on(commands::clones(root.clone(), true)).unwrap()).unwrap();
    assert_eq!(clones[0]["dirty"], false);

    block_on(commands::unmount_clone(root.clone(), "feat-x".into(), false)).unwrap();
    assert!(!block_on(commands::clones(root.clone(), false)).unwrap()[0].mounted);
    block_on(commands::mount_clone(root.clone(), "feat-x".into())).unwrap();

    let doctor = block_on(commands::doctor_repo(root.clone())).unwrap();
    assert!(doctor.iter().any(|c| c.name == "golden"));
    assert!(block_on(commands::gc(root.clone(), false)).unwrap().is_empty());

    ops::run(
        &serde_json::from_value(serde_json::json!({
            "kind": "remove", "root": root, "name": "feat-x", "delete_branch": true
        }))
        .unwrap(),
        &r,
    )
    .expect("remove");
    ops::run(
        &serde_json::from_value(serde_json::json!({"kind": "deinit", "root": root})).unwrap(),
        &r,
    )
    .expect("deinit");
    assert!(block_on(commands::repos()).unwrap().is_empty());
}
