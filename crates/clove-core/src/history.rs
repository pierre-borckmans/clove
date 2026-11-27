//! Removal history: one JSON line per clone or plain worktree clove removed,
//! so people and agents can see what went, who removed it and why, and how
//! to get the branch back.

use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::repo::Clove;
use crate::util::now_unix;

/// Who asked for a removal.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum By {
    /// `clove rm`, or remove in the TUI or app.
    #[default]
    Hand,
    /// `clove prune`, or cleanup in the TUI or app.
    Prune,
    /// The service, with `prune.auto` on.
    AutoPrune,
    /// Claude Code's WorktreeRemove hook.
    ClaudeHook,
    /// OpenCode's worktree strategy remove hook.
    OpencodeHook,
}

impl By {
    pub fn label(self) -> &'static str {
        match self {
            By::Hand => "removed",
            By::Prune => "pruned",
            By::AutoPrune => "auto-pruned",
            By::ClaudeHook => "removed by Claude Code",
            By::OpencodeHook => "removed by OpenCode",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Clone,
    Worktree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Removed {
    pub at: u64,
    pub kind: Kind,
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    /// The commit it was on, so the branch can come back.
    pub head: Option<String>,
    pub by: By,
    /// Why, when clove decided: "PR #12 merged".
    pub reason: Option<String>,
    pub branch_deleted: bool,
}

impl Removed {
    /// One line: "auto-pruned feat-x 2h ago: PR #12 merged".
    pub fn summary(&self, now: u64) -> String {
        let mut s = format!(
            "{} {} {}",
            self.by.label(),
            self.name,
            ago(now.saturating_sub(self.at))
        );
        if let Some(r) = &self.reason {
            s.push_str(": ");
            s.push_str(r);
        }
        s
    }

    /// The command that brings the branch back, when it was deleted.
    pub fn restore(&self) -> Option<String> {
        match (&self.branch, &self.head) {
            (Some(b), Some(h)) if self.branch_deleted => Some(format!("git branch {b} {}", short(h))),
            _ => None,
        }
    }
}

/// Lines kept; older ones go when the file reaches twice this.
const KEEP: usize = 500;

impl Clove {
    fn history_path(&self) -> PathBuf {
        self.state.join("history.jsonl")
    }

    /// Appends one removal. Never fails the removal it records.
    pub fn record_removal(&self, mut e: Removed) {
        if e.at == 0 {
            e.at = now_unix();
        }
        let Ok(line) = serde_json::to_string(&e) else {
            return;
        };
        let path = self.history_path();
        let appended = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| writeln!(f, "{line}"));
        if appended.is_ok() {
            self.trim_history();
        }
    }

    fn trim_history(&self) {
        let path = self.history_path();
        let Ok(s) = std::fs::read_to_string(&path) else {
            return;
        };
        let lines: Vec<&str> = s.lines().collect();
        if lines.len() < 2 * KEEP {
            return;
        }
        let kept = lines[lines.len() - KEEP..].join("\n") + "\n";
        let _ = crate::util::write_atomic(&path, kept.as_bytes());
    }

    /// Every recorded removal, newest first.
    pub fn history(&self) -> Vec<Removed> {
        let Ok(s) = std::fs::read_to_string(self.history_path()) else {
            return vec![];
        };
        let mut v: Vec<Removed> = s.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        v.reverse();
        v
    }

    /// The newest removal of a clone with this name, for "no clone named" errors.
    pub fn last_removal(&self, name: &str) -> Option<Removed> {
        self.history()
            .into_iter()
            .find(|e| e.kind == Kind::Clone && e.name == name)
    }

    /// The error for a clone that isn't there, saying when and why it went
    /// if clove removed it.
    pub fn no_clone(&self, name: &str) -> anyhow::Error {
        match self.last_removal(name) {
            Some(e) => {
                let mut msg = format!("no clone named `{name}`: {}", e.summary(now_unix()));
                if let Some(cmd) = e.restore() {
                    msg.push_str(&format!("; its branch comes back with `{cmd}`"));
                }
                anyhow::anyhow!(msg)
            }
            None => anyhow::anyhow!("no clone named `{name}`"),
        }
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

pub fn ago(secs: u64) -> String {
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", secs / 60),
        3600..86_400 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, by: By) -> Removed {
        Removed {
            at: 1000,
            kind: Kind::Clone,
            name: name.into(),
            path: PathBuf::from("/x"),
            branch: Some(format!("b-{name}")),
            head: Some("0123456789abcdef0123".into()),
            by,
            reason: Some("PR #12 merged".into()),
            branch_deleted: true,
        }
    }

    #[test]
    fn summary_and_restore_say_what_went_and_how_back() {
        let e = entry("feat", By::AutoPrune);
        assert_eq!(e.summary(1000 + 7200), "auto-pruned feat 2h ago: PR #12 merged");
        assert_eq!(e.restore().as_deref(), Some("git branch b-feat 0123456789ab"));
        let kept = Removed {
            branch_deleted: false,
            ..e
        };
        assert_eq!(kept.restore(), None);
    }

    #[test]
    fn by_round_trips_as_kebab_case() {
        let s = serde_json::to_string(&entry("a", By::AutoPrune)).unwrap();
        assert!(s.contains("\"by\":\"auto-prune\""), "{s}");
        let back: Removed = serde_json::from_str(&s).unwrap();
        assert_eq!(back.by, By::AutoPrune);
    }

    #[test]
    fn opencode_hook_round_trips_and_labels_its_removals() {
        let s = serde_json::to_string(&By::OpencodeHook).unwrap();
        assert_eq!(s, "\"opencode-hook\"");
        let by: By = serde_json::from_str(&s).unwrap();
        assert_eq!(by, By::OpencodeHook);
        assert_eq!(by.label(), "removed by OpenCode");
    }
}
