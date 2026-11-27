//! Detects how to warm a repo from the files it contains.

use crate::config::{Exec, Step};
use crate::toolchains::Toolchain;
use crate::util::which;
use std::path::Path;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Steps in run order: environment managers first, then package managers
/// (which run inside the environment the first ones set up). `max_installs`
/// caps nested projects whose dependencies are installed (see [`plan`]).
/// Reused while the root listing and the index are unchanged.
pub fn steps(root: &Path, max_installs: usize) -> Vec<Step> {
    use crate::memo::Dep;
    let deps = [Dep::file(root), Dep::file(root.join(".git/index"))];
    crate::memo::get(
        &format!("detect:{}:{max_installs}", root.display()),
        &deps,
        || find_steps(root, max_installs),
    )
}

/// Which nested projects detection warms, and which it leaves to the config.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct NestedPlan {
    /// Dirs whose dependencies are installed (`Kind::Install` toolchains).
    pub installs: Vec<String>,
    /// In groups (js, python, go, ...) with more nested projects than
    /// `nested_installs`: none of a group's are, and these are named instead.
    pub skipped_installs: Vec<String>,
    /// Those groups, with how many nested projects each has.
    pub skipped_groups: Vec<(String, usize)>,
    /// Dirs built in the golden image (`Kind::Build` toolchains).
    pub builds: Vec<String>,
    /// Over [`MAX_NESTED_BUILDS`].
    pub skipped_builds: Vec<String>,
}

impl NestedPlan {
    pub fn skipped(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .skipped_installs
            .iter()
            .chain(&self.skipped_builds)
            .cloned()
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

/// All or nothing per Install group and for builds: above a limit, which
/// ones to warm is the repo's call, not detection's.
pub fn plan(root: &Path, max_installs: usize) -> NestedPlan {
    plan_groups(root, max_installs).0
}

/// [`plan`], and [`install_groups`] it was made from.
fn plan_groups(root: &Path, max_installs: usize) -> (NestedPlan, Vec<InstallGroup>) {
    use crate::toolchains::{Kind, TOOLCHAINS};
    let dirs = nested_projects(root);
    let groups = install_groups(root, &dirs, max_installs);
    // In `dirs` order, each once.
    let union = |within: bool| -> Vec<String> {
        dirs.iter()
            .filter(|d| groups.iter().any(|(_, g, ok)| *ok == within && g.contains(d)))
            .cloned()
            .collect()
    };
    let builds: Vec<String> = dirs
        .iter()
        .filter(|d| {
            TOOLCHAINS
                .iter()
                .any(|t| t.kind == Kind::Build && t.applies(root, d))
        })
        .cloned()
        .collect();
    let (builds, skipped_builds) = if builds.len() <= MAX_NESTED_BUILDS {
        (builds, vec![])
    } else {
        (vec![], builds)
    };
    let plan = NestedPlan {
        installs: union(true),
        skipped_installs: union(false),
        skipped_groups: groups
            .iter()
            .filter(|(_, _, ok)| !ok)
            .map(|(g, d, _)| (g.to_string(), d.len()))
            .collect(),
        builds,
        skipped_builds,
    };
    (plan, groups)
}

/// An Install group, its nested project dirs, and whether they get installed.
type InstallGroup = (&'static str, Vec<String>, bool);

/// Per Install group, in registry order: the nested `dirs` it applies to, and
/// whether they are few enough (`max_installs`) to install.
fn install_groups(root: &Path, dirs: &[String], max_installs: usize) -> Vec<InstallGroup> {
    use crate::toolchains::{Kind, TOOLCHAINS};
    let mut out: Vec<InstallGroup> = vec![];
    for t in TOOLCHAINS.iter().filter(|t| t.kind == Kind::Install) {
        if !out.iter().any(|(g, _, _)| *g == t.group) {
            out.push((t.group, vec![], true));
        }
    }
    for (g, found, ok) in &mut out {
        *found = dirs
            .iter()
            .filter(|d| {
                TOOLCHAINS
                    .iter()
                    .any(|t| t.kind == Kind::Install && t.group == *g && t.applies(root, d))
            })
            .cloned()
            .collect();
        *ok = found.len() <= max_installs;
    }
    out
}

fn find_steps(root: &Path, max_installs: usize) -> Vec<Step> {
    use crate::toolchains::{Kind, TOOLCHAINS};
    let applies = |t: &Toolchain, dir: &str| t.applies(root, dir);
    let mut out = vec![];

    // Environment managers, at the root only.
    for t in TOOLCHAINS.iter().filter(|t| t.kind == Kind::Env) {
        if applies(t, "") {
            out.extend(toolchain_steps(root, t, ""));
        }
    }

    // Package managers per project dir: the root, plus nested projects with
    // their own lockfile (a Tauri app's `src-tauri`, a `site/`, a monorepo's
    // packages outside its workspace). Installs first, group by group, then
    // builds, which may need them.
    let (nested, groups) = plan_groups(root, max_installs);
    let build_dirs: Vec<String> = std::iter::once(String::new()).chain(nested.builds).collect();
    for (g, found, ok) in groups {
        let nested = if ok { found } else { vec![] };
        for d in std::iter::once(String::new()).chain(nested) {
            let first = TOOLCHAINS
                .iter()
                .find(|t| t.kind == Kind::Install && t.group == g && applies(t, &d));
            if let Some(t) = first {
                out.extend(toolchain_steps(root, t, &d));
            }
        }
    }
    for t in TOOLCHAINS.iter().filter(|t| t.kind == Kind::Build) {
        for d in &build_dirs {
            if applies(t, d) {
                out.extend(toolchain_steps(root, t, d));
            }
        }
    }

    out
}

/// `t`'s steps for `dir`, tagged with its id.
fn toolchain_steps(root: &Path, t: &Toolchain, dir: &str) -> Vec<Step> {
    (t.steps)(&root.join(dir))
        .iter()
        .map(|spec| {
            in_dir(
                dir,
                Step {
                    name: format!("{}{}", t.id, spec.suffix),
                    warm: spec.warm.map(Into::into),
                    setup: spec.setup.map(Into::into),
                    watch: s(spec.watch),
                    wrap: spec.wrap,
                    tool: Some(t.id.into()),
                    ..Default::default()
                },
            )
        })
        .collect()
}

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

/// Nested projects whose dependencies are installed by default (the
/// `nested_installs` setting). Installs are cheap to keep in the golden image:
/// clones share them, and rerun one only when its lockfile changes.
pub const DEFAULT_NESTED_INSTALLS: usize = 10;

/// Nested Rust workspaces built in the golden image. Compiling is the costly
/// part, so a repo with more leaves them to the config.
pub const MAX_NESTED_BUILDS: usize = 3;

/// Dirs below `root`, up to three levels down, holding a lockfile git tracks.
/// Untracked and ignored files don't count, so built or vendored copies are
/// left out.
pub fn nested_projects(root: &Path) -> Vec<String> {
    let mut dirs = std::collections::BTreeSet::new();
    use crate::toolchains::{Kind, files};
    let patterns: Vec<String> = [files(Kind::Install), files(Kind::Build)]
        .concat()
        .iter()
        .map(|f| format!("**/{f}"))
        .collect();
    let listed = std::process::Command::new(crate::git::bin())
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

/// The binary a step's `tool` needs on PATH, for `clove doctor`.
pub fn required_tool(step: &Step) -> Option<&'static str> {
    crate::toolchains::of(step).map(|t| t.bin)
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
        let names: Vec<_> = steps(d.path(), DEFAULT_NESTED_INSTALLS)
            .into_iter()
            .map(|s| s.name)
            .collect();
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
                std::process::Command::new(crate::git::bin())
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

        let steps = steps(r, DEFAULT_NESTED_INSTALLS);
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

    fn git_repo(files: &[&str]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for f in files {
            let p = r.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "").unwrap();
        }
        let run = |args: &[&str]| {
            std::process::Command::new(crate::git::bin())
                .arg("-C")
                .arg(r)
                .args(args)
                .status()
                .unwrap()
                .success()
        };
        assert!(run(&["init", "-q"]) && run(&["add", "."]));
        d
    }

    #[test]
    fn nested_installs_up_to_the_limit_and_none_above() {
        let mut files = vec!["pnpm-lock.yaml".to_string()];
        for i in 0..4 {
            files.push(format!("packages/p{i}/pnpm-lock.yaml"));
        }
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        let d = git_repo(&refs);
        let names = |max| {
            steps(d.path(), max)
                .into_iter()
                .map(|s| s.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(10),
            [
                "pnpm",
                "pnpm-packages-p0",
                "pnpm-packages-p1",
                "pnpm-packages-p2",
                "pnpm-packages-p3"
            ]
        );
        // Above the limit: none of them, named instead.
        assert_eq!(names(3), ["pnpm"]);
        assert_eq!(plan(d.path(), 3).skipped_installs.len(), 4);
        assert_eq!(names(0), ["pnpm"], "0 turns nested installs off");
    }

    #[test]
    fn rust_builds_have_their_own_lower_limit() {
        let mut files = vec!["pnpm-lock.yaml".to_string(), "web/pnpm-lock.yaml".to_string()];
        for i in 0..=MAX_NESTED_BUILDS {
            files.push(format!("crates/c{i}/Cargo.lock"));
        }
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        let d = git_repo(&refs);
        let names: Vec<_> = steps(d.path(), 10).into_iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            ["pnpm", "pnpm-web"],
            "too many crates: none built, installs still warmed"
        );
        let p = plan(d.path(), 10);
        assert_eq!(p.skipped_builds.len(), MAX_NESTED_BUILDS + 1);
        assert!(p.skipped_installs.is_empty());
    }

    #[test]
    fn without_git_only_the_root_counts() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("sub")).unwrap();
        std::fs::write(d.path().join("sub/Cargo.lock"), "").unwrap();
        std::fs::write(d.path().join("Cargo.lock"), "").unwrap();
        let names: Vec<_> = steps(d.path(), DEFAULT_NESTED_INSTALLS)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["cargo"]);
    }

    #[test]
    fn empty_repo_has_no_steps() {
        let d = tempfile::tempdir().unwrap();
        assert!(steps(d.path(), DEFAULT_NESTED_INSTALLS).is_empty());
    }
}
