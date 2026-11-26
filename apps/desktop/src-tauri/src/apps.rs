//! The "Open in" targets: editors, terminals and the file manager. macOS
//! opens app bundles with `open -a`; Linux runs each app's command.

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

struct App {
    id: &'static str,
    /// Bundle name under /Applications.
    mac: Option<&'static str>,
    /// Commands to look for on PATH, first found wins.
    linux: &'static [&'static str],
}

const APPS: &[App] = &[
    // On Linux, "terminal" is the desktop's default terminal (see `default_terminal`).
    App {
        id: "terminal",
        mac: Some("Terminal"),
        linux: &[],
    },
    App {
        id: "ghostty",
        mac: Some("Ghostty"),
        linux: &["ghostty"],
    },
    App {
        id: "iterm",
        mac: Some("iTerm"),
        linux: &[],
    },
    App {
        id: "warp",
        mac: Some("Warp"),
        linux: &["warp-terminal"],
    },
    App {
        id: "kitty",
        mac: None,
        linux: &["kitty"],
    },
    App {
        id: "alacritty",
        mac: None,
        linux: &["alacritty"],
    },
    App {
        id: "wezterm",
        mac: None,
        linux: &["wezterm"],
    },
    App {
        id: "vscode",
        mac: Some("Visual Studio Code"),
        linux: &["code"],
    },
    App {
        id: "cursor",
        mac: Some("Cursor"),
        linux: &["cursor"],
    },
    App {
        id: "zed",
        mac: Some("Zed"),
        linux: &["zed", "zeditor"],
    },
];

fn app(id: &str) -> Result<&'static App> {
    APPS.iter()
        .find(|a| a.id == id)
        .ok_or_else(|| anyhow::anyhow!("unknown app {id}"))
}

/// Ids of the apps installed here.
pub fn installed() -> Vec<String> {
    APPS.iter()
        .filter(|a| locate(a).is_some())
        .map(|a| a.id.to_string())
        .collect()
}

/// Opens `path` in an editor or terminal, or `finder` for the file manager.
pub fn open(path: &Path, id: &str) -> Result<()> {
    if cfg!(target_os = "macos") {
        let mut cmd = Command::new("open");
        if id != "finder" {
            let bundle = app(id)?
                .mac
                .ok_or_else(|| anyhow::anyhow!("{id} isn't a macOS app"))?;
            cmd.args(["-a", bundle]);
        }
        clove_core::util::output(cmd.arg(path))?;
        return Ok(());
    }
    if id == "finder" {
        return open_default(path);
    }
    let a = app(id)?;
    let bin = locate(a).ok_or_else(|| anyhow::anyhow!("{id} isn't installed"))?;
    if TERMINALS.contains(&id) {
        launch(&mut terminal(&bin, path, None))
    } else {
        launch(Command::new(&bin).arg(path))
    }
}

/// Linux: opens a file or folder with its default app (`xdg-open`).
pub fn open_default(path: &Path) -> Result<()> {
    launch(Command::new("xdg-open").arg(path))
}

/// Opens a terminal in `dir` running `script`: `terminal` when it can run a
/// command, else the default one.
pub fn run_in_terminal(id: &str, dir: &Path, script: &Path) -> Result<()> {
    if cfg!(target_os = "macos") {
        let mut cmd = Command::new("open");
        match id {
            // Ghostty runs a command given with -e; the others open a .command file.
            "ghostty" => cmd.args(["-na", "Ghostty", "--args", "-e"]).arg(script),
            "iterm" => cmd.args(["-a", "iTerm"]).arg(script),
            _ => cmd.args(["-a", "Terminal"]).arg(script),
        };
        clove_core::util::output(&mut cmd)?;
        return Ok(());
    }
    let bin = app(id)
        .ok()
        .filter(|a| a.id != "warp")
        .and_then(locate)
        .or_else(default_terminal)
        .ok_or_else(|| anyhow::anyhow!("no terminal found: set $TERMINAL"))?;
    launch(&mut terminal(&bin, dir, Some(script)))
}

const TERMINALS: &[&str] = &[
    "terminal",
    "ghostty",
    "iterm",
    "warp",
    "kitty",
    "alacritty",
    "wezterm",
];

fn locate(a: &App) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let name = format!("{}.app", a.mac?);
        let found = [
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications/Utilities"),
            clove_core::util::home().join("Applications"),
        ]
        .iter()
        .any(|d| d.join(&name).exists());
        return found.then(|| PathBuf::from(name));
    }
    if a.id == "terminal" {
        return default_terminal();
    }
    a.linux.iter().find_map(|b| clove_core::util::which(b))
}

/// $TERMINAL, then Debian's x-terminal-emulator, then the usual desktops' own.
fn default_terminal() -> Option<PathBuf> {
    let env = std::env::var("TERMINAL").ok().filter(|t| !t.trim().is_empty());
    env.iter()
        .map(String::as_str)
        .chain([
            "x-terminal-emulator",
            "gnome-terminal",
            "ptyxis",
            "kgx",
            "konsole",
            "xfce4-terminal",
            "mate-terminal",
            "tilix",
            "xterm",
        ])
        .find_map(clove_core::util::which)
}

/// A terminal command that starts in `dir`, running `script` when given.
/// The working directory is also passed as the process's own, which is all
/// some terminals look at.
fn terminal(bin: &Path, dir: &Path, script: Option<&Path>) -> Command {
    let name = bin.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let mut cmd = Command::new(bin);
    cmd.current_dir(dir);
    let mut wd = |flag: &str| {
        cmd.arg(format!("{flag}={}", dir.display()));
    };
    match name {
        "gnome-terminal" | "ptyxis" | "kgx" | "xfce4-terminal" | "mate-terminal" | "tilix" | "ghostty"
        | "alacritty" => wd("--working-directory"),
        "konsole" => wd("--workdir"),
        "kitty" => wd("--directory"),
        "wezterm" => {
            cmd.args(["start", "--cwd"]).arg(dir);
        }
        _ => {}
    }
    if let Some(script) = script {
        match name {
            "gnome-terminal" | "ptyxis" | "kgx" | "wezterm" => {
                cmd.arg("--");
            }
            "xfce4-terminal" => {
                cmd.arg("-x");
            }
            "kitty" => {}
            // x-terminal-emulator must take -e (Debian policy), as xterm does.
            _ => {
                cmd.arg("-e");
            }
        }
        cmd.arg(script);
    }
    cmd
}

/// Starts a GUI app without waiting for it. A launcher that fails at once
/// (bad flag, no display) reports its error; one still running after a
/// moment counts as started, and a thread reaps it when it exits.
fn launch(cmd: &mut Command) -> Result<()> {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let desc = format!("{:?}", cmd.get_program());
    // Libraries an AppImage bundles must not leak into the apps it starts.
    if std::env::var_os("APPIMAGE").is_some() {
        for v in [
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "GIO_MODULE_DIR",
            "GDK_PIXBUF_MODULE_FILE",
            "GTK_PATH",
        ] {
            cmd.env_remove(v);
        }
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("running {desc}: {e}"))?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(400) {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            let mut err = String::new();
            if let Some(mut e) = child.stderr.take() {
                use std::io::Read;
                let _ = e.read_to_string(&mut err);
            }
            bail!("{desc} failed ({status}): {}", err.trim());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    // Stop holding its stderr, so a chatty app never blocks on a full pipe.
    drop(child.stderr.take());
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(c: &Command) -> Vec<String> {
        c.get_args().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn terminals_start_in_the_clone() {
        let dir = Path::new("/w/feat");
        let gnome = terminal(Path::new("/usr/bin/gnome-terminal"), dir, None);
        assert_eq!(args(&gnome), ["--working-directory=/w/feat"]);
        assert_eq!(gnome.get_current_dir(), Some(dir));
        let wez = terminal(Path::new("/usr/bin/wezterm"), dir, None);
        assert_eq!(args(&wez), ["start", "--cwd", "/w/feat"]);
        // Unknown terminals get only the process's working directory.
        assert!(args(&terminal(Path::new("/usr/bin/x-terminal-emulator"), dir, None)).is_empty());
    }

    #[test]
    fn terminals_run_the_resolver_script() {
        let (dir, s) = (Path::new("/w"), Some(Path::new("/tmp/r.command")));
        let t = |b: &str| args(&terminal(Path::new(b), dir, s));
        assert_eq!(
            t("gnome-terminal"),
            ["--working-directory=/w", "--", "/tmp/r.command"]
        );
        assert_eq!(t("konsole"), ["--workdir=/w", "-e", "/tmp/r.command"]);
        assert_eq!(t("kitty"), ["--directory=/w", "/tmp/r.command"]);
        assert_eq!(
            t("xfce4-terminal"),
            ["--working-directory=/w", "-x", "/tmp/r.command"]
        );
        assert_eq!(t("x-terminal-emulator"), ["-e", "/tmp/r.command"]);
    }
}
