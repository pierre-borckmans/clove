//! App state and every input/message transition. Pure: side effects come back as
//! [`Effect`]s for the runtime to perform, so tests drive this directly.

use crate::fmt;
use crate::model::*;
use crate::panes::{self, Panes, SplitId};
use clove_core::repo::sanitize_name;
use clove_core::settings::{self, FIELDS, FieldState, Kind, Layer};
use clove_core::{CloneInfo, Event, NewOptions, OtherWorktree, RemoveOptions};
use ratatui::crossterm::event::{
    Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Reload status and clones for the current repo.
    Load,
    /// Read git refs for completion.
    LoadRefs,
    /// Ask GitHub which clones' PRs are merged (in the background).
    CheckPrs,
    /// Run job `id` (its task lives on the job) on a background thread.
    Spawn {
        id: u64,
        root: Option<PathBuf>,
    },
    OpenEditor(PathBuf),
    /// Open a config file in $EDITOR, creating it from a template if missing,
    /// then reload.
    EditConfig(PathBuf),
    /// Write or remove one setting in one config file, then toast the result.
    SetConfig(ConfigWrite),
    /// Persist panel sizes.
    SaveLayout,
    /// Look for git repos clove doesn't manage yet.
    FindRepos,
    /// Work out what `init` would do for this path.
    PreviewInit(PathBuf),
    /// Suspend the TUI and run the conflict resolver (`update.resolver`) in
    /// a clone stopped on conflicts, then reload.
    Resolve {
        root: PathBuf,
        name: String,
    },
    /// Read a clone's changes for the diff view.
    LoadDiff {
        root: PathBuf,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Exit {
    Quit,
    Cd(PathBuf),
    /// cd to the path, then run `clove <args>` there (removing the clone the
    /// shell was standing in, which keeps its volume busy).
    CdThen(PathBuf, Vec<String>),
}

/// Reload interval while the filesystem watcher runs: it covers clones, the
/// golden image and fetches; this catches the rest (dirty trees, mounts).
pub const RELOAD_MS: u64 = 15_000;
/// Reload interval without a watcher.
pub const RELOAD_UNWATCHED_MS: u64 = 3000;
/// How often an open TUI asks GitHub about PRs: merges by other tools and
/// agents show up without the background service.
pub const PR_CHECK_MS: u64 = 5 * 60 * 1000;
const DOUBLE_CLICK_MS: u64 = 400;

pub struct App {
    pub repos: Vec<RepoTab>,
    pub repo_idx: usize,
    pub data: HashMap<PathBuf, RepoData>,
    pub errors: HashMap<PathBuf, String>,
    /// Startup problem shown when there is no repo at all.
    pub notice: Option<String>,
    pub loading: bool,
    /// A reload was asked for while one was in flight (it may predate a change).
    pub reload_queued: bool,
    pub loaded_ms: Option<u64>,
    /// A filesystem watcher tells us about changes (`Msg::FsChanged`).
    pub watching: bool,
    /// Watch targets from the last load of the current repo, for the runtime to apply.
    pub watch: Option<Vec<clove_core::watch::Target>>,
    pub selected: usize,
    pub list_scroll: Smooth,
    /// Scroll of the plain-worktrees panel.
    pub wt_scroll: Smooth,
    /// Panel rects drawn this frame, for the hover border.
    pub panels: Vec<Rect>,
    /// Rows the clone list showed last frame (for page up/down).
    pub list_height: usize,
    pub filter: String,
    pub filtering: bool,
    pub modal: Option<Modal>,
    pub jobs: Vec<Job>,
    next_id: u64,
    pub toasts: Vec<Toast>,
    pub refs: Vec<GitRef>,
    /// Branch suggestions for a new clone (see `Clove::branch_candidates`).
    pub candidates: Vec<clove_core::BranchCandidate>,
    pub refs_root: Option<PathBuf>,
    pub now_ms: u64,
    pub now_unix: u64,
    pub hits: Vec<(Rect, Hit)>,
    pub exit: Option<Exit>,
    /// Quit asked while operations run: exit once they finish.
    pub quitting: bool,
    /// Select this clone once it shows up in a load.
    pending_select: Option<String>,
    last_click: Option<(u64, usize)>,
    pub log_height: usize,
    /// When the current repo's PRs were last checked.
    pub prs_checked: HashMap<PathBuf, u64>,
    /// Where the mouse is, for hover feedback.
    pub hover: Option<Position>,
    /// Tooltip regions, filled during draw like `hits`.
    pub tips: Vec<(Rect, crate::tooltip::Tip)>,
    /// When the mouse came to rest where it is (tooltips wait for it).
    pub hover_ms: u64,
    /// A key or click hid the tooltip until the mouse moves again.
    pub tip_hidden: bool,
    /// The pointer shape for what is under the mouse.
    pub pointer: Pointer,
    /// Resizable splits and their sizes.
    pub panes: Panes,
    /// The split whose divider the mouse is dragging.
    pub dragging: Option<SplitId>,
    /// Where `clove` was started: the shell's directory.
    pub start_dir: PathBuf,
    /// The shell integration is there to cd on exit.
    pub integrated: bool,
    /// Launchd agents installed; None until the first load.
    pub service: Option<bool>,
    /// Newest removal seen per repo, to announce auto-prunes that happen
    /// while the TUI is open.
    pub seen_removal: HashMap<PathBuf, u64>,
    /// Whether coding agents know clove; None when there are none (or before
    /// the first load).
    pub agents: Option<clove_core::agents::State>,
    /// Split or unified, wrap, sort and grouping of the diff view.
    pub diff_prefs: crate::diffview::DiffPrefs,
    /// Lines the settings view showed last frame (for page up/down).
    pub settings_height: usize,
}

impl App {
    pub fn new(repos: Vec<RepoTab>, repo_idx: usize) -> Self {
        Self {
            repo_idx: repo_idx.min(repos.len().saturating_sub(1)),
            repos,
            data: HashMap::new(),
            errors: HashMap::new(),
            notice: None,
            loading: false,
            reload_queued: false,
            loaded_ms: None,
            watching: false,
            watch: None,
            selected: 0,
            list_scroll: Smooth::default(),
            wt_scroll: Smooth::default(),
            panels: vec![],
            list_height: 10,
            filter: String::new(),
            filtering: false,
            modal: None,
            jobs: vec![],
            next_id: 1,
            toasts: vec![],
            refs: vec![],
            candidates: vec![],
            refs_root: None,
            now_ms: 0,
            now_unix: 0,
            hits: vec![],
            exit: None,
            quitting: false,
            pending_select: None,
            last_click: None,
            log_height: 10,
            hover: None,
            tips: vec![],
            hover_ms: 0,
            tip_hidden: false,
            pointer: Pointer::Default,
            panes: Panes::default(),
            dragging: None,
            prs_checked: HashMap::new(),
            start_dir: PathBuf::new(),
            integrated: false,
            service: None,
            agents: None,
            diff_prefs: Default::default(),
            seen_removal: HashMap::new(),
            settings_height: 10,
        }
    }

    // ---- queries -------------------------------------------------------

    pub fn repo(&self) -> Option<&RepoTab> {
        self.repos.get(self.repo_idx)
    }

    pub fn root(&self) -> Option<PathBuf> {
        self.repo().map(|r| r.root.clone())
    }

    pub fn current(&self) -> Option<&RepoData> {
        self.repo().and_then(|r| self.data.get(&r.root))
    }

    pub fn current_error(&self) -> Option<&String> {
        self.repo().and_then(|r| self.errors.get(&r.root))
    }

    pub fn has_golden(&self) -> bool {
        self.current().is_some_and(|d| d.status.golden.is_some())
    }

    pub fn visible(&self) -> Vec<&CloneInfo> {
        let Some(d) = self.current() else {
            return vec![];
        };
        let q = self.filter.to_lowercase();
        d.clones
            .iter()
            .filter(|c| {
                q.is_empty()
                    || c.name.to_lowercase().contains(&q)
                    || c.branch.as_deref().is_some_and(|b| b.to_lowercase().contains(&q))
            })
            .collect()
    }

    pub fn selected_clone(&self) -> Option<&CloneInfo> {
        self.visible().get(self.selected).copied()
    }

    /// Plain git worktrees matching the filter; they list after the clones.
    pub fn visible_worktrees(&self) -> Vec<&OtherWorktree> {
        let Some(d) = self.current() else {
            return vec![];
        };
        let q = self.filter.to_lowercase();
        d.worktrees
            .iter()
            .filter(|w| {
                q.is_empty()
                    || w.name.to_lowercase().contains(&q)
                    || w.branch.as_deref().is_some_and(|b| b.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// Selection indexes run over the clones, then the worktrees.
    pub fn selected_worktree(&self) -> Option<&OtherWorktree> {
        let n = self.visible().len();
        self.selected
            .checked_sub(n)
            .and_then(|i| self.visible_worktrees().get(i).copied())
    }

    /// Selectable rows: clones and plain worktrees.
    pub fn rows(&self) -> usize {
        self.visible().len() + self.visible_worktrees().len()
    }

    /// The running job on a plain worktree of the current repo.
    pub fn busy_worktree(&self, path: &std::path::Path) -> Option<&Job> {
        let root = self.repo()?.root.clone();
        self.running()
            .find(|j| j.repo == root && j.worktree() == Some(path))
    }

    pub fn job(&self, id: u64) -> Option<&Job> {
        self.jobs.iter().find(|j| j.id == id)
    }

    fn job_mut(&mut self, id: u64) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    pub fn running(&self) -> impl Iterator<Item = &Job> {
        self.jobs.iter().filter(|j| j.running())
    }

    /// The running job working on clone `name` in the current repo.
    pub fn busy(&self, name: &str) -> Option<&Job> {
        let root = self.repo()?.root.clone();
        self.running()
            .find(|j| j.repo == root && j.target() == Some(name))
    }

    /// A running init/refresh on the current repo.
    pub fn golden_job(&self) -> Option<&Job> {
        let root = self.repo()?.root.clone();
        self.running()
            .find(|j| j.repo == root && matches!(j.task, Task::Init | Task::Refresh))
    }

    /// Time to ask GitHub again about the current repo's PRs.
    pub fn needs_pr_check(&self) -> bool {
        let Some(d) = self.current() else {
            return false;
        };
        (!d.clones.is_empty() || !d.worktrees.is_empty())
            && self
                .prs_checked
                .get(&d.root)
                .is_none_or(|t| self.now_ms.saturating_sub(*t) >= PR_CHECK_MS)
    }

    pub fn needs_reload(&self) -> bool {
        let every = if self.watching {
            RELOAD_MS
        } else {
            RELOAD_UNWATCHED_MS
        };
        !self.loading
            && self.repo().is_some()
            && self
                .loaded_ms
                .is_none_or(|t| self.now_ms.saturating_sub(t) >= every)
    }

    // ---- branch completion ------------------------------------------------

    /// Candidates for the focused field of the new-clone form, best first.
    pub fn suggestions(&self, form: &NewForm) -> Vec<String> {
        let q = form.field().text.to_lowercase();
        let pool: Vec<&str> = if form.focus == 0 {
            // An empty branch field means "new branch": nothing to pick yet.
            if q.is_empty() {
                return vec![];
            }
            self.candidates.iter().map(|b| b.name.as_str()).collect()
        } else {
            self.refs.iter().map(|r| r.name.as_str()).collect()
        };
        let mut prefix = vec![];
        let mut inner = vec![];
        for n in pool {
            let l = n.to_lowercase();
            if q.is_empty() || l.starts_with(&q) {
                prefix.push(n);
            } else if l.contains(&q) {
                inner.push(n);
            }
        }
        prefix.extend(inner);
        let out: Vec<String> = prefix.into_iter().take(6).map(String::from).collect();
        if out.len() == 1 && out[0] == form.field().text {
            return vec![];
        }
        out
    }

    /// What `new` will do with the form as typed, for the preview line.
    pub fn new_preview(&self, form: &NewForm) -> (String, Option<String>) {
        let base = if form.base.text.trim().is_empty() {
            self.current().map(|d| d.status.base.clone()).unwrap_or_default()
        } else {
            form.base.text.trim().to_string()
        };
        let b = form.branch.text.trim();
        if b.is_empty() {
            return (
                format!("a generated name, new branch from {base} · or type an existing branch"),
                None,
            );
        }
        let name = sanitize_name(b);
        if name.is_empty() {
            return (
                String::new(),
                Some("that branch name has no usable characters".into()),
            );
        }
        if self
            .current()
            .is_some_and(|d| d.clones.iter().any(|c| c.name == name))
        {
            return (String::new(), Some(format!("clone `{name}` already exists")));
        }
        let local = self.refs.iter().any(|r| r.kind == RefKind::Local && r.name == b);
        let remote = self
            .refs
            .iter()
            .any(|r| r.kind == RefKind::Remote && r.name == format!("origin/{b}"));
        let what = if local {
            format!("existing branch {b}")
        } else if remote {
            format!("tracks origin/{b}")
        } else {
            format!("new branch from {base}")
        };
        let path = self
            .current()
            .map(|d| d.status.clone_root.join(&name).join(&d.status.name))
            .map(|p| fmt::tilde(&p))
            .unwrap_or_default();
        (format!("{name} · {what} · {path}"), None)
    }

    // ---- notifications -------------------------------------------------

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        let text: String = text.into();
        self.toasts.retain(|t| t.text != text);
        self.toasts.push(Toast {
            kind,
            text,
            born_ms: self.now_ms,
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    /// Advances animations and expires toasts. Returns true while anything moves.
    /// How long the event loop may sleep before the next frame: short while
    /// something scrolls, a spinner's frame while something runs, else idle.
    pub fn tick_ms(&mut self) -> u64 {
        if self.advance() {
            40
        } else if self.waiting() {
            crate::theme::SPINNER_MS
        } else {
            250
        }
    }

    /// Steps smooth scrolls and drops expired toasts; true while one moves.
    fn advance(&mut self) -> bool {
        let now = self.now_ms;
        self.toasts.retain(|t| now.saturating_sub(t.born_ms) < t.ttl_ms());
        let mut moving = self.list_scroll.tick();
        moving |= self.wt_scroll.tick();
        for j in &mut self.jobs {
            moving |= j.scroll.tick();
        }
        if let Some(Modal::Doctor(d)) = &mut self.modal {
            moving |= d.scroll.tick();
        }
        if let Some(Modal::Settings(v)) = &mut self.modal {
            moving |= v.scroll.tick();
        }
        moving
    }

    /// Advances animations one frame; true while anything still needs frames.
    #[cfg(test)]
    pub fn animate(&mut self) -> bool {
        self.advance() || self.waiting()
    }

    /// Something shows progress or is about to change: toasts, jobs, a load,
    /// a tooltip on its way.
    fn waiting(&self) -> bool {
        !self.toasts.is_empty()
            || self.running().next().is_some()
            || self.loading
            || self.tip_pending()
            || matches!(&self.modal, Some(Modal::Diff(v)) if v.busy())
    }

    /// A tooltip may be about to show: keep redrawing so it appears on time.
    fn tip_pending(&self) -> bool {
        self.hover.is_some()
            && !self.tip_hidden
            && self.now_ms.saturating_sub(self.hover_ms) <= crate::tooltip::DELAY_MS
    }

    /// Jumps every animation to its end (tests, and after a resize).
    pub fn settle(&mut self) {
        self.list_scroll.snap();
        self.wt_scroll.snap();
        for j in &mut self.jobs {
            j.scroll.snap();
        }
        if let Some(Modal::Doctor(d)) = &mut self.modal {
            d.scroll.snap();
        }
        if let Some(Modal::Settings(v)) = &mut self.modal {
            v.scroll.snap();
        }
    }

    // ---- jobs ----------------------------------------------------------

    pub fn spawn(&mut self, task: Task, after: After) -> Vec<Effect> {
        let Some(repo) = self.repo().cloned() else {
            if matches!(
                task,
                Task::Doctor | Task::Service { .. } | Task::Agents { .. } | Task::ShellInstall
            ) {
                return self.spawn_at(None, String::new(), task, after);
            }
            self.toast(ToastKind::Err, "no repository");
            return vec![];
        };
        self.spawn_at(Some(repo.root), repo.name, task, after)
    }

    pub(crate) fn spawn_at(
        &mut self,
        root: Option<PathBuf>,
        repo_name: String,
        task: Task,
        after: After,
    ) -> Vec<Effect> {
        let id = self.next_id;
        self.next_id += 1;
        let job = Job {
            id,
            task,
            after,
            repo: root.clone().unwrap_or_default(),
            repo_name,
            started_ms: self.now_ms,
            ended_ms: None,
            steps: vec![],
            log: vec![],
            log_dropped: 0,
            result: None,
            path: None,
            scroll: Smooth::default(),
            follow: true,
        };
        if job.has_progress() && !matches!(self.modal, Some(Modal::Remove(_))) {
            self.modal = Some(Modal::Job(id));
        }
        self.jobs.push(job);
        // Keep finished history bounded.
        while self.jobs.len() > 30 {
            match self.jobs.iter().position(|j| !j.running()) {
                Some(i) => {
                    self.jobs.remove(i);
                }
                None => break,
            }
        }
        vec![Effect::Spawn { id, root }]
    }

    fn on_event(&mut self, id: u64, ev: Event) {
        let now = self.now_ms;
        let Some(job) = self.job_mut(id) else { return };
        let mut toast = None;
        match ev {
            Event::Start { step } => job.steps.push(StepRow {
                name: step,
                state: StepState::Running { since_ms: now },
            }),
            Event::Done { step, took } => {
                let ms = took.as_secs_f64() * 1000.0;
                match job
                    .steps
                    .iter_mut()
                    .rev()
                    .find(|s| s.name == step && matches!(s.state, StepState::Running { .. }))
                {
                    Some(s) => s.state = StepState::Done { ms },
                    None => job.steps.push(StepRow {
                        name: step,
                        state: StepState::Done { ms },
                    }),
                }
            }
            Event::Skip { step, reason } => job.steps.push(StepRow {
                name: step,
                state: StepState::Skipped { reason },
            }),
            Event::Output { step, line } => job.push_log(LogLine {
                step: Some(step),
                text: fmt::clean_line(&line),
                kind: LogKind::Out,
            }),
            Event::Info { message } => job.push_log(LogLine {
                step: None,
                text: fmt::clean_line(&message),
                kind: LogKind::Info,
            }),
            Event::Warn { message } => {
                for s in job.steps.iter_mut() {
                    if matches!(s.state, StepState::Running { .. })
                        && message.starts_with(&format!("{} failed", s.name))
                    {
                        s.state = StepState::Failed;
                    }
                }
                for l in message.lines() {
                    job.push_log(LogLine {
                        step: None,
                        text: fmt::clean_line(l),
                        kind: LogKind::Warn,
                    });
                }
                if !job.has_progress() {
                    toast = Some(message.lines().next().unwrap_or("").to_string());
                }
            }
        }
        if let Some(t) = toast {
            self.toast(ToastKind::Warn, t);
        }
    }

    fn on_done(&mut self, id: u64, res: Result<Outcome, String>) -> Vec<Effect> {
        let now = self.now_ms;
        let Some(job) = self.job_mut(id) else {
            return vec![];
        };
        job.ended_ms = Some(now);
        let failed = res.is_err();
        for s in &mut job.steps {
            if let StepState::Running { since_ms } = s.state {
                s.state = if failed {
                    StepState::Failed
                } else {
                    StepState::Done {
                        ms: now.saturating_sub(since_ms) as f64,
                    }
                };
            }
        }
        let elapsed = fmt::secs(job.elapsed_ms(now) as f64);
        let after = job.after;
        let repo = job.repo.clone();
        let task = job.task.clone();
        let mut effects = vec![];
        let mut toasts: Vec<(ToastKind, String)> = vec![];
        let summary: Result<String, String> = match res {
            Err(e) => {
                for l in e.lines() {
                    job.push_log(LogLine {
                        step: None,
                        text: fmt::clean_line(l),
                        kind: LogKind::Warn,
                    });
                }
                let first = e.lines().next().unwrap_or("failed").to_string();
                match &task {
                    Task::Gc { .. } => {
                        if let Some(Modal::Gc(g)) = &mut self.modal {
                            g.running = false;
                            g.error = Some(first.clone());
                        }
                    }
                    Task::Doctor => {
                        if let Some(Modal::Doctor(d)) = &mut self.modal {
                            d.error = Some(first.clone());
                        }
                    }
                    _ => {}
                }
                toasts.push((ToastKind::Err, format!("{}: {first}", job_title(&task))));
                Err(first)
            }
            Ok(Outcome::Golden(m)) => {
                let s = format!(
                    "golden gen {} at {} in {elapsed}",
                    m.generation,
                    fmt::short_commit(&m.commit)
                );
                toasts.push((ToastKind::Ok, s.clone()));
                if m.dirty {
                    toasts.push((
                        ToastKind::Warn,
                        "warm steps modified tracked files; clones will start dirty".into(),
                    ));
                }
                Ok(s)
            }
            Ok(Outcome::Clone(info)) => {
                job.path = Some(info.path.clone());
                let s = format!("{} ready in {elapsed}", info.name);
                toasts.push((ToastKind::Ok, s.clone()));
                if !info.failed_steps.is_empty() {
                    toasts.push((
                        ToastKind::Warn,
                        format!("setup failed: {} (clone is usable)", info.failed_steps.join(", ")),
                    ));
                }
                self.pending_select = Some(info.name.clone());
                Ok(s)
            }
            Ok(Outcome::RemovedWhenFree(users)) => {
                let s = format!(
                    "{} is in use by {}: removing it as soon as {} exit",
                    job.subject(),
                    users.join(", "),
                    if users.len() == 1 { "it" } else { "they" }
                );
                toasts.push((ToastKind::Info, s.clone()));
                Ok(s)
            }
            Ok(Outcome::Removed) => {
                let s = format!("removed {}", job.subject());
                toasts.push((ToastKind::Ok, s.clone()));
                Ok(s)
            }
            Ok(Outcome::Mounted(p)) => {
                let name = job.target().unwrap_or("").to_string();
                match after {
                    After::Cd => self.exit = Some(Exit::Cd(p)),
                    After::Open => effects.push(Effect::OpenEditor(p)),
                    After::Nothing => toasts.push((ToastKind::Ok, format!("mounted {name}"))),
                }
                Ok(format!("mounted {name}"))
            }
            Ok(Outcome::Updated(u)) => {
                use clove_core::update::Update;
                let name = job.target().unwrap_or("").to_string();
                let (kind, s) = match u {
                    None => (
                        ToastKind::Ok,
                        format!("{name}: back where it was before the update"),
                    ),
                    Some(Update::UpToDate { base }) => {
                        (ToastKind::Info, format!("{name} is up to date with {base}"))
                    }
                    Some(Update::Updated {
                        base,
                        commits,
                        hint,
                        failed_steps,
                        ..
                    }) => {
                        let mut s = format!("{name} is on {base}");
                        if let Some(n) = commits {
                            s.push_str(&format!(" ({n} new commit{})", if n == 1 { "" } else { "s" }));
                        }
                        if !failed_steps.is_empty() {
                            s.push_str(&format!("; setup failed: {}", failed_steps.join(", ")));
                        }
                        if let Some(h) = hint {
                            s.push_str(&format!("; {h}"));
                        }
                        let kind = if failed_steps.is_empty() {
                            ToastKind::Ok
                        } else {
                            ToastKind::Warn
                        };
                        (kind, s)
                    }
                    Some(Update::Conflict { strategy, files, .. }) => {
                        let resolver = self.current().and_then(|d| d.config.update.resolver.clone());
                        if let (Some(r), Some(root)) = (&resolver, self.current().map(|d| d.root.clone()))
                            && self.modal.is_none()
                        {
                            self.modal = Some(Modal::ResolveOffer(ResolveOffer {
                                root,
                                name: name.clone(),
                                strategy,
                                files: files.clone(),
                                who: clove_core::update::resolver_name(r),
                                focus_no: false,
                            }));
                        }
                        let who = resolver
                            .map(|r| format!("R resolves with {}, ", clove_core::update::resolver_name(&r)))
                            .unwrap_or_default();
                        let files = match files.len() {
                            1 => files[0].rsplit('/').next().unwrap_or(&files[0]).to_string(),
                            n => format!("{n} files"),
                        };
                        (
                            ToastKind::Warn,
                            format!(
                                "{name}: {} stopped on conflicts ({files}). {who}or fix them, then U continues, X aborts",
                                strategy.word(),
                            ),
                        )
                    }
                };
                toasts.push((kind, s.clone()));
                Ok(s)
            }
            Ok(Outcome::Unmounted) => {
                let s = format!("unmounted {}", job.target().unwrap_or(""));
                toasts.push((ToastKind::Ok, s.clone()));
                Ok(s)
            }
            Ok(Outcome::Gc(cleanup)) => {
                let apply = matches!(task, Task::Gc { apply: true });
                if apply {
                    let wt = cleanup.removed_worktrees.len();
                    let n = cleanup.removed.len() + wt + cleanup.leftovers.len();
                    toasts.push((
                        ToastKind::Ok,
                        match (cleanup.removed.len(), wt, cleanup.leftovers.len()) {
                            (0, 0, 0) => "nothing to clean".to_string(),
                            (c, 0, 0) => format!("removed {c} merged clone{}", if c == 1 { "" } else { "s" }),
                            (0, w, 0) => {
                                format!("removed {w} merged worktree{}", if w == 1 { "" } else { "s" })
                            }
                            _ => format!("cleaned {n} item{}", if n == 1 { "" } else { "s" }),
                        },
                    ));
                }
                if let Some(Modal::Gc(g)) = &mut self.modal {
                    g.running = false;
                    g.applied = apply;
                    g.result = Some(cleanup);
                }
                Ok(String::new())
            }
            Ok(Outcome::ShellInstalled(path, changed)) => {
                let s = if changed {
                    format!("added to {}; open a new terminal to use it", fmt::tilde(&path))
                } else {
                    format!("already in {}; open a new terminal to use it", fmt::tilde(&path))
                };
                toasts.push((ToastKind::Ok, s.clone()));
                Ok(s)
            }
            Ok(Outcome::Service(on)) => {
                self.service = Some(on);
                let s = if on {
                    "service installed: refreshes on fetch, remounts at login"
                } else {
                    "service removed"
                };
                toasts.push((ToastKind::Ok, s.to_string()));
                Ok(s.to_string())
            }
            Ok(Outcome::Agents(state)) => {
                self.agents = state;
                let s = if state == Some(clove_core::agents::State::Installed) {
                    "coding agents now know clove; restart their sessions to pick it up"
                } else {
                    "clove taken out of agent instructions"
                };
                toasts.push((ToastKind::Ok, s.to_string()));
                Ok(s.to_string())
            }
            Ok(Outcome::Doctor(checks)) => {
                if let Some(Modal::Doctor(d)) = &mut self.modal {
                    d.checks = Some(checks);
                }
                Ok(String::new())
            }
        };
        if let Some(job) = self.job_mut(id) {
            job.result = Some(summary);
        }
        // The open progress view already shows how it went.
        let watching = matches!(self.modal, Some(Modal::Job(j)) if j == id);
        for (k, t) in toasts {
            if !(watching && k == ToastKind::Ok) {
                self.toast(k, t);
            }
        }
        if self.repo().is_some_and(|r| r.root == repo)
            && !matches!(
                task,
                Task::Doctor | Task::Service { .. } | Task::Agents { .. } | Task::ShellInstall
            )
        {
            effects.push(Effect::Load);
        }
        effects
    }

    pub fn on_msg(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Diff { root, name, result } => {
                match &mut self.modal {
                    Some(Modal::Diff(v)) if v.root == root && v.name == name => v.set(match result {
                        Ok((data, hl)) => crate::diffview::Load::Ready(data, hl),
                        Err(e) => crate::diffview::Load::Failed(e),
                    }),
                    // Closed meanwhile: stop highlighting what nobody will see.
                    _ => {
                        if let Ok((_, hl)) = result {
                            hl.stop();
                        }
                    }
                }
                vec![]
            }
            Msg::DiffHighlighted => vec![],
            Msg::Loaded {
                root,
                data,
                repos,
                service,
                agents,
            } => {
                self.loading = false;
                self.service = Some(service);
                self.agents = agents;
                // A load for a repo we switched away from: reload the current one now.
                let current = self.repo().is_some_and(|r| r.root == root);
                self.loaded_ms = (current && !self.reload_queued).then_some(self.now_ms);
                self.reload_queued = false;
                self.merge_repos(repos);
                match data {
                    Ok(d) => {
                        self.errors.remove(&root);
                        self.apply_data(*d);
                    }
                    Err(e) => {
                        self.errors.insert(root, e);
                    }
                }
                vec![]
            }
            Msg::FsChanged => {
                if self.loading {
                    self.reload_queued = true;
                } else {
                    self.loaded_ms = None;
                }
                vec![]
            }
            Msg::Preview(d) => {
                if !self.data.contains_key(&d.root) {
                    self.apply_data(*d);
                }
                vec![]
            }
            Msg::PrsChecked => {
                self.loaded_ms = None;
                vec![]
            }
            Msg::Refs {
                root,
                refs,
                candidates,
            } => {
                self.refs_root = Some(root);
                self.refs = refs;
                self.candidates = candidates;
                vec![]
            }
            Msg::Event(id, ev) => {
                self.on_event(id, ev);
                vec![]
            }
            Msg::Done(id, res) => {
                let fx = self.on_done(id, res);
                if self.quitting && self.running().next().is_none() {
                    self.exit = Some(Exit::Quit);
                }
                fx
            }
            Msg::ReposFound(found) => {
                self.on_repos_found(found);
                vec![]
            }
            Msg::InitPreview { path, result } => {
                self.on_init_preview(path, result);
                vec![]
            }
        }
    }

    /// Stores freshly loaded data, keeping the selection on the same clone.
    pub fn apply_data(&mut self, d: RepoData) {
        let is_current = self.repo().is_some_and(|r| r.root == d.root);
        if is_current {
            self.watch = Some(d.watch.clone());
        }
        let keep = self.selected_clone().map(|c| c.name.clone());
        let keep_wt = self.selected_worktree().map(|w| w.path.clone());
        if let Some(t) = self.repos.iter_mut().find(|t| t.root == d.root) {
            t.summary = Some(d.summary.clone());
        }
        let newest = d.history.first().map_or(0, |e| e.at);
        if let Some(prev) = self.seen_removal.insert(d.root.clone(), newest) {
            let fresh: Vec<String> = d
                .history
                .iter()
                .take_while(|e| e.at > prev)
                .filter(|e| e.by == clove_core::history::By::AutoPrune)
                .map(|e| e.summary(self.now_unix))
                .collect();
            for s in fresh {
                self.toast(ToastKind::Info, s);
            }
        }
        self.data.insert(d.root.clone(), d);
        if !is_current {
            return;
        }
        let want = self.pending_select.take().or(keep);
        let vis = self.visible();
        if let Some(w) = want
            && let Some(i) = vis.iter().position(|c| c.name == w)
        {
            self.selected = i;
        } else if let Some(p) = keep_wt
            && let Some(i) = self.visible_worktrees().iter().position(|w| w.path == p)
        {
            self.selected = vis.len() + i;
        }
        let n = self.rows();
        self.selected = self.selected.min(n.saturating_sub(1));
    }

    /// Adds repos clove learned about (e.g. after an init) without reordering,
    /// and takes their fresh summaries.
    fn merge_repos(&mut self, repos: Vec<RepoTab>) {
        for r in repos {
            match self.repos.iter_mut().find(|x| x.root == r.root) {
                Some(x) => {
                    if r.summary.is_some() {
                        x.summary = r.summary;
                    }
                }
                None => self.repos.push(r),
            }
        }
    }

    // ---- input ---------------------------------------------------------

    pub fn on_input(&mut self, ev: TermEvent) -> Vec<Effect> {
        if matches!(ev, TermEvent::Key(_) | TermEvent::Paste(_)) {
            self.tip_hidden = true;
        }
        match ev {
            TermEvent::Key(k) if k.kind != KeyEventKind::Release => self.on_key(k),
            TermEvent::Mouse(m) => self.on_mouse(m),
            TermEvent::Paste(s) => {
                if let Some(Modal::New(f)) = &mut self.modal {
                    for c in s.chars().filter(|c| !c.is_control()) {
                        f.field_mut().insert(c);
                    }
                    f.pick = None;
                } else if let Some(Modal::AddRepo(_)) = &self.modal {
                    self.add_repo_paste(&s);
                } else if self.filtering {
                    self.filter.extend(s.chars().filter(|c| !c.is_control()));
                    self.selected = 0;
                }
                vec![]
            }
            _ => vec![],
        }
    }

    pub fn on_key(&mut self, k: KeyEvent) -> Vec<Effect> {
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.request_quit();
            return vec![];
        }
        if self.modal.is_some() {
            return self.on_modal_key(k);
        }
        if self.filtering {
            return self.on_filter_key(k);
        }
        self.on_main_key(k)
    }

    /// Quits now, or once running operations finish: killing a refresh or a
    /// clone halfway leaves leftovers. Asking twice quits regardless.
    pub fn request_quit(&mut self) {
        let running: Vec<String> = self.running().map(|j| j.title()).collect();
        if running.is_empty() || self.quitting {
            self.exit = Some(Exit::Quit);
            return;
        }
        self.quitting = true;
        let what = match running.as_slice() {
            [one] => one.clone(),
            many => format!("{} operations", many.len()),
        };
        self.toast(
            ToastKind::Warn,
            format!("waiting for {what} to finish; q again quits now"),
        );
    }

    fn on_filter_key(&mut self, k: KeyEvent) -> Vec<Effect> {
        match k.code {
            KeyCode::Esc => {
                self.filtering = false;
                self.filter.clear();
                self.selected = 0;
            }
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                if self.filter.pop().is_none() {
                    self.filtering = false;
                }
                self.selected = 0;
            }
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                return self.on_main_key(k);
            }
            KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.clear();
                self.selected = 0;
            }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(c);
                self.selected = 0;
            }
            _ => {}
        }
        vec![]
    }

    fn move_sel(&mut self, delta: isize) {
        let n = self.rows();
        if n == 0 {
            self.selected = 0;
            return;
        }
        let i = self.selected as isize + delta;
        self.selected = i.clamp(0, n as isize - 1) as usize;
    }

    pub fn switch_repo(&mut self, idx: usize) -> Vec<Effect> {
        if idx >= self.repos.len() || idx == self.repo_idx {
            return vec![];
        }
        self.repo_idx = idx;
        self.selected = 0;
        self.filter.clear();
        self.filtering = false;
        self.list_scroll = Smooth::default();
        self.wt_scroll = Smooth::default();
        self.loaded_ms = None;
        if self.loading {
            // The in-flight load is for the old repo; let the next tick reload.
            return vec![];
        }
        vec![Effect::Load]
    }

    fn on_main_key(&mut self, k: KeyEvent) -> Vec<Effect> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let page = self.list_height.max(1) as isize;
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.selected = 0;
                } else {
                    self.request_quit();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::PageUp => self.move_sel(-page),
            KeyCode::PageDown => self.move_sel(page),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.move_sel(isize::MAX / 2),
            KeyCode::Tab | KeyCode::Char(']') if !self.repos.is_empty() => {
                return self.switch_repo((self.repo_idx + 1) % self.repos.len());
            }
            KeyCode::BackTab | KeyCode::Char('[') if !self.repos.is_empty() => {
                let n = self.repos.len();
                return self.switch_repo((self.repo_idx + n - 1) % n);
            }
            KeyCode::Char(c @ '1'..='9') => {
                return self.switch_repo(c as usize - '1' as usize);
            }
            KeyCode::Char('r') if ctrl => {
                // A manual reload asks git again instead of trusting the memo.
                clove_core::memo::clear();
                return vec![Effect::Load];
            }
            KeyCode::Char('n') | KeyCode::Char('a') => return self.open_new(),
            KeyCode::Char('d') | KeyCode::Delete => self.open_remove(),
            KeyCode::Char('m') => return self.mount_selected(After::Nothing),
            KeyCode::Char('u') => return self.unmount_selected(),
            KeyCode::Char('U') => return self.update_selected(false),
            KeyCode::Char('X') => return self.update_selected(true),
            KeyCode::Char('R') => return self.resolve_selected(),
            KeyCode::Char('r') => return self.refresh(),
            KeyCode::Char('i') => return self.init(),
            KeyCode::Char('A') | KeyCode::Char('+') => return self.open_add_repo(),
            KeyCode::Enter => return self.cd_selected(),
            KeyCode::Char('o') => return self.open_selected(),
            KeyCode::Char('c') => self.open_adopt(),
            KeyCode::Char('/') => {
                self.filtering = true;
            }
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Char('v') => return self.open_diff(),
            KeyCode::Char('g') => {
                if self.repo().is_none() {
                    return vec![];
                }
                self.modal = Some(Modal::Gc(GcView {
                    running: true,
                    ..Default::default()
                }));
                return self.spawn(Task::Gc { apply: false }, After::Nothing);
            }
            KeyCode::Char('L') => {
                self.modal = Some(Modal::Removals(RemovalsView::default()));
                return vec![];
            }
            KeyCode::Char('t') => {
                if self.current().is_some() {
                    self.modal = Some(Modal::Stats(RemovalsView::default()));
                }
                return vec![];
            }
            KeyCode::Char('D') => {
                self.modal = Some(Modal::Doctor(DoctorView::default()));
                return self.spawn(Task::Doctor, After::Nothing);
            }
            KeyCode::Char('s') | KeyCode::Char(',') => {
                if self.current().is_some() {
                    self.modal = Some(Modal::Settings(SettingsView::default()));
                } else {
                    self.toast(ToastKind::Info, "settings show once the repo has loaded");
                }
            }
            KeyCode::Char('<' | '>' | '{' | '}' | '=') => return self.resize_key(k.code),
            KeyCode::Char('l') => {
                if let Some(j) = self.jobs.iter().rev().find(|j| j.has_progress()) {
                    self.modal = Some(Modal::Job(j.id));
                } else {
                    self.toast(ToastKind::Info, "no activity yet");
                }
            }
            _ => {}
        }
        vec![]
    }

    /// `<` `>` move the detail panel's border, `{` `}` the sidebar's, `=`
    /// resets every panel. Applies to the layout last drawn.
    fn resize_key(&mut self, code: KeyCode) -> Vec<Effect> {
        let (id, pane, delta) = match code {
            KeyCode::Char('=') => {
                self.panes.reset_all();
                return vec![Effect::SaveLayout];
            }
            KeyCode::Char('{') => (SplitId::Body, panes::SIDEBAR, -2),
            KeyCode::Char('}') => (SplitId::Body, panes::SIDEBAR, 2),
            KeyCode::Char(c) => {
                let d = if c == '<' { -1 } else { 1 };
                if self.panes.body.shown(panes::DETAIL) {
                    (SplitId::Body, panes::DETAIL, 2 * d)
                } else {
                    (SplitId::Stack, panes::STACK_DETAIL, d)
                }
            }
            _ => return vec![],
        };
        if id == SplitId::Body && !self.panes.body.shown(pane) {
            return vec![];
        }
        if self.panes.get_mut(id).move_divider(pane, delta) {
            vec![Effect::SaveLayout]
        } else {
            vec![]
        }
    }

    /// `v`: the selected clone's changes against its base.
    fn open_diff(&mut self) -> Vec<Effect> {
        let Some(root) = self.root() else { return vec![] };
        let Some(c) = self.selected_clone().cloned() else {
            self.toast(ToastKind::Info, "select a clone to see its changes");
            return vec![];
        };
        if !c.mounted {
            self.toast(ToastKind::Info, format!("{} is unmounted: m mounts it", c.name));
            return vec![];
        }
        self.modal = Some(Modal::Diff(Box::new(crate::diffview::DiffView::new(
            root.clone(),
            c.name.clone(),
            c.branch.clone(),
        ))));
        vec![Effect::LoadDiff { root, name: c.name }]
    }

    fn diff_key(
        &mut self,
        mut v: Box<crate::diffview::DiffView>,
        k: KeyEvent,
    ) -> (Option<Modal>, Vec<Effect>) {
        use crate::diffview::KeyResult;
        // `{` `}` move the border between the file list and the diff, `=` resets it.
        let resize = match k.code {
            KeyCode::Char('{') => Some(-2),
            KeyCode::Char('}') => Some(2),
            KeyCode::Char('=') => Some(0),
            _ => None,
        };
        if let Some(delta) = resize {
            let split = &mut self.panes.diff;
            let changed = if delta == 0 {
                split.reset_all();
                true
            } else {
                split.move_divider(panes::DIFF_FILES, delta)
            };
            let effects = if changed { vec![Effect::SaveLayout] } else { vec![] };
            return (Some(Modal::Diff(v)), effects);
        }
        let mut prefs = self.diff_prefs;
        let r = v.key(k, &mut prefs);
        self.diff_prefs = prefs;
        match r {
            KeyResult::Close => {
                v.stop();
                (None, vec![])
            }
            KeyResult::Reload => {
                v.reloading();
                let e = Effect::LoadDiff {
                    root: v.root.clone(),
                    name: v.name.clone(),
                };
                (Some(Modal::Diff(v)), vec![e])
            }
            KeyResult::Edit(path) => {
                let clone = self
                    .current()
                    .and_then(|d| d.clones.iter().find(|c| c.name == v.name))
                    .map(|c| c.path.join(&path));
                (
                    Some(Modal::Diff(v)),
                    clone.map(Effect::OpenEditor).into_iter().collect(),
                )
            }
            KeyResult::None => (Some(Modal::Diff(v)), vec![]),
        }
    }

    fn open_new(&mut self) -> Vec<Effect> {
        if self.current().is_none() {
            return vec![];
        }
        if !self.has_golden() {
            self.toast(ToastKind::Info, "build the golden image first: press i");
            return vec![];
        }
        let base = self.current().map(|d| d.status.base.clone()).unwrap_or_default();
        if self.refs_root != self.root() {
            self.refs.clear();
        }
        self.modal = Some(Modal::New(NewForm {
            base: Input::new(&base),
            ..Default::default()
        }));
        vec![Effect::LoadRefs]
    }

    fn open_remove(&mut self) {
        if let Some(w) = self.selected_worktree().cloned() {
            if self.busy_worktree(&w.path).is_some() {
                return;
            }
            let base = self.current().map(|d| d.status.base.clone()).unwrap_or_default();
            let forceable = w.forceable();
            self.modal = Some(Modal::Remove(RemoveForm {
                inside: self.start_dir.starts_with(&w.path),
                name: w.name,
                branch: w.branch,
                path: w.path,
                mounted: !w.missing,
                dirty: w.dirty,
                ahead: None,
                base,
                delete_branch: false,
                force: false,
                focus: 0,
                worktree: true,
                forceable,
                locked: w.locked,
                lock_reason: w.lock_reason,
                unlock: false,
                blocker: w.blocker,
                changes: w.changes,
                changes_total: w.changes_total,
            }));
            if let Some(Modal::Remove(f)) = &mut self.modal {
                f.focus_on(RemoveItem::Remove);
            }
            return;
        }
        let Some(c) = self.selected_clone().cloned() else {
            return;
        };
        if self.busy(&c.name).is_some() {
            return;
        }
        let base = self.current().map(|d| d.status.base.clone()).unwrap_or_default();
        let inside = c.path.parent().is_some_and(|m| self.start_dir.starts_with(m));
        self.modal = Some(Modal::Remove(RemoveForm {
            name: c.name,
            branch: c.branch,
            path: c.path,
            mounted: c.mounted,
            dirty: c.dirty,
            ahead: c.ahead,
            base,
            delete_branch: false,
            force: c.dirty == Some(true),
            focus: 0,
            inside,
            worktree: false,
            blocker: None,
            forceable: false,
            locked: false,
            lock_reason: None,
            unlock: false,
            changes: vec![],
            changes_total: 0,
        }));
        if let Some(Modal::Remove(f)) = &mut self.modal {
            f.focus_on(RemoveItem::Remove);
        }
    }

    /// Asks before turning the selected plain worktree into a clone.
    fn open_adopt(&mut self) {
        let Some(w) = self.selected_worktree().cloned() else {
            if self.selected_clone().is_some() {
                self.toast(
                    ToastKind::Info,
                    "c turns a plain git worktree into a clone; this is one already",
                );
            }
            return;
        };
        if self.busy_worktree(&w.path).is_some() {
            return;
        }
        if !self.has_golden() {
            self.toast(ToastKind::Info, "build the golden image first: press i");
            return;
        }
        let blocker = w.adopt_blocker();
        self.modal = Some(Modal::Adopt(AdoptForm {
            changes: if w.dirty == Some(true) {
                w.changes_total.max(1)
            } else {
                0
            },
            name: w.name,
            path: w.path,
            branch: w.branch,
            blocker,
            focus_cancel: false,
        }));
    }

    fn adopt_key(&mut self, mut f: AdoptForm, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        match k.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => (None, vec![]),
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Up
            | KeyCode::Down => {
                f.focus_cancel = !f.focus_cancel;
                (Some(Modal::Adopt(f)), vec![])
            }
            KeyCode::Enter | KeyCode::Char(' ') if f.focus_cancel => (None, vec![]),
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('y') => {
                if let Some(b) = &f.blocker {
                    self.toast(ToastKind::Err, b.clone());
                    return (Some(Modal::Adopt(f)), vec![]);
                }
                let task = Task::Adopt {
                    path: f.path.clone(),
                    name: f.name.clone(),
                };
                (None, self.spawn(task, After::Nothing))
            }
            _ => (Some(Modal::Adopt(f)), vec![]),
        }
    }

    fn mount_selected(&mut self, after: After) -> Vec<Effect> {
        if self.selected_worktree().is_some() {
            self.toast(ToastKind::Info, "a plain git worktree has nothing to mount");
            return vec![];
        }
        let Some(c) = self.selected_clone().cloned() else {
            return vec![];
        };
        if self.busy(&c.name).is_some() {
            return vec![];
        }
        if c.mounted && after == After::Nothing {
            self.toast(ToastKind::Info, format!("{} is already mounted", c.name));
            return vec![];
        }
        self.spawn(Task::Mount { name: c.name }, after)
    }

    /// `U` updates the clone onto the latest base, or finishes an update
    /// stopped on conflicts; `X` undoes a stopped one.
    fn update_selected(&mut self, abort: bool) -> Vec<Effect> {
        if self.selected_worktree().is_some() {
            self.toast(
                ToastKind::Info,
                "update works on clones; c makes this worktree one",
            );
            return vec![];
        }
        let Some(c) = self.selected_clone().cloned() else {
            return vec![];
        };
        if self.busy(&c.name).is_some() {
            return vec![];
        }
        let step = match (abort, c.operation.is_some()) {
            (true, true) => UpdateStep::Abort,
            (true, false) => {
                self.toast(ToastKind::Info, format!("{} has no update to abort", c.name));
                return vec![];
            }
            (false, true) => UpdateStep::Continue,
            (false, false) => UpdateStep::Run,
        };
        self.spawn(Task::Update { name: c.name, step }, After::Nothing)
    }

    /// `R` on a clone stopped on conflicts: hand them to the resolver.
    fn resolve_selected(&mut self) -> Vec<Effect> {
        let Some(c) = self.selected_clone().cloned() else {
            return vec![];
        };
        if c.operation.is_none() {
            self.toast(ToastKind::Info, format!("{} has no conflicts to resolve", c.name));
            return vec![];
        }
        let Some(d) = self.current() else { return vec![] };
        if d.config.update.resolver.is_none() {
            self.toast(ToastKind::Info, "update.resolver is off");
            return vec![];
        }
        vec![Effect::Resolve {
            root: d.root.clone(),
            name: c.name,
        }]
    }

    fn unmount_selected(&mut self) -> Vec<Effect> {
        if self.selected_worktree().is_some() {
            self.toast(ToastKind::Info, "a plain git worktree has nothing to unmount");
            return vec![];
        }
        let Some(c) = self.selected_clone().cloned() else {
            return vec![];
        };
        if self.busy(&c.name).is_some() {
            return vec![];
        }
        if !c.mounted {
            self.toast(ToastKind::Info, format!("{} is not mounted", c.name));
            return vec![];
        }
        self.spawn(Task::Unmount { name: c.name }, After::Nothing)
    }

    fn refresh(&mut self) -> Vec<Effect> {
        if let Some(j) = self.golden_job() {
            let id = j.id;
            self.modal = Some(Modal::Job(id));
            return vec![];
        }
        if self.current().is_none() {
            return vec![];
        }
        if !self.has_golden() {
            self.toast(ToastKind::Info, "no golden image yet: press i to build it");
            return vec![];
        }
        self.spawn(Task::Refresh, After::Nothing)
    }

    fn init(&mut self) -> Vec<Effect> {
        if let Some(j) = self.golden_job() {
            let id = j.id;
            self.modal = Some(Modal::Job(id));
            return vec![];
        }
        if self.current().is_none() {
            return vec![];
        }
        if self.has_golden() {
            self.toast(ToastKind::Info, "golden image exists; r refreshes it");
            return vec![];
        }
        self.spawn(Task::Init, After::Nothing)
    }

    fn cd_selected(&mut self) -> Vec<Effect> {
        if let Some(w) = self.selected_worktree().cloned() {
            if w.missing {
                self.toast(ToastKind::Warn, format!("{} is gone from disk", w.name));
            } else {
                self.exit = Some(Exit::Cd(w.path));
            }
            return vec![];
        }
        let Some(c) = self.selected_clone().cloned() else {
            return vec![];
        };
        if self.busy(&c.name).is_some() {
            return vec![];
        }
        if c.mounted {
            self.exit = Some(Exit::Cd(c.path));
            vec![]
        } else {
            self.spawn(Task::Mount { name: c.name }, After::Cd)
        }
    }

    fn open_selected(&mut self) -> Vec<Effect> {
        if let Some(w) = self.selected_worktree().cloned() {
            return if w.missing {
                vec![]
            } else {
                vec![Effect::OpenEditor(w.path)]
            };
        }
        let Some(c) = self.selected_clone().cloned() else {
            return vec![];
        };
        if self.busy(&c.name).is_some() {
            return vec![];
        }
        if c.mounted {
            vec![Effect::OpenEditor(c.path)]
        } else {
            self.spawn(Task::Mount { name: c.name }, After::Open)
        }
    }

    fn on_modal_key(&mut self, k: KeyEvent) -> Vec<Effect> {
        let Some(modal) = self.modal.take() else {
            return vec![];
        };
        let (keep, effects) = match modal {
            Modal::Help => (
                (!matches!(
                    k.code,
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') | KeyCode::Enter
                ))
                .then_some(Modal::Help),
                vec![],
            ),
            Modal::New(f) => self.new_form_key(f, k),
            Modal::Remove(f) => self.remove_form_key(f, k),
            Modal::Adopt(f) => self.adopt_key(f, k),
            Modal::Gc(g) => self.gc_key(g, k),
            Modal::Doctor(d) => self.doctor_key(d, k),
            Modal::Removals(v) => self.removals_key(v, k),
            Modal::Stats(v) => self.stats_key(v, k),
            Modal::ResolveOffer(o) => self.resolve_offer_key(o, k),
            Modal::Settings(v) => self.settings_key(v, k),
            Modal::Diff(v) => self.diff_key(v, k),
            Modal::Job(id) => self.job_key(id, k),
            Modal::AddRepo(v) => self.add_repo_key(v, k),
        };
        // A handler may have opened another modal (or exited); don't clobber it.
        if self.modal.is_none() {
            self.modal = keep;
        }
        effects
    }

    fn new_form_key(&mut self, mut f: NewForm, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let sugg = self.suggestions(&f);
        match k.code {
            KeyCode::Esc => {
                if f.pick.is_some() {
                    f.pick = None;
                } else {
                    return (None, vec![]);
                }
            }
            KeyCode::Enter => {
                if let Some(i) = f.pick
                    && let Some(s) = sugg.get(i)
                {
                    f.field_mut().set(s);
                    f.pick = None;
                    return (Some(Modal::New(f)), vec![]);
                }
                let (_, err) = self.new_preview(&f);
                if let Some(e) = err {
                    self.toast(ToastKind::Err, e);
                    return (Some(Modal::New(f)), vec![]);
                }
                let branch = f.branch.text.trim();
                let base = f.base.text.trim();
                let opts = NewOptions {
                    branch: (!branch.is_empty()).then(|| branch.to_string()),
                    base: (!base.is_empty()).then(|| base.to_string()),
                    ..Default::default()
                };
                return (None, self.spawn(Task::New(opts), After::Nothing));
            }
            KeyCode::Tab => {
                let cand = f.pick.and_then(|i| sugg.get(i)).or(sugg.first());
                match cand {
                    Some(s) if *s != f.field().text => {
                        f.field_mut().set(s);
                        f.pick = None;
                    }
                    _ => {
                        f.focus = 1 - f.focus;
                        f.pick = None;
                    }
                }
            }
            KeyCode::BackTab => {
                f.focus = 1 - f.focus;
                f.pick = None;
            }
            KeyCode::Down => {
                if !sugg.is_empty() {
                    f.pick = Some(f.pick.map_or(0, |i| (i + 1) % sugg.len()));
                }
            }
            KeyCode::Up => {
                if !sugg.is_empty() {
                    f.pick = Some(
                        f.pick
                            .map_or(sugg.len() - 1, |i| (i + sugg.len() - 1) % sugg.len()),
                    );
                }
            }
            KeyCode::Left => f.field_mut().left(),
            KeyCode::Right => {
                let i = f.field_mut();
                if i.cursor == i.text.chars().count() {
                    // At the end, → accepts the top suggestion like a shell.
                    if let Some(s) = f.pick.and_then(|p| sugg.get(p)).or(sugg.first()) {
                        f.field_mut().set(s);
                        f.pick = None;
                    }
                } else {
                    i.right();
                }
            }
            KeyCode::Home => f.field_mut().home(),
            KeyCode::End => f.field_mut().end(),
            KeyCode::Backspace => {
                f.field_mut().backspace();
                f.pick = None;
            }
            KeyCode::Delete => {
                f.field_mut().delete();
                f.pick = None;
            }
            KeyCode::Char('a') if ctrl => f.field_mut().home(),
            KeyCode::Char('e') if ctrl => f.field_mut().end(),
            KeyCode::Char('u') if ctrl => {
                f.field_mut().set("");
                f.pick = None;
            }
            KeyCode::Char('w') if ctrl => {
                f.field_mut().delete_word();
                f.pick = None;
            }
            KeyCode::Char(c) if !ctrl && !c.is_whitespace() => {
                f.field_mut().insert(c);
                f.pick = None;
            }
            _ => {}
        }
        (Some(Modal::New(f)), vec![])
    }

    fn remove_form_key(&mut self, mut f: RemoveForm, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        match k.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => return (None, vec![]),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                let n = f.items().len();
                f.focus = (f.focus + n - 1) % n;
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                f.focus = (f.focus + 1) % f.items().len();
            }
            // Between the two buttons.
            KeyCode::Left | KeyCode::Right => match f.focused() {
                RemoveItem::Remove => f.focus_on(RemoveItem::Cancel),
                RemoveItem::Cancel => f.focus_on(RemoveItem::Remove),
                _ => {}
            },
            KeyCode::Char(' ') if f.toggle(f.focused()) => {}
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('y') => {
                // Enter on a checkbox takes the default: remove.
                if f.focused() == RemoveItem::Cancel && k.code != KeyCode::Char('y') {
                    return (None, vec![]);
                }
                if f.blocked() {
                    let b = f.blocker.clone().unwrap_or_default();
                    let hint = if f.forceable {
                        "; tick \"discard these changes\" to remove it anyway"
                    } else {
                        ""
                    };
                    self.toast(ToastKind::Err, format!("{b}{hint}"));
                    return (Some(Modal::Remove(f)), vec![]);
                }
                if f.inside {
                    // The shell sits in this clone and holds its volume: quit,
                    // move the shell to the main checkout, remove it from there.
                    let Some(root) = self.root() else {
                        return (None, vec![]);
                    };
                    if !self.integrated {
                        self.toast(
                            ToastKind::Err,
                            "your shell is inside this clone: quit, cd out, then remove it (or `clove shell-init --install` so clove can do it)",
                        );
                        return (Some(Modal::Remove(f)), vec![]);
                    }
                    let mut args = vec!["rm".to_string()];
                    if f.delete_branch {
                        args.push("-D".into());
                    }
                    if f.force {
                        args.push("--force".into());
                    }
                    if f.unlock {
                        args.push("--unlock".into());
                    }
                    args.push(if f.worktree {
                        f.path.to_string_lossy().to_string()
                    } else {
                        f.name.clone()
                    });
                    self.exit = Some(Exit::CdThen(root, args));
                    return (None, vec![]);
                }
                if f.worktree {
                    let task = Task::RemoveWorktree {
                        path: f.path.clone(),
                        name: f.name.clone(),
                        delete_branch: f.delete_branch,
                        force: f.force && f.forceable,
                        unlock: f.unlock && f.forceable,
                    };
                    return (None, self.spawn(task, After::Nothing));
                }
                let task = Task::Remove {
                    name: f.name.clone(),
                    opts: RemoveOptions {
                        force: f.force,
                        delete_branch: f.delete_branch,
                        branch_merged: false,
                        unlock: false,
                        ..Default::default()
                    },
                };
                return (None, self.spawn(task, After::Nothing));
            }
            _ => {}
        }
        (Some(Modal::Remove(f)), vec![])
    }

    fn gc_key(&mut self, mut g: GcView, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        let can_apply = !g.running && !g.applied && g.result.as_ref().is_some_and(cleanup_has_work);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('g') => (None, vec![]),
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Tab
            | KeyCode::BackTab
                if can_apply =>
            {
                g.focus_close = !g.focus_close;
                (Some(Modal::Gc(g)), vec![])
            }
            KeyCode::Enter | KeyCode::Char(' ') if !can_apply || g.focus_close => (None, vec![]),
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('a') if can_apply => {
                g.running = true;
                self.modal = Some(Modal::Gc(g));
                let e = self.spawn(Task::Gc { apply: true }, After::Nothing);
                (None, e)
            }
            _ => (Some(Modal::Gc(g)), vec![]),
        }
    }

    /// The settings rows Enter can act on, top to bottom.
    pub fn settings_actions(&self) -> Vec<SettingsAction> {
        let mut v = vec![
            SettingsAction::Edit(0),
            SettingsAction::Edit(1),
            SettingsAction::Edit(2),
        ];
        v.extend((0..FIELDS.len()).map(SettingsAction::Field));
        v.push(SettingsAction::Service);
        if self.agents.is_some() {
            v.push(SettingsAction::Agents);
        }
        // Nothing to do once this shell has it.
        if !self.integrated {
            v.push(SettingsAction::Shell);
        }
        v
    }

    /// A setting's values per layer, from the last load.
    pub fn field_state(&self, i: usize) -> Option<&FieldState> {
        let key = FIELDS.get(i)?.key;
        self.current()?.fields.iter().find(|s| s.field.key == key)
    }

    /// Writes `value` (None: removes) for setting `i` to the view's target file.
    fn write_setting(&mut self, v: &SettingsView, i: usize, value: Option<String>) -> Vec<Effect> {
        let Some(d) = self.current() else {
            return vec![];
        };
        let f = &FIELDS[i];
        let path = v.target.path(&d.root, &d.status.name);
        if value.is_some()
            && let Some(src) = self.field_state(i).and_then(|s| s.source)
            && src.index() > v.target.index()
        {
            self.toast(
                ToastKind::Warn,
                format!(
                    "{} sets {}, which wins over {}",
                    src.label(),
                    f.key,
                    v.target.label()
                ),
            );
        }
        vec![
            Effect::SetConfig(ConfigWrite {
                path,
                layer: v.target,
                key: f.key,
                value,
            }),
            Effect::Load,
        ]
    }

    /// Enter on a setting: flips a toggle, or opens the value editor on what
    /// the target file holds (else the effective value).
    fn settings_field(&mut self, mut v: SettingsView, i: usize) -> (Option<Modal>, Vec<Effect>) {
        let Some(st) = self.field_state(i).cloned() else {
            return (Some(Modal::Settings(v)), vec![]);
        };
        if st.field.kind == Kind::Bool {
            let on = st.effective == "true";
            let fx = self.write_setting(&v, i, Some((!on).to_string()));
            return (Some(Modal::Settings(v)), fx);
        }
        let start = match st.get(v.target) {
            Some(x) => x.to_string(),
            None if st.field.kind == Kind::List && st.source.is_none() => String::new(),
            None => st.effective.clone(),
        };
        v.editing = Some(FieldEdit {
            field: i,
            input: Input::new(&start),
        });
        (Some(Modal::Settings(v)), vec![])
    }

    fn settings_edit_key(&mut self, mut v: SettingsView, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let Some(mut e) = v.editing.take() else {
            return (Some(Modal::Settings(v)), vec![]);
        };
        let f = &FIELDS[e.field];
        let choices = match f.kind {
            Kind::Choice { options } => Some(options),
            _ => None,
        };
        match (k.code, choices) {
            (KeyCode::Esc, _) => return (Some(Modal::Settings(v)), vec![]),
            (KeyCode::Enter, _) => {
                if let Err(err) = settings::parse_value(f, &e.input.text) {
                    self.toast(ToastKind::Err, format!("{err:#}"));
                    v.editing = Some(e);
                    return (Some(Modal::Settings(v)), vec![]);
                }
                let fx = self.write_setting(&v, e.field, Some(e.input.text.clone()));
                return (Some(Modal::Settings(v)), fx);
            }
            // A choice cycles through its options; nothing to type.
            (
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down,
                Some(opts),
            ) => {
                let n = opts.len();
                let at = opts.iter().position(|o| *o == e.input.text).unwrap_or(0);
                let back = matches!(k.code, KeyCode::Left | KeyCode::Up);
                e.input
                    .set(opts[if back { (at + n - 1) % n } else { (at + 1) % n }]);
            }
            (_, Some(_)) => {}
            (KeyCode::Left, None) => e.input.left(),
            (KeyCode::Right, None) => e.input.right(),
            (KeyCode::Home, None) => e.input.home(),
            (KeyCode::End, None) => e.input.end(),
            (KeyCode::Backspace, None) => e.input.backspace(),
            (KeyCode::Delete, None) => e.input.delete(),
            (KeyCode::Char('a'), None) if ctrl => e.input.home(),
            (KeyCode::Char('e'), None) if ctrl => e.input.end(),
            (KeyCode::Char('u'), None) if ctrl => e.input.set(""),
            (KeyCode::Char('w'), None) if ctrl => e.input.delete_word(),
            (KeyCode::Char(c), None) if !ctrl => e.input.insert(c),
            _ => {}
        }
        v.editing = Some(e);
        (Some(Modal::Settings(v)), vec![])
    }

    fn settings_act(&mut self, v: SettingsView, a: SettingsAction) -> (Option<Modal>, Vec<Effect>) {
        match a {
            SettingsAction::Field(i) => self.settings_field(v, i),
            SettingsAction::Edit(i) => {
                let fx = self
                    .current()
                    .map(|d| clove_core::config::layer_paths(&d.root, &d.status.name))
                    .map(|p| vec![Effect::EditConfig(p[i].1.clone())])
                    .unwrap_or_default();
                (Some(Modal::Settings(v)), fx)
            }
            SettingsAction::Shell => {
                self.modal = Some(Modal::Settings(v));
                (None, self.spawn(Task::ShellInstall, After::Nothing))
            }
            SettingsAction::Agents => {
                if self.running().any(|j| matches!(j.task, Task::Agents { .. })) {
                    return (Some(Modal::Settings(v)), vec![]);
                }
                // Installed: take out. Missing or outdated: (re)install.
                let install = self.agents != Some(clove_core::agents::State::Installed);
                self.modal = Some(Modal::Settings(v));
                (None, self.spawn(Task::Agents { install }, After::Nothing))
            }
            SettingsAction::Service => {
                if self.running().any(|j| matches!(j.task, Task::Service { .. })) {
                    return (Some(Modal::Settings(v)), vec![]);
                }
                let install = !self.service.unwrap_or(false);
                self.modal = Some(Modal::Settings(v));
                (None, self.spawn(Task::Service { install }, After::Nothing))
            }
        }
    }

    fn settings_key(&mut self, mut v: SettingsView, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        if v.editing.is_some() {
            return self.settings_edit_key(v, k);
        }
        let page = self.settings_height.max(1);
        let actions = self.settings_actions();
        let n = actions.len();
        let on = actions[v.cursor.min(n - 1)];
        match k.code {
            KeyCode::Left | KeyCode::Right => {
                let i = v.target.index() + if k.code == KeyCode::Left { 2 } else { 1 };
                v.target = Layer::ALL[i % 3];
            }
            KeyCode::Char('x') | KeyCode::Delete | KeyCode::Backspace => {
                if let SettingsAction::Field(i) = on {
                    let fx = if self.field_state(i).is_some_and(|s| s.get(v.target).is_none()) {
                        self.toast(
                            ToastKind::Info,
                            format!("{} isn't set in {}", FIELDS[i].key, v.target.label()),
                        );
                        vec![]
                    } else {
                        self.write_setting(&v, i, None)
                    };
                    return (Some(Modal::Settings(v)), fx);
                }
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('s') | KeyCode::Char(',') => {
                return (None, vec![]);
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                v.cursor = (v.cursor + n - 1) % n;
                v.follow = true;
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                v.cursor = (v.cursor + 1) % n;
                v.follow = true;
            }
            KeyCode::PageUp => {
                v.follow = false;
                v.scroll.set(v.scroll.target().saturating_sub(page));
            }
            KeyCode::PageDown => {
                v.follow = false;
                v.scroll.set(v.scroll.target() + page);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let a = actions[v.cursor.min(n - 1)];
                return self.settings_act(v, a);
            }
            // Earlier single-letter shortcuts still work, unadvertised.
            KeyCode::Char('E') => return self.settings_act(v, SettingsAction::Edit(0)),
            KeyCode::Char('t') => return self.settings_act(v, SettingsAction::Edit(1)),
            KeyCode::Char('e') => return self.settings_act(v, SettingsAction::Edit(2)),
            KeyCode::Char('i') => return self.settings_act(v, SettingsAction::Shell),
            KeyCode::Char('S') => return self.settings_act(v, SettingsAction::Service),
            KeyCode::Char('A') if self.agents.is_some() => {
                return self.settings_act(v, SettingsAction::Agents);
            }
            _ => {}
        }
        (Some(Modal::Settings(v)), vec![])
    }

    fn resolve_offer_key(&mut self, mut o: ResolveOffer, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        match k.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => (None, vec![]),
            KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                o.focus_no = !o.focus_no;
                (Some(Modal::ResolveOffer(o)), vec![])
            }
            KeyCode::Enter | KeyCode::Char(' ') if o.focus_no => (None, vec![]),
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('y') | KeyCode::Char('R') => (
                None,
                vec![Effect::Resolve {
                    root: o.root,
                    name: o.name,
                }],
            ),
            _ => (Some(Modal::ResolveOffer(o)), vec![]),
        }
    }

    fn removals_key(&mut self, mut v: RemovalsView, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('L') | KeyCode::Enter => return (None, vec![]),
            KeyCode::Up | KeyCode::Char('k') => v.scroll.set(v.scroll.target().saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => v.scroll.set(v.scroll.target() + 1),
            KeyCode::PageUp => v.scroll.set(v.scroll.target().saturating_sub(10)),
            KeyCode::PageDown | KeyCode::Char(' ') => v.scroll.set(v.scroll.target() + 10),
            _ => {}
        }
        (Some(Modal::Removals(v)), vec![])
    }

    fn stats_key(&mut self, mut v: RemovalsView, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('t') | KeyCode::Enter => return (None, vec![]),
            KeyCode::Up | KeyCode::Char('k') => v.scroll.set(v.scroll.target().saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => v.scroll.set(v.scroll.target() + 1),
            KeyCode::PageUp => v.scroll.set(v.scroll.target().saturating_sub(10)),
            KeyCode::PageDown | KeyCode::Char(' ') => v.scroll.set(v.scroll.target() + 10),
            _ => {}
        }
        (Some(Modal::Stats(v)), vec![])
    }

    fn doctor_key(&mut self, mut d: DoctorView, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('D') | KeyCode::Enter => {
                return (None, vec![]);
            }
            KeyCode::Up | KeyCode::Char('k') => d.scroll.set(d.scroll.target().saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => d.scroll.set(d.scroll.target() + 1),
            _ => {}
        }
        (Some(Modal::Doctor(d)), vec![])
    }

    fn job_key(&mut self, id: u64, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        let page = self.log_height.max(1);
        let Some(job) = self.job_mut(id) else {
            return (None, vec![]);
        };
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('l') => return (None, vec![]),
            KeyCode::Enter => {
                if let (Some(Ok(_)), Some(p)) = (&job.result, &job.path) {
                    self.exit = Some(Exit::Cd(p.clone()));
                }
                return (None, vec![]);
            }
            KeyCode::Up | KeyCode::Char('k') => scroll_job(job, -1, page),
            KeyCode::Down | KeyCode::Char('j') => scroll_job(job, 1, page),
            KeyCode::PageUp => scroll_job(job, -(page as isize), page),
            KeyCode::PageDown => scroll_job(job, page as isize, page),
            KeyCode::Home => {
                job.follow = false;
                job.scroll.set(0);
            }
            KeyCode::End | KeyCode::Char('G') => job.follow = true,
            KeyCode::Char(c @ ('<' | '>' | '=')) => {
                let s = &mut self.panes.job;
                let changed = match c {
                    '=' => {
                        s.reset(panes::JOB_STEPS);
                        true
                    }
                    _ => s.move_divider(panes::JOB_STEPS, if c == '<' { -2 } else { 2 }),
                };
                let fx = if changed { vec![Effect::SaveLayout] } else { vec![] };
                return (Some(Modal::Job(id)), fx);
            }
            _ => {}
        }
        (Some(Modal::Job(id)), vec![])
    }

    fn on_mouse(&mut self, m: MouseEvent) -> Vec<Effect> {
        let pos = Position::new(m.column, m.row);
        if !matches!(m.kind, MouseEventKind::Moved | MouseEventKind::Drag(_)) {
            self.tip_hidden = true;
        }
        let hit = self
            .hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(pos))
            .map(|(_, h)| *h);
        match m.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let d: isize = if m.kind == MouseEventKind::ScrollUp { -1 } else { 1 };
                // The wheel goes to the innermost scrollable region under it.
                let target = self
                    .hits
                    .iter()
                    .rev()
                    .find(|(r, h)| {
                        r.contains(pos)
                            && matches!(
                                h,
                                Hit::ScrollList
                                    | Hit::ScrollLog
                                    | Hit::ScrollDoctor
                                    | Hit::ScrollRemovals
                                    | Hit::ScrollSettings
                                    | Hit::ScrollDiff
                                    | Hit::ScrollDiffList
                            )
                    })
                    .map(|(_, h)| *h);
                match target {
                    Some(Hit::ScrollList) if self.modal.is_none() => self.move_sel(d),
                    Some(Hit::ScrollLog) => {
                        let page = self.log_height.max(1);
                        if let Some(Modal::Job(id)) = self.modal
                            && let Some(j) = self.job_mut(id)
                        {
                            scroll_job(j, d * 3, page);
                        }
                    }
                    Some(Hit::ScrollSettings) => {
                        if let Some(Modal::Settings(v)) = &mut self.modal {
                            v.follow = false;
                            let t = v.scroll.target() as isize + d * 2;
                            v.scroll.set(t.max(0) as usize);
                        }
                    }
                    Some(h @ (Hit::ScrollDiff | Hit::ScrollDiffList)) => {
                        if let Some(Modal::Diff(v)) = &mut self.modal {
                            v.wheel(h == Hit::ScrollDiffList, d);
                        }
                    }
                    Some(Hit::ScrollRemovals) => {
                        if let Some(Modal::Removals(v) | Modal::Stats(v)) = &mut self.modal {
                            let t = v.scroll.target() as isize + d * 2;
                            v.scroll.set(t.max(0) as usize);
                        }
                    }
                    Some(Hit::ScrollDoctor) => {
                        if let Some(Modal::Doctor(dv)) = &mut self.modal {
                            let t = dv.scroll.target() as isize + d * 2;
                            dv.scroll.set(t.max(0) as usize);
                        }
                    }
                    _ => {}
                }
                vec![]
            }
            MouseEventKind::Down(MouseButton::Left) => match hit {
                Some(Hit::Key(code, mods)) => self.on_key(KeyEvent::new(code, mods)),
                Some(Hit::Dismiss) => self.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
                Some(Hit::Repo(i)) if self.modal.is_none() => self.switch_repo(i),
                Some(Hit::Row(i)) if self.modal.is_none() => {
                    let double = self
                        .last_click
                        .is_some_and(|(t, r)| r == i && self.now_ms.saturating_sub(t) < DOUBLE_CLICK_MS);
                    self.selected = i;
                    self.last_click = Some((self.now_ms, i));
                    if double {
                        self.last_click = None;
                        return self.cd_selected();
                    }
                    vec![]
                }
                Some(Hit::DiffEntry(i)) => {
                    let prefs = self.diff_prefs;
                    if let Some(Modal::Diff(v)) = &mut self.modal {
                        v.click_entry(i, prefs);
                    }
                    vec![]
                }
                Some(Hit::Suggestion(i)) => {
                    let sugg = match &self.modal {
                        Some(Modal::New(f)) => self.suggestions(f),
                        _ => vec![],
                    };
                    if let Some(Modal::New(f)) = &mut self.modal
                        && let Some(s) = sugg.get(i)
                    {
                        f.field_mut().set(s);
                        f.pick = None;
                    }
                    vec![]
                }
                Some(Hit::Field(i)) => {
                    if let Some(Modal::New(f)) = &mut self.modal {
                        f.focus = i;
                        f.pick = None;
                    }
                    vec![]
                }
                Some(Hit::SettingsRow(a)) => {
                    let actions = self.settings_actions();
                    if let Some(Modal::Settings(v)) = &mut self.modal
                        && let Some(i) = actions.iter().position(|x| *x == a)
                    {
                        v.cursor = i;
                    }
                    vec![]
                }
                Some(Hit::SettingsLayer(l)) => {
                    if let Some(Modal::Settings(v)) = &mut self.modal {
                        v.target = l;
                    }
                    vec![]
                }
                Some(Hit::RemoveRow(item)) => {
                    if let Some(Modal::Remove(f)) = &mut self.modal {
                        f.focus_on(item);
                        f.toggle(item);
                    }
                    vec![]
                }
                Some(Hit::Candidate(i)) => self.add_repo_click(i),
                Some(Hit::Toast(i)) => {
                    if i < self.toasts.len() {
                        self.toasts.remove(i);
                    }
                    vec![]
                }
                Some(Hit::Divider(id, pane)) => {
                    match self.panes.get_mut(id).press(pane, pos, self.now_ms) {
                        crate::split::Press::Reset => return vec![Effect::SaveLayout],
                        crate::split::Press::Drag => self.dragging = Some(id),
                    }
                    vec![]
                }
                _ => vec![],
            },
            MouseEventKind::Drag(MouseButton::Left) if self.dragging.is_some() => {
                self.hover = Some(pos);
                if let Some(id) = self.dragging {
                    self.panes.get_mut(id).drag_to(pos);
                }
                vec![]
            }
            MouseEventKind::Up(MouseButton::Left) if self.dragging.is_some() => {
                self.hover = Some(pos);
                match self.dragging.take() {
                    Some(id) if self.panes.get_mut(id).release() => vec![Effect::SaveLayout],
                    _ => vec![],
                }
            }
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                // Moving within what a showing or pending tooltip is about keeps it.
                let same_tip = !self.tip_hidden
                    && self
                        .hover
                        .and_then(|h| self.tips.iter().rev().find(|(r, _)| r.contains(h)))
                        .is_some_and(|(r, _)| r.contains(pos));
                if self.hover != Some(pos) && !same_tip {
                    self.hover_ms = self.now_ms;
                    self.tip_hidden = false;
                }
                self.hover = Some(pos);
                vec![]
            }
            _ => vec![],
        }
    }
}

fn job_title(t: &Task) -> String {
    match t {
        Task::Init => "init".into(),
        Task::Refresh => "refresh".into(),
        Task::New(_) => "new".into(),
        Task::Remove { name, .. } => format!("remove {name}"),
        Task::RemoveWorktree { name, .. } => format!("remove worktree {name}"),
        Task::Adopt { name, .. } => format!("adopt {name}"),
        Task::Mount { name } => format!("mount {name}"),
        Task::Unmount { name } => format!("unmount {name}"),
        Task::Gc { .. } => "gc".into(),
        Task::Doctor => "doctor".into(),
        Task::Service { .. } => "service".into(),
        Task::Agents { .. } => "agents".into(),
        Task::Update { name, .. } => format!("update {name}"),
        Task::ShellInstall => "shell integration".into(),
    }
}

/// Something cleanup can remove: a leftover, or a provably merged clone or worktree.
pub fn cleanup_has_work(c: &Cleanup) -> bool {
    !c.leftovers.is_empty()
        || c.checks.iter().any(|m| m.verdict == clove_core::Verdict::Prune)
        || c.worktrees
            .iter()
            .any(|m| m.verdict == clove_core::Verdict::Prune)
}

fn scroll_job(job: &mut Job, d: isize, page: usize) {
    let max = job.log.len().saturating_sub(page);
    let t = (job.scroll.target() as isize + d).clamp(0, max as isize) as usize;
    job.scroll.set(t);
    job.follow = t >= max;
}
