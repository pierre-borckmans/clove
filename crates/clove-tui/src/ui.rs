//! Drawing. Every frame is a pure function of [`App`], except that it records
//! clickable regions in `app.hits` and keeps scroll targets in view.

use crate::app::App;
use crate::fmt::{self, pad, trunc, trunc_left, width};
use crate::model::*;
use crate::theme::*;
use clove_core::CloneInfo;
use clove_core::doctor::Level;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

pub fn draw(f: &mut Frame, app: &mut App) {
    app.hits.clear();
    let area = f.area();
    if area.width < 60 || area.height < 14 {
        let msg = Paragraph::new(vec![
            Line::from(Span::styled("◆ clove", title())),
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
    draw_header(f, app, header);
    let list = draw_body(f, app, body);
    draw_footer(f, app, footer);
    draw_modal(f, app, area, list);
    draw_toasts(f, app, body);
}

// ---- building blocks -------------------------------------------------

fn panel<'a>(title_spans: Vec<Span<'a>>, focus: bool) -> Block<'a> {
    let mut t = vec![Span::raw(" ")];
    t.extend(title_spans);
    t.push(Span::raw(" "));
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(if focus { fg(RUST) } else { border() })
        .title(Line::from(t))
}

fn put(f: &mut Frame, area: Rect, line: Line) {
    if area.width > 0 && area.height > 0 {
        f.render_widget(Paragraph::new(line), area);
    }
}

fn row(area: Rect, i: u16) -> Rect {
    Rect::new(area.x, area.y + i, area.width, 1).intersection(area)
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
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
fn scrollbar(f: &mut Frame, track: Rect, offset: usize, view: usize, len: usize) {
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
            ("┃", fg(RUST))
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

fn draw_header(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border())
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled("◆ clove", title()),
            Span::raw(" "),
        ]));
    let inner = block.inner(area).inner(Margin::new(1, 0));
    f.render_widget(block, area);
    draw_tabs(
        f,
        app,
        Rect::new(area.x + 12, area.y, area.width.saturating_sub(14), 1),
    );

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
    let name_w = width(&repo.name);
    put(
        f,
        row(inner, 0),
        Line::from(vec![
            Span::styled(trunc(&repo.name, left_w), bold()),
            Span::raw("  "),
            Span::styled(trunc_left(&root, left_w.saturating_sub(name_w + 2)), muted()),
        ]),
    );
    if rw > 0 && inner.width > rw {
        put(
            f,
            Rect::new(inner.right() - rw, inner.y, rw, 1),
            Line::from(Span::styled(right, muted())),
        );
    }

    // Row 2: the golden image.
    let line = golden_line(app, data.as_ref(), inner.width as usize);
    put(f, row(inner, 1), line);
}

fn golden_line<'a>(app: &App, data: Option<&RepoData>, w: usize) -> Line<'a> {
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
        return Line::from(vec![
            label,
            Span::styled(format!("{} ", spinner(app.now_ms)), fg(RUST)),
            Span::styled(format!("{verb} · {step}"), bold()),
            Span::styled(
                format!(" · {} · l to watch", fmt::secs(j.elapsed_ms(app.now_ms) as f64)),
                muted(),
            ),
        ]);
    }
    if let Some(e) = app.current_error() {
        return Line::from(vec![
            label,
            Span::styled(trunc(&format!("✗ {e}"), w.saturating_sub(8)), fg(RED)),
        ]);
    }
    let Some(d) = data else {
        return Line::from(vec![
            label,
            Span::styled(format!("{} loading", spinner(app.now_ms)), muted()),
        ]);
    };
    let Some(g) = &d.status.golden else {
        return Line::from(vec![
            label,
            Span::styled("○ not built yet", fg(AMBER)),
            Span::styled(" · press ", muted()),
            Span::styled("i", key()),
            Span::styled(" to build it", muted()),
        ]);
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
    // Long form when it fits, else drop the base name.
    let long = w >= 70 + g.meta.base.len();
    let base = if long {
        format!(" {}", g.meta.base)
    } else {
        String::new()
    };
    match g.behind {
        Some(0) if long => spans.push(Span::styled(format!("✓ up to date with{base}"), fg(SAGE))),
        Some(0) => spans.push(Span::styled("✓ current", fg(SAGE))),
        Some(n) => spans.push(Span::styled(format!("↓{n} behind{base}"), fg(AMBER))),
        None => spans.push(Span::styled("? behind", muted())),
    }
    spans.push(sep());
    spans.push(Span::raw(fmt::bytes(g.image_bytes)));
    if g.meta.dirty {
        spans.push(sep());
        spans.push(Span::styled("dirty warm", fg(AMBER)));
    }
    Line::from(spans)
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
            put(f, r, Line::from(Span::styled(s, fg(RUST))));
            app.hits.push((r, Hit::Key(KeyCode::Tab, KeyModifiers::NONE)));
        }
    }
}

// ---- body --------------------------------------------------------------

fn draw_body(f: &mut Frame, app: &mut App, area: Rect) -> Rect {
    let (list, detail) = if area.width >= 110 {
        let [l, d] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(48)]).areas(area);
        (l, Some(d))
    } else if area.height >= 26 {
        let [l, d] = Layout::vertical([Constraint::Fill(1), Constraint::Length(12)]).areas(area);
        (l, Some(d))
    } else {
        (area, None)
    };
    draw_list(f, app, list);
    if let Some(d) = detail {
        draw_detail(f, app, d);
    }
    list
}

fn empty_state(f: &mut Frame, area: Rect, lines: Vec<Line>) {
    let h = lines.len() as u16;
    let y = area.y + area.height.saturating_sub(h) / 2;
    f.render_widget(
        Paragraph::new(lines).centered(),
        Rect::new(area.x, y, area.width, h.min(area.height)),
    );
}

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    let total = app.current().map(|d| d.clones.len()).unwrap_or(0);
    let vis_len = app.visible().len();
    let mut t = vec![Span::styled("clones", title())];
    if app.current().is_some() {
        let count = if app.filter.is_empty() {
            format!(" {total}")
        } else {
            format!(" {vis_len}/{total}")
        };
        t.push(Span::styled(count, muted()));
    }
    if app.filtering || !app.filter.is_empty() {
        t.push(Span::styled("  /", key()));
        t.push(Span::styled(app.filter.clone(), bold()));
        if app.filtering {
            t.push(Span::styled("▏", fg(RUST)));
        }
    }
    let block = panel(t, false);
    let inner = block.inner(area).inner(Margin::new(1, 0));
    f.render_widget(block, area);
    app.hits.push((inner, Hit::ScrollList));

    if app.repo().is_none() {
        let notice = app.notice.clone().unwrap_or_else(|| "no repository".into());
        let mut lines = vec![
            Line::from(Span::styled("◆", fg(RUST))),
            Line::from(Span::styled("nothing to show yet", bold())),
            Line::from(""),
        ];
        for l in fmt::wrap(&notice, inner.width.saturating_sub(4) as usize) {
            lines.push(Line::from(Span::styled(l, muted())));
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
            Line::from(Span::styled("◆", fg(RUST))),
            Line::from(Span::styled(
                format!("{} has no golden image yet", data.status.name),
                bold(),
            )),
            Line::from(""),
        ];
        if building {
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", spinner(app.now_ms)), fg(RUST)),
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
    if total == 0 {
        empty_state(
            f,
            inner,
            vec![
                Line::from(Span::styled("◆", fg(RUST))),
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
    if clones.is_empty() {
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
    draw_table(f, app, area, inner, &clones);
}

struct Cols {
    name: usize,
    branch: usize,
    changes: usize,
    age: usize,
    size: usize,
    path: usize,
}

fn columns(clones: &[CloneInfo], w: usize) -> Cols {
    let name = clones
        .iter()
        .map(|c| width(&c.name))
        .max()
        .unwrap_or(4)
        .clamp(4, 26);
    let branch = clones
        .iter()
        .map(|c| c.branch.as_deref().map(width).unwrap_or(10))
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

fn draw_table(f: &mut Frame, app: &mut App, area: Rect, inner: Rect, clones: &[CloneInfo]) {
    let cols = columns(clones, inner.width as usize);
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

    let h = inner.height.saturating_sub(1) as usize;
    let len = clones.len();
    app.list_height = h;
    app.list_scroll.follow(app.selected, h, len);
    app.list_scroll.clamp(h, len);
    let off = app.list_scroll.offset().min(len.saturating_sub(h));

    for (vi, c) in clones.iter().enumerate().skip(off).take(h) {
        let r = row(inner, 1 + (vi - off) as u16);
        let line = clone_row(app, c, &cols);
        put(f, r, line);
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
        (Some(_), _) => Span::styled(format!("{} ", spinner(app.now_ms)), fg(RUST)),
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
        fmt::bytes(c.image_bytes),
        w = cols.size
    )));
    if cols.path > 0 {
        spans.push(gap());
        spans.push(Span::styled(trunc_left(&fmt::tilde(&c.path), cols.path), muted()));
    }
    Line::from(spans)
}

/// The CHANGES cell, padded to `w`.
fn changes<'a>(app: &App, c: &CloneInfo, w: usize) -> Vec<Span<'a>> {
    let mut parts: Vec<(String, Style)> = vec![];
    if let Some(j) = app.busy(&c.name) {
        parts.push((format!("{}…", j.verb()), fg(RUST)));
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

/// Appends a TIMINGS header and one bar per step, scaled to the slowest; the
/// slowest bar takes the accent. Returns the step-name column width.
fn timing_lines(lines: &mut Vec<Line>, timings: &[clove_core::Timing], w: usize) -> usize {
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
            fg(RUST)
        } else {
            fg(SAGE)
        };
        lines.push(Line::from(vec![
            Span::styled(pad(&t.step, nw), muted()),
            Span::raw(" "),
            Span::styled(pad(&b, bw), style),
            Span::raw(" "),
            Span::raw(format!("{:>vw$}", fmt::secs(t.ms))),
        ]));
    }
    nw
}

/// With nothing selected, the detail pane describes the golden image.
fn draw_golden_detail(f: &mut Frame, app: &App, area: Rect) -> bool {
    let Some(d) = app.current() else {
        return false;
    };
    let Some(g) = &d.status.golden else {
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
    let mut lines = vec![
        kv("base", vec![Span::styled(trunc(&g.meta.base, vw), fg(PLUM))]),
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
        lines.push(kv("warm", vec![Span::raw(trunc(&g.meta.steps.join(", "), vw))]));
    }
    if !g.meta.timings.is_empty() {
        lines.push(Line::from(""));
        timing_lines(&mut lines, &g.meta.timings, w);
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("n", key()),
        Span::styled(" clones it in about a second", muted()),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
    true
}

fn draw_detail(f: &mut Frame, app: &mut App, area: Rect) {
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
    let block = panel(vec![Span::styled(c.name.clone(), title())], false);
    let inner = block.inner(area).inner(Margin::new(2, 1));
    f.render_widget(block, area);
    let w = inner.width as usize;
    let vw = w.saturating_sub(8);
    let sep = || Span::styled(" · ", muted());

    let mut lines = vec![];
    let mut branch = match &c.branch {
        Some(b) => vec![Span::styled(trunc(b, vw), fg(PLUM))],
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
        lines.push(kv("head", vec![Span::styled(fmt::short_commit(h), muted())]));
    }
    let mut status = if let Some(j) = app.busy(&c.name) {
        vec![Span::styled(
            format!("{} {}", spinner(app.now_ms), j.verb()),
            fg(RUST),
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
    status.extend([sep(), Span::raw(fmt::bytes(c.image_bytes))]);
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
        golden.push(Span::styled(
            format!("golden now {}", g.meta.generation),
            fg(AMBER),
        ));
    }
    lines.push(kv("from", golden));
    lines.push(kv("path", vec![Span::raw(trunc_left(&fmt::tilde(&c.path), vw))]));

    if !c.timings.is_empty() || !c.failed_steps.is_empty() {
        lines.push(Line::from(""));
        let nw = timing_lines(&mut lines, &c.timings, w);
        for s in &c.failed_steps {
            lines.push(Line::from(vec![
                Span::styled("✗ ", fg(RED)),
                Span::styled(pad(&format!("setup {s}"), nw), fg(RED)),
                Span::styled(" failed", fg(RED)),
            ]));
            if let Some(d) = app.current() {
                let log = d.status.state.join("logs").join(format!("{}-{s}.log", c.name));
                lines.push(Line::from(Span::styled(
                    format!("  {}", trunc_left(&fmt::tilde(&log), w.saturating_sub(2))),
                    muted(),
                )));
            }
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

// ---- footer ------------------------------------------------------------

type Hint = (&'static str, &'static str, KeyCode);

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
        Some(Modal::Remove(f)) => {
            let mut v = vec![("⏎", "confirm", Enter)];
            if f.branch.is_some() {
                v.push(("b", "delete branch", Char('b')));
            }
            v.push(("f", "force", Char('f')));
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
            if !g.applied && g.actions.as_ref().is_some_and(|a| !a.is_empty()) {
                v.push(("a", "apply", Char('a')));
            }
            v.push(("esc", "close", Esc));
            v
        }
        Some(Modal::Doctor(_)) => vec![("↑↓", "scroll", Down), ("esc", "close", Esc)],
        Some(Modal::Help) => vec![("esc", "close", Esc)],
        None if app.filtering => vec![("⏎", "keep", Enter), ("esc", "clear", Esc), ("↑↓", "move", Down)],
        None => {
            let mut v: Vec<Hint> = vec![];
            if app.current().is_some() && !app.has_golden() {
                v.push(("i", "init", Char('i')));
            } else if let Some(c) = app.selected_clone() {
                v.extend([
                    ("⏎", "cd", Enter),
                    ("n", "new", Char('n')),
                    ("d", "remove", Char('d')),
                ]);
                v.push(if c.mounted {
                    ("u", "unmount", Char('u'))
                } else {
                    ("m", "mount", Char('m'))
                });
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
            v.extend([("g", "gc", Char('g')), ("D", "doctor", Char('D'))]);
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
        [] => Line::from(""),
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
                Span::styled(format!("{} ", spinner(app.now_ms)), fg(RUST)),
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
    let sw = (line_width(&status) as u16).min(area.width / 2);
    if sw > 0 {
        put(f, Rect::new(area.right() - sw, area.y, sw, 1), status.clone());
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
        x += w + 2;
    }
}

// ---- toasts ------------------------------------------------------------

fn draw_toasts(f: &mut Frame, app: &mut App, body: Rect) {
    let mut y = body.y + 1;
    let maxw = 60.min(body.width.saturating_sub(4)) as usize;
    let toasts: Vec<Toast> = app.toasts.iter().rev().cloned().collect();
    let n = toasts.len();
    for (ri, t) in toasts.into_iter().enumerate() {
        let (icon, color) = match t.kind {
            ToastKind::Ok => ("✓", SAGE),
            ToastKind::Info => ("◆", RUST),
            ToastKind::Warn => ("!", AMBER),
            ToastKind::Err => ("✗", RED),
        };
        let lines = fmt::wrap(&t.text, maxw.saturating_sub(6));
        let lines: Vec<String> = lines.into_iter().take(3).collect();
        let tw = lines.iter().map(|l| width(l)).max().unwrap_or(0) + 6;
        let h = lines.len() as u16 + 2;
        if y + h > body.bottom() {
            break;
        }
        let r = Rect::new(body.right() - tw as u16 - 2, y, tw as u16, h);
        f.render_widget(Clear, r);
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
        y += h;
    }
}

// ---- modals ------------------------------------------------------------

fn draw_modal(f: &mut Frame, app: &mut App, area: Rect, list: Rect) {
    let Some(modal) = app.modal.clone() else {
        return;
    };
    match modal {
        Modal::New(form) => draw_new(f, app, list, &form),
        Modal::Help => draw_help(f, app, area),
        Modal::Remove(r) => draw_remove(f, app, area, &r),
        Modal::Gc(g) => draw_gc(f, app, area, &g),
        Modal::Doctor(d) => draw_doctor(f, app, area, d),
        Modal::Job(id) => draw_job(f, app, area, id),
    }
}

fn modal_block<'a>(title_spans: Vec<Span<'a>>) -> Block<'a> {
    panel(title_spans, true).padding(ratatui::widgets::Padding::new(2, 2, 1, 1))
}

/// Dims everything behind a centered modal and makes a click outside it close
/// it (the footer stays clickable).
fn open_modal(f: &mut Frame, hits: &mut Vec<(Rect, Hit)>, area: Rect, rect: Rect) {
    dim_backdrop(f, area);
    f.render_widget(Clear, rect);
    let above_footer = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    hits.push((above_footer, Hit::Key(KeyCode::Esc, KeyModifiers::NONE)));
    hits.push((rect, Hit::Inert));
}

fn key_hit(c: char) -> Hit {
    Hit::Key(KeyCode::Char(c), KeyModifiers::NONE)
}

fn button<'a>(label: &str, focused: bool, danger: bool) -> Span<'a> {
    let s = format!(" {label} ");
    if focused {
        Span::styled(
            s,
            Style::new()
                .bg(if danger { RED } else { RUST })
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
            Span::styled(
                trunc_left(&input.text, tw),
                if focus { bold() } else { Style::new() },
            )
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
    let pl = match err {
        Some(e) => Line::from(Span::styled(format!("✗ {e}"), fg(RED))),
        None => Line::from(vec![
            Span::styled("→ ", fg(RUST)),
            Span::styled(trunc(&preview, inner.width as usize - 2), muted()),
        ]),
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
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border());
    let si = block.inner(sr).inner(Margin::new(1, 0));
    f.render_widget(block, sr);
    for (i, s) in sugg.iter().enumerate() {
        let rr = row(si, i as u16);
        let tag = ref_tag(app, s, form.focus);
        let nw = (si.width as usize).saturating_sub(tagw + 1);
        let shown = trunc(s, nw);
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
        if has(RefKind::Local, name) {
            "local"
        } else {
            "origin"
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
    let sections: [(&str, &[(&str, &str)]); 3] = [
        (
            "clones",
            &[
                ("⏎", "cd into it and quit"),
                ("n", "new clone on a branch"),
                ("d", "remove (asks first)"),
                ("m / u", "mount / unmount"),
                ("o", "open in $EDITOR"),
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
                ("g", "find leftovers (gc)"),
                ("D", "doctor"),
                ("ctrl-r", "reload now"),
                ("q esc", "quit"),
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
        "rows, tabs, buttons and hints are clickable",
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
    let mut head = vec![Span::styled(r.name.clone(), bold())];
    if let Some(b) = &r.branch {
        head.push(Span::styled("  on ", muted()));
        head.push(Span::styled(b.clone(), fg(PLUM)));
    }
    lines.push(Line::from(head));
    lines.push(Line::from(Span::styled(
        trunc_left(&fmt::tilde(&r.path), iw),
        muted(),
    )));
    lines.push(Line::from(""));
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
    if let Some(a) = r.ahead.filter(|a| *a > 0) {
        let s = format!(
            "{} {a} commit{} not on {}",
            if r.delete_branch { "!" } else { "·" },
            if a == 1 { "" } else { "s" },
            r.base
        );
        lines.push(Line::from(Span::styled(
            s,
            if r.delete_branch { fg(AMBER) } else { muted() },
        )));
    }
    lines.push(Line::from(""));
    let check = |on: bool| {
        if on {
            Span::styled("[x] ", key())
        } else {
            Span::styled("[ ] ", muted())
        }
    };
    let mut toggles = vec![];
    if let Some(b) = &r.branch {
        toggles.push((lines.len(), 'b'));
        lines.push(Line::from(vec![
            check(r.delete_branch),
            Span::raw("also delete branch "),
            Span::styled(trunc(b, iw.saturating_sub(30)), fg(PLUM)),
            Span::styled("  b", key()),
        ]));
    }
    toggles.push((lines.len(), 'f'));
    lines.push(Line::from(vec![
        check(r.force),
        Span::raw("force: discard changes, eject if busy"),
        Span::styled("  f", key()),
    ]));
    lines.push(Line::from(""));
    let danger = r.force && r.dirty == Some(true);
    let confirm = if danger { "discard & remove" } else { "remove" };
    let b1 = button(confirm, r.focus == 0, danger);
    let b2 = button("cancel", r.focus == 1, false);
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
        Span::styled("remove ", title()),
        Span::styled(r.name.clone(), bold()),
    ]);
    let inner = block.inner(rect);
    f.render_widget(Paragraph::new(lines).block(block), rect);
    for (i, c) in toggles {
        app.hits.push((row(inner, i as u16), key_hit(c)));
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

fn draw_gc(f: &mut Frame, app: &mut App, area: Rect, g: &GcView) {
    let w: u16 = 70;
    let iw = (w - 6) as usize;
    let mut apply_line = None;
    let mut lines = vec![];
    match (&g.actions, &g.error) {
        (_, Some(e)) => {
            for l in fmt::wrap(&format!("✗ {e}"), iw) {
                lines.push(Line::from(Span::styled(l, fg(RED))));
            }
        }
        (None, None) => lines.push(Line::from(Span::styled(
            format!("{} scanning for leftovers", spinner(app.now_ms)),
            muted(),
        ))),
        (Some(a), None) if a.is_empty() => {
            lines.push(Line::from(Span::styled("✓ nothing to clean", fg(SAGE))))
        }
        (Some(a), None) => {
            for act in a {
                let (icon, st) = if g.applied {
                    ("✓ ", fg(SAGE))
                } else {
                    ("· ", muted())
                };
                lines.push(Line::from(vec![
                    Span::styled(icon, st),
                    Span::raw(trunc(act, iw - 2)),
                ]));
            }
            lines.push(Line::from(""));
            if g.running {
                lines.push(Line::from(Span::styled(
                    format!("{} cleaning", spinner(app.now_ms)),
                    fg(RUST),
                )));
            } else if g.applied {
                lines.push(Line::from(Span::styled("cleaned", fg(SAGE))));
            } else {
                apply_line = Some(lines.len() as u16);
                lines.push(Line::from(vec![
                    Span::styled("press ", muted()),
                    Span::styled("a", key()),
                    Span::styled(" to remove them", muted()),
                ]));
            }
        }
    }
    let h = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
    let rect = centered(area, w, h);
    open_modal(f, &mut app.hits, area, rect);
    let block = modal_block(vec![Span::styled("leftovers", title())]);
    let inner = block.inner(rect);
    f.render_widget(Paragraph::new(lines).block(block), rect);
    if let Some(y) = apply_line {
        app.hits.push((row(inner, y), key_hit('a')));
    }
}

fn draw_doctor(f: &mut Frame, app: &mut App, area: Rect, d: DoctorView) {
    let w = 84.min(area.width.saturating_sub(4));
    let iw = w.saturating_sub(6) as usize;
    let mut lines: Vec<Line> = vec![];
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
                        s.push(Span::styled(pad(&c.name, nw), bold()));
                    } else {
                        s.push(Span::raw(" ".repeat(nw + 2)));
                    }
                    s.push(Span::raw(" "));
                    s.push(Span::raw(l));
                    lines.push(Line::from(s));
                }
                if let Some(fix) = &c.fix {
                    lines.push(Line::from(vec![
                        Span::raw(" ".repeat(nw + 3)),
                        Span::styled("fix ", muted()),
                        Span::styled(trunc(fix, iw.saturating_sub(nw + 7)), key()),
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

fn step_line<'a>(s: &StepRow, now_ms: u64, w: usize) -> Line<'a> {
    let (icon, istyle, nstyle, right) = match &s.state {
        StepState::Running { since_ms } => (
            spinner(now_ms).to_string(),
            fg(RUST),
            bold(),
            fmt::secs(now_ms.saturating_sub(*since_ms) as f64),
        ),
        StepState::Done { ms } => ("✓".into(), fg(SAGE), Style::new(), fmt::secs(*ms)),
        StepState::Skipped { reason } => ("–".into(), muted(), muted(), reason.clone()),
        StepState::Failed => ("✗".into(), fg(RED), fg(RED), "failed".into()),
    };
    let nw = w.saturating_sub(width(&right) + 3).max(4);
    let name = trunc(&s.name, nw);
    let fill = w.saturating_sub(2 + width(&name) + width(&right));
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
    let w = area.width.saturating_sub(8).min(130);
    // Grow with the content (a quick clone stays small, a refresh's log fills it).
    let steps_h = job.steps.len() + 4;
    let log_h = job.log.len() + 2;
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
        None => (spinner(app.now_ms).to_string(), fg(RUST)),
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
    let (steps_area, log_area) = if wide {
        let [s, l] = Layout::horizontal([Constraint::Length(38), Constraint::Fill(1)])
            .spacing(2)
            .areas(inner);
        (s, l)
    } else {
        let sh = (job.steps.len() as u16 + 3).max(4).min(inner.height / 2);
        let [s, l] = Layout::vertical([Constraint::Length(sh), Constraint::Fill(1)]).areas(inner);
        (s, l)
    };

    // Steps, newest at the bottom; keep the tail when they overflow.
    let sw = steps_area.width as usize;
    let mut lines: Vec<Line> = job.steps.iter().map(|s| step_line(s, app.now_ms, sw)).collect();
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
    f.render_widget(Paragraph::new(shown), steps_area);
    if matches!(job.result, Some(Ok(_))) && job.path.is_some() && n > 0 {
        app.hits.push((
            Rect::new(steps_area.x, steps_area.y + last, 14, 1).intersection(steps_area),
            Hit::Key(KeyCode::Enter, KeyModifiers::NONE),
        ));
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
    let lines: Vec<Line> = job.log[off..(off + view).min(len)]
        .iter()
        .map(|l| {
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
            s.push(Span::styled(trunc(&l.text, tw), style));
            Line::from(s)
        })
        .collect();
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
            put(f, r, Line::from(Span::styled(s, fg(RUST))));
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
