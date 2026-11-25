//! Renders the core's terminal mark with ANSI colours.

use anstyle::{Color, RgbColor, Style};
use clove_core::brand::{Cell, Ink};

const BOTTLE: Color = Color::Rgb(RgbColor(0x3f, 0x8f, 0x63));

fn style(cell: &Cell) -> Style {
    // The c uses the terminal's own foreground, so it reads on light and dark themes.
    let colour = |ink: Ink| match ink {
        Ink::Accent => Some(BOTTLE),
        Ink::Fg | Ink::None => None,
    };
    Style::new().fg_color(colour(cell.fg)).bg_color(colour(cell.bg))
}

/// The banner as styled lines (anstream strips the colour when it shouldn't show).
pub fn banner_lines() -> Vec<String> {
    clove_core::brand::banner()
        .iter()
        .map(|line| {
            let mut s = String::new();
            for c in line {
                let st = style(c);
                s.push_str(&format!("{st}{}{st:#}", c.ch));
            }
            s.trim_end().to_string()
        })
        .collect()
}
