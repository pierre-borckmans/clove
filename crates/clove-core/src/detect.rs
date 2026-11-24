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

    if has("pnpm-lock.yaml") {
        out.push(Step {
            name: "pnpm".into(),
            warm: Some("pnpm install --frozen-lockfile".into()),
            // Only runs when the lockfile changed; new packages may need the network.
            setup: Some("pnpm install --prefer-offline --frozen-lockfile".into()),
            watch: s(&["pnpm-lock.yaml", "pnpm-workspace.yaml", ".pnpmfile.cjs"]),
            ..Default::default()
        });
    } else if has("bun.lock") || has("bun.lockb") {
        out.push(Step {
            name: "bun".into(),
            warm: Some("bun install --frozen-lockfile".into()),
            setup: Some("bun install --frozen-lockfile".into()),
            watch: s(&["bun.lock", "bun.lockb"]),
            ..Default::default()
        });
    } else if has("yarn.lock") {
        let berry = has(".yarnrc.yml");
        let cmd = if berry {
            "yarn install --immutable"
        } else {
            "yarn install --frozen-lockfile"
        };
        out.push(Step {
            name: "yarn".into(),
            warm: Some(cmd.into()),
            setup: Some(cmd.into()),
            watch: s(&["yarn.lock", ".yarnrc.yml"]),
            ..Default::default()
        });
    } else if has("package-lock.json") {
        out.push(Step {
            name: "npm".into(),
            warm: Some("npm ci --no-audit --no-fund".into()),
            setup: Some("npm ci --prefer-offline --no-audit --no-fund".into()),
            watch: s(&["package-lock.json"]),
            ..Default::default()
        });
    }

    if has("uv.lock") {
        // Virtualenvs embed absolute paths, so each clone re-syncs (fast with uv's cache).
        out.push(Step {
            name: "uv".into(),
            warm: Some("uv sync --frozen".into()),
            setup: Some("uv sync --frozen".into()),
            ..Default::default()
        });
    }

    out
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
    match step.name.as_str() {
        "mise" | "mise-trust" => Some("mise"),
        "direnv" | "direnv-allow" => Some("direnv"),
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
    fn empty_repo_has_no_steps() {
        let d = tempfile::tempdir().unwrap();
        assert!(steps(d.path()).is_empty());
    }
}
