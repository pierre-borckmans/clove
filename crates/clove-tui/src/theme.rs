//! The palette. Text uses the terminal's own foreground and background (no ink
//! or cream page fills), so it reads on dark and light terminals alike. Colour
//! carries meaning (accent, good, warning) in mid-tones that hold contrast on both.

use ratatui::style::{Color, Modifier, Style};

/// Bottle green: the accent. Selection, focus, keys.
pub const BOTTLE: Color = Color::Rgb(0x3f, 0x8f, 0x63);
/// Healthy, mounted, done: a lighter shoot green.
pub const SAGE: Color = Color::Rgb(0x5f, 0xa8, 0x7c);
/// Cream: text on accent fills.
pub const IVORY: Color = Color::Rgb(0xf3, 0xef, 0xd6);
/// Green-grey for secondary text.
pub const MUTED: Color = Color::Rgb(0x85, 0x90, 0x89);
/// Borders.
pub const LINE: Color = Color::Rgb(0x66, 0x71, 0x6b);
/// Dirty trees, stale images, warnings.
pub const AMBER: Color = Color::Rgb(0xc9, 0x93, 0x2b);
/// Failures.
pub const RED: Color = Color::Rgb(0xd0, 0x45, 0x4c);
/// Branch names.
pub const PLUM: Color = Color::Rgb(0x9d, 0x72, 0x96);

pub fn fg(c: Color) -> Style {
    Style::new().fg(c)
}
pub fn muted() -> Style {
    fg(MUTED)
}
pub fn bold() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}
pub fn key() -> Style {
    fg(BOTTLE).add_modifier(Modifier::BOLD)
}
pub fn selected() -> Style {
    Style::new().bg(BOTTLE).fg(IVORY).add_modifier(Modifier::BOLD)
}
pub fn border() -> Style {
    fg(LINE)
}
pub fn title() -> Style {
    fg(BOTTLE).add_modifier(Modifier::BOLD)
}

pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// One spinner frame: also how often a busy screen redraws.
pub const SPINNER_MS: u64 = 80;

pub fn spinner(now_ms: u64) -> &'static str {
    SPINNER[((now_ms / SPINNER_MS) % SPINNER.len() as u64) as usize]
}

/// The clove mark as half-block lines (6 rows). The c uses the terminal's own
/// foreground; the clove is bottle green.
pub fn mark_lines() -> Vec<ratatui::text::Line<'static>> {
    use clove_core::brand::{Ink, MARK, half_blocks};
    let colour = |ink: Ink| match ink {
        Ink::Accent => Some(BOTTLE),
        Ink::Fg | Ink::None => None,
    };
    half_blocks(&MARK)
        .into_iter()
        .map(|cells| {
            let spans: Vec<_> = cells
                .into_iter()
                .map(|c| {
                    let mut st = Style::new();
                    if let Some(fg) = colour(c.fg) {
                        st = st.fg(fg);
                    }
                    if let Some(bg) = colour(c.bg) {
                        st = st.bg(bg);
                    }
                    ratatui::text::Span::styled(c.ch.to_string(), st)
                })
                .collect();
            ratatui::text::Line::from(spans)
        })
        .collect()
}

/// "clove" with the c in bottle green, for titles.
pub fn wordmark() -> Vec<ratatui::text::Span<'static>> {
    vec![
        ratatui::text::Span::styled("c", title()),
        ratatui::text::Span::styled("love", Style::new().add_modifier(Modifier::BOLD)),
    ]
}
