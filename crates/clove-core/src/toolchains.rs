//! What clove knows about each toolchain it detects: one [`Toolchain`] per
//! entry in [`TOOLCHAINS`]. Detection, `clove doctor` and the step hooks all
//! read from here, keyed by a step's `tool`.

use crate::config::Step;
use crate::repo::Clove;
use std::collections::BTreeMap;
use std::path::Path;

/// When detection runs a toolchain, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Sets up the environment other steps run in. Root only, first.
    Env,
    /// Installs dependencies, at the root and nested projects up to
    /// `nested_installs`. Only the first match of its group per dir runs.
    Install,
    /// Builds, at the root and up to `MAX_NESTED_BUILDS` nested projects. Last.
    Build,
}

/// One step a toolchain contributes. Its name is the toolchain id plus `suffix`.
#[derive(Debug, Clone, Copy)]
pub struct StepSpec {
    pub suffix: &'static str,
    pub warm: Option<&'static str>,
    pub setup: Option<&'static str>,
    pub watch: &'static [&'static str],
    /// Run inside the `exec` environment; off for steps that set it up.
    pub wrap: bool,
}

/// Run in a clone before its setup steps, once per toolchain in use.
#[derive(Debug, Clone, Copy)]
pub struct Fixup {
    /// Timing label.
    pub label: &'static str,
    /// Gets the clone's worktree and the toolchain's `files`.
    pub run: fn(wt: &Path, files: &[&str]),
}

/// Adds to the environment of every step and `clove run`.
pub type EnvHook = fn(&Clove, &mut BTreeMap<String, String>);

#[derive(Debug, Clone, Copy)]
pub struct Toolchain {
    /// Step names start with it; a step's `tool` names it.
    pub id: &'static str,
    pub kind: Kind,
    /// What `clove doctor` looks for on PATH.
    pub bin: &'static str,
    /// Any of these in a dir means the toolchain may apply there. For Install
    /// and Build they are files git tracks, which find nested projects.
    pub files: &'static [&'static str],
    /// Install only: per dir, the first entry of a group that applies runs.
    pub group: &'static str,
    /// The steps for a project dir; none means the toolchain doesn't apply
    /// there after all (Go without `go.mod`).
    pub steps: fn(dir: &Path) -> &'static [StepSpec],
    /// Added to the environment of every step and `clove run`, unless set.
    pub env: &'static [(&'static str, &'static str)],
    pub env_hook: Option<EnvHook>,
    pub before_setup: Option<Fixup>,
    /// Projects of its own below one are part of it (Maven modules), not
    /// projects of their own.
    pub owns_subdirs: bool,
}

impl Toolchain {
    /// Whether `dir` (relative to `root`, "" for the root) is one of its projects.
    pub fn applies(&self, root: &Path, dir: &str) -> bool {
        let at = |d: &str| self.files.iter().any(|f| root.join(d).join(f).exists());
        if !at(dir) || (self.steps)(&root.join(dir)).is_empty() {
            return false;
        }
        let mut above = dir.match_indices('/').map(|(i, _)| &dir[..i]);
        !(self.owns_subdirs && !dir.is_empty() && (at("") || above.any(at)))
    }
}

const fn spec(warm: &'static str, setup: Option<&'static str>, watch: &'static [&'static str]) -> StepSpec {
    StepSpec {
        suffix: "",
        warm: Some(warm),
        setup,
        watch,
        wrap: true,
    }
}

const fn warm_only(warm: &'static str, watch: &'static [&'static str]) -> StepSpec {
    spec(warm, None, watch)
}

const fn same(cmd: &'static str, watch: &'static [&'static str]) -> StepSpec {
    spec(cmd, Some(cmd), watch)
}

const DEFAULT: Toolchain = Toolchain {
    id: "",
    kind: Kind::Install,
    bin: "",
    files: &[],
    group: "",
    steps: |_| &[],
    env: &[],
    env_hook: None,
    before_setup: None,
    owns_subdirs: false,
};

fn has(dir: &Path, f: &str) -> bool {
    dir.join(f).exists()
}

/// A PYTHONPATH from the repo's env (e.g. a nix shell) leaks into isolated
/// builds and shadows their setuptools, so sdists fail to build.
const UV_SYNC: &str = "env -u PYTHONPATH uv sync --frozen";

/// In detection order: Env, then Install groups in order of first
/// appearance, then Build. Within a group, earlier entries win.
pub const TOOLCHAINS: &[Toolchain] = &[
    Toolchain {
        id: "mise",
        kind: Kind::Env,
        bin: "mise",
        files: &["mise.toml", ".mise.toml", ".tool-versions"],
        steps: |_| {
            &[
                // Trust is keyed by path, so every clone needs it; it is instant.
                StepSpec {
                    suffix: "-trust",
                    warm: Some("mise trust --quiet --yes ."),
                    setup: Some("mise trust --quiet --yes ."),
                    watch: &[],
                    wrap: false,
                },
                // Tools install once into the golden image and rerun when mise's config changes.
                StepSpec {
                    suffix: "",
                    warm: Some("mise install --yes"),
                    setup: None,
                    watch: &["mise.toml", ".mise.toml", ".tool-versions", "mise.lock", ".mise"],
                    wrap: false,
                },
            ]
        },
        ..DEFAULT
    },
    Toolchain {
        id: "direnv",
        kind: Kind::Env,
        bin: "direnv",
        files: &[".envrc"],
        steps: |_| {
            &[
                // `direnv allow` is keyed by path, so every clone needs it; it is instant.
                StepSpec {
                    suffix: "-allow",
                    warm: Some("direnv allow ."),
                    setup: Some("direnv allow ."),
                    watch: &[],
                    wrap: false,
                },
                // The cached environment (e.g. nix-direnv's .direnv/) comes along in the
                // image; it only needs rebuilding when its inputs changed.
                StepSpec {
                    suffix: "",
                    warm: Some("direnv exec . true"),
                    setup: Some("direnv exec . true"),
                    watch: &[
                        ".envrc",
                        "flake.nix",
                        "flake.lock",
                        "shell.nix",
                        "default.nix",
                        "devenv.nix",
                        "devenv.lock",
                        "devbox.json",
                    ],
                    wrap: false,
                },
            ]
        },
        ..DEFAULT
    },
    Toolchain {
        id: "pnpm",
        kind: Kind::Install,
        bin: "pnpm",
        files: &["pnpm-lock.yaml"],
        group: "js",
        // Setup only runs when the lockfile changed; new packages may need the network.
        steps: |_| {
            const {
                &[spec(
                    "pnpm install --frozen-lockfile",
                    Some("pnpm install --prefer-offline --frozen-lockfile"),
                    &["pnpm-lock.yaml", "pnpm-workspace.yaml", ".pnpmfile.cjs"],
                )]
            }
        },
        env_hook: Some(pnpm_store_env),
        ..DEFAULT
    },
    Toolchain {
        id: "bun",
        kind: Kind::Install,
        bin: "bun",
        files: &["bun.lock", "bun.lockb"],
        group: "js",
        steps: |_| const { &[same("bun install --frozen-lockfile", &["bun.lock", "bun.lockb"])] },
        ..DEFAULT
    },
    Toolchain {
        id: "yarn",
        kind: Kind::Install,
        bin: "yarn",
        files: &["yarn.lock"],
        group: "js",
        // Berry (`.yarnrc.yml`) renamed --frozen-lockfile.
        steps: |dir| {
            const WATCH: &[&str] = &["yarn.lock", ".yarnrc.yml"];
            if has(dir, ".yarnrc.yml") {
                const { &[same("yarn install --immutable", WATCH)] }
            } else {
                const { &[same("yarn install --frozen-lockfile", WATCH)] }
            }
        },
        ..DEFAULT
    },
    Toolchain {
        id: "npm",
        kind: Kind::Install,
        bin: "npm",
        files: &["package-lock.json"],
        group: "js",
        steps: |_| {
            const {
                &[spec(
                    "npm ci --no-audit --no-fund",
                    Some("npm ci --prefer-offline --no-audit --no-fund"),
                    &["package-lock.json"],
                )]
            }
        },
        ..DEFAULT
    },
    Toolchain {
        id: "deno",
        kind: Kind::Install,
        bin: "deno",
        files: &["deno.lock"],
        // Last of its group: next to another JS lockfile, that one wins.
        group: "js",
        steps: |_| {
            const {
                &[same(
                    "deno install --frozen",
                    &["deno.lock", "deno.json", "deno.jsonc", "package.json"],
                )]
            }
        },
        ..DEFAULT
    },
    Toolchain {
        id: "uv",
        kind: Kind::Install,
        bin: "uv",
        files: &["uv.lock"],
        group: "python",
        steps: |_| const { &[same(UV_SYNC, &["uv.lock", "pyproject.toml", ".python-version"])] },
        // Venvs embed absolute paths; moving them to the clone first means an
        // unchanged lockfile needs no sync.
        before_setup: Some(VENV_RELOCATE),
        ..DEFAULT
    },
    Toolchain {
        id: "poetry",
        kind: Kind::Install,
        bin: "poetry",
        files: &["poetry.lock"],
        group: "python",
        // `--sync` works in 1.8 and 2.x (deprecated there for `poetry sync`, new in 2.0).
        steps: |_| {
            const {
                &[same(
                    "poetry install --no-interaction --sync",
                    &["poetry.lock", "pyproject.toml"],
                )]
            }
        },
        // In `.venv`, so it comes along in the image and can be relocated.
        env: &[("POETRY_VIRTUALENVS_IN_PROJECT", "true")],
        before_setup: Some(VENV_RELOCATE),
        ..DEFAULT
    },
    Toolchain {
        id: "pdm",
        kind: Kind::Install,
        bin: "pdm",
        files: &["pdm.lock"],
        group: "python",
        // PDM keeps its venv in `.venv` by default (`venv.in_project`).
        steps: |_| {
            const {
                &[same(
                    "pdm install --frozen-lockfile",
                    &["pdm.lock", "pyproject.toml"],
                )]
            }
        },
        before_setup: Some(VENV_RELOCATE),
        ..DEFAULT
    },
    Toolchain {
        id: "bundler",
        kind: Kind::Install,
        bin: "bundle",
        files: &["Gemfile.lock"],
        group: "ruby",
        steps: |_| const { &[same("bundle install", &["Gemfile.lock", "Gemfile"])] },
        ..DEFAULT
    },
    Toolchain {
        id: "composer",
        kind: Kind::Install,
        bin: "composer",
        files: &["composer.lock"],
        group: "php",
        steps: |_| {
            const {
                &[same(
                    "composer install --no-interaction --no-progress",
                    &["composer.lock", "composer.json"],
                )]
            }
        },
        ..DEFAULT
    },
    Toolchain {
        id: "go",
        kind: Kind::Install,
        bin: "go",
        files: &["go.sum"],
        group: "go",
        // Modules go to GOMODCACHE, outside the tree and shared by clones: warm only.
        steps: |dir| {
            if has(dir, "go.mod") {
                const { &[warm_only("go mod download", &["go.mod", "go.sum"])] }
            } else {
                &[]
            }
        },
        ..DEFAULT
    },
    Toolchain {
        id: "gradle",
        kind: Kind::Install,
        // The wrapper is in the repo, so a JDK is all it needs from PATH.
        bin: "java",
        files: &["gradlew"],
        group: "jvm",
        // Fetches the Gradle distribution and the root project's dependencies
        // into ~/.gradle without building. Warm only: that cache is shared.
        steps: |_| {
            const {
                &[warm_only(
                    "./gradlew --no-daemon --quiet dependencies",
                    &[
                        "gradle/libs.versions.toml",
                        "*.gradle*",
                        "gradle/wrapper/gradle-wrapper.properties",
                    ],
                )]
            }
        },
        ..DEFAULT
    },
    Toolchain {
        id: "maven",
        kind: Kind::Install,
        // Detection already requires `mvnw` or `mvn`; either needs a JDK.
        bin: "java",
        files: &["pom.xml"],
        group: "jvm",
        // Into ~/.m2, shared by clones: warm only. Needs the wrapper or mvn.
        steps: |dir| {
            const WATCH: &[&str] = &["pom.xml", "*/pom.xml", ".mvn/wrapper/maven-wrapper.properties"];
            if has(dir, "mvnw") {
                const { &[warm_only("./mvnw -q dependency:go-offline", WATCH)] }
            } else if crate::util::which("mvn").is_some() {
                const { &[warm_only("mvn -q dependency:go-offline", WATCH)] }
            } else {
                &[]
            }
        },
        // A module's pom.xml is resolved with its reactor.
        owns_subdirs: true,
        ..DEFAULT
    },
    Toolchain {
        id: "dotnet",
        kind: Kind::Install,
        bin: "dotnet",
        files: &["packages.lock.json"],
        group: "dotnet",
        // `obj/` holds absolute paths into the tree: every clone restores (no watch).
        // With several solutions, or several projects and no solution, a bare
        // `dotnet restore` fails (MSB1011) and which to pick is the repo's call.
        steps: |dir| {
            let (mut sln, mut slnx, mut projects) = (0, 0, 0);
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                match e.path().extension().and_then(|x| x.to_str()) {
                    Some("sln") => sln += 1,
                    Some("slnx") => slnx += 1,
                    Some("csproj" | "fsproj" | "vbproj") => projects += 1,
                    _ => {}
                }
            }
            match (sln, slnx, projects) {
                (1, 0, _) => const { &[same("dotnet restore *.sln --locked-mode", &[])] },
                (0, 1, _) => const { &[same("dotnet restore *.slnx --locked-mode", &[])] },
                (0, 0, 1) => const { &[same("dotnet restore --locked-mode", &[])] },
                _ => &[],
            }
        },
        ..DEFAULT
    },
    Toolchain {
        id: "flutter",
        kind: Kind::Install,
        bin: "flutter",
        files: &["pubspec.lock"],
        group: "dart",
        // Flutter writes the project's absolute path into generated files
        // (ios/Flutter/Generated.xcconfig, flutter_export_environment.sh), so
        // every clone reruns it (no watch).
        steps: |dir| {
            let pubspec = std::fs::read_to_string(dir.join("pubspec.yaml")).unwrap_or_default();
            if pubspec.contains("sdk: flutter") {
                const { &[same("flutter pub get", &[])] }
            } else {
                &[]
            }
        },
        ..DEFAULT
    },
    Toolchain {
        id: "dart",
        kind: Kind::Install,
        bin: "dart",
        files: &["pubspec.lock"],
        group: "dart",
        // `.dart_tool/package_config.json` points into the tree with relative
        // paths (absolute ones only reach the pub cache), so clones can reuse it.
        steps: |_| const { &[same("dart pub get", &["pubspec.lock", "pubspec.yaml"])] },
        ..DEFAULT
    },
    Toolchain {
        id: "mix",
        kind: Kind::Install,
        bin: "mix",
        files: &["mix.lock"],
        group: "elixir",
        steps: |_| {
            const {
                &[spec(
                    "mix deps.get && mix compile",
                    Some("mix deps.get"),
                    &["mix.lock"],
                )]
            }
        },
        ..DEFAULT
    },
    Toolchain {
        id: "swiftpm",
        kind: Kind::Install,
        bin: "swift",
        files: &["Package.resolved"],
        group: "swift",
        steps: |dir| {
            if has(dir, "Package.swift") {
                const {
                    &[warm_only(
                        "swift package resolve",
                        &["Package.swift", "Package.resolved"],
                    )]
                }
            } else {
                &[]
            }
        },
        ..DEFAULT
    },
    Toolchain {
        id: "cocoapods",
        kind: Kind::Install,
        bin: "pod",
        files: &["Podfile.lock"],
        group: "cocoapods",
        steps: |_| const { &[same("pod install", &["Podfile.lock", "Podfile"])] },
        ..DEFAULT
    },
    Toolchain {
        id: "zig",
        kind: Kind::Install,
        bin: "zig",
        files: &["build.zig.zon"],
        group: "zig",
        // Packages go to the global cache, shared by clones: warm only.
        steps: |_| const { &[warm_only("zig build --fetch", &["build.zig.zon"])] },
        ..DEFAULT
    },
    Toolchain {
        id: "cargo",
        kind: Kind::Build,
        bin: "cargo",
        files: &["Cargo.lock"],
        // `target/` comes along in the image, so clones start with every
        // dependency built. No setup: cargo builds incrementally on use.
        steps: |_| const { &[warm_only("cargo build --workspace --all-targets", &[])] },
        ..DEFAULT
    },
];

const VENV_RELOCATE: Fixup = Fixup {
    label: "move venvs",
    run: |wt, files| {
        crate::venv::relocate(wt, files);
    },
};

/// The image is its own volume; without this pnpm starts an empty store
/// there. pnpm 11+ reads only pnpm_config_store_dir, older only the npm one.
fn pnpm_store_env(c: &Clove, env: &mut BTreeMap<String, String>) {
    const KEYS: [&str; 2] = ["npm_config_store_dir", "pnpm_config_store_dir"];
    let set = KEYS.iter().find_map(|k| env.get(*k).cloned());
    if let Some(store) = set.or_else(|| c.pnpm_store()) {
        for k in KEYS {
            env.entry(k.into()).or_insert_with(|| store.clone());
        }
    }
}

pub fn get(id: &str) -> Option<&'static Toolchain> {
    TOOLCHAINS.iter().find(|t| t.id == id)
}

/// The toolchain a step names in `tool`.
pub fn of(step: &Step) -> Option<&'static Toolchain> {
    step.tool.as_deref().and_then(get)
}

/// Toolchains used by `steps`, each once, in registry order.
pub fn used<'a>(steps: impl IntoIterator<Item = &'a Step>) -> Vec<&'static Toolchain> {
    let ids: Vec<&str> = steps.into_iter().filter_map(|s| s.tool.as_deref()).collect();
    TOOLCHAINS.iter().filter(|t| ids.contains(&t.id)).collect()
}

/// Every file of the toolchains of `kind`.
pub fn files(kind: Kind) -> Vec<&'static str> {
    TOOLCHAINS
        .iter()
        .filter(|t| t.kind == kind)
        .flat_map(|t| t.files.iter().copied())
        .collect()
}

/// The `tool` of a step configured before steps had one, by the rules clove
/// used then: the name's first `-` part for these ids, else pnpm when a
/// command runs it (its store env was keyed on that).
pub fn infer(step: &Step) -> Option<&'static str> {
    const NAMED: [&str; 8] = ["mise", "direnv", "cargo", "pnpm", "bun", "yarn", "npm", "uv"];
    let prefix = step.name.split('-').next().unwrap_or_default();
    if let Some(id) = NAMED.iter().find(|id| **id == prefix) {
        return Some(id);
    }
    [&step.warm, &step.setup]
        .iter()
        .any(|c| c.as_deref().is_some_and(|c| c.contains("pnpm")))
        .then_some("pnpm")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::{DEFAULT_NESTED_INSTALLS, required_tool, steps};

    fn detect(files: &[&str]) -> Vec<Step> {
        let d = tempfile::tempdir().unwrap();
        for f in files {
            std::fs::write(d.path().join(f), "").unwrap();
        }
        steps(d.path(), DEFAULT_NESTED_INSTALLS)
    }

    /// A git repo with `files` (path, contents) tracked.
    fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for (f, body) in files {
            let p = d.path().join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        crate::git::run_ok(d.path(), &["init", "-q"]).unwrap();
        crate::git::run_ok(d.path(), &["add", "-A"]).unwrap();
        d
    }

    /// Per toolchain, the files of a smallest project it is detected in.
    /// Lockfiles are placeholders; [`E2E`] makes real ones.
    const FIXTURES: &[(&str, &[(&str, &str)])] = &[
        ("mise", &[("mise.toml", "")]),
        ("direnv", &[(".envrc", "")]),
        ("pnpm", &[("pnpm-lock.yaml", "")]),
        ("bun", &[("bun.lock", "")]),
        ("yarn", &[("yarn.lock", "")]),
        ("npm", &[("package-lock.json", "")]),
        (
            "deno",
            &[("deno.json", "{}\n"), ("deno.lock", "{\"version\": \"4\"}\n")],
        ),
        ("uv", &[("uv.lock", "")]),
        (
            "poetry",
            &[("pyproject.toml", POETRY_PYPROJECT), ("poetry.lock", "")],
        ),
        ("pdm", &[("pyproject.toml", PDM_PYPROJECT), ("pdm.lock", "")]),
        (
            "bundler",
            &[
                ("Gemfile", "source \"https://rubygems.org\"\n"),
                ("Gemfile.lock", ""),
            ],
        ),
        (
            "composer",
            &[
                ("composer.json", "{\"name\": \"x/x\", \"require\": {}}\n"),
                ("composer.lock", ""),
            ],
        ),
        (
            "go",
            &[("go.mod", "module example.com/x\n\ngo 1.21\n"), ("go.sum", "")],
        ),
        (
            "gradle",
            &[
                ("settings.gradle", "rootProject.name = 'x'\n"),
                ("build.gradle", ""),
                ("gradlew", ""),
            ],
        ),
        ("maven", &[("pom.xml", MAVEN_POM), ("mvnw", "")]),
        (
            "dotnet",
            &[("x.csproj", DOTNET_CSPROJ), ("packages.lock.json", "")],
        ),
        (
            "flutter",
            &[("pubspec.yaml", FLUTTER_PUBSPEC), ("pubspec.lock", "")],
        ),
        ("dart", &[("pubspec.yaml", DART_PUBSPEC), ("pubspec.lock", "")]),
        ("mix", &[("mix.exs", MIX_EXS), ("mix.lock", "%{}\n")]),
        (
            "swiftpm",
            &[
                ("Package.swift", SWIFT_PACKAGE),
                ("Package.resolved", "{\"pins\": [], \"version\": 2}\n"),
            ],
        ),
        (
            "cocoapods",
            &[("Podfile", "platform :ios, '15.0'\n"), ("Podfile.lock", "")],
        ),
        ("zig", &[("build.zig.zon", "")]),
        ("cargo", &[("Cargo.lock", "")]),
    ];

    const POETRY_PYPROJECT: &str = "[tool.poetry]\nname = \"x\"\nversion = \"0.1.0\"\ndescription = \"\"\nauthors = []\npackage-mode = false\n\n[tool.poetry.dependencies]\npython = \">=3.8\"\n";
    const PDM_PYPROJECT: &str = "[project]\nname = \"x\"\nversion = \"0.1.0\"\nrequires-python = \">=3.8\"\ndependencies = []\n\n[tool.pdm]\ndistribution = false\n";
    const MAVEN_POM: &str = "<project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n  <modelVersion>4.0.0</modelVersion>\n  <groupId>x</groupId>\n  <artifactId>x</artifactId>\n  <version>0.1.0</version>\n</project>\n";
    const DOTNET_CSPROJ: &str = "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    <TargetFramework>net8.0</TargetFramework>\n    <RestorePackagesWithLockFile>true</RestorePackagesWithLockFile>\n  </PropertyGroup>\n</Project>\n";
    const FLUTTER_PUBSPEC: &str =
        "name: x\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\ndependencies:\n  flutter:\n    sdk: flutter\n";
    const DART_PUBSPEC: &str = "name: x\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n";
    const MIX_EXS: &str = "defmodule X.MixProject do\n  use Mix.Project\n  def project, do: [app: :x, version: \"0.1.0\", deps: []]\nend\n";
    const SWIFT_PACKAGE: &str =
        "// swift-tools-version:5.9\nimport PackageDescription\nlet package = Package(name: \"x\")\n";

    #[test]
    fn every_toolchain_has_a_fixture() {
        for t in TOOLCHAINS {
            assert!(FIXTURES.iter().any(|(id, _)| *id == t.id), "{}", t.id);
        }
    }

    /// Each toolchain is detected from its fixture, at the root and nested.
    #[test]
    fn detects_each_toolchain_at_the_root_and_nested() {
        for (id, files) in FIXTURES {
            let t = get(id).unwrap();
            let root = repo(files);
            let found = steps(root.path(), DEFAULT_NESTED_INSTALLS);
            assert!(!found.is_empty(), "{id}");
            for s in &found {
                assert_eq!(s.tool.as_deref(), Some(*id), "{id}: {found:?}");
                assert!(s.name.starts_with(id), "{id}: {}", s.name);
                assert_eq!(required_tool(s), Some(t.bin));
            }
            if t.kind == Kind::Env {
                continue;
            }
            let moved: Vec<(String, &str)> = files.iter().map(|(f, b)| (format!("sub/{f}"), *b)).collect();
            let moved: Vec<(&str, &str)> = moved.iter().map(|(f, b)| (f.as_str(), *b)).collect();
            let nested = repo(&moved);
            let found = steps(nested.path(), DEFAULT_NESTED_INSTALLS);
            assert!(!found.is_empty(), "nested {id}");
            for s in &found {
                assert_eq!(s.tool.as_deref(), Some(*id));
                assert!(s.name.ends_with("-sub"), "{}", s.name);
                assert!(s.warm.as_deref().unwrap().starts_with("cd sub && "));
            }
        }
    }

    #[test]
    fn needs_more_than_the_lockfile() {
        assert!(detect(&["go.sum"]).is_empty(), "go.sum without go.mod");
        assert!(detect(&["Package.resolved"]).is_empty(), "no Package.swift");
        // pubspec.lock alone: a Dart project.
        assert_eq!(detect(&["pubspec.lock"])[0].name, "dart");
    }

    #[test]
    fn dotnet_restores_the_one_solution() {
        let warm = |files: &[&str]| detect(files).first().and_then(|s| s.warm.clone());
        let lock = "packages.lock.json";
        assert_eq!(
            warm(&[lock, "a.csproj"]).as_deref(),
            Some("dotnet restore --locked-mode")
        );
        assert_eq!(
            warm(&[lock, "a.csproj", "b.csproj", "x.sln"]).as_deref(),
            Some("dotnet restore *.sln --locked-mode")
        );
        assert_eq!(
            warm(&[lock, "x.sln", "y.sln"]),
            None,
            "which one is the repo's call"
        );
        assert_eq!(warm(&[lock, "a.csproj", "b.csproj"]), None);
    }

    #[test]
    fn maven_modules_belong_to_their_reactor() {
        let r = repo(&[
            ("pom.xml", MAVEN_POM),
            ("mvnw", ""),
            ("core/pom.xml", MAVEN_POM),
            ("core/api/pom.xml", MAVEN_POM),
            ("web/pnpm-lock.yaml", ""),
        ]);
        let names: Vec<String> = steps(r.path(), DEFAULT_NESTED_INSTALLS)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["pnpm-web", "maven"]);
        assert_eq!(
            crate::detect::plan(r.path(), DEFAULT_NESTED_INSTALLS).installs,
            ["web"]
        );
    }

    #[test]
    fn a_dir_runs_one_entry_per_group() {
        // A Rails app with a JS bundle: both groups run, each once.
        let names: Vec<String> = detect(&["Gemfile.lock", "yarn.lock", "package-lock.json"])
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["yarn", "bundler"]);
        let names: Vec<String> = detect(&["uv.lock", "poetry.lock", "pdm.lock"])
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["uv"]);
    }

    #[test]
    fn the_cap_is_per_group() {
        // Many Go modules don't cost the JS and Python projects their installs.
        let mut files = vec![
            ("pnpm-lock.yaml".to_string(), ""),
            ("web/pnpm-lock.yaml".into(), ""),
            ("api/uv.lock".into(), ""),
            ("api/go.sum".into(), ""),
            ("api/go.mod".into(), "module x\n"),
        ];
        for i in 0..3 {
            files.push((format!("svc/s{i}/go.sum"), ""));
            files.push((format!("svc/s{i}/go.mod"), "module x\n"));
        }
        let refs: Vec<(&str, &str)> = files.iter().map(|(f, b)| (f.as_str(), *b)).collect();
        let r = repo(&refs);
        let p = crate::detect::plan(r.path(), 3);
        assert_eq!(p.installs, ["api", "web"]);
        assert_eq!(p.skipped_installs, ["api", "svc/s0", "svc/s1", "svc/s2"]);
        assert_eq!(p.skipped_groups, [("go".to_string(), 4)]);
        let names: Vec<String> = steps(r.path(), 3).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["pnpm", "pnpm-web", "uv-api"]);
        // Within the cap, Go comes in too.
        assert!(crate::detect::plan(r.path(), 4).skipped_groups.is_empty());
    }

    /// Per toolchain: what the real run needs (skipped when it fails) and
    /// how to make a real lockfile, replacing the fixture's placeholder.
    /// None: not run for real, with why.
    /// (probe, prepare), or why it isn't run.
    type Run = Result<(&'static str, &'static str), &'static str>;

    const E2E: &[(&str, Run)] = &[
        ("deno", Ok(("deno --version", ""))),
        ("poetry", Ok(("poetry --version", "poetry lock"))),
        ("pdm", Ok(("pdm --version", "pdm lock"))),
        (
            "bundler",
            Ok(("bundle --version", "rm Gemfile.lock && bundle lock --local")),
        ),
        (
            "composer",
            Ok((
                "composer --version",
                "composer update --no-interaction --no-progress",
            )),
        ),
        ("go", Ok(("go version", ""))),
        // Makes a real wrapper; the run downloads Gradle.
        (
            "gradle",
            Ok(("java -version && gradle --version", "gradle wrapper")),
        ),
        // Without the placeholder wrapper, mvn runs; it downloads plugins.
        ("maven", Ok(("java -version && mvn -v", "rm mvnw"))),
        (
            "dotnet",
            Ok(("dotnet --version", "rm packages.lock.json && dotnet restore")),
        ),
        ("flutter", Ok(("flutter --version", "flutter pub get"))),
        ("dart", Ok(("dart --version", "dart pub get"))),
        ("mix", Ok(("mix --version", ""))),
        ("swiftpm", Ok(("swift --version", ""))),
        ("cocoapods", Err("pod install needs an Xcode project")),
        ("zig", Ok(("zig version", "rm build.zig.zon && zig init"))),
    ];

    fn sh(dir: &Path, cmd: &str, env: &[(&str, &str)]) -> std::process::Output {
        std::process::Command::new("sh")
            .args(["-c", cmd])
            .current_dir(dir)
            .envs(env.iter().copied())
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    }

    /// With CLOVE_TOOLCHAIN_E2E=1, runs each new toolchain's detected warm
    /// steps for real when its tools are installed; otherwise prints why and passes.
    #[test]
    fn warms_for_real_when_installed() {
        // Opt-in: real tools download (Gradle, Maven plugins) and not every
        // fixture has been run against every tool version.
        if std::env::var("CLOVE_TOOLCHAIN_E2E").as_deref() != Ok("1") {
            eprintln!("skip toolchain e2e: set CLOVE_TOOLCHAIN_E2E=1 to run it");
            return;
        }
        for (id, how) in E2E {
            let (probe, prepare) = match how {
                Ok(p) => *p,
                Err(why) => {
                    eprintln!("skip {id}: {why}");
                    continue;
                }
            };
            let files = FIXTURES.iter().find(|(f, _)| f == id).unwrap().1;
            let r = repo(files);
            if !sh(r.path(), probe, &[]).status.success() {
                eprintln!("skip {id}: `{probe}` failed");
                continue;
            }
            let t = get(id).unwrap();
            if !prepare.is_empty() {
                let o = sh(r.path(), prepare, t.env);
                assert!(
                    o.status.success(),
                    "{id} prepare: {}",
                    String::from_utf8_lossy(&o.stderr)
                );
                crate::git::run_ok(r.path(), &["add", "-A"]).unwrap();
            }
            let found = steps(r.path(), DEFAULT_NESTED_INSTALLS);
            assert!(
                found.iter().any(|s| s.tool.as_deref() == Some(*id)),
                "{id}: {found:?}"
            );
            for s in &found {
                let o = sh(r.path(), s.warm.as_deref().unwrap(), t.env);
                assert!(
                    o.status.success(),
                    "{id} warm `{}`: {}{}",
                    s.warm.as_deref().unwrap(),
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                eprintln!("ran {id}: {}", s.warm.as_deref().unwrap());
            }
        }
    }

    #[test]
    fn groups_pick_the_first_match() {
        let js: Vec<&str> = TOOLCHAINS
            .iter()
            .filter(|t| t.group == "js")
            .map(|t| t.files[0])
            .collect();
        // Every js lockfile at once: only the first entry runs, then each next
        // one once those before it are gone.
        for i in 0..js.len() {
            let names: Vec<String> = detect(&js[i..]).into_iter().map(|s| s.name).collect();
            let want = TOOLCHAINS.iter().filter(|t| t.group == "js").nth(i).unwrap().id;
            assert_eq!(names, [want]);
        }
        // Different groups don't compete.
        let names: Vec<String> = detect(&["yarn.lock", "uv.lock"])
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["yarn", "uv"]);
    }

    #[test]
    fn yarn_berry_is_told_apart() {
        assert_eq!(
            detect(&["yarn.lock", ".yarnrc.yml"])[0].warm.as_deref(),
            Some("yarn install --immutable")
        );
        assert_eq!(
            detect(&["yarn.lock"])[0].warm.as_deref(),
            Some("yarn install --frozen-lockfile")
        );
    }

    #[test]
    fn infers_tool_from_old_rules() {
        let step = |name: &str, warm: &str| Step {
            name: name.into(),
            warm: Some(warm.into()),
            ..Default::default()
        };
        assert_eq!(infer(&step("uv-backend", "uv sync")), Some("uv"));
        assert_eq!(infer(&step("mise-trust", "mise trust")), Some("mise"));
        assert_eq!(infer(&step("build-deps", "pnpm run build-deps")), Some("pnpm"));
        assert_eq!(infer(&step("codegen", "make generate")), None);
    }

    #[test]
    fn configured_tool_gets_doctor_check_and_hooks() {
        let f: crate::config::FileConfig =
            toml::from_str("[[step]]\nname = \"python\"\nsetup = \"cd api && uv sync\"\ntool = \"uv\"\n")
                .unwrap();
        let s = &f.steps[0];
        assert_eq!(required_tool(s), Some("uv"));
        let used = used([s]);
        assert_eq!(used.len(), 1);
        assert_eq!(used[0].before_setup.map(|f| f.label), Some("move venvs"));
    }
}
