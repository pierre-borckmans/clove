//! A clone's changes against its base: what the branch would bring if merged
//! now, plus anything uncommitted or untracked in the worktree.

use crate::git;
use crate::repo::Clove;
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::path::Path;

/// Stops a huge diff (a vendored tree, a lockfile rewrite) from freezing a viewer.
const MAX_PATCH_BYTES: usize = 8 << 20;
/// Untracked files bigger than this are listed but not inlined.
const MAX_UNTRACKED_BYTES: u64 = 1 << 20;

#[derive(Debug, Clone, Serialize)]
pub struct CloneDiff {
    /// The base ref, e.g. `origin/main`.
    pub base: String,
    /// Where the branch left the base; the diff's left side.
    pub merge_base: String,
    /// Unified git patch: committed, staged, unstaged, then untracked files.
    pub patch: String,
    /// The patch hit `MAX_PATCH_BYTES` and was cut at a file boundary.
    pub truncated: bool,
    /// Untracked files left out because they are too big.
    pub skipped: Vec<String>,
    /// What `.gitattributes` says about the changed files, for those where
    /// it changes how a viewer should show them.
    pub attrs: Vec<FileAttrs>,
    /// Packages the changed files belong to, innermost first by `dir`; a
    /// file belongs to the one with the longest `dir` prefix.
    pub packages: Vec<Package>,
}

/// A directory with a package manifest (`package.json`, `Cargo.toml`, …).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Package {
    /// Relative to the worktree root; empty for the root itself.
    pub dir: String,
    /// The manifest's name when it has one, else the directory's.
    pub name: String,
    /// The manifest's file name, e.g. `Cargo.toml`.
    pub manifest: String,
}

/// Manifests that mark a package, most specific first.
const MANIFESTS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
    "setup.py",
    "deno.json",
    "composer.json",
    "Gemfile",
    "mix.exs",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "Package.swift",
    "pubspec.yaml",
];

/// The `.gitattributes` a diff viewer has to respect for one path.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FileAttrs {
    pub path: String,
    /// `binary` or `-diff`: git shows no lines, and neither should a viewer.
    pub binary: bool,
    /// `linguist-generated` or `linguist-vendored`: collapsed by default, as on GitHub.
    pub generated: bool,
    /// A `filter` (e.g. LFS) or a `diff=<driver>` textconv: the patch is not
    /// the bytes on disk, so whole files can't be loaded to expand context.
    pub converted: bool,
}

impl Clove {
    pub fn diff(&self, name: &str) -> Result<CloneDiff> {
        let meta = self
            .clone_meta(name)?
            .with_context(|| format!("no clone named {name}"))?;
        let wt = &meta.path;
        if !wt.join(".git").exists() {
            bail!("{name} is not mounted; mount it to see its changes");
        }
        let merge_base =
            git::out(wt, &["merge-base", &meta.base, "HEAD"]).unwrap_or_else(|_| meta.base.clone());

        let mut patch = raw(
            wt,
            &[
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--find-renames",
                &merge_base,
                "--",
            ],
        )?;
        let mut truncated = cut(&mut patch);
        let mut skipped = vec![];

        let others = raw(wt, &["ls-files", "--others", "--exclude-standard", "-z"])?;
        for path in others.split('\0').filter(|p| !p.is_empty()) {
            if truncated {
                skipped.push(path.to_string());
                continue;
            }
            let big = std::fs::metadata(wt.join(path))
                .map(|m| m.len() > MAX_UNTRACKED_BYTES)
                .unwrap_or(true);
            if big {
                skipped.push(path.to_string());
                continue;
            }
            // Exits 1 when the files differ, which they always do here.
            let out = git::cmd(wt)
                .args([
                    "diff",
                    "--no-color",
                    "--no-ext-diff",
                    "--no-index",
                    "--",
                    "/dev/null",
                    path,
                ])
                .output()?;
            patch.push_str(&String::from_utf8_lossy(&out.stdout));
            truncated = cut(&mut patch);
        }

        let mut paths = raw(
            wt,
            &["diff", "--name-only", "-z", "--find-renames", &merge_base, "--"],
        )?;
        paths.push_str(&others);
        let attrs = attrs(wt, &paths)?;
        let packages = packages(wt, paths.split('\0').filter(|p| !p.is_empty()));

        Ok(CloneDiff {
            base: meta.base,
            merge_base,
            patch,
            truncated,
            skipped,
            attrs,
            packages,
        })
    }

    /// One file as it was at `rev` (`None`: as it is in the worktree now), for
    /// viewers that expand the unchanged lines around a hunk. Missing is empty.
    pub fn file_at(&self, name: &str, rev: Option<&str>, path: &str) -> Result<String> {
        let meta = self
            .clone_meta(name)?
            .with_context(|| format!("no clone named {name}"))?;
        if path.split('/').any(|p| p == "..") {
            bail!("bad path {path}");
        }
        Ok(match rev {
            Some(rev) => git::cmd(&meta.path)
                .args(["show", &format!("{rev}:{path}")])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default(),
            None => std::fs::read(meta.path.join(path))
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default(),
        })
    }
}

/// The nearest package above each path, deduplicated. Deleted files still
/// find theirs: only the directories are looked at.
fn packages<'a>(wt: &Path, paths: impl Iterator<Item = &'a str>) -> Vec<Package> {
    let mut seen = std::collections::BTreeMap::<String, Option<Package>>::new();
    for path in paths {
        let mut dir = Path::new(path).parent();
        while let Some(d) = dir {
            let key = d.to_string_lossy().to_string();
            let found = seen.entry(key.clone()).or_insert_with(|| package_at(wt, &key));
            if found.is_some() {
                break;
            }
            dir = d.parent();
        }
    }
    let mut out: Vec<Package> = seen.into_values().flatten().collect();
    out.sort_by(|a, b| b.dir.len().cmp(&a.dir.len()).then(a.dir.cmp(&b.dir)));
    out
}

fn package_at(wt: &Path, dir: &str) -> Option<Package> {
    let abs = wt.join(dir);
    let manifest = MANIFESTS.iter().find(|m| abs.join(m).is_file())?;
    let text = std::fs::read_to_string(abs.join(manifest)).unwrap_or_default();
    let named = match *manifest {
        "package.json" | "composer.json" | "deno.json" => serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("name")?.as_str().map(str::to_string)),
        "Cargo.toml" | "pyproject.toml" => toml::from_str::<toml::Value>(&text).ok().and_then(|v| {
            let t = v.get("package").or_else(|| v.get("project"))?;
            t.get("name")?.as_str().map(str::to_string)
        }),
        "go.mod" => text
            .lines()
            .find_map(|l| l.strip_prefix("module "))
            .map(|m| m.trim().to_string()),
        _ => None,
    };
    let fallback = || match dir {
        "" => wt
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        d => d.rsplit('/').next().unwrap_or(d).to_string(),
    };
    Some(Package {
        dir: dir.to_string(),
        name: named.filter(|n| !n.is_empty()).unwrap_or_else(fallback),
        manifest: manifest.to_string(),
    })
}

/// Reads the attributes that matter for `paths` (NUL-separated) with one
/// `git check-attr`, keeping only paths where one is set.
fn attrs(wt: &Path, paths: &str) -> Result<Vec<FileAttrs>> {
    use std::io::Write;
    use std::process::Stdio;
    if paths.trim_matches('\0').is_empty() {
        return Ok(vec![]);
    }
    let mut child = git::cmd(wt)
        .args([
            "check-attr",
            "-z",
            "--stdin",
            "binary",
            "diff",
            "filter",
            "linguist-generated",
            "linguist-vendored",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().context("check-attr stdin")?;
    let input = paths.as_bytes().to_vec();
    // Written from a thread: a long list would fill both pipes otherwise.
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let out = child.wait_with_output()?;
    let _ = writer.join();
    Ok(parse_attrs(&String::from_utf8_lossy(&out.stdout)))
}

/// `git check-attr -z` output: `path NUL attr NUL value NUL`, repeated.
fn parse_attrs(out: &str) -> Vec<FileAttrs> {
    let mut by_path: Vec<FileAttrs> = vec![];
    let mut f = out.split('\0');
    while let (Some(path), Some(attr), Some(value)) = (f.next(), f.next(), f.next()) {
        if value == "unspecified" {
            continue;
        }
        let i = match by_path.iter().position(|a| a.path == path) {
            Some(i) => i,
            None => {
                by_path.push(FileAttrs {
                    path: path.to_string(),
                    ..Default::default()
                });
                by_path.len() - 1
            }
        };
        let a = &mut by_path[i];
        let truthy = value != "unset" && value != "false";
        match attr {
            "binary" => a.binary |= value == "set",
            "diff" if value == "unset" => a.binary = true,
            "diff" if value != "set" => a.converted = true,
            "filter" => a.converted |= truthy,
            "linguist-generated" | "linguist-vendored" => a.generated |= truthy,
            _ => {}
        }
    }
    by_path.retain(|a| a.binary || a.generated || a.converted);
    by_path
}

/// Untrimmed stdout: a patch's whitespace is content.
fn raw(dir: &Path, args: &[&str]) -> Result<String> {
    let out = crate::util::output(git::cmd(dir).args(args))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Cuts an oversized patch back to its last whole file. True if it cut.
fn cut(patch: &mut String) -> bool {
    if patch.len() <= MAX_PATCH_BYTES {
        return false;
    }
    let mut end = MAX_PATCH_BYTES;
    while !patch.is_char_boundary(end) {
        end -= 1;
    }
    let at = patch[..end].rfind("\ndiff --git ").map(|i| i + 1).unwrap_or(0);
    patch.truncate(at);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_keeps_whole_files() {
        let file = |n: usize| format!("diff --git a/f{n} b/f{n}\n+{}\n", "x".repeat(MAX_PATCH_BYTES / 3));
        let mut p: String = (0..4).map(file).collect();
        assert!(cut(&mut p));
        assert!(p.len() <= MAX_PATCH_BYTES);
        assert_eq!(p.matches("diff --git ").count(), 2);
        assert!(p.ends_with('\n'));

        let mut small = file(0);
        assert!(!cut(&mut small));
    }

    #[test]
    fn finds_nearest_package() {
        let t = tempfile::tempdir().unwrap();
        let w = t.path();
        std::fs::create_dir_all(w.join("apps/web/src")).unwrap();
        std::fs::create_dir_all(w.join("crates/core/src")).unwrap();
        std::fs::create_dir_all(w.join("docs")).unwrap();
        std::fs::write(w.join("package.json"), r#"{"name":"mono"}"#).unwrap();
        std::fs::write(w.join("apps/web/package.json"), r#"{"name":"@acme/web"}"#).unwrap();
        std::fs::write(
            w.join("crates/core/Cargo.toml"),
            "[package]\nname = \"acme-core\"\n",
        )
        .unwrap();
        let got = packages(
            w,
            [
                "apps/web/src/a.ts",
                "apps/web/gone/b.ts",
                "crates/core/src/lib.rs",
                "docs/x.md",
                "README.md",
            ]
            .into_iter(),
        );
        let names: Vec<_> = got.iter().map(|p| (p.dir.as_str(), p.name.as_str())).collect();
        assert_eq!(
            names,
            [
                ("crates/core", "acme-core"),
                ("apps/web", "@acme/web"),
                ("", "mono")
            ]
        );
    }

    #[test]
    fn reads_check_attr_output() {
        let out = [
            "a.png",
            "binary",
            "set",
            "a.png",
            "diff",
            "unset",
            "dist/x.js",
            "linguist-generated",
            "true",
            "big.bin",
            "filter",
            "lfs",
            "doc.docx",
            "diff",
            "word",
            "src/lib.rs",
            "diff",
            "unspecified",
            "vendor/y.go",
            "linguist-vendored",
            "set",
            "keep.js",
            "linguist-generated",
            "false",
            "",
        ]
        .join("\0");
        let got = parse_attrs(&out);
        let f = |p: &str| got.iter().find(|a| a.path == p).cloned();
        assert!(f("a.png").is_some_and(|a| a.binary && !a.converted));
        assert!(f("dist/x.js").is_some_and(|a| a.generated));
        assert!(f("vendor/y.go").is_some_and(|a| a.generated));
        assert!(f("big.bin").is_some_and(|a| a.converted));
        assert!(f("doc.docx").is_some_and(|a| a.converted && !a.binary));
        assert_eq!(f("src/lib.rs"), None);
        assert_eq!(f("keep.js"), None);
    }
}
