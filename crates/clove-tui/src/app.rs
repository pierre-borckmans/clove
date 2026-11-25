//! App state and every input/message transition. Pure: side effects come back as
//! [`Effect`]s for the runtime to perform, so tests drive this directly.

use crate::fmt;
use crate::model::*;
use clove_core::repo::sanitize_name;
use clove_core::{CloneInfo, Event, NewOptions, RemoveOptions};
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
    /// Run job `id` (its task lives on the job) on a background thread.
    Spawn {
        id: u64,
        root: Option<PathBuf>,
    },
    OpenEditor(PathBuf),
    /// Open a config file in $EDITOR, creating it from a template if missing,
    /// then reload.
    EditConfig(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Exit {
    Quit,
    Cd(PathBuf),
    /// cd to the path, then run `clove <args>` there (removing the clone the
    /// shell was standing in, which keeps its volume busy).
    CdThen(PathBuf, Vec<String>),
}

pub const RELOAD_MS: u64 = 3000;
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
    pub selected: usize,
    pub list_scroll: Smooth,
    /// Rows the clone list showed last frame (for page up/down).
    pub list_height: usize,
    pub filter: String,
    pub filtering: bool,
    pub modal: Option<Modal>,
    pub jobs: Vec<Job>,
    next_id: u64,
    pub toasts: Vec<Toast>,
    pub refs: Vec<GitRef>,
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
    /// Where `clove` was started: the shell's directory.
    pub start_dir: PathBuf,
    /// The shell integration is there to cd on exit.
    pub integrated: bool,
    /// Launchd agents installed; None until the first load.
    pub service: Option<bool>,
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
            selected: 0,
            list_scroll: Smooth::default(),
            list_height: 10,
            filter: String::new(),
            filtering: false,
            modal: None,
            jobs: vec![],
            next_id: 1,
            toasts: vec![],
            refs: vec![],
            refs_root: None,
            now_ms: 0,
            now_unix: 0,
            hits: vec![],
            exit: None,
            quitting: false,
            pending_select: None,
            last_click: None,
            log_height: 10,
            start_dir: PathBuf::new(),
            integrated: false,
            service: None,
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

    pub fn needs_reload(&self) -> bool {
        !self.loading
            && self.repo().is_some()
            && self
                .loaded_ms
                .is_none_or(|t| self.now_ms.saturating_sub(t) >= RELOAD_MS)
    }

    // ---- branch completion ------------------------------------------------

    /// Candidates for the focused field of the new-clone form, best first.
    pub fn suggestions(&self, form: &NewForm) -> Vec<String> {
        let q = form.field().text.to_lowercase();
        let mut seen = std::collections::HashSet::new();
        let pool: Vec<&str> = if form.focus == 0 {
            self.refs
                .iter()
                .filter(|r| r.kind != RefKind::Tag)
                .map(|r| match r.kind {
                    RefKind::Remote => r.name.split_once('/').map(|x| x.1).unwrap_or(&r.name),
                    _ => r.name.as_str(),
                })
                .filter(|n| seen.insert(n.to_string()))
                .collect()
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
            return (format!("a generated name, new branch from {base}"), None);
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
    pub fn animate(&mut self) -> bool {
        let now = self.now_ms;
        self.toasts.retain(|t| now.saturating_sub(t.born_ms) < t.ttl_ms());
        let mut moving = self.list_scroll.tick();
        for j in &mut self.jobs {
            moving |= j.scroll.tick();
        }
        if let Some(Modal::Doctor(d)) = &mut self.modal {
            moving |= d.scroll.tick();
        }
        if let Some(Modal::Settings(v)) = &mut self.modal {
            moving |= v.scroll.tick();
        }
        moving || !self.toasts.is_empty() || self.running().next().is_some() || self.loading
    }

    /// Jumps every animation to its end (tests, and after a resize).
    pub fn settle(&mut self) {
        self.list_scroll.snap();
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
            if matches!(task, Task::Doctor | Task::Service { .. } | Task::ShellInstall) {
                return self.spawn_at(None, String::new(), task, after);
            }
            self.toast(ToastKind::Err, "no repository");
            return vec![];
        };
        self.spawn_at(Some(repo.root), repo.name, task, after)
    }

    fn spawn_at(
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
            Ok(Outcome::Removed) => {
                let s = format!("removed {}", job.target().unwrap_or(""));
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
            Ok(Outcome::Unmounted) => {
                let s = format!("unmounted {}", job.target().unwrap_or(""));
                toasts.push((ToastKind::Ok, s.clone()));
                Ok(s)
            }
            Ok(Outcome::Gc(cleanup)) => {
                let apply = matches!(task, Task::Gc { apply: true });
                if apply {
                    let n = cleanup.removed.len() + cleanup.leftovers.len();
                    toasts.push((
                        ToastKind::Ok,
                        match (cleanup.removed.len(), cleanup.leftovers.len()) {
                            (0, 0) => "nothing to clean".to_string(),
                            (c, 0) => format!("removed {c} merged clone{}", if c == 1 { "" } else { "s" }),
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
            && !matches!(task, Task::Doctor | Task::Service { .. } | Task::ShellInstall)
        {
            effects.push(Effect::Load);
        }
        effects
    }

    pub fn on_msg(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Loaded {
                root,
                data,
                repos,
                service,
            } => {
                self.loading = false;
                self.service = Some(service);
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
            Msg::Refs { root, refs } => {
                self.refs_root = Some(root);
                self.refs = refs;
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
        }
    }

    /// Stores freshly loaded data, keeping the selection on the same clone.
    pub fn apply_data(&mut self, d: RepoData) {
        let is_current = self.repo().is_some_and(|r| r.root == d.root);
        let keep = self.selected_clone().map(|c| c.name.clone());
        if let Some(t) = self.repos.iter_mut().find(|t| t.root == d.root) {
            t.summary = Some(d.summary.clone());
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
        }
        let n = self.visible().len();
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
        match ev {
            TermEvent::Key(k) if k.kind != KeyEventKind::Release => self.on_key(k),
            TermEvent::Mouse(m) => self.on_mouse(m),
            TermEvent::Paste(s) => {
                if let Some(Modal::New(f)) = &mut self.modal {
                    for c in s.chars().filter(|c| !c.is_control()) {
                        f.field_mut().insert(c);
                    }
                    f.pick = None;
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
        let n = self.visible().len();
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
            KeyCode::Char('r') if ctrl => return vec![Effect::Load],
            KeyCode::Char('n') | KeyCode::Char('a') => return self.open_new(),
            KeyCode::Char('d') | KeyCode::Delete => self.open_remove(),
            KeyCode::Char('m') => return self.mount_selected(After::Nothing),
            KeyCode::Char('u') => return self.unmount_selected(),
            KeyCode::Char('r') => return self.refresh(),
            KeyCode::Char('i') => return self.init(),
            KeyCode::Enter => return self.cd_selected(),
            KeyCode::Char('o') => return self.open_selected(),
            KeyCode::Char('/') => {
                self.filtering = true;
            }
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
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
        }));
        if let Some(Modal::Remove(f)) = &mut self.modal {
            f.focus_on(RemoveItem::Remove);
        }
    }

    fn mount_selected(&mut self, after: After) -> Vec<Effect> {
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

    fn unmount_selected(&mut self) -> Vec<Effect> {
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
            Modal::Gc(g) => self.gc_key(g, k),
            Modal::Doctor(d) => self.doctor_key(d, k),
            Modal::Settings(v) => self.settings_key(v, k),
            Modal::Job(id) => self.job_key(id, k),
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
                    args.push(f.name.clone());
                    self.exit = Some(Exit::CdThen(root, args));
                    return (None, vec![]);
                }
                let task = Task::Remove {
                    name: f.name.clone(),
                    opts: RemoveOptions {
                        force: f.force,
                        delete_branch: f.delete_branch,
                        branch_merged: false,
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
            SettingsAction::Service,
        ];
        // Nothing to do once this shell has it.
        if !self.integrated {
            v.push(SettingsAction::Shell);
        }
        v
    }

    fn settings_act(&mut self, v: SettingsView, a: SettingsAction) -> (Option<Modal>, Vec<Effect>) {
        match a {
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
        let page = self.settings_height.max(1);
        let actions = self.settings_actions();
        let n = actions.len();
        match k.code {
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
            _ => {}
        }
        (Some(Modal::Settings(v)), vec![])
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
            _ => {}
        }
        (Some(Modal::Job(id)), vec![])
    }

    fn on_mouse(&mut self, m: MouseEvent) -> Vec<Effect> {
        let pos = Position::new(m.column, m.row);
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
                                Hit::ScrollList | Hit::ScrollLog | Hit::ScrollDoctor | Hit::ScrollSettings
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
                Some(Hit::RemoveRow(item)) => {
                    if let Some(Modal::Remove(f)) = &mut self.modal {
                        f.focus_on(item);
                        f.toggle(item);
                    }
                    vec![]
                }
                Some(Hit::Toast(i)) => {
                    if i < self.toasts.len() {
                        self.toasts.remove(i);
                    }
                    vec![]
                }
                _ => vec![],
            },
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
        Task::Mount { name } => format!("mount {name}"),
        Task::Unmount { name } => format!("unmount {name}"),
        Task::Gc { .. } => "gc".into(),
        Task::Doctor => "doctor".into(),
        Task::Service { .. } => "service".into(),
        Task::ShellInstall => "shell integration".into(),
    }
}

/// Something cleanup can remove: a leftover or a provably merged clone.
pub fn cleanup_has_work(c: &Cleanup) -> bool {
    !c.leftovers.is_empty() || c.checks.iter().any(|m| m.verdict == clove_core::Verdict::Prune)
}

fn scroll_job(job: &mut Job, d: isize, page: usize) {
    let max = job.log.len().saturating_sub(page);
    let t = (job.scroll.target() as isize + d).clamp(0, max as isize) as usize;
    job.scroll.set(t);
    job.follow = t >= max;
}
