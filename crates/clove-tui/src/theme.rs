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
