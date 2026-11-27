//! Plain data the UI renders: repos, jobs, forms, toasts. No I/O here.

use clove_core::doctor::Check;
use clove_core::settings::Layer;
use clove_core::{
    CloneInfo, Config, GoldenMeta, MergeCheck, NewOptions, RemoveOptions, RepoStatus, RepoSummary,
};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct RepoTab {
    pub name: String,
    pub root: PathBuf,
    /// Clone counts and golden state for the sidebar; None until loaded.
    pub summary: Option<RepoSummary>,
}

#[derive(Debug, Clone)]
pub struct RepoData {
    pub root: PathBuf,
    pub status: RepoStatus,
    pub clones: Vec<CloneInfo>,
    /// Plain git worktrees clove didn't make (see `Clove::other_worktrees`).
    pub worktrees: Vec<clove_core::OtherWorktree>,
    pub config: Config,
    /// The last PR check per clone (`clove prune`, the service, or cleanup here).
    pub prs: BTreeMap<String, MergeCheck>,
    pub summary: RepoSummary,
    /// What to watch on disk to see this repo change.
    pub watch: Vec<clove_core::watch::Target>,
    /// Editable settings with their value per config layer.
    pub fields: Vec<clove_core::settings::FieldState>,
    /// Recent removals, newest first (`clove log`).
    pub history: Vec<clove_core::history::Removed>,
    /// Clone creations with their timings, newest first (`clove stats`).
    pub created: Vec<clove_core::stats::Created>,
    /// Golden image builds, newest first.
    pub golden_builds: Vec<clove_core::stats::GoldenBuild>,
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
    Remove {
        name: String,
        opts: RemoveOptions,
    },
    Mount {
        name: String,
    },
    Unmount {
        name: String,
    },
    /// `git worktree remove` on a plain worktree (`name` is for display).
    RemoveWorktree {
        path: PathBuf,
        name: String,
        delete_branch: bool,
        /// `git worktree remove --force`: discards uncommitted changes.
        force: bool,
        /// Lift its `git worktree lock` first.
        unlock: bool,
    },
    /// Turn a plain worktree into a clone on its branch.
    Adopt {
        path: PathBuf,
        name: String,
    },
    /// Leftovers, merged clones and merged worktrees; `apply` removes them.
    Gc {
        apply: bool,
    },
    Doctor,
    Service {
        install: bool,
    },
    /// `clove update`: bring the clone onto the latest base, or finish or
    /// undo one stopped on conflicts.
    Update {
        name: String,
        step: UpdateStep,
    },
    /// Teach coding agents clove (`clove agents install`), or take it out.
    Agents {
        install: bool,
    },
    /// Add the shell integration line to the user's rc file.
    ShellInstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStep {
    Run,
    Continue,
    Abort,
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
    /// In use (by these); removed once nothing uses it.
    RemovedWhenFree(Vec<String>),
    Mounted(PathBuf),
    Unmounted,
    Gc(Cleanup),
    Doctor(Vec<Check>),
    Service(bool),
    Agents(Option<clove_core::agents::State>),
    /// None after an abort.
    Updated(Option<clove_core::update::Update>),
    /// The rc file that now loads the integration, and whether it changed.
    ShellInstalled(PathBuf, bool),
}

pub use clove_core::Cleanup;

pub enum Msg {
    /// Something the current repo is made of changed on disk.
    FsChanged,
    Loaded {
        root: PathBuf,
        data: Result<Box<RepoData>, String>,
        repos: Vec<RepoTab>,
        /// Whether the launchd agents are installed.
        service: bool,
        /// Whether coding agents know clove; None when none is installed.
        agents: Option<clove_core::agents::State>,
    },
    Refs {
        root: PathBuf,
        refs: Vec<GitRef>,
        /// Existing branches worth offering for a new clone.
        candidates: Vec<clove_core::BranchCandidate>,
    },
    /// A background PR check finished; reload to show it.
    PrsChecked,
    /// A first load's list without git status, while the full one runs.
    Preview(Box<RepoData>),
    Event(u64, clove_core::Event),
    Done(u64, Result<Outcome, String>),
    /// A clone's diff, read and parsed; highlighting continues in the background.
    Diff {
        root: PathBuf,
        name: String,
        result: Result<
            (
                std::sync::Arc<crate::diffview::DiffData>,
                std::sync::Arc<crate::diffview::Highlights>,
            ),
            String,
        >,
    },
    /// Another file of an open diff got its syntax colours.
    DiffHighlighted,
    /// Git repos clove could start managing (add-repo dialog).
    ReposFound(Vec<clove_core::RepoCandidate>),
    /// What `init` would do for a path (add-repo dialog).
    InitPreview {
        path: PathBuf,
        result: Result<clove_core::InitPreview, String>,
    },
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
            Task::RemoveWorktree { name, .. } => format!("remove worktree {name}"),
            Task::Adopt { name, .. } => format!("make {name} a clone"),
            Task::Mount { name } => format!("mount {name}"),
            Task::Unmount { name } => format!("unmount {name}"),
            Task::Gc { apply: false } => "scan for leftovers".into(),
            Task::Gc { apply: true } => "clean leftovers".into(),
            Task::Doctor => "doctor".into(),
            Task::Service { install: true } => "install the service".into(),
            Task::Service { install: false } => "remove the service".into(),
            Task::Update {
                name,
                step: UpdateStep::Run,
            } => format!("update {name}"),
            Task::Update {
                name,
                step: UpdateStep::Continue,
            } => format!("continue updating {name}"),
            Task::Update {
                name,
                step: UpdateStep::Abort,
            } => format!("abort updating {name}"),
            Task::Agents { install: true } => "teach coding agents clove".into(),
            Task::Agents { install: false } => "remove clove from agent instructions".into(),
            Task::ShellInstall => "install shell integration".into(),
        }
    }

    /// Short verb for a spinner next to a clone row.
    pub fn verb(&self) -> &'static str {
        match &self.task {
            Task::Remove { .. } | Task::RemoveWorktree { .. } => "removing",
            Task::Adopt { .. } => "adopting",
            Task::Mount { .. } => "mounting",
            Task::Unmount { .. } => "unmounting",
            Task::New(_) => "creating",
            Task::Update { .. } => "updating",
            _ => "working",
        }
    }

    /// Clone this job works on, if any.
    pub fn target(&self) -> Option<&str> {
        match &self.task {
            Task::Remove { name, .. }
            | Task::Mount { name }
            | Task::Unmount { name }
            | Task::Update { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Plain worktree this job works on, if any.
    pub fn worktree(&self) -> Option<&std::path::Path> {
        match &self.task {
            Task::RemoveWorktree { path, .. } | Task::Adopt { path, .. } => Some(path),
            _ => None,
        }
    }

    /// What the job works on, for messages: a clone or worktree name.
    pub fn subject(&self) -> &str {
        match &self.task {
            Task::RemoveWorktree { name, .. } | Task::Adopt { name, .. } => name,
            _ => self.target().unwrap_or(""),
        }
    }

    /// Jobs with steps and output worth watching get the progress view.
    pub fn has_progress(&self) -> bool {
        matches!(
            self.task,
            Task::Init | Task::Refresh | Task::New(_) | Task::Adopt { .. }
        )
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
    /// Index into [`RemoveForm::items`].
    pub focus: usize,
    /// The shell that started the TUI is inside this clone.
    pub inside: bool,
    /// A plain git worktree, not a clone: no force, `git worktree remove`.
    pub worktree: bool,
    /// Why a plain worktree can't go (locked, dirty, ...), if it can't.
    pub blocker: Option<String>,
    /// A plain worktree blocked only by things the user may override: a lock
    /// (`unlock`) and uncommitted changes (`force`).
    pub forceable: bool,
    /// A plain worktree's `git worktree lock`, and its reason.
    pub locked: bool,
    pub lock_reason: Option<String>,
    /// Lift that lock to remove it.
    pub unlock: bool,
    /// Its `git status --short` lines (at most a dozen) and how many there are.
    pub changes: Vec<String>,
    pub changes_total: usize,
}

/// The focusable rows of the remove dialog, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveItem {
    DeleteBranch,
    Unlock,
    Force,
    Remove,
    Cancel,
}

impl RemoveForm {
    /// The remove button can't act: a plain worktree with a blocker that
    /// force doesn't lift (or force not ticked).
    pub fn blocked(&self) -> bool {
        let overridden =
            self.forceable && (!self.locked || self.unlock) && (self.dirty != Some(true) || self.force);
        self.worktree && self.blocker.is_some() && !overridden
    }

    pub fn items(&self) -> Vec<RemoveItem> {
        let mut v = vec![];
        if self.branch.is_some() {
            v.push(RemoveItem::DeleteBranch);
        }
        if self.worktree && self.forceable && self.locked {
            v.push(RemoveItem::Unlock);
        }
        if !self.worktree || (self.forceable && self.dirty == Some(true)) {
            v.push(RemoveItem::Force);
        }
        v.extend([RemoveItem::Remove, RemoveItem::Cancel]);
        v
    }

    pub fn focused(&self) -> RemoveItem {
        let items = self.items();
        items[self.focus.min(items.len() - 1)]
    }

    pub fn focus_on(&mut self, item: RemoveItem) {
        if let Some(i) = self.items().iter().position(|x| *x == item) {
            self.focus = i;
        }
    }

    /// Flips a checkbox; false for buttons.
    pub fn toggle(&mut self, item: RemoveItem) -> bool {
        match item {
            RemoveItem::DeleteBranch => self.delete_branch = !self.delete_branch,
            RemoveItem::Force => self.force = !self.force,
            RemoveItem::Unlock => self.unlock = !self.unlock,
            _ => return false,
        }
        true
    }
}

/// Confirms turning a plain worktree into a clone.
#[derive(Debug, Clone)]
pub struct AdoptForm {
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    /// Why it can't be adopted (locked, detached, ...), if it can't.
    pub blocker: Option<String>,
    /// Uncommitted changes that move to the clone (via a stash).
    pub changes: usize,
    /// Cancel has focus (else convert).
    pub focus_cancel: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GcView {
    pub result: Option<Cleanup>,
    pub applied: bool,
    pub running: bool,
    pub error: Option<String>,
    /// The close button has focus (else remove, when there is work).
    pub focus_close: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DoctorView {
    pub checks: Option<Vec<Check>>,
    pub error: Option<String>,
    pub scroll: Smooth,
}

/// What clove removed in this repo (`clove log`).
#[derive(Debug, Clone, Default)]
pub struct RemovalsView {
    pub scroll: Smooth,
}

/// Asked when an update stops on conflicts: hand them to the resolver?
#[derive(Debug, Clone)]
pub struct ResolveOffer {
    pub root: PathBuf,
    pub name: String,
    pub strategy: clove_core::update::Strategy,
    pub files: Vec<String>,
    /// "Claude" for `claude --model opus`.
    pub who: String,
    /// No has focus (else yes).
    pub focus_no: bool,
}

#[derive(Debug, Clone)]
pub enum Modal {
    Help,
    ResolveOffer(ResolveOffer),
    Removals(RemovalsView),
    /// How fast clones come (`clove stats`); scrolls like the removals.
    Stats(RemovalsView),
    New(NewForm),
    Remove(RemoveForm),
    Adopt(AdoptForm),
    Gc(GcView),
    Doctor(DoctorView),
    Settings(SettingsView),
    /// A clone's changes against its base.
    Diff(Box<crate::diffview::DiffView>),
    Job(u64),
    AddRepo(Box<crate::add_repo::AddRepo>),
}

#[derive(Debug, Clone, Default)]
pub struct SettingsView {
    pub scroll: Smooth,
    /// Index into [`crate::app::App::settings_actions`].
    pub cursor: usize,
    /// Keep the cursor in view (arrow keys); off after wheel scrolling.
    pub follow: bool,
    /// The config file edits are written to (←/→).
    pub target: Layer,
    /// A setting being typed in.
    pub editing: Option<FieldEdit>,
}

/// An open value editor on a settings row.
#[derive(Debug, Clone)]
pub struct FieldEdit {
    /// Index into [`clove_core::settings::FIELDS`].
    pub field: usize,
    pub input: Input,
}

/// What a focusable settings row does on Enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    /// Edit config layer `n` (0 all repos, 1 team, 2 you).
    Edit(usize),
    /// Setting `n` of [`clove_core::settings::FIELDS`]: toggle or type a value.
    Field(usize),
    Service,
    Agents,
    Shell,
}

/// One setting to write to (or, with no value, remove from) a config file.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigWrite {
    pub path: PathBuf,
    pub layer: Layer,
    pub key: &'static str,
    pub value: Option<String>,
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
    ScrollRemovals,
    ScrollSettings,
    ScrollDiff,
    ScrollDiffList,
    /// An entry of the diff view's file list.
    DiffEntry(usize),
    /// A remove-dialog row: focus it and toggle it.
    RemoveRow(RemoveItem),
    /// A settings row: focus it.
    SettingsRow(SettingsAction),
    /// A "save to" choice in settings.
    SettingsLayer(Layer),
    /// A found repo in the add-repo dialog.
    Candidate(usize),
    /// Swallows clicks (a modal's body over the click-to-close backdrop).
    Inert,
    /// Outside an open modal: a click closes it, hover shows nothing.
    Dismiss,
    /// A split's divider; drags resize this pane.
    Divider(crate::panes::SplitId, usize),
}

/// The mouse pointer shape (OSC 22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pointer {
    #[default]
    Default,
    Hand,
    ColResize,
    RowResize,
}

impl Pointer {
    pub fn name(self) -> &'static str {
        match self {
            Pointer::Default => "default",
            Pointer::Hand => "pointer",
            Pointer::ColResize => "col-resize",
            Pointer::RowResize => "row-resize",
        }
    }
}

impl Hit {
    /// Something a click does: hover shows it.
    pub fn clickable(&self) -> bool {
        !matches!(
            self,
            Hit::ScrollList
                | Hit::ScrollLog
                | Hit::ScrollDoctor
                | Hit::ScrollRemovals
                | Hit::ScrollSettings
                | Hit::ScrollDiff
                | Hit::ScrollDiffList
                | Hit::Inert
                | Hit::Dismiss
        )
    }
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
