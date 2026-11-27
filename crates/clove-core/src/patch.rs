//! Reads a unified git patch (`git diff`, as `Clove::diff` returns it) into
//! files, hunks and lines, and groups files the way the diff views do.

use crate::diff::{CloneDiff, FileAttrs, Package};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Added,
    Deleted,
    Modified,
    /// `old_path` holds where it came from; it may also be modified.
    Renamed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    pub kind: LineKind,
    /// Line number on the old side (context and removed lines).
    pub old: Option<u32>,
    /// Line number on the new side (context and added lines).
    pub new: Option<u32>,
    pub text: String,
    /// `\ No newline at end of file` followed this line.
    pub no_newline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hunk {
    pub old_start: u32,
    pub new_start: u32,
    /// What git printed after the second `@@`, usually the enclosing function.
    pub context: String,
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FilePatch {
    pub path: String,
    pub old_path: Option<String>,
    pub change: Change,
    /// git printed `Binary files … differ`: no lines to show.
    pub binary: bool,
    pub hunks: Vec<Hunk>,
    pub added: u32,
    pub removed: u32,
}

impl FilePatch {
    pub fn lines(&self) -> usize {
        self.hunks.iter().map(|h| h.lines.len()).sum()
    }
}

/// Splits a patch into files. Tolerant: an unexpected line is skipped, never
/// fatal, since the patch comes straight from git.
pub fn parse(patch: &str) -> Vec<FilePatch> {
    let mut files: Vec<FilePatch> = vec![];
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    for raw in patch.split_inclusive('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let (a, b) = split_git_paths(rest);
            files.push(FilePatch {
                path: b.unwrap_or_default(),
                old_path: a,
                change: Change::Modified,
                binary: false,
                hunks: vec![],
                added: 0,
                removed: 0,
            });
            continue;
        }
        let Some(f) = files.last_mut() else { continue };
        let in_hunk = !f.hunks.is_empty();
        if let Some(rest) = line.strip_prefix("@@ ") {
            if let Some((old_start, new_start, context)) = hunk_header(rest) {
                old_no = old_start;
                new_no = new_start;
                f.hunks.push(Hunk {
                    old_start,
                    new_start,
                    context,
                    lines: vec![],
                });
            }
            continue;
        }
        if in_hunk {
            let h = f.hunks.last_mut().expect("in_hunk");
            let (kind, text) = match line.as_bytes().first() {
                Some(b'+') => (LineKind::Added, &line[1..]),
                Some(b'-') => (LineKind::Removed, &line[1..]),
                Some(b' ') => (LineKind::Context, &line[1..]),
                // An empty context line some tools strip the space from.
                None => (LineKind::Context, ""),
                Some(b'\\') => {
                    if let Some(l) = h.lines.last_mut() {
                        l.no_newline = true;
                    }
                    continue;
                }
                _ => continue,
            };
            let (old, new) = match kind {
                LineKind::Context => {
                    old_no += 1;
                    new_no += 1;
                    (Some(old_no - 1), Some(new_no - 1))
                }
                LineKind::Added => {
                    new_no += 1;
                    f.added += 1;
                    (None, Some(new_no - 1))
                }
                LineKind::Removed => {
                    old_no += 1;
                    f.removed += 1;
                    (Some(old_no - 1), None)
                }
            };
            h.lines.push(Line {
                kind,
                old,
                new,
                text: text.to_string(),
                no_newline: false,
            });
            continue;
        }
        // Extended header lines, before the first hunk.
        if line.starts_with("new file mode") {
            f.change = Change::Added;
        } else if line.starts_with("deleted file mode") {
            f.change = Change::Deleted;
        } else if let Some(p) = line.strip_prefix("rename from ") {
            f.change = Change::Renamed;
            f.old_path = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("rename to ") {
            f.change = Change::Renamed;
            f.path = unquote(p);
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            f.binary = true;
        } else if let Some(p) = line.strip_prefix("--- ") {
            if p == "/dev/null" {
                f.change = Change::Added;
            } else if let Some(p) = strip_side(p, 'a') {
                f.old_path = Some(p);
            }
        } else if let Some(p) = line.strip_prefix("+++ ") {
            if p == "/dev/null" {
                f.change = Change::Deleted;
            } else if let Some(p) = strip_side(p, 'b') {
                f.path = p;
            }
        }
    }
    for f in &mut files {
        // A deleted file is named by its old path; others keep old_path only
        // when it says something (a rename).
        if f.change == Change::Deleted
            && let Some(old) = f.old_path.clone()
        {
            f.path = old;
        }
        if f.change != Change::Renamed {
            f.old_path = None;
        }
    }
    files
}

/// `-12,5 +14,7 @@ fn main()` → (12, 14, "fn main()"). A missing count is 1.
fn hunk_header(rest: &str) -> Option<(u32, u32, String)> {
    let (ranges, context) = rest.split_once(" @@").unwrap_or((rest, ""));
    let mut it = ranges.split_whitespace();
    let old = it.next()?.strip_prefix('-')?;
    let new = it.next()?.strip_prefix('+')?;
    let start = |r: &str| r.split(',').next()?.parse::<u32>().ok();
    Some((start(old)?, start(new)?, context.trim().to_string()))
}

/// `a/x b/y` (each possibly quoted) → (x, y). Paths with spaces are ambiguous
/// unquoted; the `---`/`+++`/rename lines that follow settle them.
fn split_git_paths(rest: &str) -> (Option<String>, Option<String>) {
    if rest.starts_with('"') || rest.ends_with('"') {
        let mut parts = vec![];
        let mut s = rest;
        while !s.is_empty() {
            s = s.trim_start();
            if let Some(q) = s.strip_prefix('"') {
                let end = closing_quote(q).unwrap_or(q.len());
                parts.push(unquote(&s[..end + 2.min(s.len() - end)]));
                s = &s[(end + 2).min(s.len())..];
            } else {
                let end = s.find(' ').unwrap_or(s.len());
                parts.push(s[..end].to_string());
                s = &s[end..];
            }
        }
        let side = |p: Option<&String>, c: char| {
            p.and_then(|p| p.strip_prefix(&format!("{c}/")).map(str::to_string))
        };
        return (side(parts.first(), 'a'), side(parts.get(1), 'b'));
    }
    // Unquoted: `a/<p> b/<p>` splits in the middle when the path didn't
    // change; otherwise at " b/", which a rename's own lines then confirm.
    let n = rest.len();
    if n % 2 == 1 {
        let (l, r) = (&rest[..n / 2], &rest[n / 2 + 1..]);
        if let (Some(l), Some(r)) = (l.strip_prefix("a/"), r.strip_prefix("b/"))
            && l == r
        {
            return (Some(l.to_string()), Some(r.to_string()));
        }
    }
    match (rest.strip_prefix("a/"), rest.find(" b/")) {
        (Some(_), Some(i)) => (Some(rest[2..i].to_string()), Some(rest[i + 3..].to_string())),
        _ => (None, None),
    }
}

fn closing_quote(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

fn strip_side(p: &str, side: char) -> Option<String> {
    let p = unquote(p);
    p.strip_prefix(&format!("{side}/")).map(str::to_string)
}

/// git's C-style path quoting: `"a/tab\there"`, octal escapes for bytes.
fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return s.to_string();
    };
    let mut out = Vec::with_capacity(inner.len());
    let b = inner.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' || i + 1 >= b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let c = b[i + 1];
        i += 2;
        match c {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'a' => out.push(7),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'v' => out.push(11),
            b'0'..=b'7' => {
                let mut v = (c - b'0') as u32;
                for _ in 0..2 {
                    if i < b.len() && (b'0'..=b'7').contains(&b[i]) {
                        v = v * 8 + (b[i] - b'0') as u32;
                        i += 1;
                    }
                }
                out.push(v as u8);
            }
            other => out.push(other),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Why a file starts folded in a diff view; binary files have nothing to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fold {
    Binary,
    /// `linguist-generated` or `linguist-vendored` in .gitattributes.
    Generated,
    Lockfile,
    /// More changed lines than [`LARGE`].
    Large,
}

/// Changed lines past which a file starts folded.
pub const LARGE: u32 = 800;

const LOCKFILES: &[&str] = &[
    "pnpm-lock.yaml",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "Cargo.lock",
    "go.sum",
    "poetry.lock",
    "uv.lock",
    "Pipfile.lock",
    "Gemfile.lock",
    "composer.lock",
    "flake.lock",
    "mix.lock",
];

/// Whether and why a file starts folded, given what .gitattributes says.
pub fn fold(f: &FilePatch, attrs: Option<&FileAttrs>) -> Option<Fold> {
    let name = f.path.rsplit('/').next().unwrap_or(&f.path);
    if f.binary || attrs.is_some_and(|a| a.binary) {
        Some(Fold::Binary)
    } else if attrs.is_some_and(|a| a.generated) {
        Some(Fold::Generated)
    } else if LOCKFILES.contains(&name) {
        Some(Fold::Lockfile)
    } else if f.added + f.removed > LARGE {
        Some(Fold::Large)
    } else {
        None
    }
}

/// Where a file is grouped: its package's directory, or for files only the
/// repository root's manifest covers (proto/, gen/…), their top two folders.
/// The package is None for such folders.
pub fn group_of<'a>(path: &str, packages: &'a [Package]) -> (String, Option<&'a Package>) {
    // Innermost first, so the first prefix match is the file's own package.
    let pkg = packages
        .iter()
        .find(|p| p.dir.is_empty() || path.starts_with(&format!("{}/", p.dir)));
    match pkg {
        Some(p) if !p.dir.is_empty() => (p.dir.clone(), Some(p)),
        _ => {
            let dirs: Vec<&str> = path.split('/').collect();
            let dirs = &dirs[..dirs.len().saturating_sub(1)];
            if dirs.is_empty() {
                (String::new(), pkg)
            } else {
                (dirs[..dirs.len().min(2)].join("/"), None)
            }
        }
    }
}

/// The attributes .gitattributes gave a file, if any mattered.
pub fn attrs_of<'a>(d: &'a CloneDiff, path: &str) -> Option<&'a FileAttrs> {
    d.attrs.iter().find(|a| a.path == path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,5 @@ mod a;
 use std::io;
-fn old() {}
+fn new() {}
+fn more() {}

 fn tail() {}
\\ No newline at end of file
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index 3333333..0000000
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
diff --git a/new file.md b/new file.md
new file mode 100644
index 0000000..4444444
--- /dev/null
+++ b/new file.md
@@ -0,0 +1,2 @@
+# hi
+there
diff --git a/old/name.rs b/new/name.rs
similarity index 90%
rename from old/name.rs
rename to new/name.rs
index 5555555..6666666 100644
--- a/old/name.rs
+++ b/new/name.rs
@@ -3 +3 @@
-x
+y
diff --git a/logo.png b/logo.png
index 7777777..8888888 100644
Binary files a/logo.png and b/logo.png differ
diff --git \"a/tab\\there.txt\" \"b/tab\\there.txt\"
new file mode 100644
--- /dev/null
+++ \"b/tab\\there.txt\"
@@ -0,0 +1 @@
+t
";

    #[test]
    fn parses_files_hunks_and_numbers() {
        let f = parse(PATCH);
        let names: Vec<_> = f.iter().map(|f| (f.path.as_str(), f.change)).collect();
        assert_eq!(
            names,
            [
                ("src/lib.rs", Change::Modified),
                ("gone.txt", Change::Deleted),
                ("new file.md", Change::Added),
                ("new/name.rs", Change::Renamed),
                ("logo.png", Change::Modified),
                ("tab\there.txt", Change::Added),
            ]
        );
        let lib = &f[0];
        assert_eq!((lib.added, lib.removed), (2, 1));
        let h = &lib.hunks[0];
        assert_eq!((h.old_start, h.new_start, h.context.as_str()), (1, 1, "mod a;"));
        let nums: Vec<_> = h.lines.iter().map(|l| (l.kind, l.old, l.new)).collect();
        assert_eq!(
            nums,
            [
                (LineKind::Context, Some(1), Some(1)),
                (LineKind::Removed, Some(2), None),
                (LineKind::Added, None, Some(2)),
                (LineKind::Added, None, Some(3)),
                (LineKind::Context, Some(3), Some(4)),
                (LineKind::Context, Some(4), Some(5)),
            ]
        );
        assert!(h.lines.last().unwrap().no_newline);
        assert_eq!(f[3].old_path.as_deref(), Some("old/name.rs"));
        assert_eq!(f[3].hunks[0].lines.len(), 2);
        assert!(f[4].binary && f[4].hunks.is_empty());
        assert_eq!(f[1].old_path, None);
    }

    #[test]
    fn folds_and_groups() {
        let f = parse(PATCH);
        assert_eq!(fold(&f[4], None), Some(Fold::Binary));
        assert_eq!(fold(&f[0], None), None);
        let generated = FileAttrs {
            path: "src/lib.rs".into(),
            generated: true,
            ..Default::default()
        };
        assert_eq!(fold(&f[0], Some(&generated)), Some(Fold::Generated));
        let lock = FilePatch {
            path: "app/Cargo.lock".into(),
            ..f[0].clone()
        };
        assert_eq!(fold(&lock, None), Some(Fold::Lockfile));

        let pkgs = vec![
            Package {
                dir: "crates/core".into(),
                name: "core".into(),
                manifest: "Cargo.toml".into(),
            },
            Package {
                dir: String::new(),
                name: "root".into(),
                manifest: "Cargo.toml".into(),
            },
        ];
        assert_eq!(group_of("crates/core/src/a.rs", &pkgs).0, "crates/core");
        let (k, p) = group_of("proto/orch/x.proto", &pkgs);
        assert_eq!((k.as_str(), p.is_none()), ("proto/orch", true));
        let (k, p) = group_of("README.md", &pkgs);
        assert_eq!((k.as_str(), p.map(|p| p.name.as_str())), ("", Some("root")));
    }
}
