//! Snapshot tests of key screens (rendered to text with ratatui's TestBackend)
//! and behaviour tests that drive [`App`] with keys, clicks and worker messages.

use crate::app::{App, Effect, Exit};
use crate::model::*;
use crate::theme;
use crate::ui;
use clove_core::doctor::{Check, Level};
use clove_core::{CloneInfo, Event, GoldenMeta, GoldenStatus, RepoStatus, Timing};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    Event as TermEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::path::PathBuf;
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

const NOW: u64 = 1_800_000_000;

fn t(step: &str, ms: f64) -> Timing {
    Timing {
        step: step.into(),
        ms,
    }
}

fn clone(name: &str, branch: Option<&str>, mounted: bool) -> CloneInfo {
    CloneInfo {
        name: name.into(),
        path: PathBuf::from(format!("/work/app.clove/{name}/app")),
        mounted,
        branch: branch.map(String::from),
        head: Some("9f8e7d6c5b4a".into()),
        dirty: mounted.then_some(false),
        ahead: mounted.then_some(0),
        behind: mounted.then_some(0),
        image_bytes: 14 * 1024 * 1024,
        created_at: NOW - 3 * 3600,
        golden_generation: 3,
        timings: vec![
            t("clone image", 6.0),
            t("mount", 612.0),
            t("fetch", 288.0),
            t("checkout", 141.0),
        ],
        failed_steps: vec![],
    }
}

fn clones() -> Vec<CloneInfo> {
    let mut a = clone("feat-login", Some("feat/login"), true);
    a.dirty = Some(true);
    a.ahead = Some(2);
    a.created_at = NOW - 300;
    a.timings.push(t("setup deps", 2140.0));
    let mut b = clone("fix-race", Some("fix/race"), true);
    b.ahead = Some(1);
    b.behind = Some(3);
    b.image_bytes = 230 * 1024 * 1024;
    let c = clone("spike-cache", Some("spike/cache"), false);
    let mut d = clone("pierre-onboarding-flow", Some("pierre/onboarding-flow-v2"), true);
    d.failed_steps = vec!["lint".into()];
    d.golden_generation = 2;
    d.created_at = NOW - 4 * 86_400;
    let mut e = clone("detached", None, true);
    e.branch = None;
    vec![a, b, c, d, e]
}

fn status(golden: bool, n: usize) -> RepoStatus {
    RepoStatus {
        name: "app".into(),
        root: "/work/app".into(),
        id: "app-1234".into(),
        state: "/state/repos/app-1234".into(),
        clone_root: "/work/app.clove".into(),
        base: "origin/main".into(),
        backend: None,
        golden: golden.then(|| GoldenStatus {
            meta: GoldenMeta {
                generation: 3,
                commit: "a1b2c3d4e5f60718293a".into(),
                base: "origin/main".into(),
                created_at: NOW - 7200,
                timings: vec![],
                steps: vec!["deps".into()],
                dirty: false,
            },
            age_secs: 7200,
            behind: Some(4),
            image_bytes: 1288 * 1024 * 1024,
        }),
        clones: n,
        mounted: n.saturating_sub(1),
        steps: vec![],
        steps_detected: true,
    }
}

fn repos() -> Vec<RepoTab> {
    ["app", "mono", "web"]
        .iter()
        .map(|n| RepoTab {
            name: n.to_string(),
            root: PathBuf::from(format!("/work/{n}")),
        })
        .collect()
}

fn app_with(golden: bool, clones: Vec<CloneInfo>) -> App {
    let mut app = App::new(repos(), 0);
    app.now_unix = NOW;
    app.now_ms = 10_000;
    app.loaded_ms = Some(10_000);
    let n = clones.len();
    app.apply_data(RepoData {
        root: "/work/app".into(),
        status: status(golden, n),
        clones,
    });
    app
}

fn app() -> App {
    app_with(true, clones())
}

fn text(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        let mut line = String::new();
        let mut skip = 0;
        for x in 0..buf.area.width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let s = buf[(x, y)].symbol();
            skip = s.width().saturating_sub(1);
            line.push_str(s);
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// Renders twice with animations settled in between, like a few real frames.
fn render(app: &mut App, w: u16, h: u16) -> (String, Buffer) {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw(f, app)).unwrap();
    app.settle();
    term.draw(|f| ui::draw(f, app)).unwrap();
    let buf = term.backend().buffer().clone();
    (text(&buf), buf)
}

fn key(app: &mut App, code: KeyCode) -> Vec<Effect> {
    app.on_input(TermEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

fn typed(app: &mut App, s: &str) {
    for c in s.chars() {
        key(app, KeyCode::Char(c));
    }
}

fn click(app: &mut App, x: u16, y: u16) -> Vec<Effect> {
    app.on_input(TermEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }))
}

fn find(screen: &str, needle: &str) -> (u16, u16) {
    for (y, l) in screen.lines().enumerate() {
        if let Some(i) = l.find(needle) {
            return (l[..i].width() as u16, y as u16);
        }
    }
    panic!("{needle:?} not on screen:\n{screen}");
}

fn refs() -> Vec<GitRef> {
    let r = |n: &str, kind| GitRef { name: n.into(), kind };
    vec![
        r("main", RefKind::Local),
        r("feat/login", RefKind::Local),
        r("origin/main", RefKind::Remote),
        r("origin/feat/search", RefKind::Remote),
        r("origin/feat/settings-page", RefKind::Remote),
        r("origin/fix/race", RefKind::Remote),
        r("v1.4.0", RefKind::Tag),
    ]
}

// ---- snapshots ---------------------------------------------------------

#[test]
fn main_screen() {
    let mut app = app();
    key(&mut app, KeyCode::Down);
    let (s, buf) = render(&mut app, 120, 24);
    insta::assert_snapshot!(s);
    // The selected row is filled with the accent.
    let (x, y) = find(&s, "● fix-race");
    assert_eq!(buf[(x + 2, y)].bg, theme::RUST);
    let (x, y) = find(&s, "● feat-login");
    assert_eq!(buf[(x + 2, y)].bg, ratatui::style::Color::Reset);
}

#[test]
fn narrow_screen() {
    let mut app = app();
    let (s, _) = render(&mut app, 80, 20);
    insta::assert_snapshot!(s);
}

#[test]
fn stacked_detail() {
    let mut app = app();
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Up);
    let (s, _) = render(&mut app, 96, 32);
    insta::assert_snapshot!(s);
}

#[test]
fn remove_dialog() {
    let mut app = app();
    key(&mut app, KeyCode::Char('d'));
    key(&mut app, KeyCode::Char('b'));
    let (s, _) = render(&mut app, 120, 28);
    insta::assert_snapshot!(s);
}

#[test]
fn new_clone_form() {
    let mut app = app();
    let fx = key(&mut app, KeyCode::Char('n'));
    assert_eq!(fx, vec![Effect::LoadRefs]);
    app.on_msg(Msg::Refs {
        root: "/work/app".into(),
        refs: refs(),
    });
    typed(&mut app, "feat/se");
    key(&mut app, KeyCode::Down);
    let (s, _) = render(&mut app, 120, 28);
    insta::assert_snapshot!(s);
}

fn refresh_job(app: &mut App) -> u64 {
    let fx = key(app, KeyCode::Char('r'));
    let Some(Effect::Spawn { id, .. }) = fx.first() else {
        panic!("{fx:?}")
    };
    let id = *id;
    let ev = |e| Msg::Event(id, e);
    app.on_msg(ev(Event::Start { step: "fetch".into() }));
    app.on_msg(ev(Event::Done {
        step: "fetch".into(),
        took: Duration::from_millis(412),
    }));
    for (s, ms) in [("clone image", 7), ("mount", 640), ("checkout", 233)] {
        app.on_msg(ev(Event::Start { step: s.into() }));
        app.on_msg(ev(Event::Done {
            step: s.into(),
            took: Duration::from_millis(ms),
        }));
    }
    app.on_msg(ev(Event::Start {
        step: "warm deps".into(),
    }));
    for i in 0..40 {
        app.on_msg(ev(Event::Output {
            step: "warm deps".into(),
            line: format!(
                "\x1b[32mProgress\x1b[0m: resolved {} reused {}, downloaded 0",
                i * 37,
                i * 35
            ),
        }));
    }
    app.now_ms += 4200;
    id
}

#[test]
fn refresh_progress() {
    let mut app = app();
    refresh_job(&mut app);
    let (s, _) = render(&mut app, 120, 30);
    insta::assert_snapshot!(s);
}

#[test]
fn refresh_finished() {
    let mut app = app();
    let id = refresh_job(&mut app);
    app.on_msg(Msg::Done(
        id,
        Ok(Outcome::Golden(GoldenMeta {
            generation: 4,
            commit: "0badc0ffee1234".into(),
            base: "origin/main".into(),
            created_at: NOW,
            timings: vec![],
            steps: vec![],
            dirty: false,
        })),
    ));
    let (s, _) = render(&mut app, 100, 26);
    insta::assert_snapshot!(s);
}

#[test]
fn help_overlay() {
    let mut app = app();
    key(&mut app, KeyCode::Char('?'));
    let (s, _) = render(&mut app, 100, 32);
    insta::assert_snapshot!(s);
}

#[test]
fn no_golden() {
    let mut app = app_with(false, vec![]);
    let (s, _) = render(&mut app, 100, 20);
    insta::assert_snapshot!(s);
}

#[test]
fn doctor_view() {
    let mut app = app();
    key(&mut app, KeyCode::Char('D'));
    let id = app.jobs.last().unwrap().id;
    let c = |name: &str, level, detail: &str, fix: Option<&str>| Check {
        name: name.into(),
        level,
        detail: detail.into(),
        fix: fix.map(String::from),
    };
    app.on_msg(Msg::Done(
        id,
        Ok(Outcome::Doctor(vec![
            c("disk images", Level::Ok, "ASIF via diskutil (macOS 26+)", None),
            c("state volume", Level::Ok, "~/.clove is on APFS", None),
            c("git", Level::Ok, "2.50.1", None),
            c("steps", Level::Ok, "deps (detected)", None),
            c(
                "pnpm store",
                Level::Warn,
                "clove steps pin the store, but a plain `pnpm add` in a clone would start an empty store on the clone's volume",
                Some("pnpm config set store-dir ~/Library/pnpm/store --global"),
            ),
            c("golden", Level::Ok, "gen 3 at a1b2c3d4e5 (4 commits behind origin/main)", None),
        ])),
    ));
    let (s, _) = render(&mut app, 110, 26);
    insta::assert_snapshot!(s);
}

#[test]
fn too_small() {
    let mut app = app();
    let (s, _) = render(&mut app, 40, 10);
    assert!(s.contains("needs 60×14"), "{s}");
}

#[test]
fn never_panics_at_any_size() {
    for w in [1, 20, 59, 60, 61, 79, 100, 140, 300] {
        for h in [1, 5, 13, 14, 15, 24, 60] {
            for setup in 0..6 {
                let mut app = app();
                match setup {
                    1 => {
                        key(&mut app, KeyCode::Char('d'));
                    }
                    2 => {
                        key(&mut app, KeyCode::Char('n'));
                        app.on_msg(Msg::Refs {
                            root: "/work/app".into(),
                            refs: refs(),
                        });
                        typed(&mut app, "f");
                    }
                    3 => {
                        refresh_job(&mut app);
                    }
                    4 => {
                        key(&mut app, KeyCode::Char('?'));
                    }
                    5 => app.toast(ToastKind::Err, "x ".repeat(200)),
                    _ => {}
                }
                render(&mut app, w, h);
            }
        }
    }
}

// ---- behaviour ---------------------------------------------------------

#[test]
fn creates_a_clone_and_selects_it() {
    let mut app = app();
    key(&mut app, KeyCode::Char('n'));
    app.on_msg(Msg::Refs {
        root: "/work/app".into(),
        refs: refs(),
    });
    typed(&mut app, "feat/sea");
    // Tab completes to the only match, Enter creates.
    key(&mut app, KeyCode::Tab);
    match &app.modal {
        Some(Modal::New(f)) => assert_eq!(f.branch.text, "feat/search"),
        m => panic!("{m:?}"),
    }
    let fx = key(&mut app, KeyCode::Enter);
    let Some(Effect::Spawn { id, root }) = fx.first().cloned() else {
        panic!("{fx:?}")
    };
    assert_eq!(root.as_deref(), Some(std::path::Path::new("/work/app")));
    let job = app.job(id).unwrap();
    match &job.task {
        Task::New(o) => {
            assert_eq!(o.branch.as_deref(), Some("feat/search"));
            assert_eq!(o.base.as_deref(), Some("origin/main"));
        }
        t => panic!("{t:?}"),
    }
    assert!(matches!(app.modal, Some(Modal::Job(j)) if j == id));

    let mut info = clone("feat-search", Some("feat/search"), true);
    info.created_at = NOW;
    let fx = app.on_msg(Msg::Done(id, Ok(Outcome::Clone(info.clone()))));
    assert_eq!(fx, vec![Effect::Load]);
    // The progress view is open and says it; no toast on top of it.
    assert!(app.toasts.is_empty());
    assert!(matches!(&app.job(id).unwrap().result, Some(Ok(s)) if s.starts_with("feat-search ready")));
    let mut list = clones();
    list.push(info);
    app.apply_data(RepoData {
        root: "/work/app".into(),
        status: status(true, list.len()),
        clones: list,
    });
    assert_eq!(app.selected_clone().unwrap().name, "feat-search");
    // Enter on the finished job view cds into the new clone.
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.exit, Some(Exit::Cd("/work/app.clove/feat-search/app".into())));
}

#[test]
fn new_refuses_existing_clone_name() {
    let mut app = app();
    key(&mut app, KeyCode::Char('n'));
    typed(&mut app, "fix/race");
    let fx = key(&mut app, KeyCode::Enter);
    assert!(fx.is_empty());
    assert!(matches!(app.modal, Some(Modal::New(_))));
    assert!(app.toasts.iter().any(|t| t.text.contains("already exists")));
}

#[test]
fn remove_dirty_defaults_to_force() {
    let mut app = app();
    key(&mut app, KeyCode::Char('d'));
    let Some(Modal::Remove(f)) = &app.modal else {
        panic!()
    };
    assert!(f.force, "dirty clone needs force to be removable");
    key(&mut app, KeyCode::Char('b'));
    let fx = key(&mut app, KeyCode::Enter);
    let Some(Effect::Spawn { id, .. }) = fx.first().cloned() else {
        panic!("{fx:?}")
    };
    match &app.job(id).unwrap().task {
        Task::Remove { name, opts } => {
            assert_eq!(name, "feat-login");
            assert!(opts.force && opts.delete_branch);
        }
        t => panic!("{t:?}"),
    }
    // The row shows it is busy and ignores a second remove.
    assert!(app.busy("feat-login").is_some());
    key(&mut app, KeyCode::Char('d'));
    assert!(app.modal.is_none());
}

#[test]
fn remove_cancel_via_button_focus() {
    let mut app = app();
    key(&mut app, KeyCode::Char('d'));
    key(&mut app, KeyCode::Right);
    let fx = key(&mut app, KeyCode::Enter);
    assert!(fx.is_empty());
    assert!(app.modal.is_none());
}

#[test]
fn enter_cds_or_mounts_first() {
    let mut app = app();
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.exit, Some(Exit::Cd("/work/app.clove/feat-login/app".into())));

    let mut app = self::app();
    app.selected = 2; // spike-cache, unmounted
    let fx = key(&mut app, KeyCode::Enter);
    let Some(Effect::Spawn { id, .. }) = fx.first().cloned() else {
        panic!("{fx:?}")
    };
    assert!(app.exit.is_none());
    app.on_msg(Msg::Done(
        id,
        Ok(Outcome::Mounted("/work/app.clove/spike-cache/app".into())),
    ));
    assert_eq!(app.exit, Some(Exit::Cd("/work/app.clove/spike-cache/app".into())));
}

#[test]
fn filter_narrows_and_esc_clears() {
    let mut app = app();
    key(&mut app, KeyCode::Char('/'));
    typed(&mut app, "race");
    assert_eq!(app.visible().len(), 1);
    assert_eq!(app.selected_clone().unwrap().name, "fix-race");
    // `q` is text while filtering, not quit.
    typed(&mut app, "q");
    assert!(app.exit.is_none());
    key(&mut app, KeyCode::Backspace);
    key(&mut app, KeyCode::Enter);
    let (s, _) = render(&mut app, 120, 24);
    assert!(s.contains("clones 1/5  /race"), "{s}");
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.visible().len(), 5);
    assert!(app.exit.is_none());
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.exit, Some(Exit::Quit));
}

#[test]
fn mouse_selects_rows_tabs_and_hints() {
    let mut app = app();
    let (s, _) = render(&mut app, 120, 24);
    let (x, y) = find(&s, "spike-cache");
    click(&mut app, x, y);
    assert_eq!(app.selected_clone().unwrap().name, "spike-cache");

    // Wheel over the list moves the selection.
    app.on_input(TermEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.selected, 3);

    // Footer hints are buttons.
    let (x, y) = find(&s, "? help");
    click(&mut app, x, y);
    assert!(matches!(app.modal, Some(Modal::Help)));
    key(&mut app, KeyCode::Esc);

    // Repo tabs switch repos.
    let (x, y) = find(&s, " mono ");
    let fx = click(&mut app, x + 1, y);
    assert_eq!(fx, vec![Effect::Load]);
    assert_eq!(app.repo().unwrap().name, "mono");
}

#[test]
fn double_click_cds() {
    let mut app = app();
    let (s, _) = render(&mut app, 120, 24);
    let (x, y) = find(&s, "● fix-race");
    click(&mut app, x, y);
    app.now_ms += 150;
    click(&mut app, x, y);
    assert_eq!(app.exit, Some(Exit::Cd("/work/app.clove/fix-race/app".into())));
}

#[test]
fn job_log_scrolls_and_follows() {
    let mut app = app();
    let id = refresh_job(&mut app);
    render(&mut app, 120, 30);
    let job = app.job(id).unwrap();
    assert!(job.follow);
    let bottom = job.scroll.target();
    assert!(bottom > 0);
    key(&mut app, KeyCode::PageUp);
    let job = app.job(id).unwrap();
    assert!(!job.follow);
    assert!(job.scroll.target() < bottom);
    // New output does not yank the view while reading.
    app.on_msg(Msg::Event(
        id,
        Event::Output {
            step: "warm deps".into(),
            line: "more".into(),
        },
    ));
    let (s, _) = render(&mut app, 120, 30);
    assert!(s.contains("more · end"), "{s}");
    key(&mut app, KeyCode::End);
    render(&mut app, 120, 30);
    assert!(app.job(id).unwrap().follow);
    // Esc hides the view; the job keeps running and l brings it back.
    key(&mut app, KeyCode::Esc);
    assert!(app.modal.is_none());
    key(&mut app, KeyCode::Char('l'));
    assert!(matches!(app.modal, Some(Modal::Job(j)) if j == id));
}

#[test]
fn failed_step_is_marked_from_warning() {
    let mut app = app();
    key(&mut app, KeyCode::Char('n'));
    typed(&mut app, "x");
    let fx = key(&mut app, KeyCode::Enter);
    let Some(Effect::Spawn { id, .. }) = fx.first().cloned() else {
        panic!()
    };
    app.on_msg(Msg::Event(
        id,
        Event::Start {
            step: "setup deps".into(),
        },
    ));
    app.on_msg(Msg::Event(
        id,
        Event::Warn {
            message: "setup deps failed (exit status: 1): pnpm i\nERR lockfile\nfull log: /x".into(),
        },
    ));
    let job = app.job(id).unwrap();
    assert_eq!(job.steps[0].state, StepState::Failed);
    assert_eq!(job.log.len(), 3);
}

#[test]
fn worker_errors_become_toasts() {
    let mut app = app();
    key(&mut app, KeyCode::Char('u'));
    let id = app.jobs.last().unwrap().id;
    app.on_msg(Msg::Done(
        id,
        Err("diskutil unmount failed: Resource busy\nmore".into()),
    ));
    let t = app.toasts.last().unwrap();
    assert_eq!(t.kind, ToastKind::Err);
    assert_eq!(
        t.text,
        "unmount feat-login: diskutil unmount failed: Resource busy"
    );
}

#[test]
fn toasts_expire() {
    let mut app = app();
    app.toast(ToastKind::Ok, "hi");
    app.now_ms += 5000;
    app.animate();
    assert!(app.toasts.is_empty());
}

#[test]
fn reload_cadence() {
    let mut app = app();
    assert!(!app.needs_reload());
    app.now_ms += crate::app::RELOAD_MS;
    assert!(app.needs_reload());
    app.loading = true;
    assert!(!app.needs_reload());
}
