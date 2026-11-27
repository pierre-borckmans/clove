//! Moves Python virtualenvs that came from the golden image to the clone.
//!
//! A venv records absolute paths: script shebangs, `activate`, and editable
//! installs (`.pth`, setuptools' `__editable__` finders, `direct_url.json`).
//! In a clone they still name the golden worktree, so `uv sync` rebuilds every
//! editable package (a PEP 517 build each, ~0.5s) and `.venv/bin` scripts run
//! the golden's interpreter. Rewriting those few text files instead leaves
//! `uv sync` with nothing to do.

use crate::git;
use std::path::{Path, PathBuf};

/// Rewrites the venvs next to each tracked `uv.lock` in `wt` that were made in
/// another worktree of the repo. Returns how many venvs moved.
pub fn relocate(wt: &Path) -> usize {
    let Ok(locks) = git::out(wt, &["ls-files", "--", ":(glob)uv.lock", ":(glob)**/uv.lock"]) else {
        return 0;
    };
    locks
        .lines()
        .filter_map(|l| Path::new(l).parent())
        .filter(|rel| relocate_one(wt, rel))
        .count()
}

fn relocate_one(wt: &Path, rel: &Path) -> bool {
    let venv = wt.join(rel).join(".venv");
    let Some(old_venv) = recorded_venv(&venv) else {
        return false;
    };
    // The worktree the venv was made in: its recorded path minus `<rel>/.venv`.
    let Some(old_root) = old_venv
        .strip_suffix("/.venv")
        .and_then(|p| p.strip_suffix(&*rel.to_string_lossy()))
        .map(|p| p.trim_end_matches('/').to_string())
    else {
        return false;
    };
    let new_root = wt.to_string_lossy().trim_end_matches('/').to_string();
    if old_root.is_empty() || old_root == new_root {
        return false;
    }
    let (from, to) = (format!("{old_root}/"), format!("{new_root}/"));
    let mut moved = false;
    for f in candidates(&venv) {
        let Ok(text) = std::fs::read(&f) else { continue };
        if text.contains(&0) {
            continue;
        }
        let s = String::from_utf8_lossy(&text);
        if s.contains(&from) {
            moved |= std::fs::write(&f, s.replace(&from, &to)).is_ok();
        }
    }
    moved
}

/// Where the venv was made, as its `activate` script says.
fn recorded_venv(venv: &Path) -> Option<String> {
    let s = std::fs::read_to_string(venv.join("bin/activate")).ok()?;
    s.lines().find_map(|l| {
        let v = l.trim().strip_prefix("VIRTUAL_ENV=")?;
        let v = v.trim_matches(|c| c == '\'' || c == '"');
        v.starts_with('/').then(|| v.to_string())
    })
}

/// The files of a venv that hold absolute paths: scripts, the config,
/// and what editable installs leave in site-packages.
fn candidates(venv: &Path) -> Vec<PathBuf> {
    let mut out = vec![venv.join("pyvenv.cfg")];
    let files = |dir: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    };
    // Symlinks (`python` → the interpreter) stay as they are.
    let regular = |p: &PathBuf| p.symlink_metadata().is_ok_and(|m| m.is_file());
    out.extend(files(&venv.join("bin")).into_iter().filter(regular));
    for lib in files(&venv.join("lib")) {
        for p in files(&lib.join("site-packages")) {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.ends_with(".pth") || name.starts_with("__editable__") {
                out.push(p);
            } else if name.ends_with(".dist-info") {
                out.push(p.join("direct_url.json"));
            }
        }
    }
    out.retain(regular);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_paths_from_the_golden_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("clone/app");
        let old = "/golden/app";
        let pkg = wt.join("pkg");
        let venv = pkg.join(".venv");
        let sp = venv.join("lib/python3.15/site-packages");
        std::fs::create_dir_all(venv.join("bin")).unwrap();
        std::fs::create_dir_all(sp.join("demo-0.1.dist-info")).unwrap();
        std::fs::write(
            venv.join("bin/activate"),
            format!("VIRTUAL_ENV='{old}/pkg/.venv'\nexport VIRTUAL_ENV\n"),
        )
        .unwrap();
        std::fs::write(
            venv.join("bin/tool"),
            format!("#!{old}/pkg/.venv/bin/python\nimport x\n"),
        )
        .unwrap();
        std::os::unix::fs::symlink("/usr/bin/python3", venv.join("bin/python")).unwrap();
        std::fs::write(
            sp.join("__editable___demo_finder.py"),
            format!("MAPPING = {{'demo': '{old}/pkg/demo'}}\n"),
        )
        .unwrap();
        std::fs::write(
            sp.join("demo-0.1.dist-info/direct_url.json"),
            format!("{{\"url\": \"file://{old}/pkg\"}}"),
        )
        .unwrap();
        std::fs::write(sp.join("other.py"), format!("'{old}/pkg' stays\n")).unwrap();
        crate::git::run_ok(wt.parent().unwrap(), &["init", "-q", "app"]).unwrap();
        std::fs::write(pkg.join("uv.lock"), "").unwrap();
        crate::git::run_ok(&wt, &["add", "pkg/uv.lock"]).unwrap();

        assert_eq!(relocate(&wt), 1);
        let new = wt.to_string_lossy();
        let read = |p: PathBuf| std::fs::read_to_string(p).unwrap();
        assert!(read(venv.join("bin/activate")).contains(&format!("'{new}/pkg/.venv'")));
        assert!(read(venv.join("bin/tool")).starts_with(&format!("#!{new}/pkg/.venv/bin/python")));
        assert!(read(sp.join("__editable___demo_finder.py")).contains(&format!("'{new}/pkg/demo'")));
        assert!(read(sp.join("demo-0.1.dist-info/direct_url.json")).contains(&format!("file://{new}/pkg")));
        assert!(read(sp.join("other.py")).contains(old));
        assert_eq!(
            std::fs::read_link(venv.join("bin/python")).unwrap(),
            Path::new("/usr/bin/python3")
        );
        // Already moved: nothing to do.
        assert_eq!(relocate(&wt), 0);
    }
}
