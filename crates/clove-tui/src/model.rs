//! Plain data the UI renders: repos, jobs, forms, toasts. No I/O here.

use clove_core::doctor::Check;
use clove_core::{CloneInfo, GoldenMeta, NewOptions, RemoveOptions, RepoStatus};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct RepoTab {
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RepoData {
    pub root: PathBuf,
    pub status: RepoStatus,
    pub clones: Vec<CloneInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRef {
    /// Short name: `feat/x`, `origin/feat/x`, `v1.2`.
    pub name: String,
    pub kind: RefKind,
}

/// Work for a background thread.
#[derive(Debug, Clone)]
pub enum Task {
    Init,
    Refresh,
    New(NewOptions),
    Remove { name: String, opts: RemoveOptions },
    Mount { name: String },
    Unmount { name: String },
    Gc { apply: bool },
    Doctor,
}

/// What to do once a mount finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    Nothing,
    Cd,
    Open,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Golden(GoldenMeta),
    Clone(CloneInfo),
    Removed,
    Mounted(PathBuf),
    Unmounted,
    Gc(Vec<String>),
    Doctor(Vec<Check>),
}

pub enum Msg {
    Loaded {
        root: PathBuf,
        data: Result<Box<RepoData>, String>,
        repos: Vec<RepoTab>,
    },
    Refs {
        root: PathBuf,
        refs: Vec<GitRef>,
    },
    Event(u64, clove_core::Event),
    Done(u64, Result<Outcome, String>),
}

/// A scroll offset that eases toward its target a few frames at a time.
#[derive(Debug, Clone, Copy, Default)]
pub struct Smooth {
    pos: f32,
    target: usize,
}

impl Smooth {
    pub fn target(&self) -> usize {
        self.target
    }
    pub fn set(&mut self, t: usize) {
        self.target = t;
    }
    pub fn offset(&self) -> usize {
        self.pos.round().max(0.0) as usize
    }
    pub fn snap(&mut self) {
        self.pos = self.target as f32;
    }
    /// Advances one frame. Returns true while still moving.
    pub fn tick(&mut self) -> bool {
        let t = self.target as f32;
        let d = t - self.pos;
        if d.abs() <= 0.5 {
            self.pos = t;
            return false;
        }
        let step = d * 0.45;
        self.pos += if step.abs() < 0.5 { 0.5 * d.signum() } else { step };
        true
    }
    /// Keeps `idx` inside a viewport of `h` rows over `len` rows.
    pub fn follow(&mut self, idx: usize, h: usize, len: usize) {
        if h == 0 {
            return;
        }
        let mut t = self.target;
        if idx < t {
            t = idx;
        } else if idx >= t + h {
            t = idx + 1 - h;
        }
        self.target = t.min(len.saturating_sub(h));
    }
    pub fn clamp(&mut self, h: usize, len: usize) {
        self.target = self.target.min(len.saturating_sub(h));
        let max = len.saturating_sub(h) as f32;
        if self.pos > max {
            self.pos = max;
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepState {
    Running { since_ms: u64 },
    Done { ms: f64 },
    Skipped { reason: String },
    Failed,
}

#[derive(Debug, Clone)]
pub struct StepRow {
    pub name: String,
    pub state: StepState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Out,
    Warn,
    Info,
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub step: Option<String>,
    pub text: String,
    pub kind: LogKind,
}

pub const LOG_CAP: usize = 5000;

#[derive(Debug, Clone)]
pub struct Job {
    pub id: u64,
    pub task: Task,
    pub after: After,
    pub repo: PathBuf,
    pub repo_name: String,
    pub started_ms: u64,
    pub ended_ms: Option<u64>,
    pub steps: Vec<StepRow>,
    pub log: Vec<LogLine>,
    /// Lines dropped from the front once the log hit [`LOG_CAP`].
    pub log_dropped: usize,
    /// One-line summary once finished.
    pub result: Option<Result<String, String>>,
    /// Worktree of a clone this job created.
    pub path: Option<PathBuf>,
    pub scroll: Smooth,
    pub follow: bool,
}

impl Job {
    pub fn running(&self) -> bool {
        self.ended_ms.is_none()
    }

    pub fn title(&self) -> String {
        match &self.task {
            Task::Init => "build golden image".into(),
            Task::Refresh => "refresh golden image".into(),
            Task::New(o) => match (&o.branch, &o.name) {
                (Some(b), _) => format!("new clone {b}"),
                (None, Some(n)) => format!("new clone {n}"),
                (None, None) => "new clone".into(),
            },
            Task::Remove { name, .. } => format!("remove {name}"),
            Task::Mount { name } => format!("mount {name}"),
            Task::Unmount { name } => format!("unmount {name}"),
            Task::Gc { apply: false } => "scan for leftovers".into(),
            Task::Gc { apply: true } => "clean leftovers".into(),
            Task::Doctor => "doctor".into(),
        }
    }

    /// Short verb for a spinner next to a clone row.
    pub fn verb(&self) -> &'static str {
        match &self.task {
            Task::Remove { .. } => "removing",
            Task::Mount { .. } => "mounting",
            Task::Unmount { .. } => "unmounting",
            Task::New(_) => "creating",
            _ => "working",
        }
    }

    /// Clone this job works on, if any.
    pub fn target(&self) -> Option<&str> {
        match &self.task {
            Task::Remove { name, .. } | Task::Mount { name } | Task::Unmount { name } => Some(name),
            _ => None,
        }
    }

    /// Jobs with steps and output worth watching get the progress view.
    pub fn has_progress(&self) -> bool {
        matches!(self.task, Task::Init | Task::Refresh | Task::New(_))
    }

    pub fn elapsed_ms(&self, now_ms: u64) -> u64 {
        self.ended_ms.unwrap_or(now_ms).saturating_sub(self.started_ms)
    }

    pub fn current_step(&self) -> Option<&StepRow> {
        self.steps
            .iter()
            .rev()
            .find(|s| matches!(s.state, StepState::Running { .. }))
    }

    pub fn push_log(&mut self, line: LogLine) {
        self.log.push(line);
        if self.log.len() > LOG_CAP {
            let n = self.log.len() - LOG_CAP;
            self.log.drain(..n);
            self.log_dropped += n;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Ok,
    Info,
    Warn,
    Err,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    pub born_ms: u64,
}

impl Toast {
    pub fn ttl_ms(&self) -> u64 {
        match self.kind {
            ToastKind::Err => 9000,
            ToastKind::Warn => 7000,
            _ => 4000,
        }
    }
}

/// A single-line text field.
#[derive(Debug, Clone, Default)]
pub struct Input {
    pub text: String,
    /// Cursor position in chars.
    pub cursor: usize,
}

impl Input {
    pub fn new(s: &str) -> Self {
        Self {
            text: s.to_string(),
            cursor: s.chars().count(),
        }
    }
    fn byte(&self, ch: usize) -> usize {
        self.text
            .char_indices()
            .nth(ch)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }
    pub fn insert(&mut self, c: char) {
        let b = self.byte(self.cursor);
        self.text.insert(b, c);
        self.cursor += 1;
    }
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let b = self.byte(self.cursor);
            self.text.remove(b);
        }
    }
    pub fn delete(&mut self) {
        if self.cursor < self.text.chars().count() {
            let b = self.byte(self.cursor);
            self.text.remove(b);
        }
    }
    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.text.chars().count());
    }
    pub fn home(&mut self) {
        self.cursor = 0;
    }
    pub fn end(&mut self) {
        self.cursor = self.text.chars().count();
    }
    pub fn set(&mut self, s: &str) {
        *self = Self::new(s);
    }
    /// Deletes the word before the cursor (ctrl-w).
    pub fn delete_word(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.cursor;
        while i > 0 && chars[i - 1] == ' ' {
            i -= 1;
        }
        while i > 0 && !matches!(chars[i - 1], ' ' | '/' | '-' | '_') {
            i -= 1;
        }
        if i == self.cursor && i > 0 {
            i -= 1;
        }
        let rest: String = chars[self.cursor..].iter().collect();
        self.text = chars[..i].iter().collect::<String>() + &rest;
        self.cursor = i;
    }
}

#[derive(Debug, Clone, Default)]
pub struct NewForm {
    pub branch: Input,
    pub base: Input,
    /// 0 = branch, 1 = base.
    pub focus: usize,
    /// Highlighted suggestion, once the user moves into the list.
    pub pick: Option<usize>,
}

impl NewForm {
    pub fn field(&self) -> &Input {
        if self.focus == 0 { &self.branch } else { &self.base }
    }
    pub fn field_mut(&mut self) -> &mut Input {
        if self.focus == 0 {
            &mut self.branch
        } else {
            &mut self.base
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemoveForm {
    pub name: String,
    pub branch: Option<String>,
    pub path: PathBuf,
    pub mounted: bool,
    pub dirty: Option<bool>,
    pub ahead: Option<u32>,
    pub base: String,
    pub delete_branch: bool,
    pub force: bool,
    /// 0 = remove button, 1 = cancel.
    pub focus: usize,
}

#[derive(Debug, Clone, Default)]
pub struct GcView {
    pub actions: Option<Vec<String>>,
    pub applied: bool,
    pub running: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DoctorView {
    pub checks: Option<Vec<Check>>,
    pub error: Option<String>,
    pub scroll: Smooth,
}

#[derive(Debug, Clone)]
pub enum Modal {
    Help,
    New(NewForm),
    Remove(RemoveForm),
    Gc(GcView),
    Doctor(DoctorView),
    Job(u64),
}

/// A clickable or scrollable region recorded while drawing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Repo(usize),
    Row(usize),
    /// Acts as if this key was pressed.
    Key(KeyCode, KeyModifiers),
    Suggestion(usize),
    Field(usize),
    Toast(usize),
    ScrollList,
    ScrollLog,
    ScrollDoctor,
    /// Swallows clicks (a modal's body over the click-to-close backdrop).
    Inert,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_converges() {
        let mut s = Smooth::default();
        s.set(20);
        let mut frames = 0;
        while s.tick() {
            frames += 1;
            assert!(frames < 30);
        }
        assert_eq!(s.offset(), 20);
        assert!(frames > 2, "should ease, not jump");
    }

    #[test]
    fn smooth_follow() {
        let mut s = Smooth::default();
        s.follow(15, 10, 30);
        assert_eq!(s.target(), 6);
        s.follow(2, 10, 30);
        assert_eq!(s.target(), 2);
        s.follow(29, 10, 30);
        assert_eq!(s.target(), 20);
    }

    #[test]
    fn input_editing() {
        let mut i = Input::new("feat/x");
        i.insert('y');
        assert_eq!(i.text, "feat/xy");
        i.delete_word();
        assert_eq!(i.text, "feat/");
        i.delete_word();
        assert_eq!(i.text, "feat");
        i.home();
        i.delete();
        assert_eq!(i.text, "eat");
        i.end();
        i.backspace();
        assert_eq!(i.text, "ea");
    }
}
