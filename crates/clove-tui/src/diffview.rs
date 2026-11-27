//! A clone's changes against its base: a file list (packages folded, like the
//! app) beside a unified or split diff. Only the rows on screen are drawn,
//! and syntax highlighting fills in from a background thread.

use crate::fmt::width;
use crate::model::{Hit, Msg};
use crate::theme::{self, AMBER, BOTTLE, LINE, MUTED, RED, SAGE, bold, fg, muted};
use clove_core::diff::{CloneDiff, Package};
use clove_core::patch::{self, Change, FilePatch, Fold, LineKind};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock, mpsc};

/// View choices that outlive one opening of the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffPrefs {
    pub split: bool,
    pub wrap: bool,
    /// Most changed first, instead of path order.
    pub by_changes: bool,
    pub grouped: bool,
    pub tree: bool,
}

impl Default for DiffPrefs {
    fn default() -> Self {
        Self {
            split: true,
            wrap: false,
            by_changes: false,
            grouped: false,
            tree: true,
        }
    }
}

/// What a load produced, shared (never copied) between frames.
pub struct DiffData {
    pub diff: CloneDiff,
    pub files: Vec<FilePatch>,
    pub folds: Vec<Option<Fold>>,
    /// Group key (a package's directory, or a folder) per file.
    pub group: Vec<String>,
    pub packages: Vec<Package>,
}

impl DiffData {
    pub fn new(diff: CloneDiff) -> Self {
        let files = patch::parse(&diff.patch);
        let folds = files
            .iter()
            .map(|f| patch::fold(f, patch::attrs_of(&diff, &f.path)))
            .collect();
        let group = files
            .iter()
            .map(|f| patch::group_of(&f.path, &diff.packages).0)
            .collect();
        let packages = diff.packages.clone();
        Self {
            diff,
            files,
            folds,
            group,
            packages,
        }
    }

    fn package(&self, key: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.dir == key)
    }
}

/// One highlighted line: coloured pieces of its text, and the byte ranges
/// that changed against its counterpart on the other side.
#[derive(Default, Clone)]
pub struct LineHl {
    pub spans: Vec<(Style, Range<usize>)>,
    pub words: Vec<Range<usize>>,
}

/// One file's highlighting, `[hunk][line]`.
type FileHl = Arc<Vec<Vec<LineHl>>>;

/// Highlighting per file, filled in by a background thread.
pub struct Highlights {
    files: RwLock<Vec<Option<FileHl>>>,
    done: AtomicUsize,
    cancel: AtomicBool,
}

impl Highlights {
    pub fn finished(&self) -> bool {
        self.done.load(Ordering::Relaxed) >= self.files.read().map(|f| f.len()).unwrap_or(0)
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn file(&self, i: usize) -> Option<FileHl> {
        self.files.read().ok()?.get(i).cloned().flatten()
    }
}

/// Highlights every file on a thread, shown files first, folded ones last,
/// and pokes the UI after each so it redraws.
pub fn highlight(data: Arc<DiffData>, tx: mpsc::Sender<Msg>) -> Arc<Highlights> {
    let n = data.files.len();
    let hl = Arc::new(Highlights {
        files: RwLock::new(vec![None; n]),
        done: AtomicUsize::new(0),
        cancel: AtomicBool::new(false),
    });
    let h = hl.clone();
    std::thread::spawn(move || {
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| data.folds[i].is_some());
        for i in order {
            if h.cancel.load(Ordering::Relaxed) {
                return;
            }
            let f = &data.files[i];
            let lines = if data.folds[i] == Some(Fold::Binary) {
                vec![]
            } else {
                highlight_file(f)
            };
            if let Ok(mut w) = h.files.write() {
                w[i] = Some(Arc::new(lines));
            }
            h.done.fetch_add(1, Ordering::Relaxed);
            let _ = tx.send(Msg::DiffHighlighted);
        }
    });
    hl
}

fn syntaxes() -> &'static syntect::parsing::SyntaxSet {
    static S: std::sync::OnceLock<syntect::parsing::SyntaxSet> = std::sync::OnceLock::new();
    S.get_or_init(syntect::parsing::SyntaxSet::load_defaults_newlines)
}

/// The palette as a syntect theme. The default foreground is a marker that
/// means "the terminal's own".
fn syntax_theme() -> &'static syntect::highlighting::Theme {
    use syntect::highlighting::{Color as SC, FontStyle, StyleModifier, Theme, ThemeItem, ThemeSettings};
    static T: std::sync::OnceLock<Theme> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let c = |color: Color| match color {
            Color::Rgb(r, g, b) => SC { r, g, b, a: 255 },
            _ => DEFAULT_FG,
        };
        let item = |scope: &str, color: Color, italic: bool| ThemeItem {
            scope: scope.parse().expect("scope selector"),
            style: StyleModifier {
                foreground: Some(c(color)),
                background: None,
                font_style: italic.then_some(FontStyle::ITALIC),
            },
        };
        Theme {
            settings: ThemeSettings {
                foreground: Some(DEFAULT_FG),
                ..Default::default()
            },
            scopes: vec![
                item("comment, punctuation.definition.comment", theme::SYN_COMMENT, true),
                item("string, constant.character", theme::SYN_STRING, false),
                item("constant.numeric, constant.language, constant.other", theme::SYN_NUMBER, false),
                item(
                    "keyword, storage, keyword.operator.word, variable.language",
                    theme::SYN_KEYWORD,
                    false,
                ),
                item(
                    "entity.name.function, support.function, meta.function-call entity.name",
                    theme::SYN_FUNCTION,
                    false,
                ),
                item(
                    "entity.name.type, entity.name.class, support.type, support.class, entity.name.tag, storage.type.rust",
                    theme::SYN_TYPE,
                    false,
                ),
                item("markup.heading", theme::SYN_FUNCTION, false),
            ],
            ..Default::default()
        }
    })
}

const DEFAULT_FG: syntect::highlighting::Color = syntect::highlighting::Color {
    r: 1,
    g: 2,
    b: 3,
    a: 0,
};

/// The syntax for a path; TypeScript and friends borrow JavaScript's, which
/// the bundled set lacks.
fn syntax_for(path: &str) -> &'static syntect::parsing::SyntaxReference {
    let ss = syntaxes();
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or(name);
    let ext = match ext {
        "ts" | "tsx" | "mts" | "cts" | "jsx" | "mjs" | "cjs" => "js",
        "yml" => "yaml",
        "zsh" | "bash" => "sh",
        "Dockerfile" | "Makefile" if ext == name => name,
        e => e,
    };
    ss.find_syntax_by_extension(ext)
        .unwrap_or_else(|| ss.find_syntax_plain_text())
}

/// Highlights the old side (context and removed lines) and the new side
/// (context and added lines) as two streams, so each line is parsed in the
/// state the lines before it left. Then marks the words that changed.
fn highlight_file(f: &FilePatch) -> Vec<Vec<LineHl>> {
    use syntect::easy::HighlightLines;
    let syntax = syntax_for(&f.path);
    let theme = syntax_theme();
    let mut old = HighlightLines::new(syntax, theme);
    let mut new = HighlightLines::new(syntax, theme);
    let ss = syntaxes();
    let mut out = Vec::with_capacity(f.hunks.len());
    for h in &f.hunks {
        let mut lines: Vec<LineHl> = Vec::with_capacity(h.lines.len());
        for l in &h.lines {
            let text = format!("{}\n", l.text);
            // Context lines feed both streams; the new side's colours are shown.
            let regions = match l.kind {
                LineKind::Removed => old.highlight_line(&text, ss).ok(),
                LineKind::Added => new.highlight_line(&text, ss).ok(),
                LineKind::Context => {
                    let _ = old.highlight_line(&text, ss);
                    new.highlight_line(&text, ss).ok()
                }
            };
            let mut spans = vec![];
            let mut at = 0;
            for (style, piece) in regions.unwrap_or_default() {
                let end = (at + piece.len()).min(l.text.len());
                if end > at {
                    spans.push((to_style(style), at..end));
                }
                at += piece.len();
            }
            lines.push(LineHl { spans, words: vec![] });
        }
        mark_words(&h.lines, &mut lines);
        out.push(lines);
    }
    out
}

fn to_style(s: syntect::highlighting::Style) -> Style {
    let mut st = Style::new();
    if s.foreground != DEFAULT_FG {
        st = st.fg(Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b));
    }
    if s.font_style.contains(syntect::highlighting::FontStyle::ITALIC) {
        st = st.add_modifier(Modifier::ITALIC);
    }
    st
}

/// Pairs each run of removed lines with the added run after it, line by
/// line, and marks the words that differ. Lines too unlike each other keep
/// no marks: a whole line lit up says nothing the tint doesn't.
fn mark_words(lines: &[clove_core::patch::Line], hl: &mut [LineHl]) {
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind != LineKind::Removed {
            i += 1;
            continue;
        }
        let del_start = i;
        while i < lines.len() && lines[i].kind == LineKind::Removed {
            i += 1;
        }
        let add_start = i;
        while i < lines.len() && lines[i].kind == LineKind::Added {
            i += 1;
        }
        let pairs = (add_start - del_start).min(i - add_start);
        for k in 0..pairs {
            let (a, b) = (&lines[del_start + k].text, &lines[add_start + k].text);
            if a.len() + b.len() > 2000 {
                continue;
            }
            let diff = similar::TextDiff::configure().diff_unicode_words(a, b);
            if diff.ratio() < 0.4 {
                continue;
            }
            let (mut ia, mut ib) = (0usize, 0usize);
            let (mut wa, mut wb) = (vec![], vec![]);
            for change in diff.iter_all_changes() {
                let n = change.value().len();
                match change.tag() {
                    similar::ChangeTag::Equal => {
                        ia += n;
                        ib += n;
                    }
                    similar::ChangeTag::Delete => {
                        push_range(&mut wa, ia..ia + n);
                        ia += n;
                    }
                    similar::ChangeTag::Insert => {
                        push_range(&mut wb, ib..ib + n);
                        ib += n;
                    }
                }
            }
            hl[del_start + k].words = wa;
            hl[add_start + k].words = wb;
        }
    }
}

fn push_range(v: &mut Vec<Range<usize>>, r: Range<usize>) {
    match v.last_mut() {
        Some(last) if last.end == r.start => last.end = r.end,
        _ => v.push(r),
    }
}

// ---- view state ----------------------------------------------------------

#[derive(Clone)]
pub enum Load {
    Loading,
    Failed(String),
    Ready(Arc<DiffData>, Arc<Highlights>),
}

/// One line of the diff pane. Indices point into `DiffData::files`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    File(usize),
    Folded(usize),
    Hunk(usize, usize),
    /// Unified: one patch line, `part` of it when wrapped.
    Line {
        f: usize,
        h: usize,
        l: usize,
        part: u16,
    },
    /// Split: the old line on the left, the new on the right.
    Pair {
        f: usize,
        h: usize,
        left: Option<usize>,
        right: Option<usize>,
        part: u16,
    },
    Gap,
}

/// An entry of the file list.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Group(String),
    File(usize),
}

#[derive(Clone)]
pub struct DiffView {
    pub root: PathBuf,
    pub name: String,
    pub branch: Option<String>,
    pub load: Load,
    /// Rows of the diff pane, and what they were built for.
    rows: Arc<Vec<Row>>,
    built: Option<(u16, DiffPrefs, u64)>,
    /// File display order.
    order: Arc<Vec<usize>>,
    /// Top row of the diff pane.
    pub top: usize,
    /// Columns scrolled right, without wrap.
    pub hscroll: usize,
    /// The list's cursor, by file (or group key when its group is folded).
    cursor: Option<Entry>,
    list_top: usize,
    /// Folded files the user opened, and groups opened in the list.
    opened: HashSet<usize>,
    open_groups: HashSet<String>,
    /// Bumped when `opened` changes, so rows rebuild.
    fold_gen: u64,
    pub page: usize,
    /// Restore this file at the top after a reload.
    pending_file: Option<String>,
}

impl DiffView {
    pub fn new(root: PathBuf, name: String, branch: Option<String>) -> Self {
        Self {
            root,
            name,
            branch,
            load: Load::Loading,
            rows: Arc::new(vec![]),
            built: None,
            order: Arc::new(vec![]),
            top: 0,
            hscroll: 0,
            cursor: None,
            list_top: 0,
            opened: HashSet::new(),
            open_groups: HashSet::new(),
            fold_gen: 0,
            page: 20,
            pending_file: None,
        }
    }

    /// Still highlighting: the UI ticks faster so it fills in.
    pub fn busy(&self) -> bool {
        match &self.load {
            Load::Loading => true,
            Load::Ready(_, hl) => !hl.finished(),
            Load::Failed(_) => false,
        }
    }

    /// Takes a finished load, keeping the place of a reload.
    pub fn set(&mut self, load: Load) {
        if let Load::Ready(_, hl) = &self.load {
            hl.stop();
        }
        self.load = load;
        self.built = None;
    }

    /// Stops background work for a view that is going away.
    pub fn stop(&self) {
        if let Load::Ready(_, hl) = &self.load {
            hl.stop();
        }
    }

    /// Marks where to come back to before a reload.
    pub fn reloading(&mut self) {
        self.pending_file = self
            .top_file()
            .and_then(|f| self.data().map(|d| d.files[f].path.clone()));
        if let Load::Ready(_, hl) = &self.load {
            hl.stop();
        }
        self.load = Load::Loading;
    }

    fn data(&self) -> Option<&Arc<DiffData>> {
        match &self.load {
            Load::Ready(d, _) => Some(d),
            _ => None,
        }
    }

    fn sorted(&self, d: &DiffData, prefs: DiffPrefs) -> Vec<usize> {
        let mut order: Vec<usize> = (0..d.files.len()).collect();
        let weight = |i: usize| d.files[i].added + d.files[i].removed;
        if prefs.by_changes {
            order.sort_by_key(|&i| std::cmp::Reverse(weight(i)));
        }
        if prefs.grouped {
            let mut totals: std::collections::HashMap<&str, u32> = Default::default();
            for &i in &order {
                *totals.entry(d.group[i].as_str()).or_default() += weight(i);
            }
            // Stable: within a group, files keep the order above.
            if prefs.by_changes {
                order.sort_by_key(|&i| (std::cmp::Reverse(totals[d.group[i].as_str()]), d.group[i].clone()));
            } else {
                order.sort_by(|&a, &b| d.group[a].cmp(&d.group[b]));
            }
        }
        order
    }

    /// Rebuilds rows when the width, the prefs or the folds changed.
    fn ensure_rows(&mut self, cols: u16, prefs: DiffPrefs) {
        let key = (cols, prefs, self.fold_gen);
        if self.built == Some(key) {
            return;
        }
        let Some(d) = self.data().cloned() else { return };
        let keep = self.top_file();
        let order = self.sorted(&d, prefs);
        let mut rows = vec![];
        let gutter = gutter_width(&d, prefs.split);
        for &f in &order {
            let file = &d.files[f];
            if !rows.is_empty() {
                rows.push(Row::Gap);
            }
            rows.push(Row::File(f));
            if d.folds[f].is_some() && !self.opened.contains(&f) {
                rows.push(Row::Folded(f));
                continue;
            }
            for (h, hunk) in file.hunks.iter().enumerate() {
                rows.push(Row::Hunk(f, h));
                let text_w = |w: u16| (w as usize).saturating_sub(gutter).max(8);
                let parts = |s: &str, w: usize| {
                    if prefs.wrap {
                        width(&s.replace('\t', "    ")).div_ceil(w).max(1) as u16
                    } else {
                        1
                    }
                };
                if prefs.split {
                    let side = text_w(cols.saturating_sub(1) / 2);
                    for (left, right) in pair_lines(&hunk.lines) {
                        let n = left
                            .map(|l| parts(&hunk.lines[l].text, side))
                            .unwrap_or(1)
                            .max(right.map(|l| parts(&hunk.lines[l].text, side)).unwrap_or(1));
                        for part in 0..n {
                            rows.push(Row::Pair {
                                f,
                                h,
                                left,
                                right,
                                part,
                            });
                        }
                    }
                } else {
                    let w = text_w(cols);
                    for (l, line) in hunk.lines.iter().enumerate() {
                        for part in 0..parts(&line.text, w) {
                            rows.push(Row::Line { f, h, l, part });
                        }
                    }
                }
            }
        }
        self.rows = Arc::new(rows);
        self.order = Arc::new(order);
        self.built = Some(key);
        // Keep the same file at the top across rebuilds and reloads.
        let want = self
            .pending_file
            .take()
            .and_then(|p| d.files.iter().position(|f| f.path == p))
            .or(keep);
        if let Some(f) = want {
            self.jump_to(f);
        }
    }

    /// The file at the top of the diff pane. A gap between files belongs to
    /// the file below it.
    fn top_file(&self) -> Option<usize> {
        self.rows
            .get(self.top..)?
            .iter()
            .find_map(row_file)
            .or_else(|| self.rows.iter().rev().find_map(row_file))
    }

    fn jump_to(&mut self, f: usize) {
        if let Some(i) = self.rows.iter().position(|r| *r == Row::File(f)) {
            self.top = i;
        }
    }

    fn scroll(&mut self, by: isize) {
        let max = self.rows.len().saturating_sub(1);
        self.top = (self.top as isize + by).clamp(0, max as isize) as usize;
        self.cursor = None;
    }

    /// The list as drawn: group headers, and the files of open groups.
    fn entries(&self, prefs: DiffPrefs) -> Vec<Entry> {
        let Some(d) = self.data() else { return vec![] };
        if !prefs.grouped {
            return self.order.iter().map(|&f| Entry::File(f)).collect();
        }
        let mut out = vec![];
        let mut last: Option<&str> = None;
        for &f in self.order.iter() {
            let g = d.group[f].as_str();
            if last != Some(g) {
                out.push(Entry::Group(g.to_string()));
                last = Some(g);
            }
            if self.open_groups.contains(g) {
                out.push(Entry::File(f));
            }
        }
        out
    }

    /// The entry under the cursor, or the file at the top of the diff (its
    /// group when that is folded).
    fn current(&self, entries: &[Entry]) -> Option<usize> {
        if let Some(c) = &self.cursor
            && let Some(i) = entries.iter().position(|e| e == c)
        {
            return Some(i);
        }
        let f = self.top_file()?;
        entries.iter().position(|e| *e == Entry::File(f)).or_else(|| {
            let g = &self.data()?.group[f];
            entries.iter().position(|e| *e == Entry::Group(g.clone()))
        })
    }

    fn select(&mut self, e: Entry) {
        let first = match &e {
            Entry::File(f) => Some(*f),
            Entry::Group(g) => self
                .data()
                .and_then(|d| self.order.iter().copied().find(|&f| &d.group[f] == g)),
        };
        if let Some(f) = first {
            self.jump_to(f);
        }
        self.cursor = Some(e);
    }

    /// The file of the current list entry or top row: what fold and edit act on.
    fn focus_file(&self, prefs: DiffPrefs) -> Option<usize> {
        let entries = self.entries(prefs);
        match self.current(&entries).map(|i| &entries[i]) {
            Some(Entry::File(f)) => Some(*f),
            _ => self.top_file(),
        }
    }

    /// Handles a key; Some(path) asks to open that file in the editor, and
    /// `close` asks to leave the view, `reload` to load again.
    pub fn key(&mut self, k: KeyEvent, prefs: &mut DiffPrefs) -> KeyResult {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let page = self.page.max(2) as isize;
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return KeyResult::Close,
            KeyCode::Char('r') => return KeyResult::Reload,
            KeyCode::Char('s') => prefs.split = !prefs.split,
            KeyCode::Char('w') => {
                prefs.wrap = !prefs.wrap;
                self.hscroll = 0;
            }
            KeyCode::Char('o') => prefs.by_changes = !prefs.by_changes,
            KeyCode::Char('g') => {
                prefs.grouped = !prefs.grouped;
                self.cursor = None;
            }
            KeyCode::Char('f') => prefs.tree = !prefs.tree,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Up | KeyCode::Char('k') => {
                let entries = self.entries(*prefs);
                if entries.is_empty() {
                    return KeyResult::None;
                }
                let d = if matches!(k.code, KeyCode::Down | KeyCode::Char('j')) {
                    1
                } else {
                    -1
                };
                let next = match self.current(&entries) {
                    Some(i) => (i as isize + d).clamp(0, entries.len() as isize - 1) as usize,
                    None => 0,
                };
                self.select(entries[next].clone());
            }
            KeyCode::Char('d') if ctrl => self.scroll(page / 2),
            KeyCode::Char('u') if ctrl => self.scroll(-page / 2),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll(page - 2),
            KeyCode::PageUp | KeyCode::Char('b') => self.scroll(-(page - 2)),
            KeyCode::Char('J') => self.scroll(1),
            KeyCode::Char('K') => self.scroll(-1),
            KeyCode::Home => self.scroll(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => self.scroll(isize::MAX / 2),
            KeyCode::Left | KeyCode::Char('h') if !prefs.wrap => {
                self.hscroll = self.hscroll.saturating_sub(8)
            }
            KeyCode::Right | KeyCode::Char('l') if !prefs.wrap => self.hscroll += 8,
            KeyCode::Enter | KeyCode::Char('z') => {
                let entries = self.entries(*prefs);
                match self.current(&entries).map(|i| entries[i].clone()) {
                    Some(Entry::Group(g)) if k.code == KeyCode::Enter => {
                        if !self.open_groups.remove(&g) {
                            self.open_groups.insert(g.clone());
                        }
                        self.cursor = Some(Entry::Group(g));
                    }
                    _ => {
                        if let Some(f) = self.focus_file(*prefs)
                            && self
                                .data()
                                .is_some_and(|d| matches!(d.folds[f], Some(fold) if fold != Fold::Binary))
                        {
                            if !self.opened.remove(&f) {
                                self.opened.insert(f);
                            }
                            self.fold_gen += 1;
                            self.built = None;
                            self.cursor = Some(Entry::File(f));
                            return KeyResult::None;
                        }
                    }
                }
            }
            KeyCode::Char('e') => {
                if let (Some(f), Some(d)) = (self.focus_file(*prefs), self.data())
                    && d.files[f].change != Change::Deleted
                {
                    return KeyResult::Edit(d.files[f].path.clone());
                }
            }
            _ => {}
        }
        KeyResult::None
    }

    pub fn wheel(&mut self, over_list: bool, d: isize) {
        if over_list {
            self.list_top = (self.list_top as isize + d * 2).max(0) as usize;
        } else {
            self.scroll(d * 3);
        }
    }

    pub fn click_entry(&mut self, i: usize, prefs: DiffPrefs) {
        let entries = self.entries(prefs);
        if let Some(e) = entries.get(i).cloned() {
            if let Entry::Group(g) = &e
                && self.cursor.as_ref() == Some(&e)
                && !self.open_groups.remove(g)
            {
                self.open_groups.insert(g.clone());
            }
            self.select(e);
        }
    }
}

impl std::fmt::Debug for DiffView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DiffView({})", self.name)
    }
}

pub enum KeyResult {
    None,
    Close,
    Reload,
    Edit(String),
}

fn row_file(r: &Row) -> Option<usize> {
    match *r {
        Row::File(f) | Row::Folded(f) | Row::Hunk(f, _) => Some(f),
        Row::Line { f, .. } | Row::Pair { f, .. } => Some(f),
        Row::Gap => None,
    }
}

/// Context lines pair with themselves; a run of removed lines pairs with the
/// added run after it, line by line; leftovers stand alone.
fn pair_lines(lines: &[clove_core::patch::Line]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut out = vec![];
    let mut i = 0;
    while i < lines.len() {
        match lines[i].kind {
            LineKind::Context => {
                out.push((Some(i), Some(i)));
                i += 1;
            }
            _ => {
                let ds = i;
                while i < lines.len() && lines[i].kind == LineKind::Removed {
                    i += 1;
                }
                let as_ = i;
                while i < lines.len() && lines[i].kind == LineKind::Added {
                    i += 1;
                }
                let (nd, na) = (as_ - ds, i - as_);
                for k in 0..nd.max(na) {
                    out.push(((k < nd).then_some(ds + k), (k < na).then_some(as_ + k)));
                }
            }
        }
    }
    out
}

/// Digits of the biggest line number.
fn number_width(d: &DiffData) -> usize {
    let max = d
        .files
        .iter()
        .flat_map(|f| f.hunks.iter())
        .flat_map(|h| h.lines.iter())
        .map(|l| l.old.unwrap_or(0).max(l.new.unwrap_or(0)))
        .max()
        .unwrap_or(1);
    max.to_string().len().max(3)
}

/// Columns before the text: numbers, the sign and a space.
fn gutter_width(d: &DiffData, split: bool) -> usize {
    let n = number_width(d);
    if split { n + 3 } else { 2 * n + 4 }
}

// ---- drawing -------------------------------------------------------------

pub fn draw(f: &mut Frame, app: &mut crate::app::App, area: Rect, v: &mut DiffView) {
    let prefs = app.diff_prefs;
    let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    f.render_widget(ratatui::widgets::Clear, body);
    app.hits.push((body, Hit::Inert));
    let data = v.data().cloned();

    // The header card, like the main screen's: what is compared, and how.
    let head_rect = Rect::new(body.x, body.y, body.width, 4.min(body.height));
    let block = crate::ui::panel(vec![Span::styled("changes", theme::title())], false);
    let head = block.inner(head_rect);
    f.render_widget(block, head_rect);
    let mut l1 = vec![Span::raw(" "), Span::styled(v.name.clone(), bold())];
    if let Some(b) = &v.branch {
        l1.push(Span::raw("  "));
        l1.push(Span::styled(b.clone(), fg(theme::PLUM)));
    }
    let mut l2 = vec![Span::raw(" ")];
    if let Some(d) = &data {
        l1.push(Span::styled(format!("  vs {}", d.diff.base), muted()));
        let (a, r) = d
            .files
            .iter()
            .fold((0, 0), |(a, r), f| (a + f.added, r + f.removed));
        let n = d.files.len();
        let right = vec![
            Span::styled(format!("{n} file{} · ", if n == 1 { "" } else { "s" }), muted()),
            Span::styled(format!("+{a}"), fg(SAGE)),
            Span::styled(format!(" −{r} "), fg(RED)),
        ];
        right_align(&mut l1, right, head.width as usize);
        let modes = [
            if prefs.split { "split" } else { "unified" },
            if prefs.wrap { "wrapped" } else { "no wrap" },
            if prefs.by_changes {
                "most changed first"
            } else {
                "by path"
            },
            if prefs.grouped { "by package" } else { "flat" },
        ];
        for (k, m) in modes.iter().enumerate() {
            if k > 0 {
                l2.push(Span::styled(" · ", fg(LINE)));
            }
            l2.push(Span::styled(*m, muted()));
        }
        if let Load::Ready(_, hl) = &v.load
            && !hl.finished()
        {
            let right = vec![Span::styled(
                format!("{} highlighting ", theme::spinner(app.now_ms)),
                muted(),
            )];
            right_align(&mut l2, right, head.width as usize);
        }
    }
    crate::ui::put(f, crate::ui::row(head, 0), Line::from(l1));
    crate::ui::put(f, crate::ui::row(head, 1), Line::from(l2));
    let main = Rect::new(
        body.x,
        body.y + head_rect.height,
        body.width,
        body.height.saturating_sub(head_rect.height),
    );

    let message = |f: &mut Frame, text: Line<'static>| {
        let block = crate::ui::panel(vec![Span::styled("files", theme::title())], true);
        let inner = block.inner(main);
        f.render_widget(block, main);
        crate::ui::put(f, crate::ui::row(inner, 1), text);
    };
    let d = match (&v.load, data) {
        (Load::Loading, _) => {
            let msg = format!("  {} reading changes from git…", theme::spinner(app.now_ms));
            message(f, Line::from(Span::styled(msg, muted())));
            return;
        }
        (Load::Failed(e), _) => {
            message(
                f,
                Line::from(Span::styled(format!("  can't show the changes: {e}"), fg(RED))),
            );
            return;
        }
        (_, Some(d)) => d,
        _ => return,
    };
    if d.files.is_empty() {
        let msg = format!(
            "  nothing committed, staged or modified: {} matches {}",
            v.name, d.diff.base
        );
        message(f, Line::from(Span::styled(msg, muted())));
        return;
    }

    // file list | diff, a split like the main screen's: drag the border
    // between them, or { } to move it, = to reset.
    let tree = prefs.tree && main.width >= 70;
    let (list_rect, diff_rect) = if tree {
        let rects = app.panes.diff.layout(main);
        (Some(rects[0]), rects[1])
    } else {
        (None, main)
    };
    // Rows are built for the inner width before the title needs the top file.
    let inner_w = diff_rect.width.saturating_sub(2);
    v.page = diff_rect.height.saturating_sub(2) as usize;
    v.ensure_rows(inner_w, prefs);
    let max_top = v.rows.len().saturating_sub(v.page / 2);
    v.top = v.top.min(max_top);
    if let Some(r) = list_rect {
        draw_list(f, app, r, v, &d, prefs);
    }
    // The diff panel is titled with the file at its top.
    let mut title = vec![];
    if let Some(fi) = v.top_file() {
        let file = &d.files[fi];
        let stats = format!(" +{} −{}", file.added, file.removed);
        let room = (inner_w as usize).saturating_sub(width(&stats) + 4);
        title.push(Span::styled(clip_left(&file.path, room), theme::title()));
        title.push(Span::styled(format!(" +{}", file.added), fg(SAGE)));
        title.push(Span::styled(format!(" −{}", file.removed), fg(RED)));
    }
    let block = crate::ui::panel(title, !tree);
    let inner = block.inner(diff_rect);
    f.render_widget(block, diff_rect);
    // Hits are found newest first: whole-panel scroll areas go in before
    // the rows and the divider drawn over them.
    app.hits.push((diff_rect, Hit::ScrollDiff));
    draw_rows(f, inner, v, &d, prefs);
    crate::ui::scrollbar(
        f,
        Rect::new(diff_rect.right() - 1, inner.y, 1, inner.height),
        v.top,
        inner.height as usize,
        v.rows.len(),
    );
    if tree {
        app.hits.extend(
            app.panes
                .diff
                .dividers()
                .iter()
                .map(|&(p, r)| (r, Hit::Divider(crate::panes::SplitId::Diff, p))),
        );
    }
}

/// Pads `left` so `right` ends at the line's last column.
fn right_align(left: &mut Vec<Span<'static>>, right: Vec<Span<'static>>, w: usize) {
    let used: usize = left.iter().chain(right.iter()).map(|s| width(&s.content)).sum();
    left.push(Span::raw(" ".repeat(w.saturating_sub(used))));
    left.extend(right);
}

fn draw_list(
    f: &mut Frame,
    app: &mut crate::app::App,
    area: Rect,
    v: &mut DiffView,
    d: &DiffData,
    prefs: DiffPrefs,
) {
    let entries = v.entries(prefs);
    let cur = v.current(&entries);
    let groups_n = entries.iter().filter(|e| matches!(e, Entry::Group(_))).count();
    let title = if prefs.grouped {
        vec![
            Span::styled("packages", theme::title()),
            Span::styled(format!(" {groups_n}"), muted()),
        ]
    } else {
        vec![
            Span::styled("files", theme::title()),
            Span::styled(format!(" {}", d.files.len()), muted()),
        ]
    };
    let block = crate::ui::panel(title, true);
    let outer = area;
    let area = block.inner(outer);
    f.render_widget(block, outer);
    app.hits.push((outer, Hit::ScrollDiffList));
    let h = area.height as usize;
    // Keep the current entry in view.
    if let Some(c) = cur {
        if c < v.list_top {
            v.list_top = c;
        } else if c >= v.list_top + h {
            v.list_top = c + 1 - h;
        }
    }
    v.list_top = v.list_top.min(entries.len().saturating_sub(h));
    // One column of padding on each side, like the clone list.
    let w = area.width.saturating_sub(2) as usize;
    let mut groups: std::collections::HashMap<&str, (usize, u32, u32)> = Default::default();
    for (i, file) in d.files.iter().enumerate() {
        let g = groups.entry(d.group[i].as_str()).or_default();
        g.0 += 1;
        g.1 += file.added;
        g.2 += file.removed;
    }
    for (row, (i, e)) in entries.iter().enumerate().skip(v.list_top).take(h).enumerate() {
        let r = Rect::new(area.x, area.y + row as u16, area.width, 1);
        let selected = cur == Some(i);
        let line = match e {
            Entry::Group(g) => {
                let (n, a, rm) = groups.get(g.as_str()).copied().unwrap_or_default();
                let open = v.open_groups.contains(g);
                let pkg = d.package(g);
                let label = if g.is_empty() {
                    "repository root".to_string()
                } else {
                    g.clone()
                };
                let stats = format!(" {n} +{a} −{rm}");
                let room = w.saturating_sub(width(&stats) + 4);
                let mut s = vec![
                    Span::styled(if open { "▾ " } else { "▸ " }, muted()),
                    Span::styled(
                        if pkg.is_some() { "◆ " } else { "▢ " },
                        fg(if pkg.is_some() { BOTTLE } else { MUTED }),
                    ),
                    Span::styled(clip(&label, room), bold()),
                ];
                let used: usize = s.iter().map(|s| width(&s.content)).sum();
                s.push(Span::raw(" ".repeat(w.saturating_sub(used + width(&stats)))));
                s.push(Span::styled(format!(" {n}"), muted()));
                s.push(Span::styled(format!(" +{a}"), fg(SAGE)));
                s.push(Span::styled(format!(" −{rm}"), fg(RED)));
                s
            }
            Entry::File(fi) => {
                let file = &d.files[*fi];
                let (letter, c) = match file.change {
                    Change::Added => ("A", SAGE),
                    Change::Deleted => ("D", RED),
                    Change::Renamed => ("R", theme::SYN_FUNCTION),
                    Change::Modified => ("M", AMBER),
                };
                let mut name = file.path.clone();
                if prefs.grouped {
                    let g = &d.group[*fi];
                    if !g.is_empty() {
                        name = name.strip_prefix(&format!("{g}/")).unwrap_or(&name).to_string();
                    }
                }
                let stats = format!(" +{} −{}", file.added, file.removed);
                let indent = if prefs.grouped { "  " } else { "" };
                let room = w.saturating_sub(width(&stats) + 2 + indent.len());
                // Folded files read dimmer; the diff header says why.
                let name_style = if d.folds[*fi].is_some() {
                    muted()
                } else {
                    Style::new()
                };
                let mut s = vec![
                    Span::raw(indent),
                    Span::styled(format!("{letter} "), fg(c).add_modifier(Modifier::BOLD)),
                    Span::styled(clip_left(&name, room), name_style),
                ];
                let used: usize = s.iter().map(|s| width(&s.content)).sum();
                s.push(Span::raw(" ".repeat(w.saturating_sub(used + width(&stats)))));
                s.push(Span::styled(format!(" +{}", file.added), fg(SAGE)));
                s.push(Span::styled(format!(" −{}", file.removed), fg(RED)));
                s
            }
        };
        let mut spans = vec![Span::raw(" ")];
        spans.extend(line);
        let used: usize = spans.iter().map(|s| width(&s.content)).sum();
        spans.push(Span::raw(" ".repeat((r.width as usize).saturating_sub(used))));
        let mut line = Line::from(spans);
        if selected {
            // The clone list's selection: accent fill, cream text.
            line = Line::from(
                line.spans
                    .into_iter()
                    .map(|s| Span::styled(s.content, theme::selected()))
                    .collect::<Vec<_>>(),
            );
        }
        crate::ui::put(f, r, line);
        app.hits.push((r, Hit::DiffEntry(i)));
    }
    crate::ui::scrollbar(
        f,
        Rect::new(outer.right() - 1, area.y, 1, area.height),
        v.list_top,
        h,
        entries.len(),
    );
}

fn fold_label(f: Fold) -> &'static str {
    match f {
        Fold::Binary => "binary",
        Fold::Generated => "generated",
        Fold::Lockfile => "lockfile",
        Fold::Large => "large",
    }
}

fn clip(s: &str, w: usize) -> String {
    if width(s) <= w {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        if width(&out) + 2 > w {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// Keeps the end of a path, where the file name is.
fn clip_left(s: &str, w: usize) -> String {
    if width(s) <= w {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<char> = vec![];
    for c in chars.iter().rev() {
        if out.len() + 2 > w {
            break;
        }
        out.push(*c);
    }
    out.push('…');
    out.iter().rev().collect()
}

fn draw_rows(f: &mut Frame, area: Rect, v: &mut DiffView, d: &DiffData, prefs: DiffPrefs) {
    let Load::Ready(_, hl) = &v.load else { return };
    let hl = hl.clone();
    let tints = theme::tints();
    let nw = number_width(d);
    let text_area_w = area.width;
    let rows = v.rows.clone();
    let mut cache: Option<(usize, Option<FileHl>)> = None;
    for y in 0..area.height as usize {
        let Some(row) = rows.get(v.top + y) else { break };
        let r = Rect::new(area.x, area.y + y as u16, text_area_w, 1);
        if let Some(fi) = row_file(row)
            && cache.as_ref().map(|c| c.0) != Some(fi)
        {
            cache = Some((fi, hl.file(fi)));
        }
        let fhl = cache.as_ref().and_then(|c| c.1.clone());
        let line = match *row {
            Row::Gap => Line::default(),
            Row::File(fi) => file_header(d, fi, r.width as usize, v.opened.contains(&fi)),
            Row::Folded(fi) => {
                let why = match d.folds[fi] {
                    Some(Fold::Binary) => "binary file, not shown".to_string(),
                    Some(Fold::Generated) => "generated (.gitattributes) · z shows it".to_string(),
                    Some(Fold::Lockfile) => "lockfile · z shows it".to_string(),
                    Some(Fold::Large) => format!(
                        "{} changed lines · z shows them",
                        d.files[fi].added + d.files[fi].removed
                    ),
                    None => String::new(),
                };
                Line::from(Span::styled(format!("   {why}"), muted()))
            }
            Row::Hunk(fi, h) => {
                let hunk = &d.files[fi].hunks[h];
                let txt = format!(
                    "{}@@ -{} +{} @@ {}",
                    " ".repeat(if prefs.split { nw + 3 } else { 2 * nw + 4 }),
                    hunk.old_start,
                    hunk.new_start,
                    hunk.context
                );
                Line::from(Span::styled(
                    clip(&txt, r.width as usize),
                    fg(theme::SYN_FUNCTION),
                ))
            }
            Row::Line { f: fi, h, l, part } => {
                let line = &d.files[fi].hunks[h].lines[l];
                let lh = fhl.as_ref().and_then(|x| x.get(h)).and_then(|x| x.get(l));
                unified(
                    line,
                    lh,
                    nw,
                    r.width as usize,
                    part,
                    v.hscroll,
                    prefs.wrap,
                    &tints,
                )
            }
            Row::Pair {
                f: fi,
                h,
                left,
                right,
                part,
            } => {
                let hunk = &d.files[fi].hunks[h];
                let side = r.width as usize / 2;
                let get = |i: Option<usize>| {
                    i.map(|i| {
                        (
                            &hunk.lines[i],
                            fhl.as_ref().and_then(|x| x.get(h)).and_then(|x| x.get(i)),
                        )
                    })
                };
                let mut spans = half(
                    get(left),
                    Side::Old,
                    nw,
                    side,
                    part,
                    v.hscroll,
                    prefs.wrap,
                    &tints,
                );
                spans.push(Span::styled("│", fg(LINE)));
                spans.extend(half(
                    get(right),
                    Side::New,
                    nw,
                    r.width as usize - side - 1,
                    part,
                    v.hscroll,
                    prefs.wrap,
                    &tints,
                ));
                Line::from(spans)
            }
        };
        crate::ui::put(f, r, line);
    }
}

fn file_header(d: &DiffData, fi: usize, w: usize, opened: bool) -> Line<'static> {
    let file = &d.files[fi];
    let (word, c) = match file.change {
        Change::Added => ("added", SAGE),
        Change::Deleted => ("deleted", RED),
        Change::Renamed => ("renamed", theme::SYN_FUNCTION),
        Change::Modified => ("", AMBER),
    };
    let mut s = vec![Span::styled("▍", fg(c)), Span::styled(file.path.clone(), bold())];
    if let Some(old) = &file.old_path {
        s.push(Span::styled(format!("  ← {old}"), muted()));
    }
    if !word.is_empty() {
        s.push(Span::styled(format!("  {word}"), fg(c)));
    }
    if let Some(fold) = d.folds[fi] {
        let l = fold_label(fold);
        s.push(Span::styled(
            if opened {
                format!("  {l} · z folds")
            } else {
                format!("  {l}")
            },
            muted(),
        ));
    }
    let stats = format!("+{} −{} ", file.added, file.removed);
    // A long path gives way from the left: the file name and counts stay.
    let used: usize = s.iter().map(|s| width(&s.content)).sum();
    let over = (used + width(&stats) + 1).saturating_sub(w);
    if over > 0 {
        let path_w = width(&file.path).saturating_sub(over).max(8);
        s[1] = Span::styled(clip_left(&file.path, path_w), bold());
    }
    let used: usize = s.iter().map(|s| width(&s.content)).sum();
    s.push(Span::raw(" ".repeat(w.saturating_sub(used + width(&stats)))));
    s.push(Span::styled(format!("+{}", file.added), fg(SAGE)));
    s.push(Span::styled(format!(" −{} ", file.removed), fg(RED)));
    Line::from(s).style(Style::new().add_modifier(Modifier::BOLD))
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Old,
    New,
}

/// A unified row: both line numbers, the sign, then the text.
#[allow(clippy::too_many_arguments)]
fn unified(
    line: &clove_core::patch::Line,
    lh: Option<&LineHl>,
    nw: usize,
    w: usize,
    part: u16,
    hscroll: usize,
    wrap: bool,
    tints: &Option<theme::Tints>,
) -> Line<'static> {
    let num = |n: Option<u32>| n.map(|n| format!("{n:>nw$}")).unwrap_or_else(|| " ".repeat(nw));
    let (sign, sign_c, bg, word_bg) = kind_style(line.kind, tints);
    let first = part == 0;
    let mut spans = vec![
        Span::styled(
            if first {
                format!("{} {} ", num(line.old), num(line.new))
            } else {
                " ".repeat(2 * nw + 2)
            },
            Style::new().fg(if line.kind == LineKind::Context {
                LINE
            } else {
                sign_c
            }),
        ),
        Span::styled(
            if first { sign } else { " " },
            fg(sign_c).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];
    let text_w = w.saturating_sub(2 * nw + 4);
    let start = if wrap { part as usize * text_w } else { hscroll };
    spans.extend(text_spans(line, lh, start, text_w, word_bg));
    let mut l = Line::from(spans);
    if let Some(bg) = bg {
        l = l.style(Style::new().bg(bg));
    }
    pad(l, w, bg)
}

/// One side of a split row.
#[allow(clippy::too_many_arguments)]
fn half(
    line: Option<(&clove_core::patch::Line, Option<&LineHl>)>,
    side: Side,
    nw: usize,
    w: usize,
    part: u16,
    hscroll: usize,
    wrap: bool,
    tints: &Option<theme::Tints>,
) -> Vec<Span<'static>> {
    let Some((line, lh)) = line else {
        // The other side has a line here; this side has none.
        return vec![Span::styled("╱".repeat(w), fg(LINE).add_modifier(Modifier::DIM))];
    };
    let kind = match (line.kind, side) {
        (LineKind::Context, _) => LineKind::Context,
        (_, Side::Old) => LineKind::Removed,
        (_, Side::New) => LineKind::Added,
    };
    let (sign, sign_c, bg, word_bg) = kind_style(kind, tints);
    let n = if side == Side::Old { line.old } else { line.new };
    let first = part == 0;
    let mut spans = vec![
        Span::styled(
            if first {
                n.map(|n| format!("{n:>nw$} "))
                    .unwrap_or_else(|| " ".repeat(nw + 1))
            } else {
                " ".repeat(nw + 1)
            },
            Style::new().fg(if kind == LineKind::Context { LINE } else { sign_c }),
        ),
        Span::styled(
            if first { sign } else { " " },
            fg(sign_c).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];
    let text_w = w.saturating_sub(nw + 3);
    let start = if wrap { part as usize * text_w } else { hscroll };
    spans.extend(text_spans(line, lh, start, text_w, word_bg));
    let used: usize = spans.iter().map(|s| width(&s.content)).sum();
    if used < w {
        spans.push(Span::raw(" ".repeat(w - used)));
    }
    if let Some(bg) = bg {
        for s in &mut spans {
            if s.style.bg.is_none() {
                s.style = s.style.bg(bg);
            }
        }
    }
    spans
}

fn kind_style(
    kind: LineKind,
    tints: &Option<theme::Tints>,
) -> (&'static str, Color, Option<Color>, Option<Color>) {
    match kind {
        LineKind::Added => (
            "+",
            SAGE,
            tints.as_ref().map(|t| t.added),
            tints.as_ref().map(|t| t.added_word),
        ),
        LineKind::Removed => (
            "-",
            RED,
            tints.as_ref().map(|t| t.removed),
            tints.as_ref().map(|t| t.removed_word),
        ),
        LineKind::Context => (" ", MUTED, None, None),
    }
}

fn pad(mut l: Line<'static>, w: usize, bg: Option<Color>) -> Line<'static> {
    let used: usize = l.spans.iter().map(|s| width(&s.content)).sum();
    if used < w {
        let mut s = Span::raw(" ".repeat(w - used));
        if let Some(bg) = bg {
            s.style = s.style.bg(bg);
        }
        l.spans.push(s);
    }
    l
}

/// The text of a line from display column `start`, `w` columns wide, in its
/// syntax colours, with changed words on a stronger tint (or underlined
/// when the background is unknown). Tabs become four spaces.
fn text_spans(
    line: &clove_core::patch::Line,
    lh: Option<&LineHl>,
    start: usize,
    w: usize,
    word_bg: Option<Color>,
) -> Vec<Span<'static>> {
    let text = &line.text;
    let pieces: Vec<(Style, Range<usize>)> = match lh {
        Some(lh) if !lh.spans.is_empty() => lh.spans.clone(),
        _ => vec![(Style::new(), 0..text.len())],
    };
    let words = lh.map(|l| l.words.as_slice()).unwrap_or(&[]);
    let mut out = vec![];
    let mut col = 0usize;
    let end = start + w;
    for (style, range) in pieces {
        let mut buf = String::new();
        let mut buf_word = false;
        let flush = |buf: &mut String, word: bool, out: &mut Vec<Span<'static>>| {
            if buf.is_empty() {
                return;
            }
            let mut st = style;
            if word {
                st = match word_bg {
                    Some(bg) => st.bg(bg),
                    None => st.add_modifier(Modifier::UNDERLINED),
                };
            }
            out.push(Span::styled(std::mem::take(buf), st));
        };
        let Some(slice) = text.get(range.clone()) else {
            continue;
        };
        for (off, ch) in slice.char_indices() {
            let at = range.start + off;
            let is_word = words.iter().any(|r| r.contains(&at));
            let repr: &str = if ch == '\t' { "    " } else { "" };
            let cw = if ch == '\t' {
                4
            } else {
                unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0)
            };
            if col + cw <= start {
                col += cw;
                continue;
            }
            if col >= end {
                break;
            }
            if is_word != buf_word {
                flush(&mut buf, buf_word, &mut out);
                buf_word = is_word;
            }
            if ch == '\t' {
                buf.push_str(repr);
            } else {
                buf.push(ch);
            }
            col += cw;
        }
        flush(&mut buf, buf_word, &mut out);
        if col >= end {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_runs_side_by_side() {
        let p = clove_core::patch::parse(
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,4 +1,4 @@\n a\n-b\n-c\n+B\n d\n+e\n",
        );
        let pairs = pair_lines(&p[0].hunks[0].lines);
        assert_eq!(
            pairs,
            [
                (Some(0), Some(0)),
                (Some(1), Some(3)),
                (Some(2), None),
                (Some(4), Some(4)),
                (None, Some(5)),
            ]
        );
    }

    #[test]
    fn marks_changed_words_only() {
        let p = clove_core::patch::parse(
            "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-let max_age = 60 * 60;\n+let max_age = 60 * 60 * 7;\n",
        );
        let hl = highlight_file(&p[0]);
        let (old, new) = (&hl[0][0], &hl[0][1]);
        assert!(old.words.is_empty());
        let added: String = new
            .words
            .iter()
            .map(|r| &p[0].hunks[0].lines[1].text[r.clone()])
            .collect();
        assert_eq!(added.trim(), "* 7");
        assert!(!new.spans.is_empty(), "rust gets highlighted");
    }
}
