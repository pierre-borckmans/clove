//! Detects how to warm a repo from the files it contains.

use crate::config::{Exec, Step};
use crate::util::which;
use std::path::Path;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Steps in run order: environment managers first, then package managers
/// (which run inside the environment the first ones set up).
pub fn steps(root: &Path) -> Vec<Step> {
    let has = |f: &str| root.join(f).exists();
    let mut out = vec![];

    if has("mise.toml") || has(".mise.toml") || has(".tool-versions") {
        // Trust is keyed by path, so every clone needs it; it is instant.
        out.push(Step {
            name: "mise-trust".into(),
            warm: Some("mise trust --quiet --yes .".into()),
            setup: Some("mise trust --quiet --yes .".into()),
            wrap: false,
            ..Default::default()
        });
        // Tools install once into the golden image and rerun when mise's config changes.
        out.push(Step {
            name: "mise".into(),
            warm: Some("mise install --yes".into()),
            watch: s(&["mise.toml", ".mise.toml", ".tool-versions", "mise.lock", ".mise"]),
            wrap: false,
            ..Default::default()
        });
    }

    if has(".envrc") {
        // `direnv allow` is keyed by path, so every clone needs it; it is instant.
        out.push(Step {
            name: "direnv-allow".into(),
            warm: Some("direnv allow .".into()),
            setup: Some("direnv allow .".into()),
            wrap: false,
            ..Default::default()
        });
        // The cached environment (e.g. nix-direnv's .direnv/) comes along in the
        // image; it only needs rebuilding when its inputs changed.
        out.push(Step {
            name: "direnv".into(),
            warm: Some("direnv exec . true".into()),
            setup: Some("direnv exec . true".into()),
            watch: s(&[
                ".envrc",
                "flake.nix",
                "flake.lock",
                "shell.nix",
                "default.nix",
                "devenv.nix",
                "devenv.lock",
                "devbox.json",
            ]),
            wrap: false,
            ..Default::default()
        });
    }

    // Package managers per project dir: the root, plus nested projects with
    // their own lockfile (a Tauri app's `src-tauri`, a `site/`). Installs first,
    // then builds, which may need them.
    let nested = nested_projects(root);
    let dirs: Vec<String> = std::iter::once(String::new())
        .chain(if nested.len() <= MAX_NESTED {
            nested
        } else {
            vec![]
        })
        .collect();
    for d in &dirs {
        out.extend(js_step(root, d));
    }
    for d in &dirs {
        if root.join(d).join("uv.lock").exists() {
            // Virtualenvs embed absolute paths, so each clone re-syncs (fast with uv's cache).
            out.push(in_dir(
                d,
                Step {
                    name: "uv".into(),
                    warm: Some("uv sync --frozen".into()),
                    setup: Some("uv sync --frozen".into()),
                    ..Default::default()
                },
            ));
        }
    }
    for d in &dirs {
        if root.join(d).join("Cargo.lock").exists() {
            // `target/` comes along in the image, so clones start with every
            // dependency built. No setup: cargo builds incrementally on use.
            out.push(in_dir(
                d,
                Step {
                    name: "cargo".into(),
                    warm: Some("cargo build --workspace --all-targets".into()),
                    ..Default::default()
                },
            ));
        }
    }

    out
}

fn js_step(root: &Path, dir: &str) -> Option<Step> {
    let has = |f: &str| root.join(dir).join(f).exists();
    let step = if has("pnpm-lock.yaml") {
        Step {
            name: "pnpm".into(),
            warm: Some("pnpm install --frozen-lockfile".into()),
            // Only runs when the lockfile changed; new packages may need the network.
            setup: Some("pnpm install --prefer-offline --frozen-lockfile".into()),
            watch: s(&["pnpm-lock.yaml", "pnpm-workspace.yaml", ".pnpmfile.cjs"]),
            ..Default::default()
        }
    } else if has("bun.lock") || has("bun.lockb") {
        Step {
            name: "bun".into(),
            warm: Some("bun install --frozen-lockfile".into()),
            setup: Some("bun install --frozen-lockfile".into()),
            watch: s(&["bun.lock", "bun.lockb"]),
            ..Default::default()
        }
    } else if has("yarn.lock") {
        let cmd = if has(".yarnrc.yml") {
            "yarn install --immutable"
        } else {
            "yarn install --frozen-lockfile"
        };
        Step {
            name: "yarn".into(),
            warm: Some(cmd.into()),
            setup: Some(cmd.into()),
            watch: s(&["yarn.lock", ".yarnrc.yml"]),
            ..Default::default()
        }
    } else if has("package-lock.json") {
        Step {
            name: "npm".into(),
            warm: Some("npm ci --no-audit --no-fund".into()),
            setup: Some("npm ci --prefer-offline --no-audit --no-fund".into()),
            watch: s(&["package-lock.json"]),
            ..Default::default()
        }
    } else {
        return None;
    };
    Some(in_dir(dir, step))
}

const LOCKFILES: [&str; 7] = [
    "pnpm-lock.yaml",
    "bun.lock",
    "bun.lockb",
    "yarn.lock",
    "package-lock.json",
    "uv.lock",
    "Cargo.lock",
];

/// Dirs below this are samples or copies, not projects to build.
const NOT_PROJECTS: [&str; 10] = [
    "node_modules",
    "vendor",
    "third_party",
    "fixtures",
    "__fixtures__",
    "testdata",
    "test",
    "tests",
    "examples",
    "target",
];

/// Nested projects are warmed only when there are this few (an app and a site,
/// say). A monorepo with dozens would build all of them: there they are left
/// to the config, and `clove doctor` lists them.
pub const MAX_NESTED: usize = 3;

/// Dirs below `root`, up to three levels down, holding a lockfile git tracks.
/// Untracked and ignored files don't count, so built or vendored copies are
/// left out.
pub fn nested_projects(root: &Path) -> Vec<String> {
    let mut dirs = std::collections::BTreeSet::new();
    let patterns: Vec<String> = LOCKFILES.iter().map(|f| format!("**/{f}")).collect();
    let listed = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--"])
        .args(&patterns)
        .output();
    if let Some(o) = listed.ok().filter(|o| o.status.success()) {
        for f in o.stdout.split(|b| *b == 0).filter(|f| !f.is_empty()) {
            let f = String::from_utf8_lossy(f);
            let Some((dir, _)) = f.rsplit_once('/') else {
                continue;
            };
            let parts: Vec<&str> = dir.split('/').collect();
            if parts.len() <= 3 && !parts.iter().any(|p| NOT_PROJECTS.contains(p)) {
                dirs.insert(dir.to_string());
            }
        }
    }
    dirs.into_iter().collect()
}

/// Runs `step` in `dir`: its name gets the dir (`pnpm-apps-desktop`, safe in
/// log file names), its commands `cd` there, its watch paths are prefixed.
fn in_dir(dir: &str, mut step: Step) -> Step {
    if dir.is_empty() {
        return step;
    }
    step.name = format!("{}-{}", step.name, dir.replace('/', "-"));
    let cd = format!("cd {} && ", shell_quote(dir));
    step.warm = step.warm.map(|c| format!("{cd}{c}"));
    step.setup = step.setup.map(|c| format!("{cd}{c}"));
    step.watch = step.watch.iter().map(|w| format!("{dir}/{w}")).collect();
    step
}

fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

pub fn resolve_exec(exec: Exec, root: &Path) -> Exec {
    if exec != Exec::Auto {
        return exec;
    }
    if root.join(".envrc").exists() && which("direnv").is_some() {
        Exec::Direnv
    } else if (root.join("mise.toml").exists() || root.join(".mise.toml").exists()) && which("mise").is_some()
    {
        Exec::Mise
    } else {
        Exec::None
    }
}

/// Wraps a shell command so it runs inside the repo's environment.
pub fn wrap(exec: Exec, command: &str) -> Vec<String> {
    match exec {
        Exec::Direnv => s(&["direnv", "exec", ".", "sh", "-c", command]),
        Exec::Mise => s(&["mise", "exec", "--", "sh", "-c", command]),
        Exec::None | Exec::Auto => s(&["sh", "-c", command]),
    }
}

/// Tools a step needs on PATH, for `clove doctor`.
pub fn required_tool(step: &Step) -> Option<&'static str> {
    // Nested steps are named `<tool>-<dir>`.
    match step.name.split('-').next().unwrap_or_default() {
        "mise" => Some("mise"),
        "direnv" => Some("direnv"),
        "cargo" => Some("cargo"),
        "pnpm" => Some("pnpm"),
        "bun" => Some("bun"),
        "yarn" => Some("yarn"),
        "npm" => Some("npm"),
        "uv" => Some("uv"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_in_order() {
        let d = tempfile::tempdir().unwrap();
        for f in ["mise.toml", ".envrc", "pnpm-lock.yaml"] {
            std::fs::write(d.path().join(f), "").unwrap();
        }
        let names: Vec<_> = steps(d.path()).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["mise-trust", "mise", "direnv-allow", "direnv", "pnpm"]);
    }

    #[test]
    fn detects_rust_and_nested_projects() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        let files = [
            "Cargo.lock",
            "pnpm-lock.yaml",
            "apps/desktop/pnpm-lock.yaml",
            "apps/desktop/src-tauri/Cargo.lock",
            "site/package-lock.json",
            "tests/fixtures/x/Cargo.lock",
            "a/b/c/d/Cargo.lock",
        ];
        for f in files {
            std::fs::create_dir_all(r.join(f).parent().unwrap()).unwrap();
            std::fs::write(r.join(f), "").unwrap();
        }
        // Untracked lockfiles don't count.
        std::fs::create_dir_all(r.join("scratch")).unwrap();
        std::fs::write(r.join("scratch/yarn.lock"), "").unwrap();
        let git = |a: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(r)
                    .args(a)
                    .output()
                    .unwrap()
                    .status
                    .success()
            )
        };
        git(&["init", "-q"]);
        git(&[
            "add",
            "--",
            "Cargo.lock",
            "pnpm-lock.yaml",
            "apps",
            "site",
            "tests",
            "a",
        ]);

        let steps = steps(r);
        let names: Vec<_> = steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "pnpm",
                "pnpm-apps-desktop",
                "npm-site",
                "cargo",
                "cargo-apps-desktop-src-tauri"
            ]
        );
        let desk = &steps[1];
        assert_eq!(
            desk.warm.as_deref(),
            Some("cd apps/desktop && pnpm install --frozen-lockfile")
        );
        assert!(desk.watch.contains(&"apps/desktop/pnpm-lock.yaml".to_string()));
        assert_eq!(required_tool(desk), Some("pnpm"));
        assert_eq!(required_tool(&steps[4]), Some("cargo"));
        assert!(steps[3].setup.is_none(), "cargo builds incrementally in clones");
    }

    #[test]
    fn a_monorepo_warms_only_its_root() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        std::fs::write(r.join("pnpm-lock.yaml"), "").unwrap();
        for i in 0..=MAX_NESTED {
            let p = r.join(format!("packages/p{i}"));
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("Cargo.lock"), "").unwrap();
        }
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(r)
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success()
            && std::process::Command::new("git")
                .arg("-C")
                .arg(r)
                .args(["add", "."])
                .status()
                .unwrap()
                .success();
        assert!(ok);
        let names: Vec<_> = steps(r).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["pnpm"]);
        assert_eq!(nested_projects(r).len(), MAX_NESTED + 1);
    }

    #[test]
    fn without_git_only_the_root_counts() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("sub")).unwrap();
        std::fs::write(d.path().join("sub/Cargo.lock"), "").unwrap();
        std::fs::write(d.path().join("Cargo.lock"), "").unwrap();
        let names: Vec<_> = steps(d.path()).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["cargo"]);
    }

    #[test]
    fn empty_repo_has_no_steps() {
        let d = tempfile::tempdir().unwrap();
        assert!(steps(d.path()).is_empty());
    }
}
