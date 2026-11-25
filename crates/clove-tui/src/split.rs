//! A resizable split: lays a `Rect` out as a row or column of panes with
//! draggable dividers between them. Exactly one pane fills what the others
//! leave; every other pane has a size the user can change.
//!
//! Sizes are stored in cells, not as ratios: panel content is text measured in
//! columns, so a 48-column detail panel stays 48 columns on any terminal. The
//! stored size is the user's wish; `layout` clamps it to the pane's bounds and
//! to the area every frame without changing it, so shrinking the terminal and
//! growing it back restores the chosen size.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::Color;

/// Two clicks on a divider this close together reset its pane.
pub const DOUBLE_CLICK_MS: u64 = 400;

/// A pane bound: whole cells, or a fraction of the split's length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extent {
    Cells(u16),
    Frac(f32),
}

impl Extent {
    fn of(self, total: u16) -> u16 {
        match self {
            Extent::Cells(n) => n.min(total),
            Extent::Frac(f) => (total as f32 * f.clamp(0.0, 1.0)).round() as u16,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Pane {
    min: Extent,
    max: Extent,
    default: u16,
    fill: bool,
    /// The user's size; None means the default.
    size: Option<u16>,
    shown: bool,
}

impl Pane {
    /// The pane that takes whatever the others leave, never below `min`.
    pub fn fill(min: Extent) -> Self {
        Self {
            min,
            max: Extent::Frac(1.0),
            default: 0,
            fill: true,
            size: None,
            shown: true,
        }
    }

    /// A resizable pane starting at `default` cells.
    pub fn sized(default: u16, min: Extent, max: Extent) -> Self {
        Self {
            min,
            max,
            default,
            fill: false,
            size: None,
            shown: true,
        }
    }
}

/// What a press on a divider did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// Started a drag.
    Drag,
    /// A double click: the pane went back to its default size.
    Reset,
}

#[derive(Clone, Debug)]
struct Drag {
    pane: usize,
    from: u16,
    start: u16,
}

/// The last layout, which drags and keyboard moves work against.
#[derive(Clone, Debug, Default)]
struct Laid {
    area: Rect,
    sizes: Vec<u16>,
    dividers: Vec<(usize, Rect)>,
}

#[derive(Clone, Debug)]
pub struct Split {
    dir: Direction,
    panes: Vec<Pane>,
    spacing: u16,
    laid: Laid,
    drag: Option<Drag>,
    click: Option<(u64, usize)>,
}

impl Split {
    pub fn new(dir: Direction, panes: Vec<Pane>) -> Self {
        debug_assert_eq!(panes.iter().filter(|p| p.fill).count(), 1, "one fill pane");
        Self {
            dir,
            panes,
            spacing: 0,
            laid: Laid::default(),
            drag: None,
            click: None,
        }
    }

    /// Blank cells between panes (part of the divider's grab area).
    pub fn spacing(mut self, n: u16) -> Self {
        self.spacing = n;
        self
    }

    pub fn dir(&self) -> Direction {
        self.dir
    }

    pub fn show(&mut self, pane: usize, shown: bool) {
        self.panes[pane].shown = shown;
    }

    pub fn shown(&self, pane: usize) -> bool {
        self.panes[pane].shown
    }

    /// Stored sizes (None: default), for persisting.
    pub fn sizes(&self) -> Vec<Option<u16>> {
        self.panes.iter().map(|p| p.size).collect()
    }

    /// Restores persisted sizes; ignored unless there is one per pane.
    pub fn set_sizes(&mut self, sizes: &[Option<u16>]) {
        if sizes.len() != self.panes.len() {
            return;
        }
        for (p, s) in self.panes.iter_mut().zip(sizes) {
            p.size = if p.fill { None } else { *s };
        }
    }

    pub fn reset(&mut self, pane: usize) {
        self.panes[pane].size = None;
    }

    pub fn reset_all(&mut self) {
        for p in &mut self.panes {
            p.size = None;
        }
    }

    /// The size the pane got in the last layout.
    pub fn laid_size(&self, pane: usize) -> u16 {
        self.laid.sizes.get(pane).copied().unwrap_or(0)
    }

    fn fill_idx(&self) -> usize {
        self.panes.iter().position(|p| p.fill).unwrap_or(0)
    }

    fn len(&self, r: Rect) -> u16 {
        match self.dir {
            Direction::Horizontal => r.width,
            Direction::Vertical => r.height,
        }
    }

    fn axis(&self, p: Position) -> u16 {
        match self.dir {
            Direction::Horizontal => p.x,
            Direction::Vertical => p.y,
        }
    }

    /// Room for sized panes in `total`: what spacing and the fill pane's
    /// minimum leave.
    fn room(&self, total: u16) -> u16 {
        let vis = self.panes.iter().filter(|p| p.shown).count() as u16;
        let fill = &self.panes[self.fill_idx()];
        let fill_min = if fill.shown { fill.min.of(total) } else { 0 };
        total.saturating_sub(self.spacing * vis.saturating_sub(1) + fill_min)
    }

    fn bounds(&self, pane: usize, total: u16) -> (u16, u16) {
        let p = &self.panes[pane];
        let lo = p.min.of(total);
        (lo, p.max.of(total).max(lo))
    }

    /// Splits `area`; hidden panes get an empty rect. Records divider positions.
    pub fn layout(&mut self, area: Rect) -> Vec<Rect> {
        let total = self.len(area);
        let room = self.room(total);
        let n = self.panes.len();
        let mut sizes = vec![0u16; n];
        for (i, p) in self.panes.iter().enumerate() {
            if p.shown && !p.fill {
                let (lo, hi) = self.bounds(i, total);
                sizes[i] = p.size.unwrap_or(p.default).clamp(lo, hi);
            }
        }
        // Over budget: shrink from the last pane, first to minimums, then below.
        let mut over = sizes
            .iter()
            .map(|&s| s as u32)
            .sum::<u32>()
            .saturating_sub(room as u32);
        for floor in [true, false] {
            for i in (0..n).rev() {
                if over == 0 {
                    break;
                }
                let lo = if floor { self.bounds(i, total).0 } else { 0 };
                let cut = (sizes[i].saturating_sub(lo) as u32).min(over);
                sizes[i] -= cut as u16;
                over -= cut;
            }
        }

        let vis: Vec<usize> = (0..n).filter(|&i| self.panes[i].shown).collect();
        let constraints = vis.iter().map(|&i| {
            if self.panes[i].fill {
                Constraint::Fill(1)
            } else {
                Constraint::Length(sizes[i])
            }
        });
        let parts = Layout::new(self.dir, constraints)
            .spacing(self.spacing)
            .split(area);
        let mut rects = vec![Rect::new(area.x, area.y, 0, 0); n];
        for (k, &i) in vis.iter().enumerate() {
            rects[i] = parts[k];
            sizes[i] = self.len(parts[k]);
        }

        // A divider spans a's last cell, the gap, and b's first cell (the two
        // facing borders). Panes before the fill own their trailing edge.
        let fill = self.fill_idx();
        let mut dividers = vec![];
        for w in vis.windows(2) {
            let (a, b) = (w[0], w[1]);
            let owner = if a < fill { a } else { b };
            let r = match self.dir {
                Direction::Horizontal => {
                    let x = rects[a].right().saturating_sub(1).max(area.x);
                    Rect::new(x, area.y, (rects[b].x + 1).saturating_sub(x), area.height)
                }
                Direction::Vertical => {
                    let y = rects[a].bottom().saturating_sub(1).max(area.y);
                    Rect::new(area.x, y, area.width, (rects[b].y + 1).saturating_sub(y))
                }
            };
            dividers.push((owner, r.intersection(area)));
        }
        self.laid = Laid {
            area,
            sizes,
            dividers,
        };
        rects
    }

    /// (pane it resizes, rect) for each divider in the last layout.
    pub fn dividers(&self) -> &[(usize, Rect)] {
        &self.laid.dividers
    }

    pub fn divider(&self, pane: usize) -> Option<Rect> {
        self.laid
            .dividers
            .iter()
            .find(|(p, _)| *p == pane)
            .map(|(_, r)| *r)
    }

    /// Bounds a pane can take right now: its own, and what the area leaves.
    fn live_bounds(&self, pane: usize) -> (u16, u16) {
        let total = self.len(self.laid.area);
        let (lo, hi) = self.bounds(pane, total);
        let others: u16 = (0..self.panes.len())
            .filter(|&i| i != pane && self.panes[i].shown && !self.panes[i].fill)
            .map(|i| self.laid_size(i))
            .sum();
        let hi = hi.min(self.room(total).saturating_sub(others));
        (lo.min(hi), hi)
    }

    /// +1 when moving the divider toward the end grows the pane.
    fn sign(&self, pane: usize) -> i32 {
        if pane < self.fill_idx() { 1 } else { -1 }
    }

    fn resize_to(&mut self, pane: usize, want: i32) -> bool {
        let (lo, hi) = self.live_bounds(pane);
        let new = want.clamp(lo as i32, hi as i32) as u16;
        let changed = new != self.laid_size(pane);
        if changed || self.panes[pane].size.is_some() {
            self.panes[pane].size = Some(new);
        }
        if let Some(s) = self.laid.sizes.get_mut(pane) {
            *s = new;
        }
        changed
    }

    /// Mouse down on the divider owned by `pane`.
    pub fn press(&mut self, pane: usize, pos: Position, now_ms: u64) -> Press {
        let double = self
            .click
            .is_some_and(|(t, p)| p == pane && now_ms.saturating_sub(t) < DOUBLE_CLICK_MS);
        if double {
            self.click = None;
            self.drag = None;
            self.reset(pane);
            return Press::Reset;
        }
        self.click = Some((now_ms, pane));
        self.drag = Some(Drag {
            pane,
            from: self.axis(pos),
            start: self.laid_size(pane),
        });
        Press::Drag
    }

    /// Mouse moved with the button held. True if a size changed.
    pub fn drag_to(&mut self, pos: Position) -> bool {
        let Some(d) = self.drag.clone() else {
            return false;
        };
        let delta = self.axis(pos) as i32 - d.from as i32;
        let changed = self.resize_to(d.pane, d.start as i32 + self.sign(d.pane) * delta);
        // A press that dragged something is not half of a double click.
        if changed {
            self.click = None;
        }
        changed
    }

    /// Mouse up. True if a drag was in progress.
    pub fn release(&mut self) -> bool {
        self.drag.take().is_some()
    }

    /// The pane whose divider is being dragged.
    pub fn dragging(&self) -> Option<usize> {
        self.drag.as_ref().map(|d| d.pane)
    }

    /// Moves `pane`'s divider by `delta` cells (positive: right/down).
    pub fn move_divider(&mut self, pane: usize, delta: i32) -> bool {
        let cur = self.laid_size(pane) as i32;
        self.resize_to(pane, cur + self.sign(pane) * delta)
    }
}

/// Recolours the border glyphs inside a divider's rect.
pub fn paint_divider(buf: &mut Buffer, r: Rect, color: Color) {
    let r = r.intersection(buf.area);
    for y in r.top()..r.bottom() {
        for x in r.left()..r.right() {
            let cell = &mut buf[(x, y)];
            if cell
                .symbol()
                .chars()
                .next()
                .is_some_and(|c| ('\u{2500}'..='\u{257f}').contains(&c))
            {
                cell.fg = color;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> Split {
        Split::new(
            Direction::Horizontal,
            vec![
                Pane::sized(22, Extent::Cells(16), Extent::Frac(0.4)),
                Pane::fill(Extent::Cells(30)),
                Pane::sized(48, Extent::Cells(30), Extent::Frac(0.6)),
            ],
        )
    }

    fn widths(r: &[Rect]) -> Vec<u16> {
        r.iter().map(|r| r.width).collect()
    }

    #[test]
    fn defaults_and_dividers() {
        let mut s = body();
        let r = s.layout(Rect::new(0, 0, 140, 10));
        assert_eq!(widths(&r), [22, 70, 48]);
        assert_eq!(
            s.dividers(),
            &[(0, Rect::new(21, 0, 2, 10)), (2, Rect::new(91, 0, 2, 10))]
        );
    }

    #[test]
    fn drag_clamps_to_min_and_max() {
        let mut s = body();
        s.layout(Rect::new(0, 0, 140, 10));
        assert_eq!(s.press(0, Position::new(21, 3), 0), Press::Drag);
        assert!(s.drag_to(Position::new(31, 3)));
        assert_eq!(widths(&s.layout(Rect::new(0, 0, 140, 10)))[0], 32);
        s.drag_to(Position::new(0, 3));
        assert_eq!(s.laid_size(0), 16, "min");
        s.drag_to(Position::new(139, 3));
        assert_eq!(s.laid_size(0), 56, "max 40%");
        assert!(s.release());
        // The detail pane grows when its divider moves left, up to what the
        // list's minimum leaves.
        s.layout(Rect::new(0, 0, 140, 10));
        s.press(2, Position::new(91, 0), 1000);
        s.drag_to(Position::new(0, 0));
        let r = s.layout(Rect::new(0, 0, 140, 10));
        assert_eq!(widths(&r), [56, 30, 54]);
    }

    #[test]
    fn hidden_panes_and_tiny_areas_stay_in_bounds() {
        let mut s = body();
        s.set_sizes(&[Some(200), None, Some(200)]);
        for w in [0, 1, 10, 40, 80, 300] {
            let area = Rect::new(3, 1, w, 5);
            for (a, b) in [(true, true), (false, true), (true, false)] {
                s.show(0, a);
                s.show(2, b);
                let r = s.layout(area);
                for (i, r) in r.iter().enumerate() {
                    if s.shown(i) {
                        assert!(r.x >= area.x && r.y >= area.y, "{w} {r:?}");
                        assert!(r.right() <= area.right(), "{w} {r:?}");
                    }
                }
            }
        }
        assert_eq!(
            s.sizes(),
            [Some(200), None, Some(200)],
            "layout never edits sizes"
        );
    }
}
