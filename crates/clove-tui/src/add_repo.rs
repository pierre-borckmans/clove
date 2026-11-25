//! "Add repo": pick a git repo clove doesn't manage (found on disk, or typed
//! as a path), preview what `init` would do, then run it. With no repo at all
//! it is also the first-run screen.

use crate::app::{App, Effect};
use crate::fmt::{self, pad, trunc, trunc_left, width};
use crate::model::*;
use crate::theme::*;
use crate::ui::{Hint, button, centered, key_hit, modal_block, open_modal, put, row};
use clove_core::{InitPreview, RepoCandidate};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Default)]
pub struct AddRepo {
    /// A filter over the found repos, or a path (starts with `/`, `~` or `.`).
    pub input: Input,
    /// Repos found on disk; None while the scan runs.
    pub found: Option<Vec<RepoCandidate>>,
    /// Highlighted row of [`App::repo_matches`].
    pub pick: usize,
    /// The highlight was moved (arrows, click): Enter takes it over a typed path.
    pub browsing: bool,
    pub preview: Option<Preview>,
    /// Opened at start because clove has no repo to show.
    pub first_run: bool,
}

#[derive(Debug, Clone)]
pub struct Preview {
    pub path: PathBuf,
    /// None while it loads.
    pub result: Option<Result<InitPreview, String>>,
    /// Focus is on cancel (else on init).
    pub cancel: bool,
}

pub fn looks_like_path(s: &str) -> bool {
    s.starts_with('/') || s.starts_with('~') || s.starts_with('.')
}

/// `~` is home; a relative path is taken from `base` (where clove started).
pub fn expand(s: &str, base: &Path) -> PathBuf {
    let home = clove_core::util::home();
    if s == "~" {
        return home;
    }
    if let Some(rest) = s.strip_prefix("~/") {
        return home.join(rest);
    }
    base.join(s)
}

/// Shell-like completion of a directory path: the only match, else the
/// longest common prefix. Hidden dirs only when the prefix starts with `.`.
pub fn complete(text: &str, base: &Path) -> Option<String> {
    if matches!(text, "~" | "." | "..") {
        return Some(format!("{text}/"));
    }
    let i = text.rfind('/')?;
    let (head, prefix) = (&text[..=i], &text[i + 1..]);
    let mut names: Vec<String> = std::fs::read_dir(expand(head, base))
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with(prefix) && (prefix.starts_with('.') || !n.starts_with('.')))
        .collect();
    names.sort();
    match names.as_slice() {
        [] => None,
        [one] => Some(format!("{head}{one}/")),
        many => {
            let first: Vec<char> = many[0].chars().collect();
            let n = many.iter().skip(1).fold(first.len(), |n, s| {
                s.chars().zip(&first).take(n).take_while(|(a, b)| a == *b).count()
            });
            let common: String = first[..n].iter().collect();
            (common.len() > prefix.len()).then(|| format!("{head}{common}"))
        }
    }
}

impl App {
    /// Opens the add-repo dialog and starts looking for repos.
    pub fn open_add_repo(&mut self) -> Vec<Effect> {
        self.modal = Some(Modal::AddRepo(Box::new(AddRepo {
            first_run: self.repos.is_empty(),
            ..Default::default()
        })));
        vec![Effect::FindRepos]
    }

    /// Found repos matching what is typed: name prefix first, then name or
    /// path containing it; a path keeps those under it.
    pub fn repo_matches<'a>(&self, v: &'a AddRepo) -> Vec<&'a RepoCandidate> {
        let Some(found) = &v.found else {
            return vec![];
        };
        let q = v.input.text.trim();
        if q.is_empty() {
            return found.iter().collect();
        }
        if looks_like_path(q) {
            let want = expand(q, &self.start_dir).to_string_lossy().to_string();
            return found
                .iter()
                .filter(|c| c.root.to_string_lossy().starts_with(&want))
                .collect();
        }
        let q = q.to_lowercase();
        let (mut first, mut rest) = (vec![], vec![]);
        for c in found {
            let n = c.name.to_lowercase();
            if n.starts_with(&q) {
                first.push(c);
            } else if n.contains(&q) || fmt::tilde(&c.root).to_lowercase().contains(&q) {
                rest.push(c);
            }
        }
        first.extend(rest);
        first
    }

    /// What Enter previews: the highlighted repo, or the typed path.
    pub fn add_repo_target(&self, v: &AddRepo) -> Option<PathBuf> {
        let q = v.input.text.trim();
        if looks_like_path(q) && !v.browsing {
            return Some(expand(q, &self.start_dir));
        }
        self.repo_matches(v).get(v.pick).map(|c| c.root.clone())
    }

    pub(crate) fn on_repos_found(&mut self, found: Vec<RepoCandidate>) {
        if let Some(Modal::AddRepo(v)) = &mut self.modal {
            v.found = Some(found);
        }
    }

    pub(crate) fn on_init_preview(&mut self, path: PathBuf, result: Result<InitPreview, String>) {
        if let Some(Modal::AddRepo(v)) = &mut self.modal
            && let Some(p) = &mut v.preview
            && p.path == path
        {
            p.result = Some(result);
        }
    }

    pub(crate) fn add_repo_paste(&mut self, s: &str) {
        if let Some(Modal::AddRepo(v)) = &mut self.modal
            && v.preview.is_none()
        {
            for c in s.chars().filter(|c| !c.is_control()) {
                v.input.insert(c);
            }
            v.pick = 0;
            v.browsing = false;
        }
    }

    /// A click on found repo `i`: highlight it; on the highlighted one, preview it.
    pub(crate) fn add_repo_click(&mut self, i: usize) -> Vec<Effect> {
        let Some(Modal::AddRepo(mut v)) = self.modal.take() else {
            return vec![];
        };
        if v.preview.is_some() {
            self.modal = Some(Modal::AddRepo(v));
            return vec![];
        }
        let again = v.browsing && v.pick == i;
        v.pick = i;
        v.browsing = true;
        let (keep, fx) = if again {
            self.add_repo_choose(v)
        } else {
            (Some(Modal::AddRepo(v)), vec![])
        };
        if self.modal.is_none() {
            self.modal = keep;
        }
        fx
    }

    pub(crate) fn add_repo_key(&mut self, mut v: Box<AddRepo>, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        if v.preview.is_some() {
            return self.add_repo_preview_key(v, k);
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let n = self.repo_matches(&v).len();
        let before = v.input.text.clone();
        match k.code {
            KeyCode::Esc => return (None, vec![]),
            KeyCode::Enter => return self.add_repo_choose(v),
            KeyCode::Char(' ') if v.input.text.is_empty() => return self.add_repo_choose(v),
            KeyCode::Up | KeyCode::Down if n > 0 => {
                v.pick = if k.code == KeyCode::Up {
                    (v.pick + n - 1) % n
                } else {
                    (v.pick + 1) % n
                };
                v.browsing = true;
            }
            KeyCode::Tab => {
                let t = v.input.text.clone();
                if looks_like_path(&t) && !v.browsing {
                    if let Some(c) = complete(&t, &self.start_dir) {
                        v.input.set(&c);
                    }
                } else if let Some(c) = self.repo_matches(&v).get(v.pick) {
                    let s = fmt::tilde(&c.root);
                    v.input.set(&s);
                }
            }
            KeyCode::Left => v.input.left(),
            KeyCode::Right => v.input.right(),
            KeyCode::Home => v.input.home(),
            KeyCode::End => v.input.end(),
            KeyCode::Backspace => v.input.backspace(),
            KeyCode::Delete => v.input.delete(),
            KeyCode::Char('a') if ctrl => v.input.home(),
            KeyCode::Char('e') if ctrl => v.input.end(),
            KeyCode::Char('u') if ctrl => v.input.set(""),
            KeyCode::Char('w') if ctrl => v.input.delete_word(),
            KeyCode::Char(c) if !ctrl => v.input.insert(c),
            _ => {}
        }
        if v.input.text != before {
            v.pick = 0;
            v.browsing = false;
        }
        (Some(Modal::AddRepo(v)), vec![])
    }

    fn add_repo_choose(&mut self, mut v: Box<AddRepo>) -> (Option<Modal>, Vec<Effect>) {
        let Some(path) = self.add_repo_target(&v) else {
            let msg = if v.found.is_none() {
                "still looking for repos; or type a path"
            } else {
                "type a path, or pick a repo from the list"
            };
            self.toast(ToastKind::Info, msg);
            return (Some(Modal::AddRepo(v)), vec![]);
        };
        v.preview = Some(Preview {
            path: path.clone(),
            result: None,
            cancel: false,
        });
        (Some(Modal::AddRepo(v)), vec![Effect::PreviewInit(path)])
    }

    fn add_repo_preview_key(&mut self, mut v: Box<AddRepo>, k: KeyEvent) -> (Option<Modal>, Vec<Effect>) {
        let Some(p) = &mut v.preview else {
            return (Some(Modal::AddRepo(v)), vec![]);
        };
        let ready = matches!(p.result, Some(Ok(_)));
        let confirm = match k.code {
            KeyCode::Esc | KeyCode::Backspace => {
                v.preview = None;
                return (Some(Modal::AddRepo(v)), vec![]);
            }
            KeyCode::Char('n') | KeyCode::Char('q') => return (None, vec![]),
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Tab
            | KeyCode::BackTab
                if ready =>
            {
                p.cancel = !p.cancel;
                false
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if ready && p.cancel {
                    return (None, vec![]);
                }
                true
            }
            KeyCode::Char('y') => true,
            _ => false,
        };
        if !confirm {
            return (Some(Modal::AddRepo(v)), vec![]);
        }
        match p.result.clone() {
            None => (Some(Modal::AddRepo(v)), vec![]),
            Some(Err(_)) => {
                v.preview = None;
                (Some(Modal::AddRepo(v)), vec![])
            }
            Some(Ok(pr)) if pr.initialized => (None, self.show_repo(pr.root, pr.name).0),
            Some(Ok(pr)) => (None, self.init_repo(pr.root, pr.name)),
        }
    }

    /// Selects the repo at `root`, adding a tab for it when clove doesn't list
    /// it yet. Returns the effects and its index.
    fn show_repo(&mut self, root: PathBuf, name: String) -> (Vec<Effect>, usize) {
        let idx = match self.repos.iter().position(|r| r.root == root) {
            Some(i) => i,
            None => {
                self.repos.push(RepoTab {
                    name,
                    root,
                    summary: None,
                });
                self.repos.len() - 1
            }
        };
        let mut fx = self.switch_repo(idx);
        if fx.is_empty() && !self.loading {
            // Already the current index (first run): load it now.
            self.loaded_ms = None;
            fx.push(Effect::Load);
        }
        (fx, idx)
    }

    /// Switches to the repo and builds its golden image, with progress.
    fn init_repo(&mut self, root: PathBuf, name: String) -> Vec<Effect> {
        let (mut fx, _) = self.show_repo(root.clone(), name.clone());
        if let Some(j) = self.golden_job() {
            self.modal = Some(Modal::Job(j.id));
            return fx;
        }
        fx.extend(self.spawn_at(Some(root), name, Task::Init, After::Nothing));
        fx
    }
}

// ---- background work ---------------------------------------------------

pub fn find(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let found = crate::worker::guarded(|| Ok(clove_core::candidates::find())).unwrap_or_default();
        let _ = tx.send(Msg::ReposFound(found));
    });
}

pub fn preview(tx: Sender<Msg>, path: PathBuf) {
    std::thread::spawn(move || {
        let result =
            crate::worker::guarded(|| clove_core::preview::init_preview(&path).map_err(|e| format!("{e:#}")));
        let _ = tx.send(Msg::InitPreview { path, result });
    });
}

// ---- drawing -----------------------------------------------------------

pub fn hints(v: &AddRepo) -> Vec<Hint> {
    use KeyCode::*;
    match &v.preview {
        None => vec![
            ("⏎", "preview", Enter),
            ("↑↓", "pick", Down),
            ("tab", "complete", Tab),
            ("esc", "close", Esc),
        ],
        Some(p) => match &p.result {
            Some(Ok(pr)) => vec![
                (
                    "⏎",
                    match (p.cancel, pr.initialized) {
                        (true, _) => "cancel",
                        (false, true) => "open",
                        (false, false) => "init",
                    },
                    Enter,
                ),
                ("←→", "move", Right),
                ("esc", "back", Esc),
            ],
            _ => vec![("esc", "back", Esc)],
        },
    }
}

const W: u16 = 78;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect, v: &AddRepo) {
    match &v.preview {
        None => draw_pick(f, app, area, v),
        Some(p) => draw_preview(f, app, area, p),
    }
}

fn draw_pick(f: &mut Frame, app: &mut App, area: Rect, v: &AddRepo) {
    let h = area.height.saturating_sub(2).min(24);
    let rect = centered(area, W, h);
    open_modal(f, &mut app.hits, area, rect);
    let t = if v.first_run {
        "welcome to clove · add a repo"
    } else {
        "add a repo"
    };
    let block = modal_block(vec![Span::styled(t, title())]);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 5 || inner.width < 20 {
        return;
    }
    let iw = inner.width as usize;
    let mut y = 0;
    if v.first_run {
        put(
            f,
            row(inner, 0),
            Line::from(Span::styled(
                trunc(
                    "Pick a git repo for clove to manage. Its golden image builds once.",
                    iw,
                ),
                muted(),
            )),
        );
        y = 2;
    }

    // The path / filter field.
    let label = "path ❯ ";
    let lw = width(label) as u16;
    let fr = row(inner, y);
    let tw = iw.saturating_sub(lw as usize + 1);
    let shown = if v.input.text.is_empty() {
        Span::styled(trunc("type to filter, or a path like ~/code/app", tw), muted())
    } else {
        Span::styled(trunc_left(&v.input.text, tw), bold())
    };
    put(f, fr, Line::from(vec![Span::styled(label, key()), shown]));
    let before: String = v.input.text.chars().take(v.input.cursor).collect();
    f.set_cursor_position(Position::new(fr.x + lw + width(&before).min(tw) as u16, fr.y));
    y += 2;

    let matches: Vec<RepoCandidate> = app.repo_matches(v).into_iter().cloned().collect();
    let path_mode = looks_like_path(v.input.text.trim()) && !v.browsing;
    let head = match &v.found {
        None => Line::from(Span::styled(
            format!(
                "{} looking for git repos in your code folders",
                spinner(app.now_ms)
            ),
            muted(),
        )),
        Some(all) => Line::from(vec![
            Span::styled("NOT MANAGED YET", muted().add_modifier(Modifier::BOLD)),
            Span::styled(
                if matches.len() == all.len() {
                    format!("  {}", all.len())
                } else {
                    format!("  {}/{}", matches.len(), all.len())
                },
                muted(),
            ),
        ]),
    };
    put(f, row(inner, y), head);
    y += 1;

    // Leave the last two rows for what Enter does.
    let rows = inner.height.saturating_sub(y + 2) as usize;
    if v.found.as_ref().is_some_and(|a| a.is_empty()) && rows > 0 {
        put(
            f,
            row(inner, y),
            Line::from(Span::styled(
                "none found in ~/Documents/Dev, ~/dev, ~/code, ~/src, ~/projects, ~/Developer",
                muted(),
            )),
        );
    }
    let off = v.pick.saturating_sub(rows.saturating_sub(1));
    let (nw, bw, aw) = (22, 18, 4);
    let now = app.now_unix;
    for (i, c) in matches.iter().enumerate().skip(off).take(rows) {
        let rr = row(inner, y + (i - off) as u16);
        let on = i == v.pick && !path_mode;
        let pw = iw.saturating_sub(nw + bw + aw + 8);
        let line = Line::from(vec![
            Span::styled(
                if on { "› " } else { "  " },
                fg(BOTTLE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(pad(&trunc(&c.name, nw), nw), bold()),
            Span::raw("  "),
            Span::styled(
                pad(&trunc(c.branch.as_deref().unwrap_or("(detached)"), bw), bw),
                fg(PLUM),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{:>aw$}", fmt::age(now.saturating_sub(c.active))),
                muted(),
            ),
            Span::raw("  "),
            Span::styled(trunc_left(&fmt::tilde(&c.root), pw), muted()),
        ]);
        put(f, rr, line);
        if on {
            f.buffer_mut().set_style(rr, selected());
        }
        app.hits.push((rr, Hit::Candidate(i)));
    }

    let what = match app.add_repo_target(v) {
        Some(p) if path_mode => format!("previews {}", fmt::tilde(&p)),
        Some(_) => matches
            .get(v.pick)
            .map(|c| format!("previews {}: what init does, before it does it", c.name))
            .unwrap_or_default(),
        None => "pick a repo, or type its path".into(),
    };
    put(
        f,
        row(inner, inner.height - 1),
        Line::from(vec![
            Span::styled("⏎ ", key()),
            Span::styled(trunc(&what, iw.saturating_sub(2)), muted()),
        ]),
    );
}

fn kv<'a>(label: &str, value: Vec<Span<'a>>) -> Line<'a> {
    let mut s = vec![Span::styled(format!("{label:<8}"), muted())];
    s.extend(value);
    Line::from(s)
}

fn preview_lines<'a>(pr: &InitPreview, iw: usize, max_steps: usize) -> Vec<Line<'a>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(pr.name.clone(), bold()),
        Span::raw("  "),
        Span::styled(
            trunc_left(&fmt::tilde(&pr.root), iw.saturating_sub(width(&pr.name) + 2)),
            muted(),
        ),
    ])];
    lines.push(Line::from(""));
    if pr.initialized {
        lines.push(Line::from(Span::styled(
            "✓ clove already manages this repo",
            fg(SAGE),
        )));
        return lines;
    }
    let mut base = vec![Span::raw(pr.base.clone())];
    if !pr.base_found {
        base.push(Span::styled("  not found yet; init fetches it", fg(AMBER)));
    }
    lines.push(kv("base", base));
    if pr.steps.is_empty() {
        lines.push(kv(
            "steps",
            vec![Span::styled(
                "none detected: clones get the checkout only",
                muted(),
            )],
        ));
    } else {
        let src = if pr.steps_detected {
            "detected"
        } else {
            "from config"
        };
        lines.push(kv("steps", vec![Span::styled(src, muted())]));
        let nw = pr.steps.iter().map(|s| width(&s.name)).max().unwrap_or(0).min(24);
        let shown = if pr.steps.len() > max_steps {
            max_steps.saturating_sub(1)
        } else {
            pr.steps.len()
        };
        for s in pr.steps.iter().take(shown) {
            let cmd = s.warm.as_deref().unwrap_or("(in clones only)");
            let missing = s.tool.as_ref().is_some_and(|t| pr.missing_tools.contains(t));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    pad(&trunc(&s.name, nw), nw),
                    if missing { fg(AMBER) } else { fg(BOTTLE) },
                ),
                Span::raw("  "),
                Span::styled(trunc(cmd, iw.saturating_sub(nw + 4)), muted()),
            ]));
        }
        if shown < pr.steps.len() {
            lines.push(Line::from(Span::styled(
                format!("  +{} more", pr.steps.len() - shown),
                muted(),
            )));
        }
    }
    if !pr.nested_skipped.is_empty() {
        let s = format!(
            "{} projects not warmed: {}",
            pr.nested_skipped.len(),
            pr.nested_skipped.join(", ")
        );
        lines.push(kv("nested", vec![Span::styled(trunc(&s, iw - 8), muted())]));
    }
    if !pr.copy.is_empty() {
        lines.push(kv("copy", vec![Span::raw(trunc(&pr.copy.join(", "), iw - 8))]));
    }
    if pr.exec != clove_core::Exec::None {
        lines.push(kv(
            "env",
            vec![Span::raw(format!("{:?}", pr.exec).to_lowercase())],
        ));
    }
    lines.push(kv(
        "clones",
        vec![Span::raw(trunc_left(
            &fmt::tilde(&pr.clone_root.join("<name>")),
            iw - 8,
        ))],
    ));
    let size = format!("  sparse, up to {}", pr.image_size);
    lines.push(kv(
        "image",
        vec![
            Span::raw(trunc_left(
                &fmt::tilde(&pr.state),
                (iw - 8).saturating_sub(width(&size)),
            )),
            Span::styled(size, muted()),
        ],
    ));
    for t in &pr.missing_tools {
        lines.push(Line::from(Span::styled(
            format!("! {t} isn't on PATH; its step will fail"),
            fg(AMBER),
        )));
    }
    for m in &pr.machine {
        let c = if m.level == clove_core::doctor::Level::Fail {
            RED
        } else {
            AMBER
        };
        lines.push(Line::from(Span::styled(
            trunc(&format!("! {}: {}", m.name, m.detail), iw),
            fg(c),
        )));
    }
    lines
}

fn draw_preview(f: &mut Frame, app: &mut App, area: Rect, p: &Preview) {
    let iw = (W - 6) as usize;
    let max_h = area.height.saturating_sub(2);
    // Fixed rows besides steps: header, blank, ~8 fields, footer and buttons.
    let max_steps = (max_h as usize).saturating_sub(20).max(2);
    let (mut lines, title_name) = match &p.result {
        None => (
            vec![Line::from(Span::styled(
                format!("{} reading {}", spinner(app.now_ms), fmt::tilde(&p.path)),
                muted(),
            ))],
            fmt::tilde(&p.path),
        ),
        Some(Err(e)) => {
            let mut l = vec![
                Line::from(Span::styled("✗ not a repo clove can use", fg(RED))),
                Line::from(""),
            ];
            for s in fmt::wrap(e, iw) {
                l.push(Line::from(Span::styled(s, muted())));
            }
            (l, fmt::tilde(&p.path))
        }
        Some(Ok(pr)) => (preview_lines(pr, iw, max_steps), pr.name.clone()),
    };
    let ready = matches!(&p.result, Some(Ok(_)));
    let fresh = matches!(&p.result, Some(Ok(pr)) if !pr.initialized);
    if fresh {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "slow once (checkout, warm steps); then each clone takes about a second",
            muted(),
        )));
    }
    let h = (lines.len() as u16 + 2 + 4).min(max_h);
    let rect = centered(area, W, h);
    open_modal(f, &mut app.hits, area, rect);
    let block = modal_block(vec![
        Span::styled("add ", title()),
        Span::styled(title_name, bold()),
    ]);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 2 {
        return;
    }
    let body = Rect::new(inner.x, inner.y, inner.width, inner.height - 2);
    f.render_widget(Paragraph::new(lines), body);

    let by = inner.bottom() - 1;
    let buttons: Vec<(Span, Hit)> = if ready {
        let primary = if fresh { "init" } else { "open" };
        vec![
            (button(primary, !p.cancel, false), key_hit('y')),
            (button("cancel", p.cancel, false), key_hit('n')),
        ]
    } else {
        vec![(
            button("back", true, false),
            Hit::Key(KeyCode::Esc, KeyModifiers::NONE),
        )]
    };
    let total: u16 = buttons
        .iter()
        .map(|(b, _)| width(&b.content) as u16 + 2)
        .sum::<u16>()
        - 2;
    let mut x = inner.right().saturating_sub(total);
    for (b, hit) in buttons {
        let bw = width(&b.content) as u16;
        let r = Rect::new(x, by, bw, 1).intersection(inner);
        put(f, r, Line::from(b));
        app.hits.push((r, hit));
        x += bw + 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clove_core::doctor::{Check, Level};
    use clove_core::{Exec, PreviewStep};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{Event as TermEvent, MouseButton, MouseEvent, MouseEventKind};

    const NOW: u64 = 1_800_000_000;

    fn found() -> Vec<RepoCandidate> {
        [
            ("api", "main", 600),
            ("web", "feat/new-nav", 7200),
            ("tools", "main", 3 * 86_400),
        ]
        .into_iter()
        .map(|(n, b, ago)| RepoCandidate {
            root: PathBuf::from(format!("/home/ada/code/{n}")),
            name: n.into(),
            branch: Some(b.into()),
            active: NOW - ago,
        })
        .collect()
    }

    fn preview(initialized: bool) -> InitPreview {
        InitPreview {
            root: "/home/ada/code/web".into(),
            name: "web".into(),
            initialized,
            base: "origin/main".into(),
            base_found: true,
            steps: vec![
                PreviewStep {
                    name: "pnpm".into(),
                    warm: Some("pnpm install --frozen-lockfile".into()),
                    setup: None,
                    tool: Some("pnpm".into()),
                },
                PreviewStep {
                    name: "cargo-src-tauri".into(),
                    warm: Some("cd src-tauri && cargo build --workspace --all-targets".into()),
                    setup: None,
                    tool: Some("cargo".into()),
                },
            ],
            steps_detected: true,
            nested_skipped: vec![],
            copy: vec![".env.local".into()],
            exec: Exec::Direnv,
            image_size: "300G".into(),
            clone_root: "/home/ada/code/web.clove".into(),
            state: "/home/ada/.clove/repos/web-1a2b3c4d".into(),
            missing_tools: vec!["pnpm".into()],
            machine: vec![Check {
                name: "disk images".into(),
                level: Level::Warn,
                detail: "falling back to hdiutil sparse images".into(),
                fix: None,
            }],
        }
    }

    fn app(repos: Vec<RepoTab>) -> App {
        let mut app = App::new(repos, 0);
        app.now_unix = NOW;
        app.now_ms = 10_000;
        app.start_dir = PathBuf::from("/home/ada");
        app
    }

    fn key(app: &mut App, code: KeyCode) -> Vec<Effect> {
        app.on_input(TermEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)))
    }

    fn typed(app: &mut App, s: &str) {
        for c in s.chars() {
            key(app, KeyCode::Char(c));
        }
    }

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| crate::ui::draw(f, app)).unwrap();
        app.settle();
        term.draw(|f| crate::ui::draw(f, app)).unwrap();
        let buf = term.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buf.area.height {
            let mut line = String::new();
            let mut skip = 0;
            for x in 0..buf.area.width {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                let s = buf[(x, y)].symbol();
                skip = width(s).saturating_sub(1);
                line.push_str(s);
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    fn view(app: &App) -> Box<AddRepo> {
        match &app.modal {
            Some(Modal::AddRepo(v)) => v.clone(),
            m => panic!("not the add-repo dialog: {m:?}"),
        }
    }

    fn click_on(app: &mut App, screen: &str, needle: &str) -> Vec<Effect> {
        let (x, y) = screen
            .lines()
            .enumerate()
            .find_map(|(y, l)| l.find(needle).map(|i| (width(&l[..i]) as u16, y as u16)))
            .unwrap_or_else(|| panic!("{needle:?} not on screen:\n{screen}"));
        app.on_input(TermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }))
    }

    fn tab(name: &str) -> RepoTab {
        RepoTab {
            name: name.into(),
            root: PathBuf::from(format!("/work/{name}")),
            summary: None,
        }
    }

    #[test]
    fn first_run_screen_lists_repos_and_filters_as_you_type() {
        let mut app = app(vec![]);
        assert_eq!(app.open_add_repo(), vec![Effect::FindRepos]);
        assert!(view(&app).first_run);
        let s = render(&mut app, 100, 24);
        assert!(s.contains("looking for git repos"), "{s}");
        app.on_msg(Msg::ReposFound(found()));
        let s = render(&mut app, 100, 24);
        insta::assert_snapshot!(s);
        typed(&mut app, "we");
        let v = view(&app);
        let names: Vec<_> = app.repo_matches(&v).iter().map(|c| c.name.clone()).collect();
        assert_eq!(names, ["web"]);
        // A path filters by prefix.
        app.on_input(TermEvent::Key(KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
        )));
        typed(&mut app, "/home/ada/code/t");
        let v = view(&app);
        let names: Vec<_> = app.repo_matches(&v).iter().map(|c| c.name.clone()).collect();
        assert_eq!(names, ["tools"]);
        assert_eq!(
            app.add_repo_target(&v),
            Some(PathBuf::from("/home/ada/code/t")),
            "a typed path wins until the list is browsed"
        );
        key(&mut app, KeyCode::Down);
        assert_eq!(
            app.add_repo_target(&view(&app)),
            Some(PathBuf::from("/home/ada/code/tools"))
        );
    }

    #[test]
    fn pick_preview_then_init_selects_the_new_repo() {
        let mut app = app(vec![tab("app")]);
        key(&mut app, KeyCode::Char('A'));
        assert!(!view(&app).first_run);
        app.on_msg(Msg::ReposFound(found()));
        key(&mut app, KeyCode::Down);
        let fx = key(&mut app, KeyCode::Enter);
        assert_eq!(fx, vec![Effect::PreviewInit("/home/ada/code/web".into())]);
        // A stale answer for another path is ignored.
        app.on_msg(Msg::InitPreview {
            path: "/elsewhere".into(),
            result: Err("nope".into()),
        });
        assert!(view(&app).preview.unwrap().result.is_none());
        app.on_msg(Msg::InitPreview {
            path: "/home/ada/code/web".into(),
            result: Ok(preview(false)),
        });
        let s = render(&mut app, 100, 34);
        insta::assert_snapshot!(s);
        // Esc goes back to the list, Enter previews again.
        key(&mut app, KeyCode::Esc);
        assert!(view(&app).preview.is_none());
        key(&mut app, KeyCode::Enter);
        app.on_msg(Msg::InitPreview {
            path: "/home/ada/code/web".into(),
            result: Ok(preview(false)),
        });
        let fx = key(&mut app, KeyCode::Enter);
        assert_eq!(app.repo().unwrap().name, "web", "the new repo is selected");
        assert_eq!(app.repos.len(), 2);
        assert!(matches!(fx[0], Effect::Load));
        let Effect::Spawn { id, root } = &fx[1] else {
            panic!("{fx:?}")
        };
        assert_eq!(root.as_deref(), Some(Path::new("/home/ada/code/web")));
        assert!(matches!(app.job(*id).unwrap().task, Task::Init));
        assert!(
            matches!(app.modal, Some(Modal::Job(j)) if j == *id),
            "progress shows"
        );
    }

    #[test]
    fn mouse_picks_rows_and_buttons() {
        let mut app = app(vec![tab("app")]);
        key(&mut app, KeyCode::Char('A'));
        app.on_msg(Msg::ReposFound(found()));
        let s = render(&mut app, 100, 24);
        click_on(&mut app, &s, "tools");
        assert_eq!(view(&app).pick, 2);
        let fx = click_on(&mut app, &s, "tools");
        assert_eq!(fx, vec![Effect::PreviewInit("/home/ada/code/tools".into())]);
        app.on_msg(Msg::InitPreview {
            path: "/home/ada/code/tools".into(),
            result: Ok(preview(false)),
        });
        let s = render(&mut app, 100, 34);
        click_on(&mut app, &s, " cancel ");
        assert!(app.modal.is_none(), "cancel closes the dialog");
    }

    #[test]
    fn errors_and_managed_repos() {
        let mut app = app(vec![tab("app")]);
        key(&mut app, KeyCode::Char('A'));
        app.on_msg(Msg::ReposFound(vec![]));
        typed(&mut app, "/tmp/x");
        key(&mut app, KeyCode::Enter);
        app.on_msg(Msg::InitPreview {
            path: "/tmp/x".into(),
            result: Err("not inside a git repository".into()),
        });
        let s = render(&mut app, 100, 24);
        assert!(s.contains("not a repo clove can use"), "{s}");
        key(&mut app, KeyCode::Enter);
        assert!(view(&app).preview.is_none(), "enter on an error goes back");

        // A repo clove already manages: open switches to it, no init.
        app.repos.push(tab("web"));
        app.repos[1].root = "/home/ada/code/web".into();
        key(&mut app, KeyCode::Enter);
        app.on_msg(Msg::InitPreview {
            path: "/tmp/x".into(),
            result: Ok(preview(true)),
        });
        let fx = key(&mut app, KeyCode::Char('y'));
        assert_eq!(fx, vec![Effect::Load]);
        assert_eq!(app.repo().unwrap().name, "web");
        assert!(app.modal.is_none());
    }

    #[test]
    fn completes_directories_like_a_shell() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for p in [
            "code/alpha",
            "code/alpine",
            "code/beta",
            "code/.hidden",
            "solo/only",
        ] {
            std::fs::create_dir_all(r.join(p)).unwrap();
        }
        std::fs::write(r.join("code/alfile"), "").unwrap();
        assert_eq!(complete("./co", r).as_deref(), Some("./code/"));
        assert_eq!(complete("./code/al", r).as_deref(), Some("./code/alp"));
        assert_eq!(complete("./code/alp", r), None, "ambiguous, nothing to add");
        assert_eq!(complete("./code/b", r).as_deref(), Some("./code/beta/"));
        assert_eq!(complete("./code/.h", r).as_deref(), Some("./code/.hidden/"));
        assert_eq!(complete("./solo/", r).as_deref(), Some("./solo/only/"));
        assert_eq!(complete("~", r).as_deref(), Some("~/"));
        let abs = format!("{}/so", r.display());
        assert_eq!(complete(&abs, r), Some(format!("{}/solo/", r.display())));
    }

    #[test]
    fn tab_completes_the_path_field() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("projects/clove")).unwrap();
        let mut app = app(vec![]);
        app.start_dir = d.path().to_path_buf();
        app.open_add_repo();
        app.on_msg(Msg::ReposFound(found()));
        typed(&mut app, "./pro");
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Tab);
        assert_eq!(view(&app).input.text, "./projects/clove/");
        // Tab on a plain filter fills in the highlighted repo's path.
        app.on_input(TermEvent::Key(KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
        )));
        typed(&mut app, "tools");
        key(&mut app, KeyCode::Tab);
        assert_eq!(view(&app).input.text, "/home/ada/code/tools");
    }

    #[test]
    fn never_panics_at_any_size() {
        for w in [1, 20, 60, 80, 140] {
            for h in [1, 5, 14, 24, 60] {
                for stage in 0..4 {
                    let mut app = app(vec![]);
                    app.open_add_repo();
                    if stage > 0 {
                        app.on_msg(Msg::ReposFound(found()));
                    }
                    if stage > 1 {
                        key(&mut app, KeyCode::Enter);
                    }
                    if stage > 2 {
                        app.on_msg(Msg::InitPreview {
                            path: "/home/ada/code/api".into(),
                            result: Ok(preview(false)),
                        });
                    }
                    render(&mut app, w, h);
                }
            }
        }
    }
}
