//! Terminal UI for clove: browse clones across repos, create, remove, mount,
//! refresh the golden image with live progress, and cd into a clone on exit.
//!
//! The UI draws on stderr, so `cd "$(clove ui)"` works even without the shell
//! integration: the chosen path is printed on stdout after the screen closes.

mod app;
mod fmt;
mod model;
mod theme;
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
        anyhow::bail!("clove ui needs a terminal");
    }
    let (repos, idx, notice) = worker::discover(start_dir);
    let mut app = App::new(repos, idx);
    app.notice = notice;
    app.now_unix = clove_core::util::now_unix();

    let (tx, rx) = mpsc::channel::<Msg>();
    let mut term = Term::enter()?;
    let start = Instant::now();
    let mut effects = vec![];

    while app.exit.is_none() {
        app.now_ms = start.elapsed().as_millis() as u64;
        app.now_unix = clove_core::util::now_unix();
        if app.needs_reload() {
            effects.push(Effect::Load);
        }
        for e in effects.drain(..) {
            perform(e, &mut app, &tx, &mut term);
        }
        let busy = app.animate();
        term.terminal.draw(|f| ui::draw(f, &mut app))?;

        let timeout = Duration::from_millis(if busy { 40 } else { 250 });
        if event::poll(timeout)? {
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
    }
    drop(term);

    if let Some(Exit::Cd(path)) = app.exit {
        let wrote = std::env::var_os("CLOVE_CD_FILE")
            .is_some_and(|f| std::fs::write(f, path.as_os_str().as_encoded_bytes()).is_ok());
        if !wrote {
            println!("{}", path.display());
        }
    }
    Ok(())
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
            c.arg(path);
            ("Finder".to_string(), c)
        }
        None => anyhow::bail!("set $EDITOR to open clones"),
    };
    term.suspend();
    let status = cmd.current_dir(path).status();
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

fn restore() {
    let _ = execute!(
        std::io::stderr(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
        ratatui::crossterm::cursor::Show
    );
    let _ = disable_raw_mode();
}
