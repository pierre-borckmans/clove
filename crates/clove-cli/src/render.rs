//! Terminal output: styles, the live progress reporter, tables.

use anstream::{eprintln, println};
use anstyle::{AnsiColor, Style};
use clove_core::{Event, Reporter, Timing};
use std::io::{IsTerminal, Write};
use std::sync::Mutex;

pub const DIM: Style = Style::new().dimmed();
pub const BOLD: Style = Style::new().bold();
pub const GREEN: Style = AnsiColor::Green.on_default();
pub const YELLOW: Style = AnsiColor::Yellow.on_default();
pub const RED: Style = AnsiColor::Red.on_default();
pub const CYAN: Style = AnsiColor::Cyan.on_default();
pub const MAGENTA: Style = AnsiColor::Magenta.on_default();

pub fn paint(style: Style, s: impl std::fmt::Display) -> String {
    format!("{style}{s}{style:#}")
}

pub fn secs(ms: f64) -> String {
    if ms < 1000.0 {
        format!("{:.2}s", ms / 1000.0)
    } else if ms < 60_000.0 {
        format!("{:.1}s", ms / 1000.0)
    } else {
        format!(
            "{}m{:02}s",
            (ms / 60_000.0) as u64,
            ((ms % 60_000.0) / 1000.0) as u64
        )
    }
}

pub fn total_ms(t: &[Timing]) -> f64 {
    t.iter().map(|t| t.ms).sum()
}

/// Shortens `$HOME/...` to `~/...`.
pub fn tilde(p: &std::path::Path) -> String {
    let home = clove_core::util::home();
    match p.strip_prefix(&home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

/// Prints each step as it finishes, with a live line showing command output.
pub struct TermReporter {
    live: bool,
    verbose: bool,
    state: Mutex<Option<String>>,
}

impl TermReporter {
    pub fn new(verbose: bool) -> Self {
        Self {
            live: std::io::stderr().is_terminal(),
            verbose,
            state: Mutex::new(None),
        }
    }

    fn clear(&self) {
        if self.live {
            let mut e = std::io::stderr();
            let _ = write!(e, "\r\x1b[2K");
            let _ = e.flush();
        }
    }

    fn width() -> usize {
        std::env::var("COLUMNS")
            .ok()
            .and_then(|c| c.parse().ok())
            .unwrap_or(100)
    }
}

impl Reporter for TermReporter {
    fn event(&self, event: Event) {
        let mut cur = self.state.lock().unwrap();
        match event {
            Event::Start { step } => {
                if self.live {
                    let mut e = anstream::stderr();
                    let _ = write!(e, "\r\x1b[2K  {} {}", paint(CYAN, "◌"), paint(DIM, &step));
                    let _ = e.flush();
                }
                *cur = Some(step);
            }
            Event::Done { step, took } => {
                self.clear();
                eprintln!(
                    "  {} {:<24} {}",
                    paint(GREEN, "✓"),
                    step,
                    paint(DIM, secs(took.as_secs_f64() * 1000.0))
                );
                *cur = None;
            }
            Event::Skip { step, reason } => {
                self.clear();
                eprintln!(
                    "  {} {} {}",
                    paint(DIM, "·"),
                    paint(DIM, format!("{step:<24}")),
                    paint(DIM, reason)
                );
            }
            Event::Output { step, line } => {
                if self.verbose {
                    self.clear();
                    eprintln!("    {} {}", paint(DIM, "│"), paint(DIM, line));
                } else if self.live {
                    let room = Self::width().saturating_sub(step.len() + 10);
                    let line: String = line.chars().filter(|c| !c.is_control()).take(room).collect();
                    let mut e = anstream::stderr();
                    let _ = write!(
                        e,
                        "\r\x1b[2K  {} {} {}",
                        paint(CYAN, "◌"),
                        paint(DIM, &step),
                        paint(DIM, line)
                    );
                    let _ = e.flush();
                }
            }
            Event::Warn { message } => {
                self.clear();
                for (i, l) in message.lines().enumerate() {
                    if i == 0 {
                        eprintln!("  {} {}", paint(YELLOW, "!"), paint(YELLOW, l));
                    } else {
                        eprintln!("    {}", paint(DIM, l));
                    }
                }
            }
        }
    }
}

/// Left-aligned columns, widths from content. Styled cells must pass their
/// visible width so ANSI codes do not break alignment.
pub struct Table {
    rows: Vec<Vec<(String, usize)>>,
}

impl Table {
    pub fn new(headers: &[&str]) -> Self {
        Self {
            rows: vec![
                headers
                    .iter()
                    .map(|h| (paint(DIM, h), h.chars().count()))
                    .collect(),
            ],
        }
    }

    pub fn row(&mut self, cells: Vec<(String, usize)>) {
        self.rows.push(cells);
    }

    pub fn print(&self) {
        let cols = self.rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let widths: Vec<usize> = (0..cols)
            .map(|c| {
                self.rows
                    .iter()
                    .filter_map(|r| r.get(c))
                    .map(|x| x.1)
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        for r in &self.rows {
            let mut line = String::new();
            for (i, (s, w)) in r.iter().enumerate() {
                line.push_str(s);
                if i + 1 < r.len() {
                    line.push_str(&" ".repeat(widths[i] - w + 2));
                }
            }
            println!("{}", line.trim_end());
        }
    }
}

pub fn cell(s: impl Into<String>) -> (String, usize) {
    let s = s.into();
    let w = s.chars().count();
    (s, w)
}

pub fn styled(style: Style, s: impl Into<String>) -> (String, usize) {
    let s = s.into();
    let w = s.chars().count();
    (paint(style, s), w)
}
