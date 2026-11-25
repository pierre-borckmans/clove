//! The clove mark for terminals: a lowercase c holding a clove, as half-block
//! pixel art, plus a pixel wordmark. Same drawing as brand/mark.svg.
//!
//! Pixels are `#` (drawn in the terminal's own foreground, so it works on light
//! and dark themes), `R` (the clove, rust) or `.` (empty).

/// 13×12 pixels, 6 terminal lines.
pub const MARK: [&str; 12] = [
    "...######....",
    ".##########..",
    "####....####.",
    "###......##..",
    "###.....R....",
    "###....RR....",
    "###...RRR....",
    "###...RR.....",
    "###......##..",
    "####....####.",
    ".##########..",
    "...######....",
];

/// Lowercase pixel letters, 5 rows tall.
fn glyph(ch: char) -> &'static [&'static str; 5] {
    match ch {
        'c' => &[".###", "#...", "#...", "#...", ".###"],
        'l' => &["#.", "#.", "#.", "#.", ".#"],
        'o' => &[".##.", "#..#", "#..#", "#..#", ".##."],
        'v' => &["#...#", "#...#", ".#.#.", ".#.#.", "..#.."],
        'e' => &[".##.", "#..#", "####", "#...", ".###"],
        _ => &["...", "...", "...", "...", "..."],
    }
}

/// "clove" in pixels with the `c` in the accent colour, padded to 6 rows.
pub fn wordmark() -> Vec<String> {
    let mut rows = vec![String::new(); 6];
    for (i, ch) in "clove".chars().enumerate() {
        let g = glyph(ch);
        for (y, row) in g.iter().enumerate() {
            if i > 0 {
                rows[y].push('.');
            }
            let px = if ch == 'c' {
                row.replace('#', "R")
            } else {
                row.to_string()
            };
            rows[y].push_str(&px);
        }
    }
    let w = rows[0].len();
    rows[5] = ".".repeat(w);
    rows
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// The terminal's default colour.
    None,
    /// Default foreground.
    Fg,
    /// The clove rust.
    Accent,
}

/// One terminal cell: a glyph with foreground and background ink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Ink,
    pub bg: Ink,
}

fn ink(p: u8) -> Ink {
    match p {
        b'#' => Ink::Fg,
        b'R' => Ink::Accent,
        _ => Ink::None,
    }
}

/// Packs two pixel rows into one line of half-block cells. The default
/// foreground can never be a background, so mixed cells put it in the glyph.
pub fn half_blocks(rows: &[impl AsRef<str>]) -> Vec<Vec<Cell>> {
    let width = rows.iter().map(|r| r.as_ref().len()).max().unwrap_or(0);
    let px = |y: usize, x: usize| {
        rows.get(y)
            .and_then(|r| r.as_ref().as_bytes().get(x))
            .copied()
            .unwrap_or(b'.')
    };
    (0..rows.len().div_ceil(2))
        .map(|line| {
            (0..width)
                .map(|x| {
                    let (t, b) = (ink(px(line * 2, x)), ink(px(line * 2 + 1, x)));
                    let cell = |ch, fg, bg| Cell { ch, fg, bg };
                    match (t, b) {
                        (Ink::None, Ink::None) => cell(' ', Ink::None, Ink::None),
                        (t, Ink::None) => cell('▀', t, Ink::None),
                        (Ink::None, b) => cell('▄', b, Ink::None),
                        (t, b) if t == b => cell('█', t, Ink::None),
                        (Ink::Fg, b) => cell('▀', Ink::Fg, b),
                        (t, _) => cell('▄', Ink::Fg, t),
                    }
                })
                .collect()
        })
        .collect()
}

/// The mark, a gap, and the wordmark centred beside it: 6 lines.
pub fn banner() -> Vec<Vec<Cell>> {
    let mark = half_blocks(&MARK);
    let word = half_blocks(&wordmark());
    let gap = Cell {
        ch: ' ',
        fg: Ink::None,
        bg: Ink::None,
    };
    let offset = 1; // vertical position of the 3-line wordmark next to the 6-line mark
    mark.into_iter()
        .enumerate()
        .map(|(i, mut line)| {
            line.extend([gap; 3]);
            if let Some(w) = i.checked_sub(offset).and_then(|j| word.get(j)) {
                line.extend(w.iter().copied());
            }
            line
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_shape() {
        let b = banner();
        assert_eq!(b.len(), 6);
        assert!(b.iter().all(|l| l.len() >= MARK[0].len()));
        // the clove is in the accent colour somewhere in the mark
        assert!(
            b.iter()
                .flatten()
                .any(|c| c.fg == Ink::Accent || c.bg == Ink::Accent)
        );
    }

    #[test]
    fn mixed_cells_keep_default_fg_in_the_glyph() {
        let l = half_blocks(&["#R", "R#"]);
        assert_eq!(
            l[0][0],
            Cell {
                ch: '▀',
                fg: Ink::Fg,
                bg: Ink::Accent
            }
        );
        assert_eq!(
            l[0][1],
            Cell {
                ch: '▄',
                fg: Ink::Fg,
                bg: Ink::Accent
            }
        );
    }
}
