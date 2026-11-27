//! Drawing. Every frame is a pure function of [`App`], except that it records
//! clickable regions in `app.hits` and keeps scroll targets in view.

use crate::app::App;
use crate::fmt::{self, pad, trunc, trunc_left, width};
use crate::model::*;
use crate::theme::*;
use crate::tooltip::{SpanTips, Tip, Tooltip, cut, cut_left};
use clove_core::doctor::Level;
use clove_core::settings::{FIELDS, Kind, Layer};
use clove_core::{CloneInfo, OtherWorktree};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

pub fn draw(f: &mut Frame, app: &mut App) {
    app.hits.clear();
    app.tips.clear();
    app.panels.clear();
    fmt::take_cuts();
    let area = f.area();
    if area.width < 60 || area.height < 14 {
        let msg = Paragraph::new(vec![
            Line::from(crate::theme::wordmark()),
            Line::from(Span::styled(
                format!("needs 60×14, have {}×{}", area.width, area.height),
                muted(),
            )),
        ])
        .centered();
        let y = area.height.saturating_sub(2) / 2;
        f.render_widget(msg, Rect::new(area.x, area.y + y, area.width, 2.min(area.height)));
        return;
    }
    let [header, body, footer] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(5), Constraint::Length(1)]).areas(area);
    // Wide terminals get a sidebar with every repo and the global tools;
    // narrow ones keep the repo tabs in the header.
    let sidebar = area.width >= SIDEBAR_MIN_WIDTH;
    draw_header(f, app, header, !sidebar);
    let (body, list) = draw_body(f, app, body, sidebar);
    draw_footer(f, app, footer);
    draw_modal(f, app, area, list);
    draw_toasts(f, app, body);
    tip_cuts(f, app);
    draw_hover(f, app);
    draw_tooltip(f, app);
}

/// Gives every text cut this frame (see `fmt::take_cuts`) a tooltip with its
/// full text where it shows on screen, unless a tip is already there or the
/// spot is behind an open modal. Ambiguous cuts (same shown text, different
/// full texts) are skipped.
fn tip_cuts(f: &mut Frame, app: &mut App) {
    let mut cuts: Vec<(String, String)> = fmt::take_cuts();
    cuts.sort();
    cuts.dedup();
    let buf = f.buffer_mut();
    let area = buf.area;
    for (i, (shown, full)) in cuts.iter().enumerate() {
        let ambiguous = cuts
            .iter()
            .enumerate()
            .any(|(j, (s, fl))| j != i && s == shown && fl != full);
        if ambiguous {
            continue;
        }
        let want: Vec<String> = shown.chars().map(String::from).collect();
        let w = width(shown) as u16;
        for y in area.top()..area.bottom() {
            // Cells left to right, skipping the blank halves of wide chars.
            let row: Vec<(u16, &str)> = (area.left()..area.right())
                .filter_map(|x| buf.cell((x, y)).map(|c| (x, c.symbol())))
                .filter(|(_, s)| !s.is_empty())
                .collect();
            if row.len() < want.len() {
                continue;
            }
            for start in 0..=row.len() - want.len() {
                if !want.iter().zip(&row[start..]).all(|(a, (_, b))| a == b) {
                    continue;
                }
                let r = Rect::new(row[start].0, y, w, 1);
                let at = Position::new(r.x, r.y);
                let covered = app.tips.iter().any(|(t, _)| t.contains(at));
                let behind_modal = app
                    .hits
                    .iter()
                    .rev()
                    .find(|(h, _)| h.contains(at))
                    .is_some_and(|(_, h)| matches!(h, Hit::Dismiss));
                if !covered && !behind_modal {
                    app.tips.push((r, Tip::new(full.clone())));
                }
            }
        }
    }
}

/// The tooltip of whatever the mouse rests on, once it has rested long enough.
/// Drawn last, over everything.
fn draw_tooltip(f: &mut Frame, app: &App) {
    let Some(at) = app.hover else { return };
    if app.tip_hidden || app.now_ms.saturating_sub(app.hover_ms) < crate::tooltip::DELAY_MS {
        return;
    }
    let Some((_, tip)) = app.tips.iter().rev().find(|(r, _)| r.contains(at)) else {
        return;
    };
    let t = Tooltip::new(tip);
    let r = t.area(at, f.area());
    f.render_widget(t, r);
}

/// Drops the tooltips under `r`: something was drawn over them.
fn occlude(app: &mut App, r: Rect) {
    app.tips.retain(|(t, _)| !t.intersects(r));
}

/// Registers a tooltip for `r` when there is one.
fn tip(app: &mut App, r: Rect, t: Option<Tip>) {
    if let Some(t) = t
        && !r.is_empty()
    {
        app.tips.push((r, t));
    }
}

/// Highlights whatever a click would act on under the mouse (bold, green text),
/// so it's clear the UI takes the mouse. The runtime also turns the pointer
/// into a hand, or a resize arrow over a divider (hovered or dragged).
fn draw_hover(f: &mut Frame, app: &mut App) {
    hover_panel(f, app);
    let hit = app.hover.and_then(|p| {
        app.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(p))
            .filter(|(_, h)| h.clickable())
            .map(|(r, h)| (*r, *h))
    });
    let divider = match (app.dragging, hit) {
        (Some(id), _) => {
            let s = app.panes.get(id);
            s.dragging().and_then(|p| s.divider(p)).map(|r| (r, id))
        }
        (None, Some((r, Hit::Divider(id, _)))) => Some((r, id)),
        _ => None,
    };
    if let Some((r, id)) = divider {
        crate::split::paint_divider(f.buffer_mut(), r, BOTTLE);
        app.pointer = match app.panes.get(id).dir() {
            ratatui::layout::Direction::Horizontal => Pointer::ColResize,
            ratatui::layout::Direction::Vertical => Pointer::RowResize,
        };
        return;
    }
    app.pointer = if hit.is_some() {
        Pointer::Hand
    } else {
        Pointer::Default
    };
    let Some((r, _)) = hit else { return };
    let buf = f.buffer_mut();
    for y in r.top()..r.bottom() {
        for x in r.left()..r.right() {
            if let Some(c) = buf.cell_mut((x, y)) {
                c.modifier.insert(Modifier::BOLD);
                // On a filled cell (the selected row) green text would vanish.
                if c.bg == ratatui::style::Color::Reset {
                    c.fg = SAGE;
                }
            }
        }
    }
}

/// Lights up the border of the panel under the mouse (SAGE, softer than the
/// BOTTLE of focused modals). Only plain border cells change, not titles.
fn hover_panel(f: &mut Frame, app: &App) {
    if app.modal.is_some() || app.dragging.is_some() {
        return;
    }
    let Some(p) = app.hover else { return };
    let Some(r) = app.panels.iter().rev().find(|r| r.contains(p)).copied() else {
        return;
    };
    let buf = f.buffer_mut();
    let edge = |x: u16, y: u16| x == r.left() || x + 1 == r.right() || y == r.top() || y + 1 == r.bottom();
    for y in r.top()..r.bottom() {
        for x in r.left()..r.right() {
            if !edge(x, y) {
                continue;
            }
            if let Some(c) = buf.cell_mut((x, y))
                && c.fg == LINE
                && "─│╭╮╰╯".contains(c.symbol())
            {
                c.fg = SAGE;
            }
        }
    }
}

// ---- building blocks -------------------------------------------------

pub(crate) fn panel<'a>(title_spans: Vec<Span<'a>>, focus: bool) -> Block<'a> {
    let mut t = vec![Span::raw(" ")];
    t.extend(title_spans);
    t.push(Span::raw(" "));
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(if focus { fg(BOTTLE) } else { border() })
        .title(Line::from(t))
}

pub(crate) fn put(f: &mut Frame, area: Rect, line: Line) {
    if area.width > 0 && area.height > 0 {
        f.render_widget(Paragraph::new(line), area);
    }
}

pub(crate) fn row(area: Rect, i: u16) -> Rect {
    Rect::new(area.x, area.y + i, area.width, 1).intersection(area)
}

pub(crate) fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn line_width(l: &Line) -> usize {
    l.spans.iter().map(|s| width(&s.content)).sum()
}

/// A thin scrollbar on a border column: `│` track, `┃` thumb.
pub(crate) fn scrollbar(f: &mut Frame, track: Rect, offset: usize, view: usize, len: usize) {
    if len <= view || track.height == 0 {
        return;
    }
    let h = track.height as usize;
    let thumb = ((h * view) / len).clamp(1, h);
    let max_off = len - view;
    let start = ((h - thumb) * offset.min(max_off) + max_off / 2) / max_off.max(1);
    let buf = f.buffer_mut();
    for i in 0..h {
        let (sym, style) = if i >= start && i < start + thumb {
            ("┃", fg(BOTTLE))
        } else {
            ("│", border())
        };
        if let Some(cell) = buf.cell_mut(Position::new(track.x, track.y + i as u16)) {
            cell.set_symbol(sym).set_style(style);
        }
    }
}

fn dim_backdrop(f: &mut Frame, area: Rect) {
    f.buffer_mut()
        .set_style(area, Style::new().add_modifier(Modifier::DIM));
}

fn kv<'a>(label: &'a str, value: Vec<Span<'a>>) -> Line<'a> {
    let mut s = vec![Span::styled(format!("{label:<8}"), muted())];
    s.extend(value);
    Line::from(s)
}

// ---- header ------------------------------------------------------------

fn draw_header(f: &mut Frame, app: &mut App, area: Rect, tabs: bool) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border())
        .title(Line::from(
            [
                vec![Span::raw(" ")],
                crate::theme::wordmark(),
                vec![Span::raw(" ")],
            ]
            .concat(),
        ));
    let inner = block.inner(area).inner(Margin::new(1, 0));
    f.render_widget(block, area);
    if tabs {
        draw_tabs(
            f,
            app,
            Rect::new(area.x + 12, area.y, area.width.saturating_sub(14), 1),
        );
    }

    let Some(repo) = app.repo().cloned() else {
        put(
            f,
            row(inner, 0),
            Line::from(Span::styled("no repository", muted())),
        );
        return;
    };
    let data = app.current().cloned();

    // Row 1: repo and clone counts.
    let right = match &data {
        Some(d) => {
            let busy = app.running().filter(|j| j.repo == repo.root).count();
            let mut s = format!(
                "{} clone{} · {} mounted",
                d.status.clones,
                if d.status.clones == 1 { "" } else { "s" },
                d.status.mounted
            );
            if busy > 0 {
                s = format!("{busy} running · {s}");
            }
            s
        }
        None => String::new(),
    };
    let rw = width(&right) as u16;
    let left_w = inner.width.saturating_sub(rw + 2) as usize;
    let root = fmt::tilde(&repo.root);
    let (name, name_tip) = cut(&repo.name, left_w);
    let (path, path_tip) = cut_left(&root, left_w.saturating_sub(width(&name) + 2));
    let mut tips = SpanTips::default();
    tips.add(0, 0, name_tip);
    tips.add(0, 2, path_tip);
    let line = Line::from(vec![
        Span::styled(name, bold()),
        Span::raw("  "),
        Span::styled(path, muted()),
    ]);
    tips.place(std::slice::from_ref(&line), row(inner, 0), 0, &mut app.tips);
    put(f, row(inner, 0), line);
    if rw > 0 && inner.width > rw {
        put(
            f,
            Rect::new(inner.right() - rw, inner.y, rw, 1),
            Line::from(Span::styled(right, muted())),
        );
    }

    // Row 2: the golden image.
    let (line, tips) = golden_line(app, data.as_ref(), inner.width as usize);
    tips.place(std::slice::from_ref(&line), row(inner, 1), 0, &mut app.tips);
    put(f, row(inner, 1), line);
}

/// The header's golden-image line, with tooltips for its terse parts.
fn golden_line<'a>(app: &App, data: Option<&RepoData>, w: usize) -> (Line<'a>, SpanTips) {
    let mut tips = SpanTips::default();
    let label = Span::styled("golden  ", muted());
    if let Some(j) = app.golden_job() {
        let step = j
            .current_step()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "starting".into());
        let verb = if matches!(j.task, Task::Init) {
            "building"
        } else {
            "refreshing"
        };
        let (step, step_tip) = cut(&step, w.saturating_sub(45).max(8));
        tips.add(0, 2, step_tip.map(|t| t.note("l to watch")));
        let line = Line::from(vec![
            label,
            Span::styled(format!("{} ", spinner(app.now_ms)), fg(BOTTLE)),
            Span::styled(format!("{verb} · {step}"), bold()),
            Span::styled(
                format!(" · {} · l to watch", fmt::secs(j.elapsed_ms(app.now_ms) as f64)),
                muted(),
            ),
        ]);
        return (line, tips);
    }
    if let Some(e) = app.current_error() {
        let (shown, t) = cut(&format!("✗ {e}"), w.saturating_sub(8));
        tips.add(0, 1, t);
        return (Line::from(vec![label, Span::styled(shown, fg(RED))]), tips);
    }
    let Some(d) = data else {
        let line = Line::from(vec![
            label,
            Span::styled(format!("{} loading", spinner(app.now_ms)), muted()),
        ]);
        return (line, tips);
    };
    let Some(g) = &d.status.golden else {
        let line = Line::from(vec![
            label,
            Span::styled("○ not built yet", fg(AMBER)),
            Span::styled(" · press ", muted()),
            Span::styled("i", key()),
            Span::styled(" to build it", muted()),
        ]);
        tips.add(
            0,
            1,
            Some(Tip::new(
                "No golden image: the checkout with deps warmed that every clone copies.",
            )),
        );
        return (line, tips);
    };
    let sep = || Span::styled(" · ", muted());
    let mut spans = vec![
        label,
        Span::styled("● ", fg(SAGE)),
        Span::styled(format!("gen {}", g.meta.generation), bold()),
        sep(),
        Span::styled(fmt::short_commit(&g.meta.commit), fg(PLUM)),
        sep(),
        Span::raw(format!("{} old", fmt::age(g.age_secs))),
        sep(),
    ];
    tips.add(
        0,
        2,
        Some(Tip::new(format!(
            "Golden image generation {}: new clones copy it.",
            g.meta.generation
        ))),
    );
    tips.add(
        0,
        4,
        Some(Tip::new(g.meta.commit.clone()).note(format!("the {} commit it was built from", g.meta.base))),
    );
    // Long form when it fits, else drop the base name.
    let long = w >= 70 + g.meta.base.len();
    let base = if long {
        format!(" {}", g.meta.base)
    } else {
        String::new()
    };
    let behind_tip = match g.behind {
        Some(0) => Tip::new(format!("Up to date with {}.", g.meta.base)),
        Some(n) => Tip::new(format!(
            "{} has {n} commit{} the golden image lacks.",
            g.meta.base,
            if n == 1 { "" } else { "s" }
        ))
        .note("r refreshes it"),
        None => Tip::new(format!("Unknown: {} could not be compared.", g.meta.base)),
    };
    tips.add(0, spans.len(), Some(behind_tip));
    match g.behind {
        Some(0) if long => spans.push(Span::styled(format!("✓ up to date with{base}"), fg(SAGE))),
        Some(0) => spans.push(Span::styled("✓ current", fg(SAGE))),
        Some(n) => spans.push(Span::styled(format!("↓{n} behind{base}"), fg(AMBER))),
        None => spans.push(Span::styled("? behind", muted())),
    }
    spans.push(sep());
    tips.add(
        0,
        spans.len(),
        Some(Tip::new(
            "Size of the golden image on disk; clones share its blocks.",
        )),
    );
    spans.push(Span::raw(fmt::bytes(g.image_bytes)));
    if g.meta.dirty {
        spans.push(sep());
        tips.add(
            0,
            spans.len(),
            Some(Tip::new(
                "Warm steps left tracked files modified; clones inherit the changes.",
            )),
        );
        spans.push(Span::styled("dirty warm", fg(AMBER)));
    }
    (Line::from(spans), tips)
}

fn draw_tabs(f: &mut Frame, app: &mut App, area: Rect) {
    if app.repos.len() < 2 || area.width < 8 {
        return;
    }
    let labels: Vec<String> = app
        .repos
        .iter()
        .map(|r| format!(" {} ", trunc(&r.name, 18)))
        .collect();
    let total: usize = labels.iter().map(|l| width(l) + 1).sum::<usize>() + 1;
    if total as u16 <= area.width {
        let mut x = area.right() - total as u16;
        put(
            f,
            Rect::new(x, area.y, 1, 1),
            Line::from(Span::styled(" ", border())),
        );
        x += 1;
        for (i, l) in labels.iter().enumerate() {
            let w = width(l) as u16;
            let style = if i == app.repo_idx { selected() } else { muted() };
            let r = Rect::new(x, area.y, w, 1);
            put(f, r, Line::from(Span::styled(l.clone(), style)));
            app.hits.push((r, Hit::Repo(i)));
            let name = &app.repos[i].name;
            let t = (width(name) > 18).then(|| Tip::new(name.clone()));
            tip(app, r, t);
            put(f, Rect::new(x + w, area.y, 1, 1), Line::from(" "));
            x += w + 1;
        }
    } else {
        // Too many to show: ‹ current n/m ›
        let cur = &labels[app.repo_idx];
        let s = format!(" ‹{cur}{}/{}› ", app.repo_idx + 1, app.repos.len());
        let w = width(&s) as u16;
        if w <= area.width {
            let r = Rect::new(area.right() - w, area.y, w, 1);
            put(f, r, Line::from(Span::styled(s, fg(BOTTLE))));
            app.hits.push((r, Hit::Key(KeyCode::Tab, KeyModifiers::NONE)));
        }
    }
}

// ---- sidebar -----------------------------------------------------------

pub const SIDEBAR_MIN_WIDTH: u16 = 100;

/// Every repo with its clone count and golden state, then the global tools.
fn draw_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let block = panel(vec![Span::styled("repos", title())], false);
    let inner = block.inner(area).inner(Margin::new(1, 0));
    f.render_widget(block, area);
    if inner.height < 4 {
        return;
    }
    let tools: Vec<(char, &str, Line)> = {
        let prunable = app.current().map_or(0, |d| d.summary.prunable);
        let svc = match app.service {
            Some(true) => Span::styled("● on", fg(SAGE)),
            Some(false) => Span::styled("○ off", muted()),
            None => Span::styled("", muted()),
        };
        vec![
            ('s', "settings", Line::from("")),
            (
                'g',
                "cleanup",
                Line::from(if prunable > 0 {
                    Span::styled(format!("{prunable} merged"), fg(SAGE))
                } else {
                    Span::raw("")
                }),
            ),
            ('D', "doctor", Line::from("")),
            ('S', "service", Line::from(svc)),
        ]
    };
    let tools_h = tools.len() as u16 + 1;
    // One row under the repos for "+ add repo".
    let list_h = inner.height.saturating_sub(tools_h + 1);
    let w = inner.width as usize;
    let repos = app.repos.clone();
    for (i, r) in repos.iter().enumerate().take(list_h as usize) {
        let rr = row(inner, i as u16);
        let current = i == app.repo_idx;
        let busy = app.running().any(|j| j.repo == r.root);
        let (state, state_style) = match &r.summary {
            _ if busy => (spinner(app.now_ms).to_string(), fg(BOTTLE)),
            None => (String::new(), muted()),
            Some(s) if s.generation.is_none() => ("set up".into(), fg(AMBER)),
            Some(s) if s.refreshing => (format!("{} {}", spinner(app.now_ms), s.clones), fg(BOTTLE)),
            Some(s) if s.stale => (format!("{} ●", s.clones), fg(AMBER)),
            Some(s) => (format!("{} ●", s.clones), fg(SAGE)),
        };
        let state_tip = match &r.summary {
            _ if busy => Some(Tip::new("An operation is running in this repo.")),
            None => None,
            Some(s) if s.generation.is_none() => Some(Tip::new("No golden image yet: i builds it.")),
            Some(s) => {
                let clones = format!("{} clone{}", s.clones, if s.clones == 1 { "" } else { "s" });
                Some(Tip::new(if s.refreshing {
                    format!("{clones} · golden image refreshing")
                } else if s.stale {
                    format!("{clones} · golden image stale: r refreshes it")
                } else {
                    format!("{clones} · golden image fresh")
                }))
            }
        };
        let sw = width(&state);
        let name_w = w.saturating_sub(sw + 3);
        let (name, name_tip) = cut(&r.name, name_w);
        let name_tip = name_tip.map(|t| t.note(fmt::tilde(&r.root)));
        tip(
            app,
            Rect::new(rr.x + 2, rr.y, width(&name) as u16, 1).intersection(rr),
            name_tip,
        );
        tip(
            app,
            Rect::new(rr.x + 3 + name_w as u16, rr.y, sw as u16, 1).intersection(rr),
            state_tip,
        );
        let mut spans = vec![
            Span::styled(if current { "▌" } else { " " }, fg(BOTTLE)),
            Span::raw(" "),
            Span::styled(pad(&name, name_w), if current { bold() } else { Style::new() }),
            Span::raw(" "),
        ];
        // The count stays plain; only the dot carries the state colour.
        match state.rsplit_once(' ') {
            Some((n, dot)) if dot == "●" => {
                spans.push(Span::styled(format!("{n} "), muted()));
                spans.push(Span::styled(dot.to_string(), state_style));
            }
            _ => spans.push(Span::styled(state, state_style)),
        }
        put(f, rr, Line::from(spans));
        if current {
            f.buffer_mut()
                .set_style(rr, Style::new().add_modifier(Modifier::BOLD));
        }
        app.hits.push((rr, Hit::Repo(i)));
    }
    if repos.len() > list_h as usize {
        put(
            f,
            row(inner, list_h.saturating_sub(1)),
            Line::from(Span::styled(
                format!("  +{} more · tab", repos.len() - list_h as usize + 1),
                muted(),
            )),
        );
    }
    let add = row(inner, (repos.len() as u16).min(list_h));
    put(
        f,
        add,
        Line::from(vec![Span::styled("+ ", key()), Span::styled("add repo", muted())]),
    );
    app.hits.push((add, key_hit('A')));
    let top = inner.y + inner.height - tools.len() as u16;
    put(
        f,
        Rect::new(inner.x, top - 1, inner.width, 1),
        Line::from(Span::styled("─".repeat(w), border())),
    );
    for (i, (k, label, right)) in tools.into_iter().enumerate() {
        let r = Rect::new(inner.x, top + i as u16, inner.width, 1);
        put(
            f,
            r,
            Line::from(vec![
                Span::styled(format!("{k} "), key()),
                Span::styled(label.to_string(), muted()),
            ]),
        );
        let rw = line_width(&right) as u16;
        if rw > 0 && rw < inner.width {
            let rr = Rect::new(inner.right() - rw, r.y, rw, 1);
            put(f, rr, right);
            let t = match (k, app.service) {
                ('g', _) => Some(Tip::new("Clones whose PR is merged: cleanup removes them.")),
                ('S', Some(true)) => Some(Tip::new(
                    "Background service on: refreshes stale images when you pull, remounts clones at login.",
                )),
                ('S', Some(false)) => {
                    Some(Tip::new("Background service off.").note("s, then S, turns it on"))
                }
                _ => None,
            };
            tip(app, rr, t);
        }
        // The service row opens settings, where S toggles it with context.
        let k = if k == 'S' { 's' } else { k };
        app.hits.push((r, key_hit(k)));
    }
}

// ---- body --------------------------------------------------------------

/// Sidebar | clone list | detail, through the resizable splits in
/// `app.panes`. Returns the area right of the sidebar and the list's rect.
fn draw_body(f: &mut Frame, app: &mut App, area: Rect, sidebar: bool) -> (Rect, Rect) {
    use crate::panes::{DETAIL, SIDEBAR, SIDEBAR_WIDTH, STACK_DETAIL, SplitId};
    // Where the detail goes depends on default widths, so resizing a panel
    // never flips it between the side and the bottom.
    let main_w = area.width.saturating_sub(if sidebar { SIDEBAR_WIDTH } else { 0 });
    let right = main_w >= 110;
    let stacked = !right && area.height >= 26;
    let body = &mut app.panes.body;
    body.show(SIDEBAR, sidebar);
    body.show(DETAIL, right);
    let rects = body.layout(area);
    let mut dividers: Vec<(Rect, Hit)> = body
        .dividers()
        .iter()
        .map(|&(p, r)| (r, Hit::Divider(SplitId::Body, p)))
        .collect();
    let main = rects[1];
    let (list, detail) = if right {
        (main, Some(rects[DETAIL]))
    } else if stacked {
        let r = app.panes.stack.layout(main);
        dividers.extend(
            app.panes
                .stack
                .dividers()
                .iter()
                .map(|&(p, r)| (r, Hit::Divider(SplitId::Stack, p))),
        );
        (r[0], Some(r[STACK_DETAIL]))
    } else {
        (main, None)
    };
    if sidebar {
        draw_sidebar(f, app, rects[SIDEBAR]);
    }
    // Plain git worktrees get their own panel under the clones when there
    // is room for both; on short terminals they stay in the clone list.
    let wts = !app.visible_worktrees().is_empty() && list.height >= 14;
    if wts {
        let s = &mut app.panes.worktrees;
        let r = s.layout(list);
        dividers.extend(
            s.dividers()
                .iter()
                .map(|&(p, r)| (r, Hit::Divider(SplitId::Worktrees, p))),
        );
        draw_list(f, app, r[0], false);
        draw_worktrees(f, app, r[crate::panes::WORKTREES]);
    } else {
        draw_list(f, app, list, true);
    }
    if let Some(d) = detail {
        draw_detail(f, app, d);
        app.panels.push(d);
    }
    if sidebar {
        app.panels.push(rects[SIDEBAR]);
    }
    app.hits.extend(dividers);
    let main = if sidebar {
        Rect::new(main.x, area.y, area.right() - main.x, area.height)
    } else {
        area
    };
    (main, list)
}

fn empty_state(f: &mut Frame, area: Rect, mut lines: Vec<Line>) {
    // A lone ◆ heading becomes the full mark when there is room for it.
    let mark = crate::theme::mark_lines();
    if lines.first().is_some_and(|l| l.to_string() == "◆") && (lines.len() + mark.len()) as u16 <= area.height
    {
        lines.splice(0..1, mark.into_iter().chain([Line::from("")]));
    }
    let h = lines.len() as u16;
    let y = area.y + area.height.saturating_sub(h) / 2;
    f.render_widget(
        Paragraph::new(lines).centered(),
        Rect::new(area.x, y, area.width, h.min(area.height)),
    );
}

/// The clone list. With `inline_wts`, plain worktrees follow the clones under
/// a heading; otherwise `draw_worktrees` shows them in their own panel.
fn draw_list(f: &mut Frame, app: &mut App, area: Rect, inline_wts: bool) {
    let total = app.current().map(|d| d.clones.len()).unwrap_or(0);
    let total_wt = if inline_wts {
        app.current().map(|d| d.worktrees.len()).unwrap_or(0)
    } else {
        0
    };
    let vis_len = app.visible().len();
    let mut t = vec![Span::styled("clones", title())];
    if app.current().is_some() {
        let count = if app.filter.is_empty() {
            format!(" {total}")
        } else {
            format!(" {vis_len}/{total}")
        };
        t.push(Span::styled(count, muted()));
        if total_wt > 0 {
            t.push(Span::styled(
                format!(
                    " · {total_wt} git worktree{}",
                    if total_wt == 1 { "" } else { "s" }
                ),
                muted(),
            ));
        }
    }
    if app.filtering || !app.filter.is_empty() {
        t.push(Span::styled("  /", key()));
        t.push(Span::styled(app.filter.clone(), bold()));
        if app.filtering {
            t.push(Span::styled("▏", fg(BOTTLE)));
        }
    }
    let block = panel(t, false);
    let inner = block.inner(area).inner(Margin::new(1, 0));
    f.render_widget(block, area);
    app.panels.push(area);
    app.hits.push((inner, Hit::ScrollList));

    if app.repo().is_none() {
        let mut lines = vec![
            Line::from(Span::styled("◆", fg(BOTTLE))),
            Line::from(Span::styled("no repos yet", bold())),
            Line::from(""),
            Line::from(vec![
                Span::styled("A", key()),
                Span::styled(
                    " adds one: pick a git repo, see what clove will do, build it",
                    muted(),
                ),
            ]),
        ];
        if let Some(n) = &app.notice {
            for l in fmt::wrap(n, inner.width.saturating_sub(4) as usize) {
                lines.push(Line::from(Span::styled(l, muted())));
            }
        }
        empty_state(f, inner, lines);
        return;
    }
    if let (Some(e), None) = (app.current_error(), app.current()) {
        let mut lines = vec![Line::from(Span::styled("✗ could not read this repo", fg(RED)))];
        for l in fmt::wrap(e, inner.width.saturating_sub(4) as usize) {
            lines.push(Line::from(Span::styled(l, muted())));
        }
        empty_state(f, inner, lines);
        return;
    }
    let Some(data) = app.current().cloned() else {
        empty_state(
            f,
            inner,
            vec![Line::from(Span::styled(
                format!("{} loading", spinner(app.now_ms)),
                muted(),
            ))],
        );
        return;
    };
    if data.status.golden.is_none() && total == 0 {
        let building = app.golden_job().is_some();
        let mut lines = vec![
            Line::from(Span::styled("◆", fg(BOTTLE))),
            Line::from(Span::styled(
                format!("{} has no golden image yet", data.status.name),
                bold(),
            )),
            Line::from(""),
        ];
        if building {
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", spinner(app.now_ms)), fg(BOTTLE)),
                Span::styled("building it now · ", muted()),
                Span::styled("l", key()),
                Span::styled(" to watch", muted()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("press ", muted()),
                Span::styled("i", key()),
                Span::styled(" to build it: a checkout of ", muted()),
                Span::raw(data.status.base.clone()),
                Span::styled(" with deps warmed", muted()),
            ]));
            lines.push(Line::from(Span::styled(
                "slow once, then every new clone takes about a second",
                muted(),
            )));
        }
        empty_state(f, inner, lines);
        return;
    }
    if total == 0 && total_wt == 0 {
        empty_state(
            f,
            inner,
            vec![
                Line::from(Span::styled("◆", fg(BOTTLE))),
                Line::from(Span::styled("no clones yet", bold())),
                Line::from(""),
                Line::from(vec![
                    Span::styled("n", key()),
                    Span::styled(" makes one in about a second, deps and dev env included", muted()),
                ]),
            ],
        );
        return;
    }
    let clones: Vec<CloneInfo> = app.visible().into_iter().cloned().collect();
    let all_wts: Vec<OtherWorktree> = app.visible_worktrees().into_iter().cloned().collect();
    // Columns line up across both panels.
    let cols = columns(&clones, &all_wts, inner.width as usize);
    let wts = if inline_wts { all_wts } else { vec![] };
    if !inline_wts && clones.is_empty() {
        let msg = if app.filter.is_empty() {
            vec![
                Span::styled("no clones yet · ", muted()),
                Span::styled("n", key()),
                Span::styled(" makes one · ", muted()),
                Span::styled("c", key()),
                Span::styled(" turns a worktree below into one", muted()),
            ]
        } else {
            vec![
                Span::styled("no clone matches ", muted()),
                Span::styled(format!("/{}", app.filter), bold()),
            ]
        };
        empty_state(f, inner, vec![Line::from(msg)]);
        return;
    }
    if clones.is_empty() && wts.is_empty() {
        empty_state(
            f,
            inner,
            vec![Line::from(vec![
                Span::styled("no clone matches ", muted()),
                Span::styled(format!("/{}", app.filter), bold()),
            ])],
        );
        return;
    }
    draw_table(f, app, area, inner, &clones, &wts, cols);
}

/// Plain git worktrees in their own panel under the clones. Selection runs on
/// from the last clone, so the same keys act on both.
fn draw_worktrees(f: &mut Frame, app: &mut App, area: Rect) {
    let clones: Vec<CloneInfo> = app.visible().into_iter().cloned().collect();
    let wts: Vec<OtherWorktree> = app.visible_worktrees().into_iter().cloned().collect();
    let total = app.current().map(|d| d.worktrees.len()).unwrap_or(0);
    let count = if app.filter.is_empty() {
        format!(" {total}")
    } else {
        format!(" {}/{total}", wts.len())
    };
    let block = panel(
        vec![
            Span::styled("git worktrees", title()),
            Span::styled(count, muted()),
            Span::styled(" · not clones, ", muted()),
            Span::styled("c", key()),
            Span::styled(" makes one a clone", muted()),
        ],
        false,
    );
    let inner = block.inner(area).inner(Margin::new(1, 0));
    f.render_widget(block, area);
    app.panels.push(area);
    app.hits.push((inner, Hit::ScrollList));
    let cols = columns(&clones, &wts, inner.width as usize);
    let mut head = format!(
        "  {}  {}  {}",
        pad("NAME", cols.name),
        pad("BRANCH", cols.branch),
        pad("STATE", cols.changes)
    );
    if cols.path > 0 {
        head.push_str(&" ".repeat(cols.age + cols.size + 4));
        head.push_str("  PATH");
    }
    put(
        f,
        row(inner, 0),
        Line::from(Span::styled(head, muted().add_modifier(Modifier::BOLD))),
    );
    let h = inner.height.saturating_sub(1) as usize;
    let n = clones.len();
    let len = wts.len();
    if let Some(sel) = app.selected.checked_sub(n).filter(|s| *s < len) {
        app.wt_scroll.follow(sel, h, len);
    }
    app.wt_scroll.clamp(h, len);
    let off = app.wt_scroll.offset().min(len.saturating_sub(h));
    for (i, w) in wts.iter().enumerate().skip(off).take(h) {
        let r = row(inner, 1 + (i - off) as u16);
        put(f, r, worktree_row(app, w, &cols));
        worktree_tips(app, w, &cols, r);
        if n + i == app.selected {
            f.buffer_mut().set_style(r, selected());
        }
        app.hits.push((r, Hit::Row(n + i)));
    }
    let track = Rect::new(area.right() - 1, inner.y + 1, 1, h as u16);
    scrollbar(f, track, off, h, len);
}

struct Cols {
    name: usize,
    branch: usize,
    changes: usize,
    age: usize,
    size: usize,
    path: usize,
}

fn columns(clones: &[CloneInfo], wts: &[OtherWorktree], w: usize) -> Cols {
    let name = clones
        .iter()
        .map(|c| width(&c.name))
        .chain(wts.iter().map(|w| width(&w.name)))
        .max()
        .unwrap_or(4)
        .clamp(4, 26);
    let branch = clones
        .iter()
        .map(|c| c.branch.as_deref().map(width).unwrap_or(10))
        .chain(wts.iter().map(|w| w.branch.as_deref().map(width).unwrap_or(10)))
        .max()
        .unwrap_or(6)
        .clamp(6, 30);
    let (changes, age, size) = (11, 4, 6);
    // dot(2) + gaps(2 each between six columns)
    let fixed = 2 + changes + age + size + 2 * 5;
    let mut c = Cols {
        name,
        branch,
        changes,
        age,
        size,
        path: 0,
    };
    let avail = w.saturating_sub(fixed);
    if c.name + c.branch > avail {
        // Shrink both in proportion, keeping each readable.
        let total = c.name + c.branch;
        let name = (avail * c.name / total).max(4);
        c.branch = avail.saturating_sub(name).max(6);
        c.name = name;
    }
    c.path = avail.saturating_sub(c.name + c.branch);
    if c.path < 10 {
        c.path = 0;
    }
    c
}

/// One line of the list: a clone, the worktrees' heading (with a blank line
/// before it), or a plain worktree.
enum ListLine<'a> {
    Clone(usize, &'a CloneInfo),
    Gap,
    Heading,
    Worktree(usize, &'a OtherWorktree),
}

fn draw_table(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    inner: Rect,
    clones: &[CloneInfo],
    wts: &[OtherWorktree],
    cols: Cols,
) {
    let gap = "  ";
    let mut head = format!(
        "  {}{gap}{}{gap}{}{gap}{:>age$}{gap}{:>size$}",
        pad("NAME", cols.name),
        pad("BRANCH", cols.branch),
        pad("CHANGES", cols.changes),
        "AGE",
        "SIZE",
        age = cols.age,
        size = cols.size
    );
    if cols.path > 0 {
        head.push_str(gap);
        head.push_str("PATH");
    }
    put(
        f,
        row(inner, 0),
        Line::from(Span::styled(head, muted().add_modifier(Modifier::BOLD))),
    );

    let mut lines: Vec<ListLine> = clones
        .iter()
        .enumerate()
        .map(|(i, c)| ListLine::Clone(i, c))
        .collect();
    if !wts.is_empty() {
        if !clones.is_empty() {
            lines.push(ListLine::Gap);
        }
        lines.push(ListLine::Heading);
        let n = clones.len();
        lines.extend(wts.iter().enumerate().map(|(i, w)| ListLine::Worktree(n + i, w)));
    }
    let h = inner.height.saturating_sub(1) as usize;
    let len = lines.len();
    app.list_height = h;
    // The selection counts rows; the worktrees sit below a heading.
    let sel_line = lines
        .iter()
        .position(|l| matches!(l, ListLine::Clone(i, _) | ListLine::Worktree(i, _) if *i == app.selected))
        .unwrap_or(0);
    // Keep the heading in view above the first worktree.
    let first_wt = clones.len() == app.selected && sel_line > 0;
    app.list_scroll
        .follow(if first_wt { sel_line - 1 } else { sel_line }, h, len);
    app.list_scroll.follow(sel_line, h, len);
    app.list_scroll.clamp(h, len);
    let off = app.list_scroll.offset().min(len.saturating_sub(h));

    for (li, l) in lines.iter().enumerate().skip(off).take(h) {
        let r = row(inner, 1 + (li - off) as u16);
        let vi = match l {
            ListLine::Gap => continue,
            ListLine::Heading => {
                put(
                    f,
                    r,
                    Line::from(vec![
                        Span::styled("  OTHER GIT WORKTREES", muted().add_modifier(Modifier::BOLD)),
                        Span::styled(
                            trunc(
                                "  not clones · c makes one a clone",
                                r.width.saturating_sub(22) as usize,
                            ),
                            muted(),
                        ),
                    ]),
                );
                continue;
            }
            ListLine::Clone(vi, c) => {
                put(f, r, clone_row(app, c, &cols));
                row_tips(app, c, &cols, r);
                *vi
            }
            ListLine::Worktree(vi, w) => {
                put(f, r, worktree_row(app, w, &cols));
                worktree_tips(app, w, &cols, r);
                *vi
            }
        };
        if vi == app.selected {
            f.buffer_mut().set_style(r, selected());
        }
        app.hits.push((r, Hit::Row(vi)));
    }
    let track = Rect::new(area.right() - 1, inner.y + 1, 1, h as u16);
    scrollbar(f, track, off, h, len);
}

fn clone_row<'a>(app: &App, c: &CloneInfo, cols: &Cols) -> Line<'a> {
    let gap = || Span::raw("  ");
    let busy = app.busy(&c.name);
    let dot = match (busy, c.mounted) {
        (Some(_), _) => Span::styled(format!("{} ", spinner(app.now_ms)), fg(BOTTLE)),
        (None, true) if !c.failed_steps.is_empty() => Span::styled("● ", fg(AMBER)),
        (None, true) => Span::styled("● ", fg(SAGE)),
        (None, false) => Span::styled("○ ", muted()),
    };
    let name_style = if c.mounted { bold() } else { muted() };
    let branch = match &c.branch {
        Some(b) => Span::styled(pad(b, cols.branch), fg(PLUM)),
        None => Span::styled(
            pad(
                &match c.head.as_deref() {
                    Some(h) => format!("detached @{}", &h[..h.len().min(7)]),
                    None => "detached".into(),
                },
                cols.branch,
            ),
            muted(),
        ),
    };
    let mut spans = vec![
        dot,
        Span::styled(pad(&c.name, cols.name), name_style),
        gap(),
        branch,
        gap(),
    ];
    spans.extend(changes(app, c, cols.changes));
    let age = fmt::age(app.now_unix.saturating_sub(c.created_at));
    spans.push(gap());
    spans.push(Span::styled(format!("{age:>w$}", w = cols.age), muted()));
    spans.push(gap());
    spans.push(Span::raw(format!(
        "{:>w$}",
        fmt::bytes(c.own_bytes),
        w = cols.size
    )));
    if cols.path > 0 {
        spans.push(gap());
        spans.push(Span::styled(trunc_left(&fmt::tilde(&c.path), cols.path), muted()));
    }
    Line::from(spans)
}

/// Tooltips over a clone row's cells: the full text of cut ones, and what
/// the dot and the CHANGES badges mean.
fn row_tips(app: &mut App, c: &CloneInfo, cols: &Cols, r: Rect) {
    let dot = Rect::new(r.x, r.y, 1, 1).intersection(r);
    let mut x = r.x + 2;
    let mut cell = |w: usize| {
        let c = Rect::new(x, r.y, w as u16, 1).intersection(r);
        x = x.saturating_add(w as u16 + 2);
        c
    };
    let name = cell(cols.name);
    let branch = cell(cols.branch);
    let changes = cell(cols.changes);
    let _age = cell(cols.age);
    let size = cell(cols.size);
    let path = cell(cols.path);

    let dot_tip = match (app.busy(&c.name), c.mounted) {
        (Some(j), _) => Tip::new(format!("{}…", j.verb())),
        (None, true) if !c.failed_steps.is_empty() => {
            Tip::new(format!("Mounted; setup {} failed.", c.failed_steps.join(", ")))
        }
        (None, true) => Tip::new("Mounted."),
        (None, false) => Tip::new("Not mounted: m mounts it."),
    };
    tip(app, dot, Some(dot_tip));
    tip(app, name, cut(&c.name, cols.name).1);
    if let Some(b) = &c.branch {
        tip(app, branch, cut(b, cols.branch).1);
    }
    let base = app.current().map(|d| d.status.base.clone()).unwrap_or_default();
    tip(app, changes, changes_tip(app, c, &base));
    tip(
        app,
        size,
        Some(
            Tip::new(format!(
                "{} of its own: what removing it frees.",
                fmt::bytes(c.own_bytes)
            ))
            .note(format!(
                "the rest of its {} is shared with the golden image",
                fmt::bytes(c.image_bytes)
            )),
        ),
    );
    if cols.path > 0 {
        tip(app, path, cut_left(&fmt::tilde(&c.path), cols.path).1);
    }
}

/// What each part of the CHANGES cell says, one per line.
fn changes_tip(app: &App, c: &CloneInfo, base: &str) -> Option<Tip> {
    let mut out: Vec<String> = vec![];
    let pr = app
        .current()
        .and_then(|d| d.prs.get(&c.name))
        .and_then(|m| m.pr.as_ref());
    if let Some(pr) = pr
        && pr.state == clove_core::PrState::Merged
    {
        out.push(format!("merged: PR #{} is merged; g cleans it up", pr.number));
    }
    if app.busy(&c.name).is_some() {
        // The cell says what runs; nothing to add.
    } else if let Some(issue) = &c.issue {
        out.push(format!("{issue}: g finishes removing it"));
    } else if let Some(op) = c.operation {
        out.push(format!(
            "{}: stopped on conflicts; R resolves them with the resolver, or fix and git add them, then U continues, X aborts",
            op.word()
        ));
    } else if c.removing_when_free {
        out.push("removing when free: something still uses it; it goes once nothing does".into());
    } else if !c.mounted {
        out.push("unmounted: m mounts it".into());
    } else {
        match c.dirty {
            Some(true) => out.push("dirty: uncommitted changes".into()),
            Some(false) => out.push("clean: no uncommitted changes".into()),
            None => {}
        }
        if let Some(a) = c.ahead.filter(|a| *a > 0) {
            out.push(format!(
                "↑{a}: commit{} not on {base}",
                if a == 1 { "" } else { "s" }
            ));
        }
        if let Some(b) = c.behind.filter(|b| *b > 0) {
            out.push(format!(
                "↓{b}: commit{} on {base} it lacks",
                if b == 1 { "" } else { "s" }
            ));
        }
    }
    if let Some(pr) = pr
        && pr.state == clove_core::PrState::Open
    {
        out.push(format!("#{}: open PR", pr.number));
    }
    if out.is_empty() {
        return None;
    }
    let mut t = Tip::new(out.join("\n"));
    if let Some(pr) = pr {
        t = t.note(pr.title.clone());
    }
    Some(t)
}

/// Tooltips over a plain worktree's row: what it is, and the full text of cut cells.
fn worktree_tips(app: &mut App, w: &OtherWorktree, cols: &Cols, r: Rect) {
    let dot = Rect::new(r.x, r.y, 1, 1).intersection(r);
    let name = Rect::new(r.x + 2, r.y, cols.name as u16, 1).intersection(r);
    let branch = Rect::new(name.right() + 2, r.y, cols.branch as u16, 1).intersection(r);
    let mut what = Tip::new("A plain git worktree, not a clone: c makes it one.");
    if let Some(b) = &w.blocker {
        what = what.note(b.clone());
    }
    tip(app, dot, Some(what));
    tip(app, name, cut(&w.name, cols.name).1);
    if let Some(b) = &w.branch {
        tip(app, branch, cut(b, cols.branch).1);
    }
    let state = Rect::new(branch.right() + 2, r.y, cols.changes as u16, 1).intersection(r);
    let mut lines: Vec<String> = vec![];
    if let Some(pr) = &w.pr {
        lines.push(format!(
            "PR #{} {}",
            pr.number,
            format!("{:?}", pr.state).to_lowercase()
        ));
    }
    if w.missing {
        lines.push("missing: its folder is gone; git worktree prune forgets it".into());
    }
    if w.locked {
        lines.push(match &w.lock_reason {
            Some(why) => format!("locked: {why}"),
            None => "locked".into(),
        });
    }
    match w.dirty {
        Some(true) => lines.push("dirty: uncommitted changes".into()),
        Some(false) if !w.missing => lines.push("clean: no uncommitted changes".into()),
        _ => {}
    }
    if let Some(n) = w.detached_commits.filter(|n| *n > 0) {
        lines.push(format!("{n} commit(s) on no branch: removing it would lose them"));
    }
    if !lines.is_empty() {
        tip(app, state, Some(Tip::new(lines.join("\n"))));
    }
    if cols.path > 0 {
        let x = r.right().saturating_sub(cols.path as u16);
        let path = Rect::new(x, r.y, cols.path as u16, 1).intersection(r);
        tip(app, path, cut_left(&fmt::tilde(&w.path), cols.path).1);
    }
}

/// A plain git worktree: no size or age (clove didn't make it), dimmer than clones.
fn worktree_row<'a>(app: &App, w: &OtherWorktree, cols: &Cols) -> Line<'a> {
    let gap = || Span::raw("  ");
    let busy = app.busy_worktree(&w.path);
    let dot = match busy {
        Some(_) => Span::styled(format!("{} ", spinner(app.now_ms)), fg(BOTTLE)),
        None => Span::styled("◇ ", muted()),
    };
    let branch = match &w.branch {
        Some(b) => Span::styled(pad(b, cols.branch), fg(PLUM)),
        None => Span::styled(
            pad(
                &match w.head.as_deref() {
                    Some(h) => format!("detached @{}", &h[..h.len().min(7)]),
                    None => "detached".into(),
                },
                cols.branch,
            ),
            muted(),
        ),
    };
    let mut parts: Vec<(String, Style)> = vec![];
    if let Some(j) = busy {
        parts.push((format!("{}…", j.verb()), fg(BOTTLE)));
    } else {
        if w.missing {
            parts.push(("missing".into(), fg(AMBER)));
        }
        if w.locked {
            parts.push(("locked".into(), muted()));
        }
        if w.dirty == Some(true) {
            parts.push(("dirty".into(), fg(AMBER)));
        }
        if w.detached_commits.is_some_and(|n| n > 0) {
            parts.push(("orphans".into(), fg(AMBER)));
        }
        match w.pr.as_ref().map(|p| (&p.state, p.number)) {
            Some((clove_core::PrState::Merged, _)) => parts.insert(0, ("merged".into(), fg(SAGE))),
            Some((clove_core::PrState::Open, n)) => parts.push((format!("#{n}"), muted())),
            _ => {}
        }
        if parts.is_empty() {
            let s = if w.dirty.is_some() { "clean" } else { "·" };
            parts.push((s.into(), muted()));
        }
    }
    let mut spans = vec![
        dot,
        Span::styled(pad(&w.name, cols.name), Style::new()),
        gap(),
        branch,
        gap(),
    ];
    spans.extend(cell_parts(parts, cols.changes));
    spans.push(gap());
    spans.push(Span::raw(" ".repeat(cols.age)));
    spans.push(gap());
    spans.push(Span::raw(" ".repeat(cols.size)));
    if cols.path > 0 {
        spans.push(gap());
        spans.push(Span::styled(trunc_left(&fmt::tilde(&w.path), cols.path), muted()));
    }
    Line::from(spans)
}

/// Joins styled words into a cell `w` wide, cutting what doesn't fit.
fn cell_parts<'a>(parts: Vec<(String, Style)>, w: usize) -> Vec<Span<'a>> {
    let mut out = vec![];
    let mut used = 0;
    for (i, (s, st)) in parts.into_iter().enumerate() {
        let s = if i > 0 { format!(" {s}") } else { s };
        let room = w.saturating_sub(used);
        if room == 0 {
            break;
        }
        let s = trunc(&s, room);
        used += width(&s);
        out.push(Span::styled(s, st));
    }
    out.push(Span::raw(" ".repeat(w.saturating_sub(used))));
    out
}

/// The CHANGES cell, padded to `w`.
fn changes<'a>(app: &App, c: &CloneInfo, w: usize) -> Vec<Span<'a>> {
    let mut parts: Vec<(String, Style)> = vec![];
    if let Some(j) = app.busy(&c.name) {
        parts.push((format!("{}…", j.verb()), fg(BOTTLE)));
    } else if let Some(issue) = &c.issue {
        parts.push((issue.clone(), fg(AMBER)));
    } else if let Some(op) = c.operation {
        parts.push((format!("{}: conflicts", op.word()), fg(AMBER)));
    } else if c.removing_when_free {
        parts.push(("removing when free".into(), muted()));
    } else if !c.mounted {
        parts.push(("unmounted".into(), muted()));
    } else {
        if c.dirty == Some(true) {
            parts.push(("dirty".into(), fg(AMBER)));
        }
        if let Some(a) = c.ahead.filter(|a| *a > 0) {
            parts.push((format!("↑{a}"), fg(SAGE)));
        }
        if let Some(b) = c.behind.filter(|b| *b > 0) {
            parts.push((format!("↓{b}"), muted()));
        }
        if parts.is_empty() {
            let s = if c.dirty.is_some() { "clean" } else { "·" };
            parts.push((s.into(), muted()));
        }
    }
    // The last PR check: a merged clone says so first, an open PR goes last.
    if let Some(pr) = app
        .current()
        .and_then(|d| d.prs.get(&c.name))
        .and_then(|m| m.pr.as_ref())
    {
        match pr.state {
            clove_core::PrState::Merged => parts.insert(0, ("merged".into(), fg(SAGE))),
            clove_core::PrState::Open => parts.push((format!("#{}", pr.number), muted())),
            clove_core::PrState::Closed => {}
        }
    }
    cell_parts(parts, w)
}

/// Appends a TIMINGS header and one bar per step, scaled to the slowest; the
/// slowest bar takes the accent. Returns the step-name column width.
fn timing_lines(
    lines: &mut Vec<Line>,
    tips: &mut SpanTips,
    timings: &[clove_core::Timing],
    w: usize,
) -> usize {
    let total: f64 = timings.iter().map(|t| t.ms).sum();
    let head = "TIMINGS";
    let tot = fmt::secs(total);
    lines.push(Line::from(vec![
        Span::styled(head, muted().add_modifier(Modifier::BOLD)),
        Span::raw(" ".repeat(w.saturating_sub(head.len() + width(&tot)))),
        Span::styled(tot, bold()),
    ]));
    let max = timings.iter().map(|t| t.ms).fold(0.0, f64::max);
    let nw = timings
        .iter()
        .map(|t| width(&t.step))
        .max()
        .unwrap_or(4)
        .clamp(4, 16);
    let vw = 6;
    let bw = w.saturating_sub(nw + vw + 2);
    for t in timings {
        let frac = if max > 0.0 { t.ms / max } else { 0.0 };
        let b = fmt::bar(frac, bw);
        let style = if t.ms == max && timings.len() > 1 {
            fg(BOTTLE)
        } else {
            fg(SAGE)
        };
        let (name, name_tip) = cut(&t.step, nw);
        tips.add(lines.len(), 0, name_tip);
        lines.push(Line::from(vec![
            Span::styled(pad(&name, nw), muted()),
            Span::raw(" "),
            Span::styled(pad(&b, bw), style),
            Span::raw(" "),
            Span::raw(format!("{:>vw$}", fmt::secs(t.ms))),
        ]));
    }
    nw
}

/// With nothing selected, the detail pane describes the golden image.
fn draw_golden_detail(f: &mut Frame, app: &mut App, area: Rect) -> bool {
    let Some(d) = app.current() else {
        return false;
    };
    let Some(g) = d.status.golden.clone() else {
        return false;
    };
    let block = panel(
        vec![Span::styled(format!("golden gen {}", g.meta.generation), title())],
        false,
    );
    let inner = block.inner(area).inner(Margin::new(2, 1));
    f.render_widget(block, area);
    let w = inner.width as usize;
    let vw = w.saturating_sub(8);
    let mut tips = SpanTips::default();
    let (base, base_tip) = cut(&g.meta.base, vw);
    tips.add(0, 1, base_tip);
    tips.add(1, 1, Some(Tip::new(g.meta.commit.clone())));
    let mut lines = vec![
        kv("base", vec![Span::styled(base, fg(PLUM))]),
        kv(
            "commit",
            vec![Span::styled(fmt::short_commit(&g.meta.commit), muted())],
        ),
        kv(
            "built",
            vec![
                Span::raw(format!("{} ago", fmt::age(g.age_secs))),
                Span::styled(" · ", muted()),
                Span::raw(fmt::bytes(g.image_bytes)),
            ],
        ),
    ];
    if !g.meta.steps.is_empty() {
        let (warm, warm_tip) = cut(&g.meta.steps.join(", "), vw);
        tips.add(lines.len(), 1, warm_tip);
        lines.push(kv("warm", vec![Span::raw(warm)]));
    }
    if !g.meta.timings.is_empty() {
        lines.push(Line::from(""));
        timing_lines(&mut lines, &mut tips, &g.meta.timings, w);
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("n", key()),
        Span::styled(" clones it in about a second", muted()),
    ]));
    tips.place(&lines, inner, 0, &mut app.tips);
    f.render_widget(Paragraph::new(lines), inner);
    true
}

fn draw_worktree_detail(f: &mut Frame, app: &App, area: Rect, w: &OtherWorktree) {
    let block = panel(
        vec![
            Span::styled(w.name.clone(), title()),
            Span::styled(" · git worktree", muted()),
        ],
        false,
    );
    let inner = block.inner(area).inner(Margin::new(2, 1));
    f.render_widget(block, area);
    let iw = inner.width as usize;
    let vw = iw.saturating_sub(8);
    let sep = || Span::styled(" · ", muted());
    let mut lines = vec![];
    lines.push(kv(
        "branch",
        match &w.branch {
            Some(b) => vec![Span::styled(trunc(b, vw), fg(PLUM))],
            None => vec![Span::styled("detached", muted())],
        },
    ));
    if let Some(h) = &w.head {
        lines.push(kv("head", vec![Span::styled(fmt::short_commit(h), muted())]));
    }
    let mut status = if let Some(j) = app.busy_worktree(&w.path) {
        vec![Span::styled(
            format!("{} {}", spinner(app.now_ms), j.verb()),
            fg(BOTTLE),
        )]
    } else if w.missing {
        vec![Span::styled("folder is gone", fg(AMBER))]
    } else {
        match w.dirty {
            Some(true) => vec![Span::styled("dirty", fg(AMBER))],
            Some(false) => vec![Span::styled("clean", muted())],
            None => vec![Span::styled("·", muted())],
        }
    };
    if w.locked {
        status.extend([sep(), Span::styled("locked", muted())]);
    }
    if let Some(n) = w.detached_commits.filter(|n| *n > 0) {
        status.extend([
            sep(),
            Span::styled(format!("{n} commits on no branch"), fg(AMBER)),
        ]);
    }
    lines.push(kv("status", status));
    if let Some(r) = &w.lock_reason {
        lines.push(kv("lock", vec![Span::styled(trunc(r, vw), muted())]));
    }
    if let Some(p) = &w.pr {
        let (word, st) = match p.state {
            clove_core::PrState::Merged => ("merged", fg(SAGE)),
            clove_core::PrState::Open => ("open", Style::new()),
            clove_core::PrState::Closed => ("closed", muted()),
        };
        lines.push(kv("pr", vec![Span::styled(format!("#{} {word}", p.number), st)]));
    }
    lines.push(kv("path", vec![Span::raw(trunc_left(&fmt::tilde(&w.path), vw))]));
    lines.push(Line::from(""));
    for l in fmt::wrap(
        "a plain git worktree clove didn't make: no image, no warm deps",
        iw,
    ) {
        lines.push(Line::from(Span::styled(l, muted())));
    }
    lines.push(Line::from(vec![
        Span::styled("c", key()),
        Span::styled(" makes it a clone on its branch · ", muted()),
        Span::styled("d", key()),
        Span::styled(" removes it", muted()),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_detail(f: &mut Frame, app: &mut App, area: Rect) {
    if let Some(w) = app.selected_worktree().cloned() {
        draw_worktree_detail(f, app, area, &w);
        return;
    }
    let Some(c) = app.selected_clone().cloned() else {
        if draw_golden_detail(f, app, area) {
            return;
        }
        let block = panel(vec![Span::styled("details", title())], false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        empty_state(
            f,
            inner,
            vec![Line::from(Span::styled("select a clone", muted()))],
        );
        return;
    };
    // The title sits between the corners and its padding.
    let (name, name_tip) = cut(&c.name, area.width.saturating_sub(6) as usize);
    tip(
        app,
        Rect::new(area.x + 2, area.y, width(&name) as u16, 1).intersection(area),
        name_tip,
    );
    let block = panel(vec![Span::styled(name, title())], false);
    let inner = block.inner(area).inner(Margin::new(2, 1));
    f.render_widget(block, area);
    let w = inner.width as usize;
    let vw = w.saturating_sub(8);
    let sep = || Span::styled(" · ", muted());

    let mut lines = vec![];
    let mut tips = SpanTips::default();
    let mut branch = match &c.branch {
        Some(b) => {
            let (shown, t) = cut(b, vw);
            tips.add(0, 1, t);
            vec![Span::styled(shown, fg(PLUM))]
        }
        None => vec![Span::styled("detached", muted())],
    };
    if c.mounted {
        if let Some(a) = c.ahead.filter(|a| *a > 0) {
            branch.push(Span::styled(format!("  ↑{a}"), fg(SAGE)));
        }
        if let Some(b) = c.behind.filter(|b| *b > 0) {
            branch.push(Span::styled(format!("  ↓{b}"), muted()));
        }
    }
    lines.push(kv("branch", branch));
    if let Some(h) = &c.head {
        tips.add(lines.len(), 1, Some(Tip::new(h.clone())));
        lines.push(kv("head", vec![Span::styled(fmt::short_commit(h), muted())]));
    }
    let mut status = if let Some(j) = app.busy(&c.name) {
        vec![Span::styled(
            format!("{} {}", spinner(app.now_ms), j.verb()),
            fg(BOTTLE),
        )]
    } else if c.mounted {
        let mut s = vec![Span::styled("● mounted", fg(SAGE))];
        match c.dirty {
            Some(true) => s.extend([sep(), Span::styled("dirty", fg(AMBER))]),
            Some(false) => s.extend([sep(), Span::styled("clean", muted())]),
            None => {}
        }
        s
    } else {
        vec![Span::styled("○ unmounted", muted())]
    };
    status.extend([sep(), Span::raw(fmt::bytes(c.own_bytes))]);
    lines.push(kv("status", status));
    let created = fmt::age(app.now_unix.saturating_sub(c.created_at));
    let mut golden = vec![
        Span::raw(format!("gen {}", c.golden_generation)),
        sep(),
        Span::styled(format!("{created} old"), muted()),
    ];
    if let Some(g) = app.current().and_then(|d| d.status.golden.as_ref())
        && g.meta.generation > c.golden_generation
    {
        golden.push(sep());
        tips.add(
            lines.len(),
            golden.len() + 1,
            Some(Tip::new(format!(
                "Made from golden gen {}; new clones start from gen {}.",
                c.golden_generation, g.meta.generation
            ))),
        );
        golden.push(Span::styled(
            format!("golden now {}", g.meta.generation),
            fg(AMBER),
        ));
    }
    lines.push(kv("from", golden));
    let (path, path_tip) = cut_left(&fmt::tilde(&c.path), vw);
    tips.add(lines.len(), 1, path_tip);
    lines.push(kv("path", vec![Span::raw(path)]));

    if !c.timings.is_empty() || !c.failed_steps.is_empty() {
        lines.push(Line::from(""));
        let nw = timing_lines(&mut lines, &mut tips, &c.timings, w);
        for s in &c.failed_steps {
            let (step, step_tip) = cut(&format!("setup {s}"), nw);
            tips.add(lines.len(), 1, step_tip);
            lines.push(Line::from(vec![
                Span::styled("✗ ", fg(RED)),
                Span::styled(pad(&step, nw), fg(RED)),
                Span::styled(" failed", fg(RED)),
            ]));
            if let Some(d) = app.current() {
                let log = d.status.state.join("logs").join(format!("{}-{s}.log", c.name));
                let (shown, t) = cut_left(&fmt::tilde(&log), w.saturating_sub(2));
                tips.add(lines.len(), 1, t);
                lines.push(Line::from(vec![Span::raw("  "), Span::styled(shown, muted())]));
            }
        }
    }
    tips.place(&lines, inner, 0, &mut app.tips);
    f.render_widget(Paragraph::new(lines), inner);
}

// ---- footer ------------------------------------------------------------

pub(crate) type Hint = (&'static str, &'static str, KeyCode);

fn hints(app: &App) -> Vec<Hint> {
    use KeyCode::*;
    match &app.modal {
        Some(Modal::New(_)) => vec![
            ("⏎", "create", Enter),
            ("tab", "complete", Tab),
            ("↑↓", "pick", Down),
            ("⇧tab", "field", BackTab),
            ("esc", "cancel", Esc),
        ],
        Some(Modal::ResolveOffer(_)) => vec![("y", "resolve", Char('y')), ("esc", "no", Esc)],
        Some(Modal::Adopt(a)) => vec![
            if a.focus_cancel {
                ("⏎", "cancel", Enter)
            } else {
                ("⏎", "make clone", Enter)
            },
            ("←→", "move", Right),
            ("esc", "cancel", Esc),
        ],
        Some(Modal::Remove(f)) => {
            let mut v = vec![("↑↓", "move", Down)];
            if matches!(
                f.focused(),
                RemoveItem::DeleteBranch | RemoveItem::Unlock | RemoveItem::Force
            ) {
                v.push(("space", "toggle", Char(' ')));
            }
            v.push(if f.focused() == RemoveItem::Cancel {
                ("⏎", "cancel", Enter)
            } else {
                ("⏎", "remove", Enter)
            });
            v.push(("esc", "cancel", Esc));
            v
        }
        Some(Modal::Job(id)) => {
            let mut v = vec![];
            if let Some(j) = app.job(*id)
                && matches!(j.result, Some(Ok(_)))
                && j.path.is_some()
            {
                v.push(("⏎", "cd into it", Enter));
            }
            v.extend([
                ("↑↓", "scroll", Down),
                ("end", "follow", End),
                ("esc", "hide", Esc),
            ]);
            v
        }
        Some(Modal::Gc(g)) => {
            let mut v = vec![];
            if !g.running && !g.applied && g.result.as_ref().is_some_and(crate::app::cleanup_has_work) {
                v.push(("←→", "move", Right));
                v.push(if g.focus_close {
                    ("⏎", "close", Enter)
                } else {
                    ("⏎", "remove", Enter)
                });
            }
            v.push(("esc", "close", Esc));
            v
        }
        Some(Modal::Diff(_)) => {
            let p = app.diff_prefs;
            vec![
                ("↑↓", "file", Down),
                ("space", "page", Char(' ')),
                ("⏎", "open", Enter),
                ("s", if p.split { "unified" } else { "split" }, Char('s')),
                ("w", if p.wrap { "no wrap" } else { "wrap" }, Char('w')),
                ("o", if p.by_changes { "by path" } else { "by size" }, Char('o')),
                ("g", if p.grouped { "ungroup" } else { "packages" }, Char('g')),
                ("e", "edit", Char('e')),
                ("esc", "back", Esc),
            ]
        }
        Some(Modal::Doctor(_)) | Some(Modal::Removals(_)) | Some(Modal::Stats(_)) => {
            vec![("↑↓", "scroll", Down), ("esc", "close", Esc)]
        }
        Some(Modal::Settings(sv)) => {
            let actions = app.settings_actions();
            if let Some(e) = &sv.editing {
                let mut v = vec![];
                if matches!(FIELDS[e.field].kind, Kind::Choice { .. }) {
                    v.push(("←→", "choose", Right));
                }
                v.extend([("⏎", "save", Enter), ("esc", "cancel", Esc)]);
                return v;
            }
            let on = actions[sv.cursor.min(actions.len() - 1)];
            let what = match on {
                SettingsAction::Edit(_) => "open file",
                SettingsAction::Field(i) if FIELDS[i].kind == Kind::Bool => "toggle",
                SettingsAction::Field(_) => "edit",
                SettingsAction::Service if app.service == Some(true) => "turn off",
                SettingsAction::Service => "turn on",
                SettingsAction::Agents if app.agents == Some(clove_core::agents::State::Installed) => {
                    "remove"
                }
                SettingsAction::Agents if app.agents == Some(clove_core::agents::State::Outdated) => "update",
                SettingsAction::Agents => "install",
                SettingsAction::Shell => "install",
            };
            let mut v = vec![("↑↓", "move", Down), ("⏎", what, Enter)];
            if matches!(on, SettingsAction::Field(_)) {
                v.push(("x", "reset", Char('x')));
            }
            v.extend([("←→", "save to", Right), ("esc", "close", Esc)]);
            v
        }
        Some(Modal::Help) => vec![("esc", "close", Esc)],
        Some(Modal::AddRepo(v)) => crate::add_repo::hints(v),
        None if app.filtering => vec![("⏎", "keep", Enter), ("esc", "clear", Esc), ("↑↓", "move", Down)],
        None => {
            let mut v: Vec<Hint> = vec![];
            if app.current().is_some() && !app.has_golden() {
                v.push(("i", "init", Char('i')));
            } else if app.selected_worktree().is_some() {
                v.extend([
                    ("⏎", "cd", Enter),
                    ("c", "make clone", Char('c')),
                    ("d", "remove", Char('d')),
                    ("n", "new", Char('n')),
                    ("o", "open", Char('o')),
                    ("/", "filter", Char('/')),
                ]);
            } else if let Some(c) = app.selected_clone() {
                v.push(("⏎", "cd", Enter));
                // Early in the bar, so a narrow terminal still shows it.
                if c.mounted {
                    v.push(("v", "changes", Char('v')));
                }
                v.extend([("n", "new", Char('n')), ("d", "remove", Char('d'))]);
                v.push(if c.mounted {
                    ("u", "unmount", Char('u'))
                } else {
                    ("m", "mount", Char('m'))
                });
                if c.operation.is_some() {
                    v.extend([
                        ("R", "resolve", Char('R')),
                        ("U", "continue", Char('U')),
                        ("X", "abort", Char('X')),
                    ]);
                } else {
                    v.push(("U", "update", Char('U')));
                }
                v.extend([
                    ("r", "refresh", Char('r')),
                    ("o", "open", Char('o')),
                    ("/", "filter", Char('/')),
                ]);
            } else if app.current().is_some() {
                v.extend([("n", "new", Char('n')), ("r", "refresh", Char('r'))]);
                if !app.filter.is_empty() {
                    v.push(("esc", "clear filter", Esc));
                }
            }
            if app.repos.len() > 1 {
                v.push(("tab", "repo", Tab));
            }
            v.extend([
                ("s", "settings", Char('s')),
                ("g", "cleanup", Char('g')),
                ("L", "log", Char('L')),
                ("t", "timings", Char('t')),
                ("D", "doctor", Char('D')),
                ("A", "add repo", Char('A')),
            ]);
            v
        }
    }
}

fn draw_footer(f: &mut Frame, app: &mut App, area: Rect) {
    // Right: what is running.
    let running: Vec<&Job> = app.running().collect();
    let status = match running.as_slice() {
        [] if app.loading && app.current().is_none() => {
            Line::from(Span::styled(format!("{} loading ", spinner(app.now_ms)), muted()))
        }
        [] => upgrade_badge(app),
        _ if app.quitting => Line::from(vec![
            Span::styled(
                format!("{} finishing before quit · ", spinner(app.now_ms)),
                fg(AMBER),
            ),
            Span::styled("q", key()),
            Span::styled(" now ", muted()),
        ]),
        [j, rest @ ..] => {
            let mut s = vec![
                Span::styled(format!("{} ", spinner(app.now_ms)), fg(BOTTLE)),
                Span::raw(j.title()),
            ];
            if let Some(step) = j.current_step() {
                s.push(Span::styled(format!(" · {}", step.name), muted()));
            }
            if !rest.is_empty() {
                s.push(Span::styled(format!(" +{}", rest.len()), muted()));
            }
            s.push(Span::raw(" "));
            Line::from(s)
        }
    };
    let full = line_width(&status) as u16;
    let sw = full.min(area.width / 2);
    if sw > 0 {
        let r = Rect::new(area.right() - sw, area.y, sw, 1);
        put(f, r, status.clone());
        let badge = running.is_empty() && (app.upgraded_to.is_some() || app.upgrade_available().is_some());
        let t = if badge {
            app.hits
                .push((r, Hit::Key(KeyCode::Char('K'), KeyModifiers::NONE)));
            Some(match &app.upgraded_to {
                Some(v) => Tip::new(format!("clove {v} is installed; this TUI still runs the old one")),
                None => Tip::new("upgrade clove with Homebrew, then restart the service").note("K upgrades"),
            })
        } else if running.is_empty() {
            None
        } else {
            let jobs: Vec<String> = running
                .iter()
                .map(|j| match j.current_step() {
                    Some(s) => format!("{} · {}", j.title(), s.name),
                    None => j.title(),
                })
                .collect();
            Some(Tip::new(jobs.join("\n")).note("l watches the last one"))
        };
        tip(app, r, t);
    }

    let mut list = hints(app);
    let tail: Vec<Hint> = if app.modal.is_none() && !app.filtering {
        vec![
            ("?", "help", KeyCode::Char('?')),
            ("q", "quit", KeyCode::Char('q')),
        ]
    } else {
        vec![]
    };
    let avail = area.width.saturating_sub(sw + 1) as usize;
    let hw = |h: &Hint| width(h.0) + 1 + width(h.1) + 2;
    let tail_w: usize = tail.iter().map(hw).sum();
    let mut used = 1 + tail_w;
    let mut keep = 0;
    for h in &list {
        if used + hw(h) > avail {
            break;
        }
        used += hw(h);
        keep += 1;
    }
    list.truncate(keep);
    list.extend(tail);
    let mut x = area.x + 1;
    for (k, label, code) in list {
        let w = (width(k) + 1 + width(label)) as u16;
        if x + w > area.right() {
            break;
        }
        let r = Rect::new(x, area.y, w, 1);
        put(
            f,
            r,
            Line::from(vec![
                Span::styled(k, key()),
                Span::raw(" "),
                Span::styled(label, muted()),
            ]),
        );
        let mods = if code == KeyCode::BackTab {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        app.hits.push((r, Hit::Key(code, mods)));
        tip(app, r, hint_tip(k, label).map(Tip::new));
        x += w + 2;
    }
}

/// Right of the footer when nothing runs: a newer clove, or one installed
/// that this process isn't running yet.
fn upgrade_badge(app: &App) -> Line<'static> {
    if let Some(v) = &app.upgraded_to {
        return Line::from(Span::styled(
            format!("clove {v} installed · restart to use it "),
            fg(AMBER),
        ));
    }
    match app.upgrade_available() {
        Some(v) => Line::from(vec![
            Span::styled(format!("↑ clove {v} "), fg(BOTTLE)),
            Span::styled("K", key()),
            Span::styled(" upgrade ", muted()),
        ]),
        None => Line::from(""),
    }
}

/// The longer story of a footer hint, where the label is terse.
fn hint_tip(k: &str, label: &str) -> Option<&'static str> {
    Some(match (k, label) {
        ("⏎", "cd") => "cd into the clone and quit (mounts it first)",
        ("n", "new") => "new clone on a branch, from the golden image",
        ("d", "remove") => "remove the clone (asks first)",
        ("m", "mount") => "mount the clone's disk image",
        ("u", "unmount") => "unmount the clone; its files stay in the image",
        ("U", "update") => "bring the clone onto the latest base, then rerun changed setup steps",
        ("U", "continue") => "finish the update once the conflicts are fixed and added",
        ("R", "resolve") => {
            "hand the conflicts to the resolver (update.resolver, claude by default), in this terminal"
        }
        ("X", "abort") => "undo the stopped update: the branch goes back where it was",
        ("r", "refresh") => "refresh the golden image to the latest base",
        ("o", "open") => "open the clone in $EDITOR",
        ("/", "filter") => "filter clones by name or branch",
        ("i", "init") => "build the golden image: a checkout of the base with deps warmed",
        ("tab", "repo") => "next repository (1-9 jumps, [ ] steps)",
        ("s", "settings") => "settings, config files, service, shell integration",
        ("g", "cleanup") => "remove clones whose PR is merged, and leftovers",
        ("D", "doctor") => "check the setup: disk images, git, steps, golden image",
        ("?", "help") => "every key",
        ("end", "follow") => "follow the output as it comes",
        ("x", "reset") => "drop this setting from the chosen file",
        ("←→", "save to") => "which config file edits go to",
        _ => return None,
    })
}

// ---- toasts ------------------------------------------------------------

/// Bottom-right corner of the body, newest at the bottom, older ones stacked
/// above it.
fn draw_toasts(f: &mut Frame, app: &mut App, body: Rect) {
    let mut bottom = body.bottom().saturating_sub(1);
    let maxw = 60.min(body.width.saturating_sub(4)) as usize;
    let toasts: Vec<Toast> = app.toasts.iter().rev().cloned().collect();
    let n = toasts.len();
    for (ri, t) in toasts.into_iter().enumerate() {
        let (icon, color) = match t.kind {
            ToastKind::Ok => ("✓", SAGE),
            ToastKind::Info => ("◆", BOTTLE),
            ToastKind::Warn => ("!", AMBER),
            ToastKind::Err => ("✗", RED),
        };
        let all = fmt::wrap(&t.text, maxw.saturating_sub(6));
        let cut_off = all.len() > 3 || all.iter().any(|l| l.ends_with('…'));
        let lines: Vec<String> = all.into_iter().take(3).collect();
        let tw = lines.iter().map(|l| width(l)).max().unwrap_or(0) + 6;
        let h = lines.len() as u16 + 2;
        if bottom < body.y + h {
            break;
        }
        let r = Rect::new(body.right() - tw as u16 - 2, bottom - h, tw as u16, h);
        f.render_widget(Clear, r);
        occlude(app, r);
        tip(
            app,
            r,
            cut_off.then(|| Tip::new(t.text.clone()).note("click dismisses")),
        );
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(fg(color));
        let inner = block.inner(r).inner(Margin::new(1, 0));
        f.render_widget(block, r);
        let content: Vec<Line> = lines
            .into_iter()
            .enumerate()
            .map(|(i, l)| {
                Line::from(vec![
                    Span::styled(
                        if i == 0 { format!("{icon} ") } else { "  ".into() },
                        fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(l),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(content), inner);
        app.hits.push((r, Hit::Toast(n - 1 - ri)));
        bottom -= h;
    }
}

// ---- modals ------------------------------------------------------------

fn draw_modal(f: &mut Frame, app: &mut App, area: Rect, list: Rect) {
    let Some(modal) = app.modal.clone() else {
        return;
    };
    // Centered modals cover everything but the footer.
    if !matches!(modal, Modal::New(_)) {
        occlude(
            app,
            Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1)),
        );
    }
    match modal {
        Modal::New(form) => draw_new(f, app, list, &form),
        Modal::Help => draw_help(f, app, area),
        Modal::Remove(r) => draw_remove(f, app, area, &r),
        Modal::Adopt(a) => draw_adopt(f, app, area, &a),
        Modal::ResolveOffer(o) => draw_resolve_offer(f, app, area, &o),
        Modal::Gc(g) => draw_gc(f, app, area, &g),
        Modal::Doctor(d) => draw_doctor(f, app, area, d),
        Modal::Removals(v) => draw_removals(f, app, area, v),
        Modal::Stats(v) => draw_stats(f, app, area, v),
        Modal::Settings(v) => draw_settings(f, app, area, v),
        Modal::Diff(mut v) => {
            crate::diffview::draw(f, app, area, &mut v);
            // Drawing settles scroll and rows: keep them for the next frame.
            if matches!(app.modal, Some(Modal::Diff(_))) {
                app.modal = Some(Modal::Diff(v));
            }
        }
        Modal::Job(id) => draw_job(f, app, area, id),
        Modal::AddRepo(v) => crate::add_repo::draw(f, app, area, &v),
    }
}

pub(crate) fn modal_block<'a>(title_spans: Vec<Span<'a>>) -> Block<'a> {
    panel(title_spans, true).padding(ratatui::widgets::Padding::new(2, 2, 1, 1))
}

/// Dims everything behind a centered modal and makes a click outside it close
/// it (the footer stays clickable).
pub(crate) fn open_modal(f: &mut Frame, hits: &mut Vec<(Rect, Hit)>, area: Rect, rect: Rect) {
    dim_backdrop(f, area);
    f.render_widget(Clear, rect);
    let above_footer = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    hits.push((above_footer, Hit::Dismiss));
    hits.push((rect, Hit::Inert));
}

pub(crate) fn key_hit(c: char) -> Hit {
    Hit::Key(KeyCode::Char(c), KeyModifiers::NONE)
}

/// A button that can't act right now: muted, no fill, even when focused.
pub(crate) fn button_off<'a>(label: &str) -> Span<'a> {
    Span::styled(format!(" {label} "), muted().add_modifier(Modifier::DIM))
}

pub(crate) fn button<'a>(label: &str, focused: bool, danger: bool) -> Span<'a> {
    let s = format!(" {label} ");
    if focused {
        Span::styled(
            s,
            Style::new()
                .bg(if danger { RED } else { BOTTLE })
                .fg(IVORY)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(s, muted().add_modifier(Modifier::REVERSED))
    }
}

fn draw_new(f: &mut Frame, app: &mut App, list: Rect, form: &NewForm) {
    // Inline composer at the bottom of the clone list.
    let h = 4;
    if list.height < h + 2 {
        return;
    }
    let r = Rect::new(list.x + 1, list.bottom() - 1 - h, list.width - 2, h);
    f.render_widget(Clear, r);
    occlude(app, r);
    let block = panel(vec![Span::styled("new clone", title())], true);
    let inner = block.inner(r).inner(Margin::new(1, 0));
    f.render_widget(block, r);

    let bw = (inner.width as usize * 3 / 5) as u16;
    let fields = [
        ("branch", &form.branch, Rect::new(inner.x, inner.y, bw, 1)),
        (
            "base",
            &form.base,
            Rect::new(inner.x + bw + 2, inner.y, inner.width.saturating_sub(bw + 2), 1),
        ),
    ];
    for (i, (label, input, fr)) in fields.iter().enumerate() {
        let focus = form.focus == i;
        let lstyle = if focus { key() } else { muted() };
        let prefix = format!("{label} ❯ ");
        let pw = width(&prefix) as u16;
        let tw = fr.width.saturating_sub(pw + 1) as usize;
        let shown = if input.text.is_empty() && i == 0 {
            Span::styled(trunc("(generated)", tw), muted())
        } else {
            let (s, t) = cut_left(&input.text, tw);
            tip(
                app,
                Rect::new(fr.x + pw, fr.y, width(&s) as u16, 1).intersection(*fr),
                t,
            );
            Span::styled(s, if focus { bold() } else { Style::new() })
        };
        put(f, *fr, Line::from(vec![Span::styled(prefix, lstyle), shown]));
        app.hits.push((*fr, Hit::Field(i)));
        if focus {
            let before: String = input.text.chars().take(input.cursor).collect();
            let cx = (width(&before).min(tw)) as u16;
            f.set_cursor_position(Position::new(fr.x + pw + cx, fr.y));
        }
    }
    let (preview, err) = app.new_preview(form);
    let pr = row(inner, 1);
    let pl = match err {
        Some(e) => {
            let (s, t) = cut(&format!("✗ {e}"), inner.width as usize);
            tip(app, pr, t);
            Line::from(Span::styled(s, fg(RED)))
        }
        None => {
            let (s, t) = cut(&preview, (inner.width as usize).saturating_sub(2));
            tip(app, pr, t);
            Line::from(vec![Span::styled("→ ", fg(BOTTLE)), Span::styled(s, muted())])
        }
    };
    put(f, row(inner, 1), pl);

    // Suggestions pop up above the focused field.
    let sugg = app.suggestions(form);
    if sugg.is_empty() {
        return;
    }
    let field = fields[form.focus].2;
    let q = form.field().text.to_lowercase();
    let tagw = 7;
    let sw = sugg.iter().map(|s| width(s)).max().unwrap_or(0) + tagw + 6;
    let sw = (sw as u16).max(28).min(list.width.saturating_sub(4));
    let sh = sugg.len() as u16 + 2;
    if r.y < list.y + sh {
        return;
    }
    let x = (field.x.saturating_sub(2)).min(list.right().saturating_sub(sw + 1));
    let sr = Rect::new(x, r.y - sh, sw, sh);
    f.render_widget(Clear, sr);
    occlude(app, sr);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border());
    let si = block.inner(sr).inner(Margin::new(1, 0));
    f.render_widget(block, sr);
    for (i, s) in sugg.iter().enumerate() {
        let rr = row(si, i as u16);
        let tag = ref_tag(app, s, form.focus);
        let nw = (si.width as usize).saturating_sub(tagw + 1);
        let (shown, t) = cut(s, nw);
        tip(app, Rect::new(rr.x, rr.y, nw as u16, 1).intersection(rr), t);
        let mut spans = highlight(&shown, &q);
        spans.push(Span::raw(" ".repeat(nw.saturating_sub(width(&shown)) + 1)));
        spans.push(Span::styled(format!("{tag:>tagw$}"), muted()));
        put(f, rr, Line::from(spans));
        if form.pick == Some(i) {
            f.buffer_mut().set_style(rr, selected());
        }
        app.hits.push((rr, Hit::Suggestion(i)));
    }
}

/// Bolds the first case-insensitive match of `q` in `s`.
fn highlight<'a>(s: &str, q: &str) -> Vec<Span<'a>> {
    if q.is_empty() {
        return vec![Span::raw(s.to_string())];
    }
    let lower = s.to_lowercase();
    match lower.find(q) {
        // Lowercasing can change byte lengths for some scripts; only split when safe.
        Some(i) if lower.len() == s.len() && s.is_char_boundary(i + q.len()) => vec![
            Span::raw(s[..i].to_string()),
            Span::styled(s[i..i + q.len()].to_string(), key()),
            Span::raw(s[i + q.len()..].to_string()),
        ],
        _ => vec![Span::raw(s.to_string())],
    }
}

fn ref_tag(app: &App, name: &str, focus: usize) -> &'static str {
    let has = |k: RefKind, n: &str| app.refs.iter().any(|r| r.kind == k && r.name == n);
    if focus == 0 {
        match app.candidates.iter().find(|b| b.name == name) {
            Some(b) if b.remote => "origin",
            _ => "local",
        }
    } else if has(RefKind::Local, name) {
        "local"
    } else if has(RefKind::Tag, name) {
        "tag"
    } else {
        "remote"
    }
}

fn draw_help(f: &mut Frame, app: &mut App, area: Rect) {
    let sections: [(&str, &[(&str, &str)]); 4] = [
        (
            "clones",
            &[
                ("⏎", "cd into it and quit"),
                ("n", "new clone on a branch"),
                ("d", "remove (asks first)"),
                ("m / u", "mount / unmount"),
                ("U / X", "update to latest base / abort"),
                ("o", "open in $EDITOR"),
                ("v", "see its changes (diff)"),
                ("t", "how fast clones come (timings)"),
                ("c", "make a plain git worktree a clone"),
                ("/", "filter by name or branch"),
                ("↑↓ jk", "move · wheel scrolls"),
            ],
        ),
        (
            "golden image",
            &[
                ("r", "refresh to latest base"),
                ("i", "build it (first time)"),
                ("l", "watch the last operation"),
            ],
        ),
        (
            "anywhere",
            &[
                ("tab [ ] 1-9", "switch repository"),
                ("A", "add a repo clove doesn't manage yet"),
                ("s", "settings, service"),
                ("g", "cleanup: merged clones/worktrees"),
                ("L", "log: what clove removed, and why"),
                ("D", "doctor"),
                ("K", "check for a new clove / upgrade"),
                ("ctrl-r", "reload now"),
                ("q esc", "quit"),
            ],
        ),
        (
            "panels",
            &[
                ("< > / { }", "move the detail / sidebar border"),
                ("=", "reset sizes · or double-click"),
            ],
        ),
    ];
    let mut lines = vec![];
    for (i, (name, keys)) in sections.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            name.to_uppercase(),
            muted().add_modifier(Modifier::BOLD),
        )));
        for (k, d) in keys.iter() {
            lines.push(Line::from(vec![
                Span::styled(format!("{k:>12}"), key()),
                Span::raw("   "),
                Span::raw(*d),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "rows, tabs and hints are clickable; borders drag",
        muted(),
    )));
    let h = lines.len() as u16 + 4;
    let rect = centered(area, 56, h);
    open_modal(f, &mut app.hits, area, rect);
    let block = modal_block(vec![Span::styled("keys", title())]);
    f.render_widget(Paragraph::new(lines).block(block), rect);
}

fn draw_remove(f: &mut Frame, app: &mut App, area: Rect, r: &RemoveForm) {
    let w: u16 = 62;
    let iw = (w - 6) as usize;
    let mut lines = vec![];
    let mut tips = SpanTips::default();
    let bw = r.branch.as_deref().map(width).unwrap_or(0);
    let name_room = if bw > 0 {
        iw.saturating_sub(5 + bw.min(iw / 2))
    } else {
        iw
    };
    let (name, name_tip) = cut(&r.name, name_room);
    tips.add(0, 0, name_tip);
    let name_w = width(&name);
    let mut head = vec![Span::styled(name, bold())];
    if let Some(b) = &r.branch {
        let (b, t) = cut(b, iw.saturating_sub(name_w + 5));
        tips.add(0, 2, t);
        head.push(Span::styled("  on ", muted()));
        head.push(Span::styled(b, fg(PLUM)));
    }
    lines.push(Line::from(head));
    let (path, path_tip) = cut_left(&fmt::tilde(&r.path), iw);
    tips.add(1, 0, path_tip);
    lines.push(Line::from(Span::styled(path, muted())));
    lines.push(Line::from(""));
    if r.worktree {
        lines.push(Line::from(Span::styled(
            "plain git worktree: removed with `git worktree remove`",
            muted(),
        )));
        let gone = || {
            Line::from(Span::styled(
                "· its folder is gone: removing drops git's record of it",
                muted(),
            ))
        };
        match &r.blocker {
            Some(_) if r.forceable => {
                if r.locked {
                    let why = r.lock_reason.as_deref().unwrap_or("no reason given");
                    for l in fmt::wrap(&format!("! locked: {why}"), iw) {
                        lines.push(Line::from(Span::styled(
                            l,
                            fg(AMBER).add_modifier(Modifier::BOLD),
                        )));
                    }
                }
                if !r.mounted {
                    lines.push(gone());
                }
            }
            _ => {}
        }
        match &r.blocker {
            Some(_) if r.forceable && r.dirty != Some(true) => {}
            Some(_) if r.forceable => {
                let n = r.changes_total;
                let count = if n == 0 {
                    "uncommitted changes".to_string()
                } else {
                    format!("{n} uncommitted change{}", if n == 1 { "" } else { "s" })
                };
                lines.push(Line::from(Span::styled(
                    format!(
                        "! {count}{}{}",
                        if r.force { ", discarded on remove" } else { "" },
                        if n == 0 { "" } else { ":" }
                    ),
                    fg(AMBER).add_modifier(Modifier::BOLD),
                )));
                let shown = r.changes.len().min(6);
                for c in &r.changes[..shown] {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", trunc(c, iw.saturating_sub(2))),
                        muted(),
                    )));
                }
                if n > shown {
                    lines.push(Line::from(Span::styled(
                        format!("  and {} more", n - shown),
                        muted(),
                    )));
                }
            }
            Some(b) => {
                for l in fmt::wrap(&format!("✗ {b}"), iw) {
                    lines.push(Line::from(Span::styled(
                        l,
                        fg(AMBER).add_modifier(Modifier::BOLD),
                    )));
                }
            }
            None if !r.mounted => lines.push(gone()),
            None => lines.push(Line::from(Span::styled("✓ working tree is clean", fg(SAGE)))),
        }
    } else {
        match (r.mounted, r.dirty) {
            (true, Some(true)) => lines.push(Line::from(Span::styled(
                "! uncommitted changes will be discarded",
                fg(AMBER).add_modifier(Modifier::BOLD),
            ))),
            (true, Some(false)) => lines.push(Line::from(Span::styled("✓ working tree is clean", fg(SAGE)))),
            (true, None) => lines.push(Line::from(Span::styled("· working tree state unknown", muted()))),
            (false, _) => lines.push(Line::from(Span::styled(
                "○ not mounted: anything uncommitted inside the image goes too",
                muted(),
            ))),
        }
    }
    if let Some(a) = r.ahead.filter(|a| *a > 0) {
        let s = format!(
            "{} {a} commit{} not on {}",
            if r.delete_branch { "!" } else { "·" },
            if a == 1 { "" } else { "s" },
            r.base
        );
        let (s, t) = cut(&s, iw);
        tips.add(lines.len(), 0, t);
        lines.push(Line::from(Span::styled(
            s,
            if r.delete_branch { fg(AMBER) } else { muted() },
        )));
    }
    if r.inside {
        let what = if r.worktree { "worktree" } else { "clone" };
        for l in fmt::wrap(
            &format!(
                "you're in this {what}: clove quits, moves your shell to the main checkout, then removes it"
            ),
            iw,
        ) {
            lines.push(Line::from(Span::styled(l, muted())));
        }
    }
    lines.push(Line::from(""));
    let focused = r.focused();
    // `›` marks the focused checkbox; space toggles it.
    let check = |item: RemoveItem, on: bool| {
        let cursor = if focused == item {
            Span::styled("› ", fg(BOTTLE).add_modifier(Modifier::BOLD))
        } else {
            Span::raw("  ")
        };
        let mark = if on {
            Span::styled("[x] ", key())
        } else {
            Span::styled("[ ] ", muted())
        };
        (cursor, mark, if focused == item { bold() } else { Style::new() })
    };
    let mut toggles = vec![];
    if let Some(b) = &r.branch {
        toggles.push((lines.len(), RemoveItem::DeleteBranch));
        let (cursor, mark, st) = check(RemoveItem::DeleteBranch, r.delete_branch);
        let (b, t) = cut(b, iw.saturating_sub(30));
        tips.add(lines.len(), 3, t);
        lines.push(Line::from(vec![
            cursor,
            mark,
            Span::styled("also delete branch ", st),
            Span::styled(b, fg(PLUM)),
        ]));
    }
    if r.worktree && r.forceable && r.locked {
        toggles.push((lines.len(), RemoveItem::Unlock));
        let (cursor, mark, st) = check(RemoveItem::Unlock, r.unlock);
        lines.push(Line::from(vec![
            cursor,
            mark,
            Span::styled("unlock it (git worktree unlock)", st),
        ]));
    }
    if !r.worktree || (r.forceable && r.dirty == Some(true)) {
        toggles.push((lines.len(), RemoveItem::Force));
        let (cursor, mark, st) = check(RemoveItem::Force, r.force);
        let label = if r.worktree {
            "discard these changes (git worktree remove --force)"
        } else {
            "force: discard changes, eject if busy"
        };
        lines.push(Line::from(vec![cursor, mark, Span::styled(label, st)]));
    }
    lines.push(Line::from(""));
    let danger = r.force && r.dirty == Some(true);
    let confirm = if danger { "discard & remove" } else { "remove" };
    let b1 = if r.blocked() {
        button_off(confirm)
    } else {
        button(confirm, focused == RemoveItem::Remove, danger)
    };
    let b2 = button("cancel", focused == RemoveItem::Cancel, false);
    let (b1w, b2w) = (width(&b1.content) as u16, width(&b2.content) as u16);
    let bw = (b1w + 2 + b2w) as usize;
    let btn_line = lines.len() as u16;
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(iw.saturating_sub(bw))),
        b1,
        Span::raw("  "),
        b2,
    ]));
    let h = lines.len() as u16 + 4;
    let rect = centered(area, w, h);
    open_modal(f, &mut app.hits, area, rect);
    let block = modal_block(vec![
        Span::styled(if r.worktree { "remove worktree " } else { "remove " }, title()),
        Span::styled(
            trunc(
                &r.name,
                (w as usize).saturating_sub(if r.worktree { 23 } else { 14 }),
            ),
            bold(),
        ),
    ]);
    let inner = block.inner(rect);
    tips.place(&lines, inner, 0, &mut app.tips);
    f.render_widget(Paragraph::new(lines).block(block), rect);
    for (i, item) in toggles {
        app.hits.push((row(inner, i as u16), Hit::RemoveRow(item)));
    }
    let bx = inner.x + (iw.saturating_sub(bw)) as u16;
    let by = inner.y + btn_line;
    if by < inner.bottom() {
        app.hits.push((Rect::new(bx, by, b1w, 1), key_hit('y')));
        app.hits.push((
            Rect::new(bx + b1w + 2, by, b2w, 1),
            Hit::Key(KeyCode::Esc, KeyModifiers::NONE),
        ));
    }
}

/// After an update stops on conflicts: hand them to the resolver?
fn draw_resolve_offer(f: &mut Frame, app: &mut App, area: Rect, o: &ResolveOffer) {
    let w: u16 = 66.min(area.width.saturating_sub(4));
    let iw = w.saturating_sub(6) as usize;
    let mut lines = vec![];
    let n = o.files.len();
    for l in fmt::wrap(
        &format!(
            "{}: {} stopped on conflicts in {n} file{}:",
            o.name,
            o.strategy.word(),
            if n == 1 { "" } else { "s" }
        ),
        iw,
    ) {
        lines.push(Line::from(Span::raw(l)));
    }
    for file in o.files.iter().take(6) {
        lines.push(Line::from(Span::styled(
            format!("  {}", trunc_left(file, iw.saturating_sub(2))),
            fg(AMBER),
        )));
    }
    if n > 6 {
        lines.push(Line::from(Span::styled(format!("  and {} more", n - 6), muted())));
    }
    lines.push(Line::from(""));
    for l in fmt::wrap(
        &format!(
            "{} runs in the clone, here: it resolves what's clear, then continues the update, and stops to ask about the rest. No leaves it stopped: R resolves later, U continues once fixed, X aborts.",
            o.who
        ),
        iw,
    ) {
        lines.push(Line::from(Span::styled(l, muted())));
    }
    lines.push(Line::from(""));
    let b1 = button(&format!("resolve with {}", o.who), !o.focus_no, false);
    let b2 = button("no", o.focus_no, false);
    let (b1w, b2w) = (width(&b1.content) as u16, width(&b2.content) as u16);
    let bw = (b1w + 2 + b2w) as usize;
    let btn_line = lines.len() as u16;
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(iw.saturating_sub(bw))),
        b1,
        Span::raw("  "),
        b2,
    ]));
    let h = lines.len() as u16 + 4;
    let rect = centered(area, w, h);
    open_modal(f, &mut app.hits, area, rect);
    let block = modal_block(vec![Span::styled("conflicts", title())]);
    let inner = block.inner(rect);
    f.render_widget(Paragraph::new(lines).block(block), rect);
    let bx = inner.x + (iw.saturating_sub(bw)) as u16;
    let by = inner.y + btn_line;
    if by < inner.bottom() {
        app.hits.push((Rect::new(bx, by, b1w, 1), key_hit('y')));
        app.hits.push((
            Rect::new(bx + b1w + 2, by, b2w, 1),
            Hit::Key(KeyCode::Esc, KeyModifiers::NONE),
        ));
    }
}

fn draw_adopt(f: &mut Frame, app: &mut App, area: Rect, a: &AdoptForm) {
    let w: u16 = 64;
    let iw = (w - 6) as usize;
    let mut lines = vec![];
    let mut head = vec![Span::styled(a.name.clone(), bold())];
    if let Some(b) = &a.branch {
        head.push(Span::styled("  on ", muted()));
        head.push(Span::styled(b.clone(), fg(PLUM)));
    }
    lines.push(Line::from(head));
    lines.push(Line::from(Span::styled(
        trunc_left(&fmt::tilde(&a.path), iw),
        muted(),
    )));
    lines.push(Line::from(""));
    for l in fmt::wrap(
        "makes a clove clone on the same branch, then removes this worktree with `git worktree remove`. Ignored files in it (build output, .env) go with it. If the clone fails, the worktree stays as it was.",
        iw,
    ) {
        lines.push(Line::from(Span::styled(l, muted())));
    }
    if a.changes > 0 && a.blocker.is_none() {
        lines.push(Line::from(""));
        let n = a.changes;
        for l in fmt::wrap(
            &format!(
                "→ its {n} uncommitted change{} move{} to the clone (through a git stash, untracked files too)",
                if n == 1 { "" } else { "s" },
                if n == 1 { "s" } else { "" }
            ),
            iw,
        ) {
            lines.push(Line::from(Span::styled(l, fg(SAGE))));
        }
    }
    if let Some(b) = &a.blocker {
        lines.push(Line::from(""));
        for l in fmt::wrap(&format!("✗ {b}"), iw) {
            lines.push(Line::from(Span::styled(
                l,
                fg(AMBER).add_modifier(Modifier::BOLD),
            )));
        }
    }
    lines.push(Line::from(""));
    let b1 = button("make clone", !a.focus_cancel, false);
    let b2 = button("cancel", a.focus_cancel, false);
    let (b1w, b2w) = (width(&b1.content) as u16, width(&b2.content) as u16);
    let bw = (b1w + 2 + b2w) as usize;
    let btn_line = lines.len() as u16;
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(iw.saturating_sub(bw))),
        b1,
        Span::raw("  "),
        b2,
    ]));
    let h = lines.len() as u16 + 4;
    let rect = centered(area, w, h);
    open_modal(f, &mut app.hits, area, rect);
    let block = modal_block(vec![
        Span::styled("make a clone of ", title()),
        Span::styled(a.name.clone(), bold()),
    ]);
    let inner = block.inner(rect);
    f.render_widget(Paragraph::new(lines).block(block), rect);
    let bx = inner.x + (iw.saturating_sub(bw)) as u16;
    let by = inner.y + btn_line;
    if by < inner.bottom() {
        app.hits.push((Rect::new(bx, by, b1w, 1), key_hit('y')));
        app.hits.push((
            Rect::new(bx + b1w + 2, by, b2w, 1),
            Hit::Key(KeyCode::Esc, KeyModifiers::NONE),
        ));
    }
}

fn draw_gc(f: &mut Frame, app: &mut App, area: Rect, g: &GcView) {
    use clove_core::Verdict;
    let w: u16 = 76.min(area.width.saturating_sub(4));
    let iw = w.saturating_sub(6) as usize;
    let mut apply_line = None;
    let mut lines = vec![];
    let mut tips = SpanTips::default();
    let section = |s: &str| {
        Line::from(Span::styled(
            s.to_uppercase(),
            muted().add_modifier(Modifier::BOLD),
        ))
    };
    match (&g.result, &g.error) {
        (_, Some(e)) => {
            for l in fmt::wrap(&format!("✗ {e}"), iw) {
                lines.push(Line::from(Span::styled(l, fg(RED))));
            }
        }
        (None, None) => lines.push(Line::from(Span::styled(
            format!(
                "{} asking GitHub about PRs, scanning for leftovers",
                spinner(app.now_ms)
            ),
            muted(),
        ))),
        (Some(c), None) => {
            if let Some(warn) = &c.warning {
                for l in fmt::wrap(&format!("! {warn}"), iw) {
                    lines.push(Line::from(Span::styled(l, fg(AMBER))));
                }
                lines.push(Line::from(""));
            }
            let shown: Vec<_> = c.checks.iter().filter(|m| m.verdict != Verdict::Keep).collect();
            let kept = c.checks.len() - shown.len();
            lines.push(section("merged clones"));
            if shown.is_empty() {
                lines.push(Line::from(Span::styled("  none", muted())));
            }
            let nw = shown.iter().map(|m| width(&m.clone)).max().unwrap_or(4).min(24);
            for m in &shown {
                let gone = c.removed.contains(&m.clone);
                let (icon, st) = match m.verdict {
                    _ if gone => ("✓ ", fg(SAGE)),
                    Verdict::Prune => ("✓ ", fg(SAGE)),
                    _ => ("! ", fg(AMBER)),
                };
                let size = fmt::bytes(m.own_bytes);
                let why = if gone {
                    "removed".to_string()
                } else {
                    m.reason.clone()
                };
                let room = iw.saturating_sub(nw + 4 + width(&size));
                let (name, name_tip) = cut(&m.clone, nw);
                let (why, why_tip) = cut(&why, room);
                tips.add(lines.len(), 1, name_tip);
                tips.add(lines.len(), 3, why_tip);
                lines.push(Line::from(vec![
                    Span::styled(icon, st),
                    Span::styled(pad(&name, nw), bold()),
                    Span::raw("  "),
                    Span::styled(pad(&why, room), st),
                    Span::styled(size, muted()),
                ]));
            }
            if kept > 0 {
                lines.push(Line::from(Span::styled(
                    format!(
                        "  {kept} other clone{} kept: open PR, no PR, or not merged",
                        if kept == 1 { "" } else { "s" }
                    ),
                    muted(),
                )));
            }
            let wt_shown: Vec<_> = c
                .worktrees
                .iter()
                .filter(|m| m.verdict != Verdict::Keep)
                .collect();
            let wt_kept = c.worktrees.len() - wt_shown.len();
            if !c.worktrees.is_empty() {
                lines.push(Line::from(""));
                lines.push(section("merged git worktrees"));
                if wt_shown.is_empty() {
                    lines.push(Line::from(Span::styled("  none", muted())));
                }
                let nw = wt_shown.iter().map(|m| width(&m.name)).max().unwrap_or(4).min(24);
                for m in &wt_shown {
                    let gone = c.removed_worktrees.contains(&m.path);
                    let (icon, st) = match m.verdict {
                        _ if gone => ("✓ ", fg(SAGE)),
                        Verdict::Prune => ("✓ ", fg(SAGE)),
                        _ => ("! ", fg(AMBER)),
                    };
                    let why = if gone {
                        "removed".to_string()
                    } else {
                        m.reason.clone()
                    };
                    let room = iw.saturating_sub(nw + 4);
                    lines.push(Line::from(vec![
                        Span::styled(icon, st),
                        Span::styled(pad(&trunc(&m.name, nw), nw), Style::new()),
                        Span::raw("  "),
                        Span::styled(trunc(&why, room), st),
                    ]));
                }
                if wt_kept > 0 {
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  {wt_kept} other worktree{} kept: no PR, open, or detached",
                            if wt_kept == 1 { "" } else { "s" }
                        ),
                        muted(),
                    )));
                }
            }
            lines.push(Line::from(""));
            lines.push(section("leftovers"));
            if c.leftovers.is_empty() {
                lines.push(Line::from(Span::styled("  none", muted())));
            }
            for act in &c.leftovers {
                let (icon, st) = if g.applied {
                    ("✓ ", fg(SAGE))
                } else {
                    ("· ", muted())
                };
                let (act, t) = cut(act, iw.saturating_sub(2));
                tips.add(lines.len(), 1, t);
                lines.push(Line::from(vec![Span::styled(icon, st), Span::raw(act)]));
            }
            if let Some(d) = app.current()
                && !d.history.is_empty()
            {
                lines.push(Line::from(""));
                lines.push(section("recently removed · L for all"));
                for e in d.history.iter().take(5) {
                    let (s, t) = cut(&e.summary(app.now_unix), iw.saturating_sub(2));
                    tips.add(lines.len(), 1, t);
                    lines.push(Line::from(vec![
                        Span::styled("− ", muted()),
                        Span::styled(s, muted()),
                    ]));
                }
            }
            lines.push(Line::from(""));
            if g.running {
                lines.push(Line::from(Span::styled(
                    format!("{} cleaning", spinner(app.now_ms)),
                    fg(BOTTLE),
                )));
            } else if g.applied {
                lines.push(Line::from(Span::styled("done", fg(SAGE))));
            } else if crate::app::cleanup_has_work(c) {
                lines.push(Line::from(Span::styled(
                    "removes the ✓ ones (and their branches) and the leftovers",
                    muted(),
                )));
                lines.push(Line::from(""));
                let b1 = button("remove", !g.focus_close, true);
                let b2 = button("close", g.focus_close, false);
                let (b1w, b2w) = (width(&b1.content) as u16, width(&b2.content) as u16);
                let bw = (b1w + 2 + b2w) as usize;
                apply_line = Some((lines.len() as u16, (iw.saturating_sub(bw)) as u16, b1w, b2w));
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(iw.saturating_sub(bw))),
                    b1,
                    Span::raw("  "),
                    b2,
                ]));
            } else {
                lines.push(Line::from(Span::styled("✓ nothing to clean", fg(SAGE))));
            }
        }
    }
    let h = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
    let rect = centered(area, w, h);
    open_modal(f, &mut app.hits, area, rect);
    let block = modal_block(vec![Span::styled("cleanup", title())]);
    let inner = block.inner(rect);
    tips.place(&lines, inner, 0, &mut app.tips);
    f.render_widget(Paragraph::new(lines).block(block), rect);
    if let Some((y, x, b1w, b2w)) = apply_line {
        let by = inner.y + y;
        if by < inner.bottom() {
            app.hits.push((Rect::new(inner.x + x, by, b1w, 1), key_hit('a')));
            app.hits.push((
                Rect::new(inner.x + x + b1w + 2, by, b2w, 1),
                Hit::Key(KeyCode::Esc, KeyModifiers::NONE),
            ));
        }
    }
}

/// What the settings modal draws, and where its clickable parts are.
struct SettingsLayout {
    lines: Vec<Line<'static>>,
    /// Line of each actionable row.
    rows: Vec<(usize, SettingsAction)>,
    /// "save edits to" choices: line, column, width.
    chips: Vec<(usize, u16, u16, Layer)>,
    /// Text cursor of the open editor: line, column.
    cursor: Option<(usize, u16)>,
    tips: SpanTips,
}

/// Resolved config for the current repo, where each part comes from, and the
/// machine-wide pieces: service, shell integration, state folder. Settings
/// render from [`clove_core::settings::FIELDS`], one widget per kind.
fn settings_lines(app: &App, iw: usize) -> SettingsLayout {
    let mut out = SettingsLayout {
        lines: vec![],
        rows: vec![],
        chips: vec![],
        cursor: None,
        tips: SpanTips::default(),
    };
    let Some(d) = app.current() else {
        out.lines.push(Line::from(Span::styled("loading", muted())));
        return out;
    };
    let actions = app.settings_actions();
    let view = match &app.modal {
        Some(Modal::Settings(v)) => v.clone(),
        _ => SettingsView::default(),
    };
    let cur = actions
        .get(view.cursor.min(actions.len().saturating_sub(1)))
        .copied();
    let kw = 15;
    // An actionable row: `›` and a bold label when it has the cursor.
    let kva = |a: SettingsAction, k: &str, v: Vec<Span<'static>>| {
        let on = cur == Some(a);
        let mut s = vec![
            if on {
                Span::styled("› ", fg(BOTTLE).add_modifier(Modifier::BOLD))
            } else {
                Span::raw("  ")
            },
            Span::styled(pad(k, kw), if on { bold() } else { muted() }),
        ];
        s.extend(v);
        Line::from(s)
    };
    let c = &d.config;
    let st = &d.status;
    let section = |s: &str| {
        Line::from(Span::styled(
            s.to_uppercase(),
            muted().add_modifier(Modifier::BOLD),
        ))
    };
    let kv = |k: &str, v: Vec<Span<'static>>| {
        let mut s = vec![Span::styled(format!("  {}", pad(k, kw)), muted())];
        s.extend(v);
        Line::from(s)
    };
    let l = &mut out.lines;
    let tips = &mut out.tips;
    l.push(Line::from(vec![
        Span::styled(st.name.clone(), bold()),
        Span::raw("  "),
        Span::styled(fmt::tilde(&st.root), muted()),
    ]));
    l.push(Line::from(""));

    l.push(section("config files · later ones win"));
    let layers = clove_core::config::layer_paths(&st.root, &st.name);
    let pw = layers
        .iter()
        .map(|(_, p)| width(&fmt::tilde(p)))
        .max()
        .unwrap_or(0)
        .min(iw.saturating_sub(kw + 14));
    for (i, (label, path)) in layers.into_iter().enumerate() {
        let exists = path.exists();
        let (p, t) = cut_left(&fmt::tilde(&path), pw);
        let p = pad(&p, pw);
        tips.add(l.len(), 2, t);
        out.rows.push((l.len(), SettingsAction::Edit(i)));
        l.push(kva(
            SettingsAction::Edit(i),
            label,
            vec![
                Span::styled(p, if exists { Style::new() } else { muted() }),
                Span::styled(if exists { "" } else { "  (none)" }, muted()),
            ],
        ));
    }
    // Where edits go: ←/→ or a click.
    let mut chip_line = vec![Span::styled(format!("  {}", pad("save edits to", kw)), muted())];
    let mut x = (2 + kw) as u16;
    for layer in Layer::ALL {
        let text = format!(" {} ", layer.label());
        let w = width(&text) as u16;
        out.chips.push((l.len(), x, w, layer));
        chip_line.push(if layer == view.target {
            Span::styled(text, selected())
        } else {
            Span::styled(text, muted())
        });
        chip_line.push(Span::raw(" "));
        x += w + 1;
    }
    chip_line.push(Span::styled(" ←→", key()));
    l.push(Line::from(chip_line));

    let mut sections: Vec<&str> = vec![];
    for f in FIELDS {
        if !sections.contains(&f.section) {
            sections.push(f.section);
        }
    }
    let vw = iw.saturating_sub(kw + 2 + 20).max(10);
    // Values pad to a column so the source lines up after short ones.
    let col = vw.min(26);
    for sec in sections {
        l.push(Line::from(""));
        l.push(match sec {
            "steps" if c.steps_detected => section("steps · detected from the repo"),
            "steps" => section("steps · from config"),
            _ => section(sec),
        });
        for (i, f) in FIELDS.iter().enumerate().filter(|(_, f)| f.section == sec) {
            let Some(fs) = d.fields.iter().find(|s| s.field.key == f.key) else {
                continue;
            };
            let a = SettingsAction::Field(i);
            let editing = view.editing.as_ref().filter(|e| e.field == i);
            let mut v = vec![];
            match (editing, f.kind) {
                (Some(e), Kind::Choice { .. }) => {
                    v.push(Span::styled("‹ ", key()));
                    v.push(Span::styled(e.input.text.clone(), bold()));
                    v.push(Span::styled(" ›", key()));
                }
                (Some(e), _) => {
                    let shown = trunc_left(&e.input.text, vw);
                    let before: String = e.input.text.chars().take(e.input.cursor).collect();
                    let cx = width(&before).min(vw) as u16;
                    out.cursor = Some((l.len(), (2 + kw) as u16 + cx));
                    v.push(Span::styled(
                        pad(&shown, vw),
                        bold().add_modifier(Modifier::UNDERLINED),
                    ));
                }
                (None, Kind::Bool) => v.push(if fs.effective == "true" {
                    Span::styled(pad("● on", col), fg(SAGE))
                } else {
                    Span::styled(pad("○ off", col), muted())
                }),
                (None, _) if fs.effective.is_empty() => v.push(Span::styled(pad("none", col), muted())),
                (None, _) => {
                    let (s, t) = cut(&fs.effective, vw);
                    tips.add(l.len(), 2, t);
                    v.push(Span::raw(pad(&s, col)));
                }
            }
            if editing.is_some() {
                v.push(Span::styled(format!("  → {}", view.target.label()), fg(BOTTLE)));
            } else {
                let from = fs.source.map(|s| s.label()).unwrap_or("default");
                v.push(Span::styled(format!(" {from}"), muted()));
                if let Some(mine) = fs.get(view.target)
                    && fs.source != Some(view.target)
                {
                    tips.add(
                        l.len(),
                        2 + v.len(),
                        Some(Tip::new(format!(
                            "{} sets {mine}, but {from} wins.",
                            view.target.label()
                        ))),
                    );
                    v.push(Span::styled(
                        format!("  ({}: {}, overridden)", view.target.label(), trunc(mine, 16)),
                        fg(AMBER),
                    ));
                }
            }
            out.rows.push((l.len(), a));
            l.push(kva(a, f.label, v));
            if cur == Some(a) {
                let (help, t) = cut(f.help, iw.saturating_sub(kw + 2));
                tips.add(l.len(), 1, t);
                l.push(Line::from(vec![
                    Span::raw(format!("  {}", " ".repeat(kw))),
                    Span::styled(help, muted().add_modifier(Modifier::ITALIC)),
                ]));
            }
        }
        // Read-only context that is not a setting.
        match sec {
            "clones" if c.copy.is_none() && !st.copy.is_empty() => {
                let (s, t) = cut(
                    &format!("detected: {}", st.copy.join(", ")),
                    iw.saturating_sub(kw + 2),
                );
                tips.add(l.len(), 1, t);
                l.push(kv("", vec![Span::styled(s, muted())]));
            }
            "steps" => {
                l.push(kv(
                    "",
                    vec![Span::styled(
                        "edit [[step]] and [[add_step]] in a config file",
                        muted(),
                    )],
                ));
                let cmd_w = iw.saturating_sub(kw + 8);
                for step in &c.steps {
                    l.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(step.name.clone(), bold()),
                    ]));
                    if let Some(w) = &step.warm {
                        let (s, t) = cut(w, cmd_w);
                        tips.add(l.len(), 1, t);
                        l.push(kv("  warm", vec![Span::raw(s)]));
                    }
                    if let Some(s) = &step.setup {
                        let (s, t) = cut(s, cmd_w);
                        tips.add(l.len(), 1, t);
                        l.push(kv("  setup", vec![Span::raw(s)]));
                    }
                    let (watch, t) = if step.watch.is_empty() {
                        ("always".to_string(), None)
                    } else {
                        cut(&step.watch.join(", "), cmd_w)
                    };
                    tips.add(l.len(), 1, t);
                    l.push(kv("  reruns when", vec![Span::styled(watch, muted())]));
                }
            }
            "freshness" => {
                if let Some(fr) = &st.freshness {
                    let span = match &fr.reason {
                        Some(r) => {
                            let (s, t) = cut(&format!("stale: {r}"), iw.saturating_sub(kw + 2));
                            tips.add(l.len(), 1, t);
                            Span::styled(s, fg(AMBER))
                        }
                        None => Span::styled("fresh", fg(SAGE)),
                    };
                    l.push(kv("now", vec![span]));
                }
            }
            _ => {}
        }
    }
    l.push(Line::from(""));

    l.push(section("this machine"));
    out.rows.push((l.len(), SettingsAction::Service));
    l.push(kva(
        SettingsAction::Service,
        "service",
        vec![
            match app.service {
                Some(true) => Span::styled("● on   ", fg(SAGE)),
                _ => Span::styled("○ off  ", muted()),
            },
            Span::styled(
                "refreshes stale images when you pull, remounts clones at login",
                muted(),
            ),
        ],
    ));
    if let Some(state) = app.agents {
        use clove_core::agents::State;
        out.rows.push((l.len(), SettingsAction::Agents));
        l.push(kva(
            SettingsAction::Agents,
            "coding agents",
            match state {
                State::Installed => vec![
                    Span::styled("● on   ", fg(SAGE)),
                    Span::styled("installed coding agents use clove for worktrees", muted()),
                ],
                State::Outdated => vec![
                    Span::styled("◐ old  ", fg(AMBER)),
                    Span::styled("agent setup needs attention; Enter updates it", muted()),
                ],
                State::Missing => vec![
                    Span::styled("○ off  ", muted()),
                    Span::styled("agents don't know clove and use git worktree add", muted()),
                ],
            },
        ));
    }
    let active = app.integrated;
    let in_rc = clove_core::shellrc::detect().is_some_and(clove_core::shellrc::installed);
    let shell_spans = if active {
        vec![
            Span::styled("● on", fg(SAGE)),
            Span::styled("   Enter cds into a clone", muted()),
        ]
    } else if in_rc {
        vec![
            Span::styled("◐ installed", fg(AMBER)),
            Span::styled("   open a new terminal so Enter can cd", muted()),
        ]
    } else {
        vec![
            Span::styled("○ off  ", muted()),
            Span::styled("installs it so Enter can cd into a clone", muted()),
        ]
    };
    if active {
        l.push(kv("shell", shell_spans));
    } else {
        out.rows.push((l.len(), SettingsAction::Shell));
        l.push(kva(SettingsAction::Shell, "shell", shell_spans));
    }
    l.push(kv(
        "state",
        vec![Span::raw(fmt::tilde(&clove_core::state_home()))],
    ));
    out
}

fn draw_settings(f: &mut Frame, app: &mut App, area: Rect, v: SettingsView) {
    let w = 92.min(area.width.saturating_sub(4));
    let iw = w.saturating_sub(6) as usize;
    let SettingsLayout {
        lines,
        rows,
        chips,
        cursor,
        tips,
    } = settings_lines(app, iw);
    let h = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
    let rect = centered(area, w, h);
    open_modal(f, &mut app.hits, area, rect);
    let block = modal_block(vec![Span::styled("settings", title())]);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let view = inner.height as usize;
    app.settings_height = view;
    let len = lines.len();
    let mut scroll = v.scroll;
    let cur_row = rows.iter().find(|(_, a)| {
        let actions = app.settings_actions();
        actions.get(v.cursor.min(actions.len().saturating_sub(1))) == Some(a)
    });
    if v.follow
        && let Some((line, _)) = cur_row
    {
        // Keep the help line under the row in view too.
        scroll.follow((*line + 1).min(len.saturating_sub(1)), view, len);
        scroll.follow(*line, view, len);
    }
    scroll.clamp(view, len);
    let off = scroll.offset().min(len.saturating_sub(view));
    if let Some(Modal::Settings(sv)) = &mut app.modal {
        sv.scroll = scroll;
    }
    tips.place(&lines, inner, off, &mut app.tips);
    let visible: Vec<Line> = lines.into_iter().skip(off).take(view).collect();
    f.render_widget(Paragraph::new(visible), inner);
    app.hits.push((rect, Hit::ScrollSettings));
    let shown = |line: usize| line >= off && line < off + view;
    for (line, a) in rows {
        if shown(line) {
            app.hits
                .push((row(inner, (line - off) as u16), Hit::SettingsRow(a)));
        }
    }
    for (line, x, w, layer) in chips {
        if shown(line) {
            let y = inner.y + (line - off) as u16;
            app.hits
                .push((Rect::new(inner.x + x, y, w, 1), Hit::SettingsLayer(layer)));
        }
    }
    if let Some((line, x)) = cursor
        && shown(line)
    {
        f.set_cursor_position(Position::new(inner.x + x, inner.y + (line - off) as u16));
    }
    scrollbar(
        f,
        Rect::new(rect.right() - 1, inner.y, 1, inner.height),
        off,
        view,
        len,
    );
}

/// `clove log` for this repo: when, by what, what, why, and how to get the
/// branch back.
fn draw_removals(f: &mut Frame, app: &mut App, area: Rect, v: RemovalsView) {
    use clove_core::history::{By, Kind};
    let w = 96.min(area.width.saturating_sub(4));
    let iw = w.saturating_sub(6) as usize;
    let mut lines: Vec<Line> = vec![];
    let mut tips = SpanTips::default();
    let history = app.current().map(|d| d.history.clone()).unwrap_or_default();
    if history.is_empty() {
        for line in fmt::wrap(
            "nothing removed yet. Removals by hand, by prune, by auto-prune and by coding agents land here.",
            iw,
        ) {
            lines.push(Line::from(Span::styled(line, muted())));
        }
    }
    let nw = history.iter().map(|e| width(&e.name)).max().unwrap_or(4).min(28);
    for e in &history {
        let when = clove_core::history::ago(app.now_unix.saturating_sub(e.at));
        let what = match e.by {
            By::Hand => "removed",
            By::Prune => "pruned",
            By::AutoPrune => "auto-pruned",
            By::ClaudeHook => "by Claude",
            By::OpencodeHook => "by OpenCode",
        };
        let (name, t) = cut(&e.name, nw);
        tips.add(lines.len(), 3, t);
        let mut s = vec![
            Span::styled(pad(&when, 9), muted()),
            Span::styled(
                pad(what, 12),
                if e.by == By::AutoPrune { fg(AMBER) } else { muted() },
            ),
            Span::styled(pad(&name, nw), bold()),
        ];
        if e.kind == Kind::Worktree {
            s.push(Span::styled(" worktree", muted()));
        }
        let rest = iw.saturating_sub(9 + 12 + nw + 2);
        let why = match (&e.reason, &e.branch) {
            (Some(r), _) => r.clone(),
            (None, Some(b)) => b.clone(),
            (None, None) => "detached".into(),
        };
        let (why, t) = cut(&why, rest);
        tips.add(lines.len(), s.len() + 1, t);
        s.push(Span::raw("  "));
        s.push(Span::styled(why, muted()));
        lines.push(Line::from(s));
        if let Some(cmd) = e.restore() {
            let (c, t) = cut(&cmd, iw.saturating_sub(23));
            tips.add(lines.len(), 2, t);
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(21)),
                Span::styled("branch back ", muted()),
                Span::styled(c, key()),
            ]));
        }
    }
    let h = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
    let rect = centered(area, w, h);
    open_modal(f, &mut app.hits, area, rect);
    let block = modal_block(vec![Span::styled("removed · clove log", title())]);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let view = inner.height as usize;
    let len = lines.len();
    let mut scroll = v.scroll;
    scroll.clamp(view, len);
    let off = scroll.offset().min(len.saturating_sub(view));
    if let Some(Modal::Removals(rv)) = &mut app.modal {
        rv.scroll = scroll;
    }
    tips.place(&lines, inner, off, &mut app.tips);
    let visible: Vec<Line> = lines.into_iter().skip(off).take(view).collect();
    f.render_widget(Paragraph::new(visible), inner);
    app.hits.push((rect, Hit::ScrollRemovals));
    scrollbar(
        f,
        Rect::new(rect.right() - 1, inner.y, 1, inner.height),
        off,
        view,
        len,
    );
}

/// `clove stats` in the TUI: how long clones take, where the time goes,
/// and the recent creations.
fn draw_stats(f: &mut Frame, app: &mut App, area: Rect, v: RemovalsView) {
    use clove_core::stats::{FAST_MS, RERAN_MS};
    let w = 100.min(area.width.saturating_sub(4));
    let iw = w.saturating_sub(6) as usize;
    let secs = |ms: f64| {
        if ms < 1000.0 {
            format!("{:.2}s", ms / 1000.0)
        } else if ms < 60_000.0 {
            format!("{:.1}s", ms / 1000.0)
        } else {
            format!(
                "{}m{:02}s",
                (ms / 60_000.0) as u64,
                ((ms % 60_000.0) / 1000.0) as u64
            )
        }
    };
    let (created, golden) = app
        .current()
        .map(|d| (d.created.clone(), d.golden_builds.clone()))
        .unwrap_or_default();
    let s = clove_core::stats::stats(&created, &golden);
    let mut lines: Vec<Line> = vec![];
    if s.clones == 0 {
        lines.push(Line::from(Span::styled(
            "no clones made yet. Each new clone's timings land here.",
            muted(),
        )));
    } else {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} clone{}", s.clones, if s.clones == 1 { "" } else { "s" }),
                bold(),
            ),
            Span::styled("  median ", muted()),
            Span::styled(secs(s.median_ms), bold()),
            Span::styled("  p90 ", muted()),
            Span::raw(secs(s.p90_ms)),
            Span::styled(
                format!("  ·  {} of {} under {}", s.fast, s.clones, secs(FAST_MS)),
                muted(),
            ),
        ]));
        if let Some(m) = s.clean_median_ms {
            lines.push(Line::from(vec![
                Span::styled("clove itself  ", fg(SAGE)),
                Span::styled(secs(m), bold()),
                Span::styled(" median when no setup step had to rerun", muted()),
            ]));
        }
        if s.reran > 0 {
            lines.push(Line::from(vec![
                Span::styled("setup reran   ", fg(AMBER)),
                Span::raw(format!("{} of {}", s.reran, s.clones)),
                Span::styled(
                    format!(
                        " (over {}): the branch changed what the golden image set up",
                        secs(RERAN_MS)
                    ),
                    muted(),
                ),
            ]));
        }
        if let Some(g) = s.golden_median_ms {
            lines.push(Line::from(vec![
                Span::styled("golden        ", muted()),
                Span::raw(format!(
                    "{} build{} · median {}",
                    s.golden_builds,
                    if s.golden_builds == 1 { "" } else { "s" },
                    secs(g)
                )),
            ]));
        }
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "WHERE THE TIME GOES  median when the step ran",
            muted(),
        )));
        // On the scale of a clone that reran nothing; reruns overflow with ▸.
        let scale = s.clean_median_ms.unwrap_or(s.median_ms).max(1.0);
        let nw = s.steps.iter().map(|x| width(&x.step)).max().unwrap_or(4);
        let bar_w = iw.saturating_sub(nw + 24).clamp(8, 30);
        for x in s.steps.iter().filter(|x| x.max_ms >= 5.0) {
            let cells = ((x.median_ms / scale) * bar_w as f64).round().max(1.0) as usize;
            let bar = if cells > bar_w {
                format!("{}▸", "█".repeat(bar_w))
            } else {
                "█".repeat(cells)
            };
            let mut l = vec![
                Span::raw(format!("{:<nw$}  ", x.step)),
                Span::styled(format!("{:>6}  ", secs(x.median_ms)), bold()),
                Span::styled(
                    bar,
                    fg(if x.step.starts_with("setup ") {
                        AMBER
                    } else {
                        BOTTLE
                    }),
                ),
            ];
            if x.runs < s.clones {
                l.push(Span::styled(format!("  in {} of {}", x.runs, s.clones), muted()));
            }
            lines.push(Line::from(l));
        }
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!(
                "{:<9}{:<26}{:>8}{:>8}{:>10}{:>9}",
                "WHEN", "RECENT", "TOTAL", "MOUNT", "CHECKOUT", "SETUP"
            ),
            muted(),
        )));
        for e in &created {
            let step = |n: &str| {
                e.timings
                    .iter()
                    .find(|t| t.step == n)
                    .map(|t| secs(t.ms))
                    .unwrap_or_else(|| "–".into())
            };
            let when = clove_core::history::ago(app.now_unix.saturating_sub(e.at));
            let total_c = if e.total_ms < FAST_MS {
                SAGE
            } else if e.reran() {
                AMBER
            } else {
                RED
            };
            let (name, _) = cut(&e.name, 24);
            lines.push(Line::from(vec![
                Span::styled(format!("{:<9}", when), muted()),
                Span::styled(format!("{:<26}", name), bold()),
                Span::styled(format!("{:>8}", secs(e.total_ms)), fg(total_c)),
                Span::raw(format!("{:>8}{:>10}", step("mount"), step("checkout"))),
                Span::styled(
                    format!("{:>9}", secs(e.setup_ms())),
                    if e.reran() { fg(AMBER) } else { Style::new() },
                ),
            ]));
        }
    }
    let h = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
    let rect = centered(area, w, h);
    open_modal(f, &mut app.hits, area, rect);
    let block = modal_block(vec![Span::styled("timings · clove stats", title())]);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let view = inner.height as usize;
    let len = lines.len();
    let mut scroll = v.scroll;
    scroll.clamp(view, len);
    let off = scroll.offset().min(len.saturating_sub(view));
    if let Some(Modal::Stats(sv)) = &mut app.modal {
        sv.scroll = scroll;
    }
    let visible: Vec<Line> = lines.into_iter().skip(off).take(view).collect();
    f.render_widget(Paragraph::new(visible), inner);
    app.hits.push((rect, Hit::ScrollRemovals));
    scrollbar(
        f,
        Rect::new(rect.right() - 1, inner.y, 1, inner.height),
        off,
        view,
        len,
    );
}

fn draw_doctor(f: &mut Frame, app: &mut App, area: Rect, d: DoctorView) {
    let w = 84.min(area.width.saturating_sub(4));
    let iw = w.saturating_sub(6) as usize;
    let mut lines: Vec<Line> = vec![];
    let mut tips = SpanTips::default();
    match (&d.checks, &d.error) {
        (_, Some(e)) => lines.push(Line::from(Span::styled(format!("✗ {e}"), fg(RED)))),
        (None, None) => lines.push(Line::from(Span::styled(
            format!("{} checking", spinner(app.now_ms)),
            muted(),
        ))),
        (Some(checks), None) => {
            let nw = 14;
            for c in checks {
                let (icon, color) = match c.level {
                    Level::Ok => ("✓", SAGE),
                    Level::Warn => ("!", AMBER),
                    Level::Fail => ("✗", RED),
                };
                let detail = fmt::wrap(&c.detail, iw.saturating_sub(nw + 3));
                for (i, l) in detail.into_iter().enumerate() {
                    let mut s = vec![];
                    if i == 0 {
                        s.push(Span::styled(
                            format!("{icon} "),
                            fg(color).add_modifier(Modifier::BOLD),
                        ));
                        let (name, t) = cut(&c.name, nw);
                        tips.add(lines.len(), 1, t);
                        s.push(Span::styled(pad(&name, nw), bold()));
                    } else {
                        s.push(Span::raw(" ".repeat(nw + 2)));
                    }
                    s.push(Span::raw(" "));
                    s.push(Span::raw(l));
                    lines.push(Line::from(s));
                }
                if let Some(fix) = &c.fix {
                    let (s, t) = cut(fix, iw.saturating_sub(nw + 7));
                    tips.add(lines.len(), 2, t);
                    lines.push(Line::from(vec![
                        Span::raw(" ".repeat(nw + 3)),
                        Span::styled("fix ", muted()),
                        Span::styled(s, key()),
                    ]));
                }
            }
            let fails = checks.iter().filter(|c| c.level == Level::Fail).count();
            let warns = checks.iter().filter(|c| c.level == Level::Warn).count();
            lines.push(Line::from(""));
            lines.push(match (fails, warns) {
                (0, 0) => Line::from(Span::styled("all good", fg(SAGE))),
                (0, w) => Line::from(Span::styled(
                    format!("{w} warning{}", if w == 1 { "" } else { "s" }),
                    fg(AMBER),
                )),
                (f, _) => Line::from(Span::styled(
                    format!("{f} problem{} to fix", if f == 1 { "" } else { "s" }),
                    fg(RED),
                )),
            });
        }
    }
    let h = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
    let rect = centered(area, w, h);
    open_modal(f, &mut app.hits, area, rect);
    let block = modal_block(vec![Span::styled("doctor", title())]);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let view = inner.height as usize;
    let len = lines.len();
    let mut scroll = d.scroll;
    scroll.clamp(view, len);
    let off = scroll.offset().min(len.saturating_sub(view));
    if let Some(Modal::Doctor(dv)) = &mut app.modal {
        dv.scroll = scroll;
    }
    tips.place(&lines, inner, off, &mut app.tips);
    let visible: Vec<Line> = lines.into_iter().skip(off).take(view).collect();
    f.render_widget(Paragraph::new(visible), inner);
    app.hits.push((rect, Hit::ScrollDoctor));
    scrollbar(
        f,
        Rect::new(rect.right() - 1, inner.y, 1, inner.height),
        off,
        view,
        len,
    );
}

/// One step of a job, its name and detail cut to `w`; tips for both go on
/// line `li` of `tips`.
fn step_line<'a>(s: &StepRow, now_ms: u64, w: usize, li: usize, tips: &mut SpanTips) -> Line<'a> {
    let (icon, istyle, nstyle, right) = match &s.state {
        StepState::Running { since_ms } => (
            spinner(now_ms).to_string(),
            fg(BOTTLE),
            bold(),
            fmt::secs(now_ms.saturating_sub(*since_ms) as f64),
        ),
        StepState::Done { ms } => ("✓".into(), fg(SAGE), Style::new(), fmt::secs(*ms)),
        StepState::Skipped { reason } => ("–".into(), muted(), muted(), reason.clone()),
        StepState::Failed => ("✗".into(), fg(RED), fg(RED), "failed".into()),
    };
    // A skipped step keeps its name; its reason gives way instead.
    let (name, right) = if matches!(s.state, StepState::Skipped { .. }) {
        let (name, t) = cut(&s.name, w.saturating_sub(6).max(4));
        tips.add(li, 1, t);
        let room = w.saturating_sub(2 + width(&name) + 2);
        let (right, t) = cut(&right, room);
        tips.add(li, 3, t.map(|t| t.note(format!("{} skipped", s.name))));
        (name, right)
    } else {
        let nw = w.saturating_sub(width(&right) + 3).max(4);
        let (name, t) = cut(&s.name, nw);
        tips.add(li, 1, t);
        (name, right)
    };
    let fill = w.saturating_sub(2 + width(&name) + width(&right)).max(1);
    Line::from(vec![
        Span::styled(format!("{icon} "), istyle),
        Span::styled(name, nstyle),
        Span::raw(" ".repeat(fill)),
        Span::styled(right, muted()),
    ])
}

fn draw_job(f: &mut Frame, app: &mut App, area: Rect, id: u64) {
    let Some(job) = app.job(id).cloned() else {
        return;
    };
    // The output panel appears once a step prints something; a quick clone
    // with silent steps is just its step list.
    let has_log = !job.log.is_empty();
    let w = if has_log {
        area.width.saturating_sub(8).min(130)
    } else {
        area.width.saturating_sub(8).min(64)
    };
    // Grow with the content (a quick clone stays small, a refresh's log fills it).
    let steps_h = job.steps.len() + 4;
    let log_h = if has_log { job.log.len() + 2 } else { 0 };
    let want = if area.width.saturating_sub(8) >= 94 {
        steps_h.max(log_h)
    } else {
        steps_h + log_h
    };
    let h = ((want.max(10) + 4) as u16)
        .min(area.height.saturating_sub(4))
        .min(40);
    let rect = centered(area, w, h);
    open_modal(f, &mut app.hits, area, rect);
    let (icon, istyle) = match &job.result {
        None => (spinner(app.now_ms).to_string(), fg(BOTTLE)),
        Some(Ok(_)) => ("✓".into(), fg(SAGE)),
        Some(Err(_)) => ("✗".into(), fg(RED)),
    };
    let elapsed = fmt::secs(job.elapsed_ms(app.now_ms) as f64);
    let block = panel(
        vec![
            Span::styled(format!("{icon} "), istyle),
            Span::styled(job.title(), title()),
            Span::styled(format!("  {} · {elapsed}", job.repo_name), muted()),
        ],
        true,
    );
    let inner = block.inner(rect).inner(Margin::new(2, 1));
    f.render_widget(block, rect);

    let wide = inner.width >= 90;
    let (steps_area, log_area) = if !has_log {
        (inner, Rect::default())
    } else if wide {
        let r = app.panes.job.layout(inner);
        (r[0], r[1])
    } else {
        let sh = (job.steps.len() as u16 + 3).max(4).min(inner.height / 2);
        let [s, l] = Layout::vertical([Constraint::Length(sh), Constraint::Fill(1)]).areas(inner);
        (s, l)
    };

    // Steps, newest at the bottom; keep the tail when they overflow.
    let sw = steps_area.width as usize;
    let mut tips = SpanTips::default();
    let mut lines: Vec<Line> = job
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| step_line(s, app.now_ms, sw, i, &mut tips))
        .collect();
    if lines.is_empty() && job.running() {
        lines.push(Line::from(Span::styled(
            format!("{} starting", spinner(app.now_ms)),
            muted(),
        )));
    }
    let mut summary: Vec<Line> = vec![Line::from("")];
    match &job.result {
        None => {}
        Some(Ok(s)) => {
            for l in fmt::wrap(s, sw) {
                summary.push(Line::from(Span::styled(l, fg(SAGE).add_modifier(Modifier::BOLD))));
            }
            if job.path.is_some() {
                summary.push(Line::from(vec![
                    Span::styled("⏎", key()),
                    Span::styled(" cd into it", muted()),
                ]));
            }
        }
        Some(Err(e)) => {
            for l in fmt::wrap(e, sw).into_iter().take(4) {
                summary.push(Line::from(Span::styled(l, fg(RED))));
            }
        }
    }
    let room = (steps_area.height as usize).saturating_sub(summary.len());
    let skip = lines.len().saturating_sub(room);
    let mut shown: Vec<Line> = lines.into_iter().skip(skip).collect();
    if job.result.is_some() {
        shown.extend(summary);
    }
    let last = shown.len().saturating_sub(1) as u16;
    let n = shown.len();
    // `shown` starts at step `skip`: shift the tips to match.
    let mut shifted = SpanTips::default();
    for (li, si, t) in tips.0 {
        if li >= skip {
            shifted.add(li - skip, si, Some(t));
        }
    }
    shifted.place(&shown, steps_area, 0, &mut app.tips);
    f.render_widget(Paragraph::new(shown), steps_area);
    if matches!(job.result, Some(Ok(_))) && job.path.is_some() && n > 0 {
        app.hits.push((
            Rect::new(steps_area.x, steps_area.y + last, 14, 1).intersection(steps_area),
            Hit::Key(KeyCode::Enter, KeyModifiers::NONE),
        ));
    }

    if !has_log {
        return;
    }
    // Output log.
    let block = panel(
        vec![
            Span::styled("output", muted().add_modifier(Modifier::BOLD)),
            Span::styled(
                if job.log.is_empty() {
                    String::new()
                } else {
                    format!(" {}", job.log.len() + job.log_dropped)
                },
                muted(),
            ),
        ],
        false,
    );
    let li = block.inner(log_area).inner(Margin::new(1, 0));
    f.render_widget(block, log_area);
    app.hits.push((log_area, Hit::ScrollLog));
    if wide {
        for &(p, r) in app.panes.job.dividers() {
            app.hits.push((r, Hit::Divider(crate::panes::SplitId::Job, p)));
        }
    }
    let view = li.height as usize;
    app.log_height = view;
    let len = job.log.len();
    let follow = job.follow;
    let off = if let Some(j) = app.jobs.iter_mut().find(|j| j.id == id) {
        if follow {
            j.scroll.set(len.saturating_sub(view));
        }
        j.scroll.clamp(view, len);
        j.scroll.offset().min(len.saturating_sub(view))
    } else {
        0
    };
    if len == 0 {
        let msg = if job.running() {
            "step output streams here"
        } else {
            "no output"
        };
        empty_state(f, li, vec![Line::from(Span::styled(msg, muted()))]);
        return;
    }
    let pw = job.log[off..(off + view).min(len)]
        .iter()
        .filter_map(|l| l.step.as_deref().map(step_tag))
        .map(width)
        .max()
        .unwrap_or(0)
        .min(12);
    let tw = (li.width as usize).saturating_sub(if pw > 0 { pw + 3 } else { 0 });
    let mut tips = SpanTips::default();
    let lines: Vec<Line> = job.log[off..(off + view).min(len)]
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let mut s = vec![];
            if pw > 0 {
                let tag = l.step.as_deref().map(step_tag).unwrap_or("");
                s.push(Span::styled(pad(tag, pw), muted()));
                s.push(Span::styled(" │ ", border()));
            }
            let style = match l.kind {
                LogKind::Out => Style::new(),
                LogKind::Warn => fg(AMBER),
                LogKind::Info => muted(),
            };
            let (text, t) = cut(&l.text, tw);
            let t = t.map(|t| match &l.step {
                Some(step) => t.note(step.clone()),
                None => t,
            });
            tips.add(i, s.len(), t);
            s.push(Span::styled(text, style));
            Line::from(s)
        })
        .collect();
    tips.place(&lines, li, 0, &mut app.tips);
    f.render_widget(Paragraph::new(lines), li);
    scrollbar(
        f,
        Rect::new(log_area.right() - 1, li.y, 1, li.height),
        off,
        view,
        len,
    );
    if !follow && len > view {
        let more = len - off - view.min(len - off);
        if more > 0 {
            let s = format!(" ↓ {more} more · end ");
            let sw = width(&s) as u16;
            let r = Rect::new(
                log_area.right().saturating_sub(sw + 2),
                log_area.bottom() - 1,
                sw,
                1,
            );
            put(f, r, Line::from(Span::styled(s, fg(BOTTLE))));
            app.hits.push((r, Hit::Key(KeyCode::End, KeyModifiers::NONE)));
        }
    }
}

/// `setup deps` → `deps`: the log prefix only needs the step's own name.
fn step_tag(s: &str) -> &str {
    s.strip_prefix("setup ")
        .or_else(|| s.strip_prefix("warm "))
        .unwrap_or(s)
}
