//! Snapshot tests of key screens (rendered to text with ratatui's TestBackend)
//! and behaviour tests that drive [`App`] with keys, clicks and worker messages.

use crate::app::{App, Effect, Exit};
use crate::model::*;
use crate::theme;
use crate::ui;
use clove_core::config::RefreshPolicy;
use clove_core::doctor::{Check, Level};
use clove_core::{
    CloneInfo, Config, Event, Exec, GoldenMeta, GoldenStatus, RepoStatus, RepoSummary, Step, Timing,
};
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
        image_bytes: 3 * 1024 * 1024 * 1024,
        own_bytes: 14 * 1024 * 1024,
        created_at: NOW - 3 * 3600,
        golden_generation: 3,
        timings: vec![
            t("clone image", 6.0),
            t("mount", 612.0),
            t("fetch", 288.0),
            t("checkout", 141.0),
        ],
        total_ms: 1047.0,
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
    b.own_bytes = 230 * 1024 * 1024;
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
                step_hashes: Default::default(),
                copied: vec![],
            },
            age_secs: 7200,
            behind: Some(4),
            image_bytes: 1288 * 1024 * 1024,
        }),
        clones: n,
        mounted: n.saturating_sub(1),
        steps: vec![],
        steps_detected: true,
        freshness: None,
        copy: vec![],
    }
}

fn summary(clones: usize, generation: Option<u32>, stale: bool) -> RepoSummary {
    RepoSummary {
        clones,
        mounted: clones,
        generation,
        stale,
        refreshing: false,
        prunable: 0,
    }
}

fn repos() -> Vec<RepoTab> {
    [
        ("app", summary(5, Some(3), false)),
        ("mono", summary(2, Some(7), true)),
        ("web", summary(0, None, false)),
    ]
    .into_iter()
    .map(|(n, s)| RepoTab {
        name: n.to_string(),
        root: PathBuf::from(format!("/work/{n}")),
        summary: Some(s),
    })
    .collect()
}

fn config() -> Config {
    Config {
        base: "origin/main".into(),
        clone_root: "/work/app.clove".into(),
        image_size: "300G".into(),
        fetch: true,
        fetch_interval: 300,
        exec: Exec::Auto,
        env: Default::default(),
        steps: vec![Step {
            name: "deps".into(),
            warm: Some("pnpm install --frozen-lockfile".into()),
            setup: Some("pnpm install --prefer-offline".into()),
            watch: vec!["pnpm-lock.yaml".into()],
            ..Default::default()
        }],
        steps_detected: true,
        copy: None,
        refresh: RefreshPolicy {
            auto: true,
            max_age_secs: 86_400,
            max_behind: 100,
        },
        sources: vec![],
    }
}

fn data(root: impl Into<PathBuf>, status: RepoStatus, clones: Vec<CloneInfo>) -> RepoData {
    let summary = summary(
        clones.len(),
        status.golden.as_ref().map(|g| g.meta.generation),
        false,
    );
    RepoData {
        root: root.into(),
        status,
        clones,
        config: config(),
        prs: Default::default(),
        summary,
    }
}

fn app_with(golden: bool, clones: Vec<CloneInfo>) -> App {
    let mut app = App::new(repos(), 0);
    app.now_unix = NOW;
    app.now_ms = 10_000;
    app.loaded_ms = Some(10_000);
    let n = clones.len();
    app.apply_data(data("/work/app", status(golden, n), clones));
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
    assert_eq!(buf[(x + 2, y)].bg, theme::BOTTLE);
    let (x, y) = find(&s, "● feat-login");
    assert_eq!(buf[(x + 2, y)].bg, ratatui::style::Color::Reset);
}

#[test]
fn wide_screen_has_sidebar_and_detail() {
    let mut app = app();
    app.service = Some(true);
    let (s, _) = render(&mut app, 160, 26);
    insta::assert_snapshot!(s);
}

#[test]
fn sidebar_switches_repo_and_opens_tools() {
    let mut app = app();
    let (s, _) = render(&mut app, 140, 26);
    let (x, y) = find(&s, "mono");
    let fx = click(&mut app, x, y);
    assert_eq!(app.repo().unwrap().name, "mono");
    assert_eq!(fx, vec![Effect::Load]);
    let (s, _) = render(&mut app, 140, 26);
    let (x, y) = find(&s, "s settings");
    click(&mut app, x, y);
    assert!(app.modal.is_none(), "settings wait until the repo has loaded");
}

#[test]
fn settings_view() {
    let mut app = app();
    key(&mut app, KeyCode::Char('s'));
    assert!(matches!(app.modal, Some(Modal::Settings(_))));
    let (s, _) = render(&mut app, 120, 60);
    for want in [
        "CONFIG FILES",
        "all repos",
        "you, this repo",
        "origin/main",
        "STEPS · DETECTED FROM THE REPO",
        "pnpm install --frozen-lockfile",
        "pnpm-lock.yaml",
        "FRESHNESS",
        "THIS MACHINE",
        "service",
    ] {
        assert!(s.contains(want), "{want:?} missing:\n{s}");
    }
    // e edits your personal file for this repo
    let fx = key(&mut app, KeyCode::Char('e'));
    assert!(
        matches!(&fx[..], [Effect::EditConfig(p)] if p.ends_with("repos/app.toml")),
        "{fx:?}"
    );
    // S turns the service on when it is off
    app.service = Some(false);
    let fx = key(&mut app, KeyCode::Char('S'));
    assert!(matches!(&fx[..], [Effect::Spawn { root: Some(_), .. }]));
    assert!(matches!(
        app.jobs.last().unwrap().task,
        Task::Service { install: true }
    ));
    key(&mut app, KeyCode::Esc);
    assert!(app.modal.is_none());
}

#[test]
fn cleanup_view() {
    use clove_core::{MergeCheck, Pr, PrState, Verdict};
    let m = |clone: &str, verdict, reason: &str, pr: Option<u32>| MergeCheck {
        clone: clone.into(),
        branch: Some(clone.into()),
        head: Some("9f8e7d6c5b".into()),
        pr: pr.map(|n| Pr {
            number: n,
            state: PrState::Merged,
            head_oid: "9f8e7d6c5b".into(),
            title: String::new(),
            url: String::new(),
        }),
        verdict,
        reason: reason.into(),
        own_bytes: 14 * 1024 * 1024,
    };
    let mut app = app();
    app.modal = Some(Modal::Gc(GcView {
        result: Some(Cleanup {
            leftovers: vec!["unregistered clone image /state/clones/old".into()],
            checks: vec![
                m("feat-login", Verdict::Prune, "PR #3672 merged", Some(3672)),
                m(
                    "fix-race",
                    Verdict::Review,
                    "PR #3690 merged, but 2 commits after it",
                    Some(3690),
                ),
                m("spike-cache", Verdict::Keep, "no PR", None),
            ],
            warning: None,
            removed: vec![],
        }),
        ..Default::default()
    }));
    let (s, _) = render(&mut app, 120, 30);
    insta::assert_snapshot!(s);
    let fx = key(&mut app, KeyCode::Char('a'));
    assert!(matches!(&fx[..], [Effect::Spawn { .. }]));
    assert!(matches!(app.jobs.last().unwrap().task, Task::Gc { apply: true }));
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
    // From the remove button, up twice reaches "also delete branch".
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Char(' '));
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
            step_hashes: Default::default(),
            copied: vec![],
        })),
    ));
    let (s, _) = render(&mut app, 100, 26);
    insta::assert_snapshot!(s);
}

#[test]
fn quick_clone_without_output() {
    // A clone whose steps print nothing: just the step list, no empty
    // output panel; a skipped step keeps its full name.
    let mut app = app();
    app.modal = None;
    let id = match app.spawn(Task::New(Default::default()), After::Nothing).first() {
        Some(Effect::Spawn { id, .. }) => *id,
        fx => panic!("{fx:?}"),
    };
    let ev = |e| Msg::Event(id, e);
    for (s, ms) in [("clone image", 0), ("mount", 646)] {
        app.on_msg(ev(Event::Start { step: s.into() }));
        app.on_msg(ev(Event::Done {
            step: s.into(),
            took: Duration::from_millis(ms),
        }));
    }
    app.on_msg(ev(Event::Skip {
        step: "fetch".into(),
        reason: "fetched 3m ago (--fetch to force)".into(),
    }));
    app.on_msg(ev(Event::Skip {
        step: "setup pnpm".into(),
        reason: "unchanged since golden".into(),
    }));
    app.on_msg(Msg::Done(
        id,
        Ok(Outcome::Clone(clone("pierre-test", Some("pierre/test"), true))),
    ));
    let (s, _) = render(&mut app, 120, 24);
    insta::assert_snapshot!(s);
    assert!(!s.contains("output"), "no empty output panel:\n{s}");
    assert!(
        s.contains("– fetch ") && s.contains("fetched 3m ago (--fetch to force)"),
        "skipped step keeps its name:\n{s}"
    );
}

#[test]
fn removing_the_clone_you_are_in_leaves_it_first() {
    // Started from inside feat-login: the shell holds its volume, so the TUI
    // quits, moves the shell to the main checkout and removes it from there.
    let mut inside = app();
    inside.start_dir = "/work/app.clove/feat-login/app/src".into();
    inside.integrated = true;
    assert_eq!(inside.selected_clone().unwrap().name, "feat-login");
    key(&mut inside, KeyCode::Char('d'));
    assert!(matches!(&inside.modal, Some(Modal::Remove(f)) if f.inside));
    let (s, _) = render(&mut inside, 120, 30);
    assert!(s.contains("you're in this clone"), "{s}");
    key(&mut inside, KeyCode::Up);
    key(&mut inside, KeyCode::Up);
    key(&mut inside, KeyCode::Char(' '));
    let fx = key(&mut inside, KeyCode::Char('y'));
    assert!(fx.is_empty(), "nothing runs in the TUI: {fx:?}");
    match &inside.exit {
        Some(Exit::CdThen(root, args)) => {
            assert_eq!(root, &PathBuf::from("/work/app"));
            assert_eq!(args.join(" "), "rm -D --force feat-login");
        }
        e => panic!("{e:?}"),
    }

    // Without the shell integration it can't move the shell: it says so.
    let mut other = app();
    other.start_dir = "/work/app.clove/feat-login/app".into();
    key(&mut other, KeyCode::Char('d'));
    key(&mut other, KeyCode::Char('y'));
    assert!(other.exit.is_none());
    assert!(other.toasts.iter().any(|t| t.text.contains("inside this clone")));

    // Another clone is removed in place as before.
    let mut elsewhere = app();
    elsewhere.start_dir = "/work/app".into();
    key(&mut elsewhere, KeyCode::Char('d'));
    let fx = key(&mut elsewhere, KeyCode::Char('y'));
    assert!(matches!(&fx[..], [Effect::Spawn { .. }]));
}

#[test]
fn dialogs_use_arrows_and_enter() {
    // Settings: a cursor over the actionable rows; Enter acts on it.
    let mut st = app();
    st.service = Some(false);
    key(&mut st, KeyCode::Char('s'));
    let (s, _) = render(&mut st, 120, 60);
    assert!(
        s.contains("› all repos"),
        "cursor starts on the first config file:\n{s}"
    );
    key(&mut st, KeyCode::Down);
    key(&mut st, KeyCode::Down);
    let fx = key(&mut st, KeyCode::Enter);
    assert!(
        matches!(&fx[..], [Effect::EditConfig(p)] if p.ends_with("repos/app.toml")),
        "{fx:?}"
    );
    key(&mut st, KeyCode::Down);
    let fx = key(&mut st, KeyCode::Enter);
    assert!(
        matches!(&fx[..], [Effect::Spawn { .. }]),
        "service row toggles: {fx:?}"
    );
    assert!(matches!(
        st.jobs.last().unwrap().task,
        Task::Service { install: true }
    ));

    // Cleanup: Enter on the focused button; arrows move to close.
    let mut gc = app();
    gc.modal = Some(Modal::Gc(GcView {
        result: Some(Cleanup {
            leftovers: vec!["remove old golden gen-2".into()],
            ..Default::default()
        }),
        ..Default::default()
    }));
    key(&mut gc, KeyCode::Right);
    key(&mut gc, KeyCode::Enter);
    assert!(gc.modal.is_none(), "close had focus");
    gc.modal = Some(Modal::Gc(GcView {
        result: Some(Cleanup {
            leftovers: vec!["remove old golden gen-2".into()],
            ..Default::default()
        }),
        ..Default::default()
    }));
    let fx = key(&mut gc, KeyCode::Enter);
    assert!(matches!(&fx[..], [Effect::Spawn { .. }]));
    assert!(matches!(gc.jobs.last().unwrap().task, Task::Gc { apply: true }));
}

#[test]
fn toasts_sit_bottom_right() {
    let mut app = app();
    app.toast(ToastKind::Ok, "first");
    app.toast(ToastKind::Err, "second, newest");
    let (s, _) = render(&mut app, 120, 24);
    let (x1, y1) = find(&s, "first");
    let (x2, y2) = find(&s, "second, newest");
    assert!(y2 > y1, "newest at the bottom:\n{s}");
    assert!(y2 >= 18, "near the bottom of the screen:\n{s}");
    assert!(x1 > 80 && x2 > 80, "on the right:\n{s}");
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
    app.apply_data(data("/work/app", status(true, list.len()), list));
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
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Char(' '));
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

#[test]
fn quit_waits_for_running_operations() {
    let mut app = app();
    let fx = key(&mut app, KeyCode::Char('r'));
    let Some(Effect::Spawn { id, .. }) = fx.first().cloned() else {
        panic!("{fx:?}")
    };
    key(&mut app, KeyCode::Esc); // hide the progress view
    key(&mut app, KeyCode::Char('q'));
    assert!(app.exit.is_none(), "must not kill a refresh halfway");
    assert!(app.quitting);
    let (s, _) = render(&mut app, 120, 24);
    assert!(s.contains("finishing before quit"), "{s}");
    app.on_msg(Msg::Done(id, Err("interrupted".into())));
    assert_eq!(app.exit, Some(Exit::Quit));

    // Asking twice quits regardless.
    let mut app = self::app();
    key(&mut app, KeyCode::Char('u'));
    app.on_input(TermEvent::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));
    assert!(app.exit.is_none());
    app.on_input(TermEvent::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));
    assert_eq!(app.exit, Some(Exit::Quit));
}

#[test]
fn golden_detail_when_no_clones() {
    let mut app = App::new(repos(), 0);
    app.now_unix = NOW;
    let mut st = status(true, 0);
    if let Some(g) = &mut st.golden {
        g.meta.timings = vec![
            t("create image", 1830.0),
            t("mount", 702.0),
            t("fetch", 1204.0),
            t("checkout", 3410.0),
            t("warm deps", 48_200.0),
            t("warm direnv", 6100.0),
            t("unmount", 390.0),
        ];
        g.meta.steps = vec!["deps".into(), "direnv".into()];
    }
    app.apply_data(data("/work/app", st, vec![]));
    let (s, _) = render(&mut app, 110, 22);
    insta::assert_snapshot!(s);
}

#[test]
fn compact_golden_line() {
    let mut app = app();
    let (s, _) = render(&mut app, 72, 20);
    assert!(s.contains("↓4 behind ·"), "{s}");
}

#[test]
fn remove_dialog_is_clickable() {
    let mut app = app();
    key(&mut app, KeyCode::Char('d'));
    let (s, _) = render(&mut app, 120, 28);
    let (x, y) = find(&s, "also delete branch");
    click(&mut app, x, y);
    let Some(Modal::Remove(f)) = &app.modal else {
        panic!()
    };
    assert!(f.delete_branch);
    // A click inside the dialog but off any control does nothing.
    let (x, y) = find(&s, "uncommitted changes");
    click(&mut app, x, y);
    assert!(app.modal.is_some());
    // Cancel button closes it; so does a click outside.
    let (x, y) = find(&s, " cancel ");
    click(&mut app, x + 1, y);
    assert!(app.modal.is_none());
    key(&mut app, KeyCode::Char('d'));
    render(&mut app, 120, 28);
    click(&mut app, 2, 6);
    assert!(app.modal.is_none());
    // The confirm button removes.
    key(&mut app, KeyCode::Char('d'));
    let (s, _) = render(&mut app, 120, 28);
    let (x, y) = find(&s, "discard & remove");
    let fx = click(&mut app, x, y);
    assert!(matches!(fx.first(), Some(Effect::Spawn { .. })), "{fx:?}");
}

#[test]
fn long_list_scrolls_with_a_scrollbar() {
    let many: Vec<CloneInfo> = (0..40)
        .map(|i| {
            let mut c = clone(
                &format!("task-{i:02}"),
                Some(&format!("agent/task-{i:02}")),
                i % 3 != 0,
            );
            c.ahead = c.mounted.then_some(i % 4);
            c
        })
        .collect();
    let mut app = app_with(true, many);
    for _ in 0..25 {
        key(&mut app, KeyCode::Down);
    }
    let (s, buf) = render(&mut app, 100, 24);
    insta::assert_snapshot!(s);
    let (x, y) = find(&s, "● task-25");
    assert_eq!(buf[(x + 2, y)].bg, theme::BOTTLE, "selection stays in view");
    assert!(s.contains('┃'), "scrollbar thumb drawn");
    // Easing: right after a jump the offset is still travelling.
    key(&mut app, KeyCode::Home);
    let mut term = Terminal::new(TestBackend::new(100, 24)).unwrap();
    term.draw(|f| ui::draw(f, &mut app)).unwrap(); // draw sets the new target
    let before = app.list_scroll.offset();
    app.animate();
    let after = app.list_scroll.offset();
    assert!(after < before && after > 0, "{before} -> {after}");
}

#[test]
fn switching_repo_mid_load_reloads_at_once() {
    let mut app = app();
    app.loading = true;
    let fx = key(&mut app, KeyCode::Tab);
    assert!(fx.is_empty(), "a load is in flight");
    app.on_msg(Msg::Loaded {
        root: "/work/app".into(),
        data: Err("stale".into()),
        repos: vec![],
        service: false,
    });
    assert!(app.needs_reload(), "mono should load now, not in 3s");
    // Branch suggestions from the old repo are not offered in the new one.
    app.refs = refs();
    app.refs_root = Some("/work/app".into());
    app.apply_data(data("/work/mono", status(true, 0), vec![]));
    key(&mut app, KeyCode::Char('n'));
    assert!(app.refs.is_empty());
}

#[test]
fn reload_requested_mid_load_runs_after_it() {
    let mut app = app();
    app.loading = true;
    app.reload_queued = true; // what the runtime does with a Load while loading
    app.on_msg(Msg::Loaded {
        root: "/work/app".into(),
        data: Err("x".into()),
        repos: vec![],
        service: false,
    });
    assert!(app.needs_reload());
}
