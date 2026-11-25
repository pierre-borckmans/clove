//! Hover tooltips. Drawing code registers a [`Tip`] for a region in
//! `app.tips` (like `app.hits`); after a short hover, [`Tooltip`] draws the
//! topmost one next to the mouse, over everything else.

use crate::fmt::{self, width};
use crate::theme::*;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph, Widget};
use unicode_width::UnicodeWidthChar;

/// How long the mouse rests on something before its tooltip shows.
pub const DELAY_MS: u64 = 500;
/// Widest tooltip, borders included.
pub const MAX_WIDTH: u16 = 50;
/// Text lines past this end in `…`.
const MAX_LINES: usize = 12;

/// What a tooltip says: the text, then an optional muted note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tip {
    pub text: String,
    pub note: Option<String>,
}

impl Tip {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            note: None,
        }
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// Cuts `s` to `w` columns like [`fmt::trunc`], with a tip holding the full
/// text when it had to cut.
pub fn cut(s: &str, w: usize) -> (String, Option<Tip>) {
    let shown = fmt::trunc(s, w);
    let tip = (shown != s).then(|| Tip::new(s));
    (shown, tip)
}

/// [`cut`] keeping the end, like [`fmt::trunc_left`] (paths).
pub fn cut_left(s: &str, w: usize) -> (String, Option<Tip>) {
    let shown = fmt::trunc_left(s, w);
    let tip = (shown != s).then(|| Tip::new(s));
    (shown, tip)
}

/// Tips for spans of lines whose screen position is known only once they are
/// laid out: (line, span, tip).
#[derive(Debug, Default, Clone)]
pub struct SpanTips(pub Vec<(usize, usize, Tip)>);

impl SpanTips {
    pub fn add(&mut self, line: usize, span: usize, tip: Option<Tip>) {
        if let Some(t) = tip {
            self.0.push((line, span, t));
        }
    }

    /// Screen regions of the tipped spans of `lines` drawn in `area` (no
    /// wrapping) from line `off` on.
    pub fn place(&self, lines: &[Line], area: Rect, off: usize, out: &mut Vec<(Rect, Tip)>) {
        for (li, si, tip) in &self.0 {
            let (Some(line), Some(y)) = (lines.get(*li), li.checked_sub(off)) else {
                continue;
            };
            if y >= area.height as usize || *si >= line.spans.len() {
                continue;
            }
            let x: usize = line.spans[..*si].iter().map(|s| width(&s.content)).sum();
            let w = width(&line.spans[*si].content);
            let r = Rect::new(
                area.x.saturating_add(x.min(u16::MAX as usize) as u16),
                area.y + y as u16,
                w.min(u16::MAX as usize) as u16,
                1,
            )
            .intersection(area);
            if !r.is_empty() {
                out.push((r, tip.clone()));
            }
        }
    }
}

/// Word wrap to `w` columns that breaks words longer than a line instead of
/// cutting them: a tooltip shows the whole text.
fn wrap(s: &str, w: usize) -> Vec<String> {
    let w = w.max(1);
    let mut out = vec![];
    for para in s.lines() {
        let mut line = String::new();
        let mut lw = 0;
        for word in para.split(' ') {
            let ww = width(word);
            if lw > 0 && lw + 1 + ww > w {
                out.push(std::mem::take(&mut line));
                lw = 0;
            }
            if lw > 0 {
                line.push(' ');
                lw += 1;
            }
            for c in word.chars() {
                let cw = c.width().unwrap_or(0);
                if lw + cw > w {
                    out.push(std::mem::take(&mut line));
                    lw = 0;
                }
                line.push(c);
                lw += cw;
            }
        }
        out.push(line);
    }
    out
}

/// A small rounded box with the tip's text, drawn over whatever is under it.
pub struct Tooltip<'a> {
    tip: &'a Tip,
}

impl<'a> Tooltip<'a> {
    pub fn new(tip: &'a Tip) -> Self {
        Self { tip }
    }

    fn lines(&self, max_text: usize) -> Vec<Line<'static>> {
        let mut lines: Vec<Line> = wrap(&self.tip.text, max_text)
            .into_iter()
            .map(Line::from)
            .collect();
        if let Some(n) = &self.tip.note {
            lines.extend(
                wrap(n, max_text)
                    .into_iter()
                    .map(|l| Line::from(Span::styled(l, muted()))),
            );
        }
        if lines.len() > MAX_LINES {
            lines.truncate(MAX_LINES);
            let last = &mut lines[MAX_LINES - 1];
            let s = last.to_string();
            *last = Line::from(fmt::trunc(&format!("{s} …"), max_text));
        }
        lines
    }

    /// Where it goes for the mouse at `at`: below and right of it, flipped
    /// above when it would leave the bottom, slid left at the right edge, and
    /// kept off the hovered cell.
    pub fn area(&self, at: Position, screen: Rect) -> Rect {
        let max_w = MAX_WIDTH.min(screen.width);
        let lines = self.lines(max_w.saturating_sub(4).max(1) as usize);
        let tw = lines.iter().map(|l| width(&l.to_string())).max().unwrap_or(0);
        let w = (tw as u16 + 4).min(max_w);
        let h = (lines.len() as u16 + 2).min(screen.height);
        let fits_below = at.y + 1 + h <= screen.bottom();
        let fits_above = at.y >= screen.y + h;
        let (y, covers_row) = if fits_below {
            (at.y + 1, false)
        } else if fits_above {
            (at.y - h, false)
        } else {
            (screen.bottom().saturating_sub(h).max(screen.y), true)
        };
        let right = screen.right().saturating_sub(w).max(screen.x);
        let mut x = (at.x + 1).min(right);
        // Over the hovered row: stay beside the cell rather than on it.
        if covers_row && x <= at.x && at.x < x + w {
            x = if at.x + 1 + w <= screen.right() {
                at.x + 1
            } else {
                at.x.saturating_sub(w).max(screen.x)
            };
        }
        Rect::new(x, y, w, h).intersection(screen)
    }
}

impl Widget for Tooltip<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 3 || area.height < 3 {
            return;
        }
        Clear.render(area, buf);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(fg(BOTTLE))
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        let lines = self.lines(inner.width.max(1) as usize);
        block.render(area, buf);
        Paragraph::new(lines).render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_tips_only_when_cut() {
        assert_eq!(cut("short", 10), ("short".into(), None));
        let (s, t) = cut("a-rather-long-name", 8);
        assert_eq!(s, "a-rathe…");
        assert_eq!(t, Some(Tip::new("a-rather-long-name")));
        let (s, t) = cut_left("/a/b/c/d", 5);
        assert_eq!(s, "…/c/d");
        assert_eq!(t.unwrap().text, "/a/b/c/d");
    }

    #[test]
    fn wraps_long_words_whole() {
        assert_eq!(wrap("abcdefgh ij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap("aa bb cc", 5), vec!["aa bb", "cc"]);
    }

    #[test]
    fn places_below_right_and_flips_at_edges() {
        let screen = Rect::new(0, 0, 80, 24);
        let tip = Tip::new("hello world");
        let t = Tooltip::new(&tip);
        // Plain: below-right of the mouse.
        assert_eq!(t.area(Position::new(10, 5), screen), Rect::new(11, 6, 15, 3));
        // Right edge: slides left, still below.
        let r = t.area(Position::new(79, 5), screen);
        assert_eq!((r.right(), r.y), (80, 6));
        // Bottom edge: above the mouse.
        let r = t.area(Position::new(10, 23), screen);
        assert_eq!(r.bottom(), 23);
        // Corner: both.
        let r = t.area(Position::new(79, 23), screen);
        assert_eq!((r.right(), r.bottom()), (80, 23));
        assert!(!r.contains(Position::new(79, 23)));
    }

    #[test]
    fn stays_off_the_hovered_cell_when_tall() {
        let screen = Rect::new(0, 0, 80, 6);
        let tip = Tip::new("one two three four five six seven eight nine ten ".repeat(3));
        let t = Tooltip::new(&tip);
        let at = Position::new(70, 3);
        let r = t.area(at, screen);
        assert!(screen.contains(r.as_position()) && r.right() <= 80 && r.bottom() <= 6);
        assert!(!r.contains(at), "{r:?}");
    }

    #[test]
    fn span_tips_land_on_their_span() {
        let lines = vec![
            Line::from("skipped"),
            Line::from(vec![Span::raw("ab"), Span::raw("cdef"), Span::raw("g")]),
        ];
        let mut st = SpanTips::default();
        st.add(1, 1, Some(Tip::new("x")));
        st.add(0, 0, None);
        let mut out = vec![];
        st.place(&lines, Rect::new(10, 3, 20, 5), 0, &mut out);
        assert_eq!(out, vec![(Rect::new(12, 4, 4, 1), Tip::new("x"))]);
        out.clear();
        st.place(&lines, Rect::new(10, 3, 20, 5), 1, &mut out);
        assert_eq!(out[0].0, Rect::new(12, 3, 4, 1));
    }
}
