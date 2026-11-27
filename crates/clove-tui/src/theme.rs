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

// ---- diffs -------------------------------------------------------------

/// Syntax colours for diffs: mid-tones from the same family as the palette,
/// so code reads on dark and light terminals. Plain text keeps the
/// terminal's own foreground.
pub const SYN_KEYWORD: Color = PLUM;
pub const SYN_STRING: Color = SAGE;
pub const SYN_NUMBER: Color = AMBER;
pub const SYN_FUNCTION: Color = Color::Rgb(0x5b, 0x8e, 0xc2);
pub const SYN_TYPE: Color = Color::Rgb(0x3d, 0x9b, 0x9b);
pub const SYN_COMMENT: Color = MUTED;

/// The terminal's background, when it answered the query at startup.
static BACKGROUND: std::sync::OnceLock<Option<(u8, u8, u8)>> = std::sync::OnceLock::new();

/// Asks the terminal for its background colour (OSC 11), once, before the
/// screen is taken over. Terminals that don't answer leave it unknown.
pub fn detect_background() {
    BACKGROUND.get_or_init(|| {
        let mut opts = terminal_colorsaurus::QueryOptions::default();
        opts.timeout = std::time::Duration::from_millis(300);
        terminal_colorsaurus::background_color(opts)
            .ok()
            .map(|c| ((c.r >> 8) as u8, (c.g >> 8) as u8, (c.b >> 8) as u8))
    });
}

fn mix(bg: (u8, u8, u8), c: Color, amount: f32) -> Color {
    let Color::Rgb(r, g, b) = c else { return c };
    let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
    Color::Rgb(m(bg.0, r), m(bg.1, g), m(bg.2, b))
}

/// Line tints for added and removed lines, and stronger ones for the words
/// that changed. None when the background is unknown: a guessed tint could
/// make text unreadable, so the gutter alone marks changes then.
pub struct Tints {
    pub added: Color,
    pub removed: Color,
    pub added_word: Color,
    pub removed_word: Color,
}

pub fn tints() -> Option<Tints> {
    let bg = (*BACKGROUND.get()?)?;
    let dark = (bg.0 as u32 * 299 + bg.1 as u32 * 587 + bg.2 as u32 * 114) / 1000 < 128;
    let (line, word) = if dark { (0.16, 0.34) } else { (0.13, 0.28) };
    Some(Tints {
        added: mix(bg, SAGE, line),
        removed: mix(bg, RED, line),
        added_word: mix(bg, SAGE, word),
        removed_word: mix(bg, RED, word),
    })
}
