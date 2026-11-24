//! Text helpers: widths, truncation, durations, paths, cleaning command output.

use std::path::Path;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn width(s: &str) -> usize {
    s.width()
}

/// Cuts `s` to `w` columns, ending with `…` when it had to cut.
pub fn trunc(s: &str, w: usize) -> String {
    if s.width() <= w {
        return s.to_string();
    }
    if w == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut cur = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if cur + cw > w - 1 {
            break;
        }
        out.push(c);
        cur += cw;
    }
    out.push('…');
    out
}

/// Like [`trunc`] but keeps the end: paths read best from the right.
pub fn trunc_left(s: &str, w: usize) -> String {
    if s.width() <= w {
        return s.to_string();
    }
    if w == 0 {
        return String::new();
    }
    let mut tail: Vec<char> = vec![];
    let mut cur = 0;
    for c in s.chars().rev() {
        let cw = c.width().unwrap_or(0);
        if cur + cw > w - 1 {
            break;
        }
        tail.push(c);
        cur += cw;
    }
    let mut out = String::from("…");
    out.extend(tail.into_iter().rev());
    out
}

/// Right-pads to `w` columns (truncating first if needed).
pub fn pad(s: &str, w: usize) -> String {
    let s = trunc(s, w);
    let n = w.saturating_sub(s.width());
    format!("{s}{}", " ".repeat(n))
}

pub fn secs(ms: f64) -> String {
    if ms < 1000.0 {
        format!("{}ms", ms.round() as u64)
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

pub fn age(secs: u64) -> String {
    clove_core::util::human_age(secs)
}

pub fn bytes(b: u64) -> String {
    clove_core::util::human_bytes(b)
}

/// Shortens `$HOME/...` to `~/...`.
pub fn tilde(p: &Path) -> String {
    let home = clove_core::util::home();
    match p.strip_prefix(&home) {
        Ok(rest) if !home.as_os_str().is_empty() && home != Path::new("/") => {
            format!("~/{}", rest.display())
        }
        _ => p.display().to_string(),
    }
}

pub fn short_commit(c: &str) -> String {
    c.chars().take(10).collect()
}

/// Makes a line of command output safe to draw: drops ANSI escapes and control
/// characters, keeps what follows the last carriage return (progress bars), and
/// expands tabs.
pub fn clean_line(s: &str) -> String {
    let s = s.rsplit('\r').find(|p| !p.is_empty()).unwrap_or("");
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\x1b' => match it.next() {
                // CSI: parameters then a final byte in @..~
                Some('[') => {
                    for d in it.by_ref() {
                        if ('@'..='~').contains(&d) {
                            break;
                        }
                    }
                }
                // OSC: until BEL or ST
                Some(']') => {
                    while let Some(d) = it.next() {
                        if d == '\x07' {
                            break;
                        }
                        if d == '\x1b' {
                            it.next();
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\t' => out.push_str("    "),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Eighth-block bar of `frac` (0..=1) over `w` cells. Never empty for a non-zero value.
pub fn bar(frac: f64, w: usize) -> String {
    const PARTS: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    let frac = if frac.is_finite() {
        frac.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let eighths = (frac * w as f64 * 8.0).round() as usize;
    let eighths = if frac > 0.0 { eighths.max(1) } else { 0 };
    let full = eighths / 8;
    let mut s = "█".repeat(full);
    if full < w && eighths % 8 > 0 {
        s.push(PARTS[eighths % 8]);
    }
    s
}

/// Greedy word wrap to `w` columns; words longer than a line are cut.
pub fn wrap(s: &str, w: usize) -> Vec<String> {
    let w = w.max(1);
    let mut out = vec![];
    for para in s.lines() {
        let mut line = String::new();
        for word in para.split(' ') {
            let ww = width(word);
            let lw = width(&line);
            if lw > 0 && lw + 1 + ww > w {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            if ww > w {
                line.push_str(&trunc(word, w));
            } else {
                line.push_str(word);
            }
        }
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation() {
        assert_eq!(trunc("hello", 5), "hello");
        assert_eq!(trunc("hello world", 6), "hello…");
        assert_eq!(trunc_left("/a/b/c/d", 5), "…/c/d");
        assert_eq!(pad("ab", 4), "ab  ");
    }

    #[test]
    fn cleans_output() {
        assert_eq!(clean_line("\x1b[32mok\x1b[0m done"), "ok done");
        assert_eq!(clean_line("10%\r50%\r100%"), "100%");
        assert_eq!(clean_line("a\tb"), "a    b");
        assert_eq!(clean_line("\x1b]0;title\x07x"), "x");
    }

    #[test]
    fn bars() {
        assert_eq!(bar(1.0, 4), "████");
        assert_eq!(bar(0.5, 4), "██");
        assert_eq!(bar(0.001, 4), "▏");
        assert_eq!(bar(0.0, 4), "");
        assert_eq!(bar(f64::NAN, 4), "");
    }

    #[test]
    fn wraps() {
        assert_eq!(wrap("aa bb cc", 5), vec!["aa bb", "cc"]);
        assert_eq!(wrap("x\ny", 5), vec!["x", "y"]);
    }

    #[test]
    fn durations() {
        assert_eq!(secs(8.2), "8ms");
        assert_eq!(secs(1240.0), "1.2s");
        assert_eq!(secs(125_000.0), "2m05s");
    }
}
