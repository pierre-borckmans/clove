//! The TUI's resizable splits and their sizes in `<state home>/tui.json`.

use crate::split::{Extent, Pane, Split};
use ratatui::layout::Direction;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SIDEBAR_WIDTH: u16 = 22;
pub const DETAIL_WIDTH: u16 = 48;
pub const DETAIL_HEIGHT: u16 = 12;
pub const JOB_STEPS_WIDTH: u16 = 38;
/// Rows of the plain-worktrees panel under the clones: borders, header, five rows.
pub const WORKTREES_HEIGHT: u16 = 8;
pub const DIFF_FILES_WIDTH: u16 = 34;

/// Pane indices in each split.
pub const SIDEBAR: usize = 0;
pub const DETAIL: usize = 2;
pub const STACK_DETAIL: usize = 1;
pub const JOB_STEPS: usize = 0;
pub const WORKTREES: usize = 1;
pub const DIFF_FILES: usize = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitId {
    /// sidebar | clone list | detail
    Body,
    /// clone list over detail, on narrow terminals
    Stack,
    /// steps | output, in the progress modal
    Job,
    /// clones over plain git worktrees
    Worktrees,
    /// file list | diff, in the diff view
    Diff,
}

#[derive(Clone, Debug)]
pub struct Panes {
    pub body: Split,
    pub stack: Split,
    pub job: Split,
    pub worktrees: Split,
    pub diff: Split,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    splits: BTreeMap<String, Vec<Option<u16>>>,
}

impl Default for Panes {
    fn default() -> Self {
        Self {
            body: Split::new(
                Direction::Horizontal,
                vec![
                    Pane::sized(SIDEBAR_WIDTH, Extent::Cells(16), Extent::Frac(0.4)),
                    Pane::fill(Extent::Cells(30)),
                    Pane::sized(DETAIL_WIDTH, Extent::Cells(30), Extent::Frac(0.6)),
                ],
            ),
            stack: Split::new(
                Direction::Vertical,
                vec![
                    Pane::fill(Extent::Cells(5)),
                    Pane::sized(DETAIL_HEIGHT, Extent::Cells(6), Extent::Frac(0.7)),
                ],
            ),
            job: Split::new(
                Direction::Horizontal,
                vec![
                    Pane::sized(JOB_STEPS_WIDTH, Extent::Cells(24), Extent::Frac(0.6)),
                    Pane::fill(Extent::Cells(30)),
                ],
            )
            .spacing(2),
            worktrees: Split::new(
                Direction::Vertical,
                vec![
                    Pane::fill(Extent::Cells(5)),
                    Pane::sized(WORKTREES_HEIGHT, Extent::Cells(4), Extent::Frac(0.7)),
                ],
            ),
            diff: Split::new(
                Direction::Horizontal,
                vec![
                    Pane::sized(DIFF_FILES_WIDTH, Extent::Cells(20), Extent::Frac(0.6)),
                    Pane::fill(Extent::Cells(40)),
                ],
            ),
        }
    }
}

impl Panes {
    pub fn get(&self, id: SplitId) -> &Split {
        match id {
            SplitId::Body => &self.body,
            SplitId::Stack => &self.stack,
            SplitId::Job => &self.job,
            SplitId::Worktrees => &self.worktrees,
            SplitId::Diff => &self.diff,
        }
    }

    pub fn get_mut(&mut self, id: SplitId) -> &mut Split {
        match id {
            SplitId::Body => &mut self.body,
            SplitId::Stack => &mut self.stack,
            SplitId::Job => &mut self.job,
            SplitId::Worktrees => &mut self.worktrees,
            SplitId::Diff => &mut self.diff,
        }
    }

    fn named(&mut self) -> [(&'static str, &mut Split); 5] {
        [
            ("body", &mut self.body),
            ("stack", &mut self.stack),
            ("job", &mut self.job),
            ("worktrees", &mut self.worktrees),
            ("diff", &mut self.diff),
        ]
    }

    pub fn reset_all(&mut self) {
        for (_, s) in self.named() {
            s.reset_all();
        }
    }

    pub fn path() -> PathBuf {
        clove_core::state_home().join("tui.json")
    }

    /// Defaults, with whatever the file holds on top; a missing or corrupt
    /// file (or entry) means defaults.
    pub fn load(path: &Path) -> Self {
        let mut p = Self::default();
        let file: File = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        for (name, s) in p.named() {
            if let Some(sizes) = file.splits.get(name) {
                s.set_sizes(sizes);
            }
        }
        p
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut file = File::default();
        let mut me = self.clone();
        for (name, s) in me.named() {
            file.splits.insert(name.to_string(), s.sizes());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&file)?)?;
        std::fs::rename(tmp, path)
    }
}
