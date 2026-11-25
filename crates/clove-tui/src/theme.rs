//! The palette. Text uses the terminal's own foreground and background (no ink
//! or ivory page fills), so it reads on dark and light terminals alike. Colour
//! carries meaning (accent, good, warning) in mid-tones that hold contrast on both.

use ratatui::style::{Color, Modifier, Style};

/// Clove rust: the accent. Selection, focus, keys.
pub const RUST: Color = Color::Rgb(0xb4, 0x53, 0x2a);
/// Sage: healthy, mounted, done.
pub const SAGE: Color = Color::Rgb(0x6b, 0x8f, 0x71);
/// Garlic ivory: text on accent fills.
pub const IVORY: Color = Color::Rgb(0xf4, 0xee, 0xe0);
/// Warm grey for secondary text.
pub const MUTED: Color = Color::Rgb(0x8f, 0x86, 0x7c);
/// Borders.
pub const LINE: Color = Color::Rgb(0x74, 0x6a, 0x61);
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
    fg(RUST).add_modifier(Modifier::BOLD)
}
pub fn selected() -> Style {
    Style::new().bg(RUST).fg(IVORY).add_modifier(Modifier::BOLD)
}
pub fn border() -> Style {
    fg(LINE)
}
pub fn title() -> Style {
    fg(RUST).add_modifier(Modifier::BOLD)
}

pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner(now_ms: u64) -> &'static str {
    SPINNER[((now_ms / 80) % SPINNER.len() as u64) as usize]
}

/// The clove mark as half-block lines (6 rows). The c uses the terminal's own
/// foreground; the clove is rust.
pub fn mark_lines() -> Vec<ratatui::text::Line<'static>> {
    use clove_core::brand::{Ink, MARK, half_blocks};
    let colour = |ink: Ink| match ink {
        Ink::Accent => Some(RUST),
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

/// "clove" with the c in rust, for titles.
pub fn wordmark() -> Vec<ratatui::text::Span<'static>> {
    vec![
        ratatui::text::Span::styled("c", title()),
        ratatui::text::Span::styled("love", Style::new().add_modifier(Modifier::BOLD)),
    ]
}
