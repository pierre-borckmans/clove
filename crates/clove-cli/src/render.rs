//! Terminal output: styles, the live progress reporter, tables.

use anstream::{eprintln, println};
use anstyle::{AnsiColor, Style};
use clove_core::{Event, Reporter, Timing};
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

/// The brand green, for spinners and bars.
pub const ACCENT: Style =
    Style::new().fg_color(Some(anstyle::Color::Rgb(anstyle::RgbColor(0x3f, 0x8f, 0x63))));

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const NAME_W: usize = 24;
/// Steps slower than this get their time in bold.
const SLOW_MS: f64 = 10_000.0;

struct Running {
    step: String,
    since: Instant,
    last: String,
}

#[derive(Default)]
struct State {
    running: Vec<Running>,
    /// A live line is on screen and must be cleared before printing.
    drawn: bool,
    frame: usize,
}

/// Prints each step as it finishes. In a terminal, the running step gets a
/// spinner, a ticking timer and its latest output line, redrawn by a small
/// ticker thread; piped output stays one plain line per finished step.
pub struct TermReporter {
    live: bool,
    verbose: bool,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    ticker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl TermReporter {
    pub fn new(verbose: bool) -> Self {
        Self {
            live: std::io::stderr().is_terminal(),
            verbose,
            state: Arc::new(Mutex::new(State::default())),
            stop: Arc::new(AtomicBool::new(false)),
            ticker: Mutex::new(None),
        }
    }

    fn ensure_ticker(&self) {
        let mut t = self.ticker.lock().unwrap();
        if t.is_some() || !self.live {
            return;
        }
        let (state, stop) = (self.state.clone(), self.stop.clone());
        *t = Some(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(80));
                let mut s = state.lock().unwrap();
                if !s.running.is_empty() {
                    s.frame += 1;
                    draw(&mut s);
                }
            }
        }));
    }
}

impl Drop for TermReporter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.ticker.lock().unwrap().take() {
            let _ = t.join();
        }
        clear(&mut self.state.lock().unwrap());
    }
}

/// Terminal width from the tty itself; COLUMNS is rarely exported.
pub fn term_width() -> usize {
    // SAFETY: TIOCGWINSZ fills a zeroed winsize; fd 2 may not be a tty, then rc != 0.
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::ioctl(2, libc::TIOCGWINSZ, &mut ws) };
    if rc == 0 && ws.ws_col > 0 {
        return ws.ws_col as usize;
    }
    std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(100)
}

fn clear(s: &mut State) {
    if s.drawn {
        let mut e = std::io::stderr();
        let _ = write!(e, "\r\x1b[2K");
        let _ = e.flush();
        s.drawn = false;
    }
}

fn pad(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        s.chars().take(w).collect()
    } else {
        format!("{s}{}", " ".repeat(w - n))
    }
}

/// Right-aligned step time; slow ones in bold so they stand out.
fn time_cell(ms: f64) -> String {
    let t = format!("{:>6}", secs(ms));
    if ms >= SLOW_MS {
        paint(BOLD, t)
    } else {
        paint(DIM, t)
    }
}

/// The live line: spinner, step (+N when several run), timer, last output.
fn draw(s: &mut State) {
    let Some(cur) = s.running.last() else {
        return;
    };
    let width = term_width();
    let others = s.running.len() - 1;
    let name = if others > 0 {
        format!("{} +{others}", cur.step)
    } else {
        cur.step.clone()
    };
    let ms = cur.since.elapsed().as_secs_f64() * 1000.0;
    // "  ⠋ " + name + " " + time(6) + "  "
    let used = 4 + NAME_W + 1 + 6 + 2;
    let room = width.saturating_sub(used + 1);
    let last: String = cur.last.chars().filter(|c| !c.is_control()).take(room).collect();
    let mut e = anstream::stderr();
    let _ = write!(
        e,
        "\r\x1b[2K  {} {} {}  {}",
        paint(ACCENT, FRAMES[s.frame % FRAMES.len()]),
        pad(&name, NAME_W),
        time_cell(ms),
        paint(DIM, last)
    );
    let _ = e.flush();
    s.drawn = true;
}

impl Reporter for TermReporter {
    fn event(&self, event: Event) {
        let mut s = self.state.lock().unwrap();
        match event {
            Event::Start { step } => {
                s.running.push(Running {
                    step,
                    since: Instant::now(),
                    last: String::new(),
                });
                if self.live {
                    draw(&mut s);
                    drop(s);
                    self.ensure_ticker();
                    return;
                }
            }
            Event::Done { step, took } => {
                s.running.retain(|r| r.step != step);
                clear(&mut s);
                eprintln!(
                    "  {} {} {}",
                    paint(GREEN, "✓"),
                    pad(&step, NAME_W),
                    time_cell(took.as_secs_f64() * 1000.0)
                );
            }
            Event::Skip { step, reason } => {
                clear(&mut s);
                eprintln!(
                    "  {} {} {}",
                    paint(DIM, "·"),
                    paint(DIM, pad(&step, NAME_W)),
                    paint(DIM, reason)
                );
            }
            Event::Output { step, line } => {
                if self.verbose {
                    clear(&mut s);
                    eprintln!("    {} {}", paint(DIM, "│"), paint(DIM, line));
                } else if let Some(r) = s.running.iter_mut().find(|r| r.step == step) {
                    let line = line.trim();
                    if !line.is_empty() {
                        r.last = line.to_string();
                    }
                }
            }
            Event::Warn { message } => {
                clear(&mut s);
                for (i, l) in message.lines().enumerate() {
                    if i == 0 {
                        eprintln!("  {} {}", paint(YELLOW, "!"), paint(YELLOW, l));
                    } else {
                        eprintln!("    {}", paint(DIM, l));
                    }
                }
            }
        }
        if self.live && !s.running.is_empty() {
            draw(&mut s);
        }
    }
}

/// After a long operation: one bar per step that took a real share of the
/// time, scaled to the slowest, so where the time went is obvious.
pub fn timing_bars(timings: &[Timing]) {
    let total = total_ms(timings);
    if total < 10_000.0 || !std::io::stderr().is_terminal() {
        return;
    }
    let shown: Vec<&Timing> = timings.iter().filter(|t| t.ms >= total * 0.03).collect();
    let max = shown.iter().map(|t| t.ms).fold(0.0, f64::max);
    let bar_w = term_width().saturating_sub(4 + NAME_W + 1 + 8).clamp(10, 40);
    const EIGHTHS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    eprintln!();
    for t in shown {
        let cells = t.ms / max * bar_w as f64;
        let full = cells.floor() as usize;
        let part = ((cells - full as f64) * 8.0) as usize;
        let bar = format!("{}{}", "█".repeat(full), EIGHTHS[part.min(7)]);
        let pad_n = bar_w.saturating_sub(full + usize::from(part > 0));
        eprintln!(
            "    {} {}{} {}",
            paint(DIM, pad(&t.step, NAME_W)),
            paint(ACCENT, bar),
            " ".repeat(pad_n),
            time_cell(t.ms)
        );
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
