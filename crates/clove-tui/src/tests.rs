//! Snapshot tests of key screens (rendered to text with ratatui's TestBackend)
//! and behaviour tests that drive [`App`] with keys, clicks and worker messages.

use crate::app::{App, Effect, Exit};
use crate::model::*;
use crate::panes::{self, Panes, SplitId};
use crate::theme;
use crate::ui;
use clove_core::config::RefreshPolicy;
use clove_core::doctor::{Check, Level};
use clove_core::settings::Layer;
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
        issue: None,
        removing_when_free: false,
        operation: None,
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
        nested_installs: 10,
        copy: None,
        refresh: RefreshPolicy {
            auto: true,
            max_age_secs: 86_400,
            max_behind: 100,
        },
        prune: Default::default(),
        update: Default::default(),
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
        worktrees: vec![],
        fields: clove_core::settings::resolve(&[None, None, None], &config()),
        config: config(),
        prs: Default::default(),
        summary,
        watch: vec![],
        history: vec![],
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

/// What core would offer from refs(): no base (main), nothing checked out in
/// a clone (feat/login, fix/race).
fn candidates() -> Vec<clove_core::BranchCandidate> {
    ["feat/search", "feat/settings-page"]
        .into_iter()
        .map(|n| clove_core::BranchCandidate {
            name: n.into(),
            remote: true,
        })
        .collect()
}

#[test]
fn branch_suggestions_wait_for_typing_and_skip_taken_branches() {
    let mut app = app();
    key(&mut app, KeyCode::Char('n'));
    app.on_msg(Msg::Refs {
        root: "/work/app".into(),
        refs: refs(),
        candidates: candidates(),
    });
    let Some(Modal::New(f)) = app.modal.clone() else {
        panic!()
    };
    assert!(app.suggestions(&f).is_empty(), "empty field: no list");
    typed(&mut app, "f");
    let Some(Modal::New(f)) = app.modal.clone() else {
        panic!()
    };
    let s = app.suggestions(&f);
    assert_eq!(
        s,
        vec!["feat/search", "feat/settings-page"],
        "fix/race is checked out in a clone"
    );
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
            ..Default::default()
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
        candidates: candidates(),
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

fn on_field(app: &mut App, key_name: &str) {
    let i = clove_core::settings::FIELDS
        .iter()
        .position(|f| f.key == key_name)
        .unwrap();
    let at = app
        .settings_actions()
        .iter()
        .position(|a| *a == SettingsAction::Field(i))
        .unwrap();
    if let Some(Modal::Settings(v)) = &mut app.modal {
        v.cursor = at;
    }
}

fn writes(fx: &[Effect]) -> Vec<(String, Layer, Option<String>)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::SetConfig(w) => Some((w.key.to_string(), w.layer, w.value.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn settings_toggle_edit_and_reset() {
    let mut app = app();
    key(&mut app, KeyCode::Char('s'));
    // A toggle writes the flipped value to your file, then reloads.
    on_field(&mut app, "fetch");
    let fx = key(&mut app, KeyCode::Enter);
    assert_eq!(writes(&fx), [("fetch".into(), Layer::You, Some("false".into()))]);
    assert!(fx.contains(&Effect::Load));
    match &fx[0] {
        Effect::SetConfig(w) => assert!(w.path.ends_with("repos/app.toml"), "{:?}", w.path),
        e => panic!("{e:?}"),
    }

    // ← moves the target to team; typing edits a value, Enter saves it there.
    key(&mut app, KeyCode::Left);
    on_field(&mut app, "refresh.max_age");
    key(&mut app, KeyCode::Enter);
    let (s, _) = render(&mut app, 120, 60);
    assert!(s.contains("→ team"), "editor shows where it saves:\n{s}");
    assert!(s.contains("⏎ save"), "{s}");
    for _ in 0..3 {
        key(&mut app, KeyCode::Backspace);
    }
    for c in "2d".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    let fx = key(&mut app, KeyCode::Enter);
    assert_eq!(
        writes(&fx),
        [("refresh.max_age".into(), Layer::Team, Some("2d".into()))]
    );
    match &fx[0] {
        Effect::SetConfig(w) => assert!(w.path.ends_with("app/.clove.toml"), "{:?}", w.path),
        e => panic!("{e:?}"),
    }

    // A bad value stays in the editor with an error toast, and Esc drops it.
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('x'));
    let fx = key(&mut app, KeyCode::Enter);
    assert!(writes(&fx).is_empty());
    assert!(
        app.toasts.iter().any(|t| t.text.contains("bad duration")),
        "{:?}",
        app.toasts
    );
    key(&mut app, KeyCode::Esc);
    assert!(matches!(&app.modal, Some(Modal::Settings(v)) if v.editing.is_none()));

    // A choice cycles with arrows.
    on_field(&mut app, "exec");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Right);
    let fx = key(&mut app, KeyCode::Enter);
    assert_eq!(writes(&fx), [("exec".into(), Layer::Team, Some("direnv".into()))]);

    // x resets only what the target file sets.
    let fx = key(&mut app, KeyCode::Char('x'));
    assert!(writes(&fx).is_empty(), "nothing set in team yet");
    assert!(app.toasts.iter().any(|t| t.text.contains("isn't set in team")));
    if let Some(d) = app.data.values_mut().next() {
        let i = d.fields.iter().position(|f| f.field.key == "exec").unwrap();
        d.fields[i].layers[1] = Some("mise".into());
    }
    let fx = key(&mut app, KeyCode::Char('x'));
    assert_eq!(writes(&fx), [("exec".into(), Layer::Team, None)]);
}

#[test]
fn settings_mouse_picks_the_target_file() {
    let mut app = app();
    key(&mut app, KeyCode::Char('s'));
    let (s, _) = render(&mut app, 120, 60);
    let line = s.lines().find(|l| l.contains("save edits to")).unwrap();
    let y = s.lines().position(|l| l.contains("save edits to")).unwrap() as u16;
    let x = line.find(" team ").unwrap();
    let x = UnicodeWidthStr::width(&line[..x]) as u16 + 2;
    click(&mut app, x, y);
    assert!(matches!(&app.modal, Some(Modal::Settings(v)) if v.target == Layer::Team));
    // Clicking a setting focuses it; Enter then edits it.
    let (s, _) = render(&mut app, 120, 60);
    let (x, y) = find(&s, "image size");
    click(&mut app, x, y);
    key(&mut app, KeyCode::Enter);
    assert!(matches!(&app.modal, Some(Modal::Settings(v)) if v.editing.is_some()));
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
    for _ in 0..=clove_core::settings::FIELDS.len() {
        key(&mut st, KeyCode::Down);
    }
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
fn hover_highlights_what_a_click_would_do() {
    use ratatui::style::Modifier;
    let mut hov = app();
    let (s, _) = render(&mut hov, 120, 24);
    let moved = |a: &mut App, x: u16, y: u16| {
        a.on_input(TermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }))
    };
    // A footer hint.
    let (x, y) = find(&s, "n new");
    moved(&mut hov, x, y);
    let (_, buf) = render(&mut hov, 120, 24);
    assert!(buf[(x, y)].modifier.contains(Modifier::BOLD));
    assert_eq!(buf[(x + 2, y)].fg, crate::theme::SAGE);
    assert_eq!(hov.pointer, Pointer::Hand, "hand pointer over a hint");
    // A clone row.
    let (x, y) = find(&s, "fix-race");
    moved(&mut hov, x, y);
    let (_, buf) = render(&mut hov, 120, 24);
    assert!(buf[(x, y)].modifier.contains(Modifier::BOLD));
    // Empty space: nothing, plain pointer.
    moved(&mut hov, 60, 20);
    let (_, buf) = render(&mut hov, 120, 24);
    assert!(!buf[(60, 20)].modifier.contains(Modifier::BOLD));
    assert_eq!(hov.pointer, Pointer::Default);
    // Behind an open modal: a click closes it, but hover highlights nothing.
    key(&mut hov, KeyCode::Char('s'));
    moved(&mut hov, 2, 2);
    let (_, buf) = render(&mut hov, 120, 24);
    assert_ne!(buf[(2, 2)].fg, crate::theme::SAGE);
    assert_eq!(hov.pointer, Pointer::Default, "no hand over the backdrop");
    click(&mut hov, 2, 2);
    assert!(hov.modal.is_none(), "a click outside still closes it");
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
                            candidates: candidates(),
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
        candidates: candidates(),
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
    app.now_ms += crate::app::RELOAD_UNWATCHED_MS;
    assert!(app.needs_reload(), "polls fast without a watcher");
    app.loading = true;
    assert!(!app.needs_reload());
}

#[test]
fn a_watcher_slows_polling_and_changes_reload_at_once() {
    let mut app = app();
    app.watching = true;
    app.now_ms += crate::app::RELOAD_UNWATCHED_MS;
    assert!(!app.needs_reload(), "the watcher covers the fast path");
    app.on_msg(Msg::FsChanged);
    assert!(app.needs_reload(), "a change on disk reloads right away");
    app.now_ms = 10_000;
    app.loaded_ms = Some(10_000);
    app.now_ms += crate::app::RELOAD_MS;
    assert!(app.needs_reload(), "still polls now and then");
    // A change during a load: that load may predate it, so another follows.
    app.loading = true;
    app.on_msg(Msg::FsChanged);
    assert!(app.reload_queued);
}

#[test]
fn a_load_hands_its_watch_targets_to_the_runtime() {
    let mut app = app();
    let mut d = data("/work/app", status(true, 0), vec![]);
    d.watch = vec![clove_core::watch::repos_target()];
    app.apply_data(d);
    assert_eq!(app.watch.as_ref().map(Vec::len), Some(1));
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
        agents: None,
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
        agents: None,
    });
    assert!(app.needs_reload());
}

// ---- tooltips ----------------------------------------------------------

fn moved(app: &mut App, x: u16, y: u16) {
    app.on_input(TermEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }));
}

const LONG: &str = "pierre-onboarding-flow-with-a-long-name";

fn long_app() -> App {
    let mut cs = clones();
    cs.push(clone(LONG, Some("pierre/onboarding-flow-v3"), true));
    app_with(true, cs)
}

#[test]
fn tooltip_shows_a_cut_name_after_the_delay() {
    let mut app = long_app();
    let (s, _) = render(&mut app, 120, 24);
    assert!(!s.contains(LONG), "the name is cut in the list:\n{s}");
    let (x, y) = find(&s, "pierre-onboarding-flow-wi");
    moved(&mut app, x + 3, y);
    assert!(app.animate(), "keeps redrawing while a tooltip is pending");
    let (s, _) = render(&mut app, 120, 24);
    assert!(!s.contains(LONG), "not at once:\n{s}");
    app.now_ms += crate::tooltip::DELAY_MS / 2;
    let (s, _) = render(&mut app, 120, 24);
    assert!(!s.contains(LONG), "not before the delay:\n{s}");
    app.now_ms += crate::tooltip::DELAY_MS / 2 + 1;
    let (s, buf) = render(&mut app, 120, 24);
    let (tx, ty) = find(&s, LONG);
    // Below-right of the mouse, in a bottle-bordered box.
    assert_eq!(ty, y + 2, "{s}");
    assert!(tx > x + 3, "{s}");
    assert_eq!(buf[(tx - 2, ty - 1)].symbol(), "╭");
    assert_eq!(buf[(tx - 2, ty - 1)].fg, theme::BOTTLE);
    assert!(!app.animate(), "shown: no more redraws needed");
    // Moving within the name keeps it; moving off hides it.
    moved(&mut app, x + 4, y);
    assert!(render(&mut app, 120, 24).0.contains(LONG));
    moved(&mut app, 60, 20);
    assert!(!render(&mut app, 120, 24).0.contains(LONG));
}

#[test]
fn no_tooltip_on_text_that_fits() {
    let mut app = long_app();
    let (s, _) = render(&mut app, 120, 24);
    let (x, y) = find(&s, "fix-race");
    moved(&mut app, x + 2, y);
    app.now_ms += 2 * crate::tooltip::DELAY_MS;
    let (after, _) = render(&mut app, 120, 24);
    assert_eq!(s, after);
}

#[test]
fn badges_explain_themselves() {
    let mut app = long_app();
    let (s, _) = render(&mut app, 120, 24);
    // The status dot of an unmounted clone.
    let (x, y) = find(&s, "○ spike-cache");
    moved(&mut app, x, y);
    app.now_ms += crate::tooltip::DELAY_MS;
    let (s2, _) = render(&mut app, 120, 24);
    assert!(s2.contains("Not mounted: m mounts it."), "{s2}");
    // A footer hint.
    let (x, y) = find(&s, "U update");
    moved(&mut app, x, y);
    app.now_ms += crate::tooltip::DELAY_MS;
    let (s3, _) = render(&mut app, 120, 24);
    assert!(s3.contains("bring the clone onto the latest base"), "{s3}");
}

#[test]
fn tooltip_stays_on_screen_at_the_edges() {
    let mut app = long_app();
    let (s, _) = render(&mut app, 120, 24);
    // The PATH column ends at the right edge; the long clone's path is cut.
    let (_, y) = find(&s, "pierre-onboarding-flow-wi");
    let x = 116;
    moved(&mut app, x, y);
    app.now_ms += crate::tooltip::DELAY_MS;
    let (s, buf) = render(&mut app, 120, 24);
    let (tx, ty) = find(&s, "/work/app.clove/");
    assert_eq!(ty, y + 2, "still below:\n{s}");
    assert!(tx < x, "slid left of the mouse:\n{s}");
    let right = (0..120)
        .rev()
        .find(|&cx| buf[(cx, ty - 1)].symbol() == "╮")
        .unwrap();
    assert!(right < 120 && right > tx, "{s}");

    // The bottom row (footer): above the mouse, the hint left uncovered.
    let (x, y) = find(&s, "? help");
    assert_eq!(y, 23);
    moved(&mut app, x + 2, y);
    app.now_ms += crate::tooltip::DELAY_MS;
    let (s, _) = render(&mut app, 120, 24);
    let (_, ty) = find(&s, "every key");
    assert_eq!(ty, 21, "the tooltip ends just above:\n{s}");
    assert_eq!(find(&s, "? help"), (x, y), "{s}");
}

#[test]
fn a_key_or_click_hides_the_tooltip() {
    let mut app = long_app();
    let (s, _) = render(&mut app, 120, 24);
    let (x, y) = find(&s, "pierre-onboarding-flow-wi");
    moved(&mut app, x + 3, y);
    app.now_ms += crate::tooltip::DELAY_MS;
    assert!(render(&mut app, 120, 24).0.contains(LONG));
    key(&mut app, KeyCode::Char('x'));
    assert!(!render(&mut app, 120, 24).0.contains(LONG), "a key hides it");
    // Coming back shows it again after the delay.
    moved(&mut app, x + 4, y);
    app.now_ms += crate::tooltip::DELAY_MS;
    assert!(render(&mut app, 120, 24).0.contains(LONG));
    click(&mut app, x + 4, y);
    assert!(!render(&mut app, 120, 24).0.contains(LONG), "a click hides it");
}

#[test]
fn modals_hide_the_tooltips_behind_them() {
    let mut app = long_app();
    let (s, _) = render(&mut app, 120, 24);
    let (x, y) = find(&s, "pierre-onboarding-flow-wi");
    key(&mut app, KeyCode::Char('?'));
    moved(&mut app, x + 3, y);
    app.now_ms += crate::tooltip::DELAY_MS;
    assert!(!render(&mut app, 120, 24).0.contains(LONG));
}

fn worktree(name: &str, branch: Option<&str>) -> clove_core::OtherWorktree {
    clove_core::OtherWorktree {
        name: name.into(),
        path: PathBuf::from(format!("/work/{name}")),
        branch: branch.map(String::from),
        head: Some("1a2b3c4d5e6f".into()),
        locked: false,
        lock_reason: None,
        prunable: None,
        missing: false,
        dirty: Some(false),
        changes: vec![],
        changes_total: 0,
        can_force: false,
        adopt_blocker: None,
        detached_commits: branch.is_none().then_some(0),
        pr: None,
        blocker: None,
    }
}

/// The app with three plain worktrees after the five clones: a merged one,
/// a locked dirty one, and a detached one.
fn app_with_worktrees() -> App {
    let mut app = app();
    let mut merged = worktree("app-docs", Some("pierre/docs"));
    merged.pr = Some(clove_core::Pr {
        number: 41,
        state: clove_core::PrState::Merged,
        head_oid: "1a2b3c".into(),
        title: String::new(),
        url: String::new(),
    });
    let mut locked = worktree("app-agent", Some("agent/task"));
    locked.locked = true;
    locked.lock_reason = Some("agent busy".into());
    locked.dirty = Some(true);
    locked.blocker = Some("app-agent is locked (agent busy)".into());
    let det = worktree("app-use", None);
    let root = PathBuf::from("/work/app");
    let mut d = app.data.get(&root).unwrap().clone();
    d.worktrees = vec![merged, locked, det];
    app.apply_data(d);
    app
}

#[test]
fn worktrees_list_under_the_clones() {
    let mut app = app_with_worktrees();
    let (s, _) = render(&mut app, 140, 30);
    insta::assert_snapshot!(s);
    // Selection runs from the last clone into the worktrees.
    key(&mut app, KeyCode::End);
    assert_eq!(app.selected_worktree().unwrap().name, "app-use");
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Up);
    assert_eq!(app.selected_worktree().unwrap().name, "app-docs");
    key(&mut app, KeyCode::Up);
    assert_eq!(app.selected_clone().unwrap().name, "detached");
    // The filter covers worktrees too.
    key(&mut app, KeyCode::Char('/'));
    typed(&mut app, "agent");
    key(&mut app, KeyCode::Enter);
    assert!(app.visible().is_empty());
    assert_eq!(app.selected_worktree().unwrap().name, "app-agent");
}

#[test]
fn worktree_detail_and_mouse() {
    let mut app = app_with_worktrees();
    let (s, _) = render(&mut app, 140, 30);
    let (x, y) = find(&s, "app-docs");
    click(&mut app, x, y);
    assert_eq!(app.selected_worktree().unwrap().name, "app-docs");
    let (s, _) = render(&mut app, 140, 30);
    assert!(s.contains("app-docs · git worktree"), "{s}");
    assert!(s.contains("#41 merged"), "{s}");
    // Double click cds into it, like a clone.
    app.now_ms += 150;
    click(&mut app, x, y);
    assert_eq!(app.exit, Some(Exit::Cd("/work/app-docs".into())));
}

#[test]
fn removing_a_worktree_never_forces() {
    let mut app = app_with_worktrees();
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Char('d'));
    let Some(Modal::Remove(f)) = &app.modal else {
        panic!("no remove dialog");
    };
    assert!(f.worktree);
    assert!(!f.items().contains(&RemoveItem::Force));
    let (s, _) = render(&mut app, 120, 28);
    assert!(s.contains("git worktree remove"), "{s}");
    // Up from the remove button reaches "also delete branch".
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Char(' '));
    key(&mut app, KeyCode::Down);
    let fx = key(&mut app, KeyCode::Enter);
    assert!(matches!(&fx[..], [Effect::Spawn { .. }]));
    match &app.jobs.last().unwrap().task {
        Task::RemoveWorktree {
            path, delete_branch, ..
        } => {
            assert_eq!(path, &PathBuf::from("/work/app-docs"));
            assert!(*delete_branch);
        }
        t => panic!("{t:?}"),
    }
    // A locked worktree: the dialog says why, and Enter does nothing.
    key(&mut app, KeyCode::Down);
    assert_eq!(app.selected_worktree().unwrap().name, "app-agent");
    key(&mut app, KeyCode::Char('d'));
    let (s, _) = render(&mut app, 120, 28);
    assert!(s.contains("locked: agent busy"), "{s}");
    assert!(s.contains("[ ] unlock it"), "{s}");
    let jobs = app.jobs.len();
    assert!(key(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(app.jobs.len(), jobs);
    assert!(matches!(app.modal, Some(Modal::Remove(_))));
}

#[test]
fn make_a_worktree_a_clone() {
    let mut app = app_with_worktrees();
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Char('c'));
    let (s, _) = render(&mut app, 120, 28);
    insta::assert_snapshot!(s);
    let fx = key(&mut app, KeyCode::Enter);
    assert!(matches!(&fx[..], [Effect::Spawn { .. }]));
    let job = app.jobs.last().unwrap();
    assert!(matches!(&job.task, Task::Adopt { path, .. } if path == &PathBuf::from("/work/app-docs")));
    // It streams setup steps like a new clone.
    assert!(matches!(app.modal, Some(Modal::Job(_))));

    // A detached worktree has no branch for a clone.
    app.modal = None;
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Char('c'));
    let Some(Modal::Adopt(a)) = &app.modal else {
        panic!("no adopt dialog");
    };
    assert!(a.blocker.as_deref().is_some_and(|b| b.contains("detached")));
    let n = app.jobs.len();
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.jobs.len(), n);
}

#[test]
fn cleanup_lists_merged_worktrees() {
    use clove_core::{Verdict, WorktreeCheck};
    let wc = |name: &str, verdict, reason: &str| WorktreeCheck {
        name: name.into(),
        path: PathBuf::from(format!("/work/{name}")),
        branch: Some(name.into()),
        head: None,
        pr: None,
        verdict,
        reason: reason.into(),
    };
    let mut app = app_with_worktrees();
    let result = Cleanup {
        worktrees: vec![
            wc("app-docs", Verdict::Prune, "PR #41 merged"),
            wc(
                "app-agent",
                Verdict::Review,
                "PR #40 merged, but locked (agent busy)",
            ),
            wc("app-use", Verdict::Keep, "detached, no PR"),
        ],
        ..Default::default()
    };
    assert!(crate::app::cleanup_has_work(&result));
    app.modal = Some(Modal::Gc(GcView {
        result: Some(result),
        ..Default::default()
    }));
    let (s, _) = render(&mut app, 120, 34);
    assert!(s.contains("MERGED GIT WORKTREES"), "{s}");
    assert!(s.contains("✓ app-docs"), "{s}");
    assert!(s.contains("! app-agent"), "{s}");
    assert!(s.contains("1 other worktree kept"), "{s}");
}

// ---- resizable panels ----------------------------------------------------

fn mouse(app: &mut App, kind: MouseEventKind, x: u16, y: u16) -> Vec<Effect> {
    app.on_input(TermEvent::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }))
}

fn drag(app: &mut App, from: (u16, u16), to: (u16, u16)) -> Vec<Effect> {
    mouse(app, MouseEventKind::Down(MouseButton::Left), from.0, from.1);
    mouse(app, MouseEventKind::Drag(MouseButton::Left), to.0, to.1);
    mouse(app, MouseEventKind::Up(MouseButton::Left), to.0, to.1)
}

/// x of the first divider of `id` owned by `pane` in the last frame.
fn divider_x(app: &App, id: SplitId, pane: usize) -> (u16, u16) {
    let r = app.panes.get(id).divider(pane).expect("divider drawn");
    (r.x, r.y + 2)
}

#[test]
fn drag_resizes_panels_and_clamps() {
    let mut app = app();
    render(&mut app, 160, 30);
    assert_eq!(app.panes.body.laid_size(panes::SIDEBAR), 22);
    assert_eq!(app.panes.body.laid_size(panes::DETAIL), 48);

    let (x, y) = divider_x(&app, SplitId::Body, panes::SIDEBAR);
    let fx = drag(&mut app, (x, y), (x + 8, y));
    assert_eq!(fx, vec![Effect::SaveLayout]);
    render(&mut app, 160, 30);
    assert_eq!(app.panes.body.laid_size(panes::SIDEBAR), 30);
    assert!(app.dragging.is_none());

    // Way past either end: held at the minimum and at 40% of the width.
    let (x, y) = divider_x(&app, SplitId::Body, panes::SIDEBAR);
    drag(&mut app, (x, y), (0, y));
    render(&mut app, 160, 30);
    assert_eq!(app.panes.body.laid_size(panes::SIDEBAR), 16);
    let (x, y) = divider_x(&app, SplitId::Body, panes::SIDEBAR);
    app.now_ms += 1000;
    drag(&mut app, (x, y), (159, y));
    render(&mut app, 160, 30);
    assert_eq!(app.panes.body.laid_size(panes::SIDEBAR), 64);

    // The detail grows leftwards until the list hits its minimum.
    let (x, y) = divider_x(&app, SplitId::Body, panes::DETAIL);
    drag(&mut app, (x, y), (0, y));
    let (s, _) = render(&mut app, 160, 30);
    let body = &app.panes.body;
    assert_eq!(body.laid_size(panes::DETAIL), 160 - 64 - 30);
    assert!(s.contains("clones"), "{s}");
}

#[test]
fn double_click_on_a_divider_resets_it() {
    let mut app = app();
    render(&mut app, 160, 30);
    let (x, y) = divider_x(&app, SplitId::Body, panes::DETAIL);
    drag(&mut app, (x, y), (x - 10, y));
    render(&mut app, 160, 30);
    assert_eq!(app.panes.body.laid_size(panes::DETAIL), 58);
    let (x, y) = divider_x(&app, SplitId::Body, panes::DETAIL);
    app.now_ms += 1000;
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), x, y);
    app.now_ms += 100;
    let fx = mouse(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
    assert_eq!(fx, vec![Effect::SaveLayout]);
    render(&mut app, 160, 30);
    assert_eq!(app.panes.body.laid_size(panes::DETAIL), 48);
    assert_eq!(app.panes.body.sizes()[panes::DETAIL], None);
}

#[test]
fn keys_resize_panels() {
    let mut app = app();
    render(&mut app, 160, 30);
    assert_eq!(key(&mut app, KeyCode::Char('<')), vec![Effect::SaveLayout]);
    key(&mut app, KeyCode::Char('<'));
    key(&mut app, KeyCode::Char('}'));
    render(&mut app, 160, 30);
    assert_eq!(app.panes.body.laid_size(panes::DETAIL), 52);
    assert_eq!(app.panes.body.laid_size(panes::SIDEBAR), 24);
    for _ in 0..100 {
        key(&mut app, KeyCode::Char('{'));
    }
    render(&mut app, 160, 30);
    assert_eq!(
        app.panes.body.laid_size(panes::SIDEBAR),
        16,
        "clamped at the minimum"
    );
    assert_eq!(
        key(&mut app, KeyCode::Char('{')),
        vec![],
        "nothing to save at a bound"
    );
    key(&mut app, KeyCode::Char('='));
    render(&mut app, 160, 30);
    assert_eq!(app.panes.body.laid_size(panes::SIDEBAR), 22);
    assert_eq!(app.panes.body.laid_size(panes::DETAIL), 48);

    // Stacked: < and > move the border between the list and the detail.
    render(&mut app, 96, 32);
    key(&mut app, KeyCode::Char('<'));
    render(&mut app, 96, 32);
    assert_eq!(app.panes.stack.laid_size(panes::STACK_DETAIL), 13);

    // The progress modal's steps column.
    let id = refresh_job(&mut app);
    app.modal = Some(Modal::Job(id));
    render(&mut app, 160, 40);
    let before = app.panes.job.laid_size(panes::JOB_STEPS);
    key(&mut app, KeyCode::Char('>'));
    render(&mut app, 160, 40);
    assert_eq!(app.panes.job.laid_size(panes::JOB_STEPS), before + 2);
    assert!(matches!(app.modal, Some(Modal::Job(j)) if j == id));
}

#[test]
fn terminal_resize_keeps_panes_in_bounds() {
    let mut app = app();
    app.panes.body.set_sizes(&[Some(60), None, Some(90)]);
    for w in [60, 99, 100, 131, 132, 140, 200, 300] {
        for h in [14, 24, 26, 40] {
            let (s, _) = render(&mut app, w, h);
            let area = ratatui::layout::Rect::new(0, 0, w, h);
            for (_, r) in app.panes.body.dividers() {
                assert!(area.contains(r.as_position()), "{w}x{h} {r:?}");
                assert!(r.right() <= w, "{w}x{h} {r:?}");
            }
            let list = app.panes.body.laid_size(1);
            assert!(list >= 30 || w < 100, "{w}x{h}: list {list}\n{s}");
        }
    }
    // Growing back restores the chosen sizes.
    render(&mut app, 300, 40);
    assert_eq!(app.panes.body.laid_size(panes::SIDEBAR), 60);
    assert_eq!(app.panes.body.laid_size(panes::DETAIL), 90);
}

#[test]
fn hovering_or_dragging_a_divider_shows_resize_feedback() {
    let mut app = app();
    render(&mut app, 160, 30);
    let (x, y) = divider_x(&app, SplitId::Body, panes::DETAIL);
    mouse(&mut app, MouseEventKind::Moved, x, y);
    let (_, buf) = render(&mut app, 160, 30);
    assert_eq!(app.pointer, Pointer::ColResize);
    assert_eq!(buf[(x, y)].fg, theme::BOTTLE);
    assert_eq!(buf[(x + 1, y)].fg, theme::BOTTLE);

    // Dragging keeps the feedback even when the mouse outruns the border.
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), x - 4, y);
    let (_, buf) = render(&mut app, 160, 30);
    let r = app.panes.body.divider(panes::DETAIL).unwrap();
    assert_eq!(r.x, x - 4);
    assert_eq!(buf[(r.x, y)].fg, theme::BOTTLE);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), x - 4, 0);
    render(&mut app, 160, 30);
    assert_eq!(app.pointer, Pointer::ColResize);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), x - 4, 0);
    render(&mut app, 160, 30);
    assert_ne!(app.pointer, Pointer::ColResize);

    // The stacked layout's border is a row.
    render(&mut app, 96, 32);
    let r = app.panes.stack.divider(panes::STACK_DETAIL).unwrap();
    mouse(&mut app, MouseEventKind::Moved, r.x + 5, r.y);
    render(&mut app, 96, 32);
    assert_eq!(app.pointer, Pointer::RowResize);
}

#[test]
fn panel_sizes_persist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/tui.json");
    // Missing file: defaults.
    let p = Panes::load(&path);
    assert_eq!(p.body.sizes(), [None, None, None]);

    let mut p = Panes::default();
    p.body.set_sizes(&[Some(30), None, Some(60)]);
    p.stack.set_sizes(&[None, Some(15)]);
    p.save(&path).unwrap();
    let back = Panes::load(&path);
    assert_eq!(back.body.sizes(), [Some(30), None, Some(60)]);
    assert_eq!(back.stack.sizes(), [None, Some(15)]);
    assert_eq!(back.job.sizes(), [None, None]);

    // Corrupt file, or a bad entry: defaults where it doesn't fit.
    std::fs::write(&path, "{not json").unwrap();
    assert_eq!(Panes::load(&path).body.sizes(), [None, None, None]);
    std::fs::write(&path, r#"{"splits":{"body":[1,2],"stack":[null,9],"x":[]}}"#).unwrap();
    let p = Panes::load(&path);
    assert_eq!(p.body.sizes(), [None, None, None]);
    assert_eq!(p.stack.sizes(), [None, Some(9)]);
}

#[test]
fn hovering_a_panel_lights_its_border() {
    let mut app = app_with_worktrees();
    let (s, _) = render(&mut app, 140, 30);
    let (x, y) = find(&s, "app-agent");
    moved(&mut app, x, y);
    let (s, buf) = render(&mut app, 140, 30);
    // The worktrees panel's title row: its corner turns SAGE, the clones panel's stays LINE.
    let (tx, ty) = find(&s, "git worktrees 3");
    assert_eq!(buf[(tx - 2, ty)].symbol(), "╭");
    assert_eq!(buf[(tx - 2, ty)].fg, theme::SAGE);
    let (cx, cy) = find(&s, "clones 5");
    assert_eq!(buf[(cx - 2, cy)].fg, theme::LINE);
    // The title text keeps its own colour.
    assert_ne!(buf[(tx, ty)].fg, theme::SAGE);
    // Nothing lit with the mouse elsewhere or a modal open.
    key(&mut app, KeyCode::Char('?'));
    let (_, buf) = render(&mut app, 140, 30);
    assert_ne!(buf[(tx - 2, ty)].fg, theme::SAGE);
}

#[test]
fn the_worktrees_panel_resizes_and_falls_back_inline_when_short() {
    let mut app = app_with_worktrees();
    let (s, _) = render(&mut app, 140, 30);
    let (_, top) = find(&s, "git worktrees 3");
    // Drag the border between the clones and the worktrees up by three rows.
    let (dx, _) = find(&s, "git worktrees 3");
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), dx, top);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), dx, top - 3);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), dx, top - 3);
    let (s, _) = render(&mut app, 140, 30);
    assert_eq!(find(&s, "git worktrees 3").1, top - 3, "{s}");
    // Too short for two panels: one list with a heading, as before.
    let (s, _) = render(&mut app, 140, 18);
    assert!(!s.contains("git worktrees 3"), "{s}");
    assert!(s.contains("OTHER GIT WORKTREES"), "{s}");
}

#[test]
fn any_cut_text_gets_its_full_text_as_a_tooltip() {
    const WHY: &str = "vmc-runner deploy checkout, held while railway link runs against the staging cluster";
    let mut app = app_with_worktrees();
    let root = PathBuf::from("/work/app");
    let mut d = app.data.get(&root).unwrap().clone();
    d.worktrees[1].lock_reason = Some(WHY.into());
    app.apply_data(d);
    // Select the locked worktree: its detail pane shows the reason, cut.
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Up);
    let (s, _) = render(&mut app, 140, 30);
    assert!(!s.contains(WHY), "{s}");
    let (x, y) = find(&s, "vmc-runner deploy");
    moved(&mut app, x + 2, y);
    app.now_ms += crate::tooltip::DELAY_MS;
    let (s, _) = render(&mut app, 140, 30);
    assert!(s.contains("held while railway"), "{s}");
    assert!(s.contains("staging cluster"), "{s}");
}

#[test]
fn a_dirty_worktree_lists_its_changes_and_can_be_discarded() {
    let mut app = app();
    let mut w = worktree("audit-cache", None);
    w.detached_commits = Some(0);
    w.dirty = Some(true);
    w.changes = vec!["?? node_modules".into()];
    w.changes_total = 1;
    w.blocker = w.blocker();
    let root = PathBuf::from("/work/app");
    let mut d = app.data.get(&root).unwrap().clone();
    d.worktrees = vec![w];
    app.apply_data(d);
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Char('d'));
    let (s, _) = render(&mut app, 120, 30);
    assert!(s.contains("1 uncommitted change:"), "{s}");
    assert!(s.contains("?? node_modules"), "{s}");
    assert!(s.contains("[ ] discard these changes"), "{s}");
    // Remove without ticking it: refused, with the way out in the toast.
    let fx = key(&mut app, KeyCode::Enter);
    assert!(fx.is_empty());
    assert!(matches!(app.modal, Some(Modal::Remove(_))));
    assert!(app.toasts.last().unwrap().text.contains("discard these changes"));
    // Tick it (Up from the button reaches the toggle), then remove.
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Char(' '));
    let (s, _) = render(&mut app, 120, 30);
    assert!(s.contains("[x] discard these changes"), "{s}");
    assert!(s.contains("discard & remove"), "{s}");
    key(&mut app, KeyCode::Down);
    let fx = key(&mut app, KeyCode::Enter);
    assert!(matches!(&fx[..], [Effect::Spawn { .. }]), "{fx:?}");
    match &app.jobs.last().unwrap().task {
        Task::RemoveWorktree { force, .. } => assert!(*force),
        t => panic!("{t:?}"),
    }
}

#[test]
fn a_locked_worktree_can_be_unlocked_and_removed() {
    let mut app = app();
    let mut w = worktree("vmc-deploy", None);
    w.locked = true;
    w.lock_reason = Some("vmc-runner deploy checkout".into());
    w.missing = true;
    w.dirty = None;
    w.detached_commits = None;
    w.blocker = w.removal_blocker(false, false);
    w.can_force = w.forceable();
    assert!(w.can_force, "a lock alone can be lifted");
    let root = PathBuf::from("/work/app");
    let mut d = app.data.get(&root).unwrap().clone();
    d.worktrees = vec![w];
    app.apply_data(d);
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Char('d'));
    let (s, _) = render(&mut app, 120, 30);
    assert!(s.contains("! locked: vmc-runner deploy checkout"), "{s}");
    assert!(s.contains("its folder is gone"), "{s}");
    assert!(s.contains("[ ] unlock it"), "{s}");
    assert!(!s.contains("discard these changes"), "nothing to discard: {s}");
    // Refused until unlock is ticked.
    assert!(key(&mut app, KeyCode::Enter).is_empty());
    key(&mut app, KeyCode::Up);
    key(&mut app, KeyCode::Char(' '));
    key(&mut app, KeyCode::Down);
    let fx = key(&mut app, KeyCode::Enter);
    assert!(matches!(&fx[..], [Effect::Spawn { .. }]), "{fx:?}");
    match &app.jobs.last().unwrap().task {
        Task::RemoveWorktree { unlock, force, .. } => assert!(*unlock && !*force),
        t => panic!("{t:?}"),
    }
}

#[test]
fn a_dirty_worktree_becomes_a_clone_with_its_changes() {
    let mut app = app();
    let mut w = worktree("surveyor", Some("pierre/wip/surveyor"));
    w.dirty = Some(true);
    w.changes = vec![" M src/a.rs".into(), "?? notes.md".into()];
    w.changes_total = 2;
    w.blocker = w.blocker();
    w.adopt_blocker = w.adopt_blocker();
    let root = PathBuf::from("/work/app");
    let mut d = app.data.get(&root).unwrap().clone();
    d.worktrees = vec![w];
    app.apply_data(d);
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Char('c'));
    let (s, _) = render(&mut app, 120, 30);
    assert!(s.contains("2 uncommitted changes move"), "{s}");
    let fx = key(&mut app, KeyCode::Enter);
    assert!(matches!(&fx[..], [Effect::Spawn { .. }]), "{fx:?}");
    assert!(matches!(app.jobs.last().unwrap().task, Task::Adopt { .. }));
}

#[test]
fn a_first_load_shows_the_list_before_git_status_lands() {
    let mut app = App::new(repos(), 0);
    let mut quick = clones();
    for c in &mut quick {
        c.dirty = None;
    }
    let n = quick.len();
    app.loading = true;
    app.on_msg(Msg::Preview(Box::new(data("/work/app", status(true, n), quick))));
    assert_eq!(app.current().map(|d| d.clones.len()), Some(n));
    assert!(app.loading, "the full load is still running");
    // The full answer replaces it; a late preview never overwrites full data.
    let full = clones();
    app.apply_data(data("/work/app", status(true, n), full.clone()));
    let mut stale = clones();
    stale[0].dirty = None;
    app.on_msg(Msg::Preview(Box::new(data("/work/app", status(true, n), stale))));
    assert_eq!(app.current().unwrap().clones[0].dirty, full[0].dirty);
}

#[test]
fn an_auto_prune_while_open_shows_a_toast_once() {
    use clove_core::history::{By, Kind, Removed};
    let mut app = App::new(repos(), 0);
    app.now_unix = NOW;
    let gone = |at: u64, by: By| Removed {
        at,
        kind: Kind::Clone,
        name: format!("feat-{at}"),
        path: "/x".into(),
        branch: Some("feat".into()),
        head: None,
        by,
        reason: Some("PR #4 merged".into()),
        branch_deleted: true,
    };
    // What happened before the TUI opened is in `clove log`, not a toast.
    let mut d = data("/work/app", status(true, 0), vec![]);
    d.history = vec![gone(NOW - 100, By::AutoPrune)];
    app.apply_data(d.clone());
    assert!(!app.toasts.iter().any(|t| t.text.contains("auto-pruned")));
    d.history.insert(0, gone(NOW - 5, By::Hand));
    d.history.insert(0, gone(NOW - 1, By::AutoPrune));
    app.apply_data(d.clone());
    let said: Vec<&str> = app
        .toasts
        .iter()
        .map(|t| t.text.as_str())
        .filter(|t| t.contains("feat-"))
        .collect();
    assert_eq!(
        said,
        [format!("auto-pruned feat-{} just now: PR #4 merged", NOW - 1)]
    );
    app.toasts.clear();
    app.apply_data(d);
    assert!(app.toasts.is_empty(), "not again on the next load");
}

#[test]
fn the_agents_row_installs_and_removes() {
    use clove_core::agents::State;
    let mut app = app();
    app.service = Some(true);
    app.agents = Some(State::Missing);
    key(&mut app, KeyCode::Char('s'));
    let (s, _) = render(&mut app, 120, 60);
    assert!(s.contains("coding agents"), "{s}");
    let fx = key(&mut app, KeyCode::Char('A'));
    assert!(matches!(&fx[..], [Effect::Spawn { .. }]), "{fx:?}");
    assert!(matches!(
        app.jobs.last().unwrap().task,
        Task::Agents { install: true }
    ));
    let mut app = self::app();
    app.agents = None;
    key(&mut app, KeyCode::Char('s'));
    let (s, _) = render(&mut app, 120, 60);
    assert!(!s.contains("coding agents"), "no agent on this machine, no row");
}

#[test]
fn l_lists_what_clove_removed_and_how_to_get_it_back() {
    use clove_core::history::{By, Kind, Removed};
    let mut app = app_with(true, vec![]);
    key(&mut app, KeyCode::Char('L'));
    let (s, _) = render(&mut app, 120, 40);
    assert!(s.contains("nothing removed yet"), "{s}");
    key(&mut app, KeyCode::Esc);
    let mut d = data("/work/app", status(true, 0), vec![]);
    d.history = vec![Removed {
        at: NOW - 7200,
        kind: Kind::Clone,
        name: "feat-login".into(),
        path: "/x".into(),
        branch: Some("feat/login".into()),
        head: Some("0123456789abcdef".into()),
        by: By::AutoPrune,
        reason: Some("PR #42 merged".into()),
        branch_deleted: true,
    }];
    app.apply_data(d);
    key(&mut app, KeyCode::Char('L'));
    let (s, _) = render(&mut app, 120, 40);
    for want in [
        "2h ago",
        "auto-pruned",
        "feat-login",
        "PR #42 merged",
        "git branch feat/login 0123456789ab",
    ] {
        assert!(s.contains(want), "{want}:\n{s}");
    }
    key(&mut app, KeyCode::Char('L'));
    assert!(app.modal.is_none(), "L closes it again");
}

#[test]
fn an_update_stopped_on_conflicts_offers_the_resolver() {
    use clove_core::update::{Strategy, Update};
    let conflict = || {
        let mut app = app();
        app.modal = None;
        for d in app.data.values_mut() {
            d.config.update.resolver = Some("claude".into());
        }
        let name = app.selected_clone().unwrap().name.clone();
        let task = Task::Update {
            name: name.clone(),
            step: UpdateStep::Run,
        };
        let id = match app.spawn(task, After::Nothing).first() {
            Some(Effect::Spawn { id, .. }) => *id,
            other => panic!("{other:?}"),
        };
        app.modal = None;
        app.on_msg(Msg::Done(
            id,
            Ok(Outcome::Updated(Some(Update::Conflict {
                base: "origin/main".into(),
                strategy: Strategy::Rebase,
                files: vec!["src/table.go".into()],
            }))),
        ));
        (app, name)
    };
    let (mut app, name) = conflict();
    let (s, _) = render(&mut app, 120, 40);
    assert!(
        s.contains("Resolve with Claude") || s.contains("resolve with Claude"),
        "{s}"
    );
    assert!(s.contains("src/table.go"), "{s}");
    let fx = key(&mut app, KeyCode::Enter);
    assert!(
        matches!(&fx[..], [Effect::Resolve { name: n, .. }] if *n == name),
        "Enter resolves: {fx:?}"
    );
    let (mut app, _) = conflict();
    let fx = key(&mut app, KeyCode::Esc);
    assert!(fx.is_empty() && app.modal.is_none(), "Esc leaves it stopped");
}

// ---- diff view ---------------------------------------------------------

const DIFF: &str = "\
diff --git a/apps/web/src/session.ts b/apps/web/src/session.ts
index 1111111..2222222 100644
--- a/apps/web/src/session.ts
+++ b/apps/web/src/session.ts
@@ -1,6 +1,7 @@ import
 import { cookies } from \"next/headers\";
-import { verify } from \"./jwt\";
+import { verify, refresh } from \"./jwt\";
 
-const MAX_AGE = 60 * 60 * 24;
+const MAX_AGE = 60 * 60 * 24 * 7;
+const REFRESH = 60 * 60;
 export const COOKIE = \"session\";
diff --git a/proto/orch/vm.proto b/proto/orch/vm.proto
index 3333333..4444444 100644
--- a/proto/orch/vm.proto
+++ b/proto/orch/vm.proto
@@ -10,3 +10,4 @@ message Vm {
   string id = 1;
+  string cron = 2;
 }
diff --git a/pnpm-lock.yaml b/pnpm-lock.yaml
index 5555555..6666666 100644
--- a/pnpm-lock.yaml
+++ b/pnpm-lock.yaml
@@ -1 +1 @@
-lockfileVersion: '6.0'
+lockfileVersion: '9.0'
";

/// Opens the diff view on `fix-race` with a fixed patch, highlighting done.
fn diff_app() -> App {
    let mut app = app();
    key(&mut app, KeyCode::Down);
    let effects = key(&mut app, KeyCode::Char('v'));
    let Some(Effect::LoadDiff { root, name }) = effects.first().cloned() else {
        panic!("v asks for the diff: {effects:?}");
    };
    let diff = clove_core::CloneDiff {
        base: "origin/main".into(),
        merge_base: "abc".into(),
        patch: DIFF.into(),
        truncated: false,
        skipped: vec![],
        attrs: vec![],
        packages: vec![clove_core::diff::Package {
            dir: "apps/web".into(),
            name: "@acme/web".into(),
            manifest: "package.json".into(),
        }],
    };
    let data = std::sync::Arc::new(crate::diffview::DiffData::new(diff));
    let (tx, _rx) = std::sync::mpsc::channel();
    let hl = crate::diffview::highlight(data.clone(), tx);
    while !hl.finished() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    app.on_msg(Msg::Diff {
        root,
        name,
        result: Ok((data, hl)),
    });
    app
}

#[test]
fn diff_view_split() {
    let mut app = diff_app();
    let (s, _) = render(&mut app, 140, 30);
    insta::assert_snapshot!(s);
}

#[test]
fn diff_view_unified_grouped_with_a_lockfile_opened() {
    let mut app = diff_app();
    key(&mut app, KeyCode::Char('s'));
    key(&mut app, KeyCode::Char('g'));
    let (s, _) = render(&mut app, 120, 30);
    assert!(s.contains("▸ ◆ apps/web"), "{s}");
    assert!(s.contains("lockfile · z shows it"), "{s}");
    // The lockfile sits in the repository root's group, first: open the
    // group, step onto the file, unfold it.
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('z'));
    let (s, _) = render(&mut app, 120, 30);
    insta::assert_snapshot!(s);
    assert!(s.contains("lockfileVersion: '9.0'"), "{s}");
    assert!(key(&mut app, KeyCode::Esc).is_empty());
    assert!(app.modal.is_none());
}

/// Every scroll position, in every mode, with the wheel and the keys: the
/// view must never panic, whatever row (a gap between files, say) is on top.
#[test]
fn diff_view_survives_every_scroll_position() {
    use ratatui::crossterm::event::{MouseEvent, MouseEventKind};
    for keys in [&[][..], &['g'], &['s'], &['s', 'g', 'w'], &['o', 'g', 'f']] {
        let mut app = diff_app();
        for &c in keys {
            key(&mut app, KeyCode::Char(c));
        }
        let (_, buf) = render(&mut app, 100, 18);
        // Every row on top, gaps between files included.
        for top in 0..120 {
            if let Some(crate::model::Modal::Diff(v)) = &mut app.modal {
                v.top = top;
            }
            render(&mut app, 100, 18);
            key(&mut app, KeyCode::Char('j'));
            render(&mut app, 100, 18);
        }
        for _ in 0..60 {
            for (x, y) in [(60, 10), (5, 10), (60, 1), (60, 17)] {
                app.on_input(TermEvent::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollDown,
                    column: x,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                }));
            }
            render(&mut app, 100, 18);
        }
        for c in [
            KeyCode::End,
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char(' '),
            KeyCode::Char('z'),
        ] {
            key(&mut app, c);
            render(&mut app, 100, 18);
        }
        for _ in 0..400 {
            app.on_input(TermEvent::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 60,
                row: 10,
                modifiers: KeyModifiers::NONE,
            }));
            render(&mut app, 100, 18);
        }
        assert!(
            matches!(app.modal, Some(crate::model::Modal::Diff(_))),
            "still open after {keys:?}"
        );
        let _ = buf;
    }
}

/// Hovering a panel of the diff view lights up nothing but a file row: the
/// panels scroll, they are not buttons.
#[test]
fn diff_view_hover_marks_only_rows() {
    use ratatui::crossterm::event::{MouseEvent, MouseEventKind};
    let mut app = diff_app();
    render(&mut app, 120, 24);
    let mv = |app: &mut App, column, row| {
        app.on_input(TermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    };
    // Over diff text: nothing turns green.
    mv(&mut app, 80, 12);
    let (s, buf) = render(&mut app, 120, 24);
    let (x, y) = find(&s, "export const COOKIE");
    assert_ne!(buf[(x, y)].fg, theme::SAGE, "{s}");
    // Over a file row: that row is highlighted, the next one is not.
    let (x, y) = find(&s, "vm.proto");
    mv(&mut app, x, y);
    let (s, buf) = render(&mut app, 120, 24);
    assert_eq!(buf[(x, y)].fg, theme::SAGE, "{s}");
    let (x2, y2) = find(&s, "pnpm-lock.yaml");
    assert_ne!(buf[(x2, y2)].fg, theme::SAGE, "{s}");
}

/// The file list and the diff are a split like the main screen's: } widens
/// the list, = puts it back, and the border can be dragged.
#[test]
fn diff_view_panels_resize() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let mut app = diff_app();
    let list_right = |s: &str| {
        let line = s.lines().find(|l| l.contains("╭ files")).unwrap();
        line.chars().position(|c| c == '╮').unwrap()
    };
    let (s, _) = render(&mut app, 120, 24);
    let start = list_right(&s);
    assert_eq!(key(&mut app, KeyCode::Char('}')), vec![Effect::SaveLayout]);
    let (s, _) = render(&mut app, 120, 24);
    assert_eq!(list_right(&s), start + 2);
    key(&mut app, KeyCode::Char('='));
    let (s, _) = render(&mut app, 120, 24);
    assert_eq!(list_right(&s), start);
    // Drag the border 6 columns to the right.
    let y = 10;
    let ev = |kind, column| {
        TermEvent::Mouse(MouseEvent {
            kind,
            column,
            row: y,
            modifiers: KeyModifiers::NONE,
        })
    };
    let x = start as u16;
    app.on_input(ev(MouseEventKind::Down(MouseButton::Left), x));
    app.on_input(ev(MouseEventKind::Drag(MouseButton::Left), x + 6));
    let saved = app.on_input(ev(MouseEventKind::Up(MouseButton::Left), x + 6));
    assert_eq!(saved, vec![Effect::SaveLayout]);
    let (s, _) = render(&mut app, 120, 24);
    assert_eq!(list_right(&s), start + 6, "{s}");
    assert!(matches!(app.modal, Some(crate::model::Modal::Diff(_))));
}
