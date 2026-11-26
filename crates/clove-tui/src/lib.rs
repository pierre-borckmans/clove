//! Terminal UI for clove: browse clones across repos, create, remove, mount,
//! refresh the golden image with live progress, and cd into a clone on exit.
//!
//! The UI draws on stderr, so `cd "$(clove)"` works even without the shell
//! integration: the chosen path is printed on stdout after the screen closes.

mod add_repo;
mod app;
mod fmt;
mod model;
mod panes;
mod split;
mod theme;
mod tooltip;
mod ui;
mod worker;

#[cfg(test)]
mod tests;

use anyhow::Result;
use app::{App, Effect, Exit};
use model::Msg;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use std::io::{IsTerminal, Stderr};
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Once, OnceLock};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

pub fn run(start_dir: &Path) -> Result<()> {
    if !std::io::stderr().is_terminal() {
        anyhow::bail!("clove's terminal UI needs a terminal");
    }
    let (repos, idx, notice) = worker::discover(start_dir);
    let mut app = App::new(repos, idx);
    app.start_dir = start_dir.to_path_buf();
    app.integrated = std::env::var_os("CLOVE_CD_FILE").is_some();
    app.notice = notice;
    app.now_unix = clove_core::util::now_unix();

    let (tx, rx) = mpsc::channel::<Msg>();
    let mut term = Term::enter()?;
    let start = Instant::now();
    let mut effects = vec![];
    if app.repos.is_empty() {
        // First run: nothing to show but the way to add a repo.
        effects.extend(app.open_add_repo());
    }
    let mut pointer_shown = model::Pointer::Default;
    app.panes = panes::Panes::load(&panes::Panes::path());
    // Without it (no inotify watches left, say) the TUI polls faster instead.
    let watcher = {
        let tx = tx.clone();
        clove_core::watch::Watcher::new(move |_| {
            let _ = tx.send(Msg::FsChanged);
        })
        .ok()
    };
    app.watching = watcher.is_some();

    while app.exit.is_none() {
        app.now_ms = start.elapsed().as_millis() as u64;
        app.now_unix = clove_core::util::now_unix();
        if app.needs_reload() {
            effects.push(Effect::Load);
        }
        if app.needs_pr_check() {
            effects.push(Effect::CheckPrs);
        }
        for e in effects.drain(..) {
            perform(e, &mut app, &tx, &mut term);
        }
        let tick = app.tick_ms();
        term.terminal.draw(|f| ui::draw(f, &mut app))?;
        if app.pointer != pointer_shown {
            set_pointer(app.pointer);
            pointer_shown = app.pointer;
        }

        let timeout = Duration::from_millis(tick);
        if event::poll(timeout)? {
            app.now_ms = start.elapsed().as_millis() as u64;
            loop {
                let ev = event::read()?;
                if matches!(ev, event::Event::Resize(..)) {
                    app.settle();
                }
                effects.extend(app.on_input(ev));
                if app.exit.is_some() || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        while let Ok(msg) = rx.try_recv() {
            effects.extend(app.on_msg(msg));
        }
        if let (Some(w), Some(t)) = (&watcher, app.watch.take()) {
            w.sync(t);
        }
    }
    drop(term);

    if let Some(Exit::CdThen(path, args)) = &app.exit
        && let Some(f) = std::env::var_os("CLOVE_CD_FILE")
    {
        let mut then = std::path::PathBuf::from(&f).into_os_string();
        then.push(".then");
        std::fs::write(&f, path.as_os_str().as_encoded_bytes())?;
        std::fs::write(then, args.join(" "))?;
        return Ok(());
    }
    if let Some(Exit::Cd(path)) = app.exit {
        let wrote = std::env::var_os("CLOVE_CD_FILE")
            .is_some_and(|f| std::fs::write(f, path.as_os_str().as_encoded_bytes()).is_ok());
        if !wrote {
            if std::io::stdout().is_terminal() {
                // Nobody captures stdout, so the path would just be printed: a
                // program can't change its parent shell's directory by itself.
                eprintln!(
                    "clove can't cd your shell without its shell integration.\n  \
                     once: clove shell-init --install   (then open a new terminal)\n  \
                     now:  cd {}",
                    shell_quote(&path.to_string_lossy())
                );
            } else {
                // `cd "$(clove)"`
                println!("{}", path.display());
            }
        }
    }
    Ok(())
}

fn shell_quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-~+".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

fn perform(e: Effect, app: &mut App, tx: &mpsc::Sender<Msg>, term: &mut Term) {
    match e {
        Effect::Load => {
            if app.loading {
                app.reload_queued = true;
            } else if let Some(root) = app.root() {
                app.loading = true;
                worker::load(tx.clone(), root);
            }
        }
        Effect::CheckPrs => {
            if let Some(root) = app.root() {
                app.prs_checked.insert(root.clone(), app.now_ms);
                worker::check_prs(tx.clone(), root);
            }
        }
        Effect::LoadRefs => {
            if let Some(root) = app.root() {
                worker::load_refs(tx.clone(), root);
            }
        }
        Effect::Spawn { id, root } => {
            if let Some(job) = app.jobs.iter().find(|j| j.id == id) {
                worker::run_task(tx.clone(), id, root, job.task.clone());
            }
        }
        Effect::OpenEditor(path) => match open_editor(term, &path) {
            Ok(what) => app.toast(model::ToastKind::Ok, format!("opened in {what}")),
            Err(e) => app.toast(model::ToastKind::Err, format!("{e:#}")),
        },
        Effect::FindRepos => add_repo::find(tx.clone()),
        Effect::PreviewInit(path) => add_repo::preview(tx.clone(), path),
        Effect::SetConfig(w) => match worker::write_config(&w) {
            Ok(msg) => app.toast(model::ToastKind::Ok, msg),
            Err(e) => app.toast(model::ToastKind::Err, format!("{e:#}")),
        },
        Effect::SaveLayout => {
            if let Err(e) = app.panes.save(&panes::Panes::path()) {
                app.toast(model::ToastKind::Err, format!("saving panel sizes: {e}"));
            }
        }
        Effect::EditConfig(path) => {
            let res = clove_core::config::ensure_file(&path).and_then(|_| open_editor(term, &path));
            match res {
                // The editor may have changed anything: reload everything.
                Ok(_) => {
                    app.toast(model::ToastKind::Ok, format!("opened {}", fmt::tilde(&path)));
                    app.loaded_ms = None;
                }
                Err(e) => app.toast(model::ToastKind::Err, format!("{e:#}")),
            }
        }
    }
}

/// Runs `$VISUAL`/`$EDITOR` on the path with the screen handed back to it.
fn open_editor(term: &mut Term, path: &Path) -> Result<String> {
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.trim().is_empty());
    let (shown, mut cmd) = match editor {
        Some(ed) => {
            let mut c = std::process::Command::new("sh");
            // $EDITOR may carry flags ("code -w"), so let the shell split it.
            c.arg("-c").arg(format!("{ed} \"$1\"")).arg("sh").arg(path);
            (ed.split_whitespace().next().unwrap_or("editor").to_string(), c)
        }
        None if cfg!(target_os = "macos") => {
            let mut c = std::process::Command::new("open");
            // A file opens in the default text editor; a folder in Finder.
            if path.is_file() {
                c.arg("-t");
            }
            c.arg(path);
            let what = if path.is_file() {
                "your text editor"
            } else {
                "Finder"
            };
            (what.to_string(), c)
        }
        None => anyhow::bail!("set $EDITOR to open clones"),
    };
    term.suspend();
    let dir = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let status = cmd.current_dir(dir).status();
    term.resume()?;
    let status = status?;
    if !status.success() {
        anyhow::bail!("{shown} exited with {status}");
    }
    Ok(shown)
}

static MAIN_THREAD: OnceLock<ThreadId> = OnceLock::new();
static HOOK: Once = Once::new();

struct Term {
    terminal: Terminal<CrosstermBackend<Stderr>>,
    active: bool,
}

impl Term {
    fn enter() -> Result<Self> {
        MAIN_THREAD.get_or_init(|| std::thread::current().id());
        HOOK.call_once(|| {
            let prev = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                // Worker panics become error toasts; printing them would tear the screen.
                if MAIN_THREAD.get() != Some(&std::thread::current().id()) {
                    return;
                }
                restore();
                prev(info);
            }));
        });
        setup()?;
        let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stderr()))?;
        full_clear(&mut terminal)?;
        Ok(Self {
            terminal,
            active: true,
        })
    }

    fn suspend(&mut self) {
        if self.active {
            restore();
            self.active = false;
        }
    }

    fn resume(&mut self) -> Result<()> {
        setup()?;
        self.active = true;
        full_clear(&mut self.terminal)?;
        Ok(())
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        if self.active {
            restore();
        }
    }
}

/// Clears the screen and forgets the last frame so the next draw is complete.
/// `Terminal::clear` would query the cursor position, which some terminals
/// (and multiplexers over slow links) never answer.
fn full_clear(t: &mut Terminal<CrosstermBackend<Stderr>>) -> Result<()> {
    let size = t.size()?;
    t.resize(ratatui::layout::Rect::new(0, 0, size.width, size.height))?;
    Ok(())
}

fn setup() -> Result<()> {
    enable_raw_mode()?;
    execute!(
        std::io::stderr(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    Ok(())
}

/// OSC 22: a hand over clickable things, a resize arrow over dividers, in
/// terminals that support pointer shapes (ghostty, kitty, WezTerm, foot);
/// others ignore it.
fn set_pointer(shape: model::Pointer) {
    use std::io::Write;
    let mut e = std::io::stderr();
    let _ = write!(e, "\x1b]22;{}\x1b\\", shape.name());
    let _ = e.flush();
}

fn restore() {
    set_pointer(model::Pointer::Default);
    let _ = execute!(
        std::io::stderr(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
        ratatui::crossterm::cursor::Show
    );
    let _ = disable_raw_mode();
}
