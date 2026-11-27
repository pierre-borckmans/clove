pub mod apps;
pub mod commands;
pub mod ops;
pub mod upgrade;

use std::sync::Mutex;
use std::time::{Duration, Instant};
#[cfg(target_os = "macos")]
use tauri::RunEvent;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_positioner::{Position, WindowExt};

/// When the popover last hid itself on blur. A tray click that arrives right
/// after is the same click that blurred it, so it should not reopen it.
struct PopoverState(Mutex<Option<Instant>>);

fn toggle_popover(app: &AppHandle) {
    let Some(w) = app.get_webview_window("popover") else {
        return;
    };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    let recently_hidden = app
        .state::<PopoverState>()
        .0
        .lock()
        .ok()
        .and_then(|t| *t)
        .is_some_and(|t| t.elapsed() < Duration::from_millis(250));
    if recently_hidden {
        return;
    }
    // Linux trays report no clicks or position (appindicator): the popover
    // opens from the tray menu instead, in the middle of the screen.
    if cfg!(target_os = "linux") {
        let _ = w.center();
    } else {
        let _ = w.move_window(Position::TrayBottomCenter);
    }
    let _ = w.show();
    let _ = w.set_focus();
    let _ = w.emit("popover-shown", ());
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Brings the main window forward, optionally at a route (`/repo/<id>`) or
/// with an action (`new-clone`), and hides the popover.
#[tauri::command]
fn open_main(app: AppHandle, route: Option<String>, action: Option<String>) {
    if let Some(p) = app.get_webview_window("popover") {
        let _ = p.hide();
    }
    show_main(&app);
    if let Some(main) = app.get_webview_window("main") {
        if let Some(route) = route {
            let _ = main.emit("navigate", route);
        }
        if let Some(action) = action {
            let _ = main.emit("action", action);
        }
    }
}

/// "Check for Updates…" from a menu: the main window comes forward and runs
/// a forced check, then shows the update or says clove is up to date.
fn check_updates(app: &AppHandle) {
    show_main(app);
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.emit("check-updates", ());
    }
}

/// macOS's standard menu bar, with "Check for Updates…" under About.
#[cfg(target_os = "macos")]
fn app_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::default(app)?;
    if let Some(first) = menu.items()?.first()
        && let Some(sub) = first.as_submenu()
    {
        let item = MenuItem::with_id(app, "app-check-updates", "Check for Updates…", true, None::<&str>)?;
        sub.insert(&item, 1)?;
    }
    Ok(menu)
}

#[tauri::command]
fn hide_popover(app: AppHandle) {
    if let Some(p) = app.get_webview_window("popover") {
        let _ = p.hide();
    }
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// Emits `repos-changed` when a known repo changes on disk (clones added or
/// removed by any tool, golden refreshes, fetches, PR checks), so windows
/// refetch at once and otherwise poll slowly. Targets are resynced after each
/// change and every 15s, to pick up new repos and clones.
fn watch_repos(app: AppHandle) {
    let (tx, rx) = std::sync::mpsc::channel::<bool>();
    let emitter = app.clone();
    let Ok(watcher) = clove_core::watch::Watcher::new(move |c| {
        // Edits in a working tree only touch clone and worktree lists.
        let _ = emitter.emit("repos-changed", serde_json::json!({ "treesOnly": c.trees_only }));
        let _ = tx.send(c.trees_only);
    }) else {
        return;
    };
    std::thread::spawn(move || {
        let mut resync = true;
        loop {
            if resync {
                watcher.sync(clove_core::watch::all_targets());
            }
            match rx.recv_timeout(Duration::from_secs(15)) {
                // Edits in a tree add nothing to watch.
                Ok(trees_only) => resync = !trees_only,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => resync = true,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

/// Opened from Finder or a Linux app launcher, the app gets the session's bare
/// PATH, so steps can't find pnpm, direnv or nix, and the service would be
/// installed with that PATH. Take PATH from the login shell instead, as a
/// terminal would. Started from a terminal, it already has it.
fn adopt_login_path() {
    if std::env::var_os("TERM").is_some() {
        return;
    }
    let fallback = if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/sh"
    };
    let shell = std::env::var("SHELL").unwrap_or_else(|_| fallback.into());
    const MARK: &str = "__clove_path__";
    let Ok(mut child) = std::process::Command::new(&shell)
        .args(["-l", "-i", "-c", &format!("printf '\\n{MARK}%s' \"$PATH\"")])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return;
    };
    // An rc file that waits on input must not hold the app up.
    let start = Instant::now();
    while child.try_wait().ok().flatten().is_none() {
        if start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let Ok(out) = child.wait_with_output() else { return };
    let text = String::from_utf8_lossy(&out.stdout);
    if let Some(path) = text.rsplit_once(MARK).map(|(_, p)| p.trim())
        && path.contains("/usr/bin")
    {
        // SAFETY: called first thing in `run`, before any other thread exists.
        unsafe { std::env::set_var("PATH", path) };
    }
}

pub fn run() {
    adopt_login_path();
    // Last known clone statuses at once; git is asked in the background.
    clove_core::memo::serve_saved(true);
    let builder = tauri::Builder::default();
    // macOS keeps one instance itself; on Linux, launching the app again
    // brings the running one forward (its window may be hidden in the tray).
    #[cfg(target_os = "linux")]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_main(app)))
        .plugin(tauri_plugin_clipboard_manager::init());
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu).on_menu_event(|app, e| {
        if e.id().as_ref() == "app-check-updates" {
            check_updates(app);
        }
    });
    let app = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_positioner::init())
        .manage(PopoverState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::repos,
            commands::repo_list,
            commands::repo_status,
            commands::forget_cache,
            commands::clones,
            commands::worktrees,
            commands::refs,
            commands::doctor_machine,
            commands::doctor_repo,
            commands::gc,
            commands::mount_clone,
            commands::unmount_clone,
            commands::discover,
            commands::repo_candidates,
            commands::init_preview,
            commands::installed_apps,
            commands::open_in,
            commands::copy_text,
            commands::resolve_in_terminal,
            commands::repo_config,
            commands::edit_config,
            commands::config_set,
            commands::service_status,
            commands::service_set,
            commands::agents_status,
            commands::agents_set,
            commands::history,
            commands::stats,
            commands::prs,
            commands::check_prs,
            commands::branch_candidates,
            commands::cleanup,
            commands::clone_diff,
            commands::clone_file,
            commands::shell_status,
            commands::shell_install,
            ops::start_op,
            upgrade::upgrade_status,
            upgrade::check_upgrade,
            upgrade::clear_upgrade_error,
            upgrade::start_upgrade,
            upgrade::restart_app,
            upgrade::open_release,
            open_main,
            hide_popover,
            quit,
        ])
        .setup(|app| {
            watch_repos(app.handle().clone());
            let open = MenuItem::with_id(app, "open", "Open clove", true, None::<&str>)?;
            let quick = MenuItem::with_id(app, "quick", "Quick open…", true, None::<&str>)?;
            let updates = MenuItem::with_id(app, "check-updates", "Check for Updates…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit clove", true, Some("CmdOrCtrl+Q"))?;
            let sep = PredefinedMenuItem::separator(app)?;
            // A click on a Linux tray icon always opens its menu, so the
            // popover gets a menu item there.
            let menu = if cfg!(target_os = "linux") {
                Menu::with_items(app, &[&open, &quick, &updates, &sep, &quit])?
            } else {
                Menu::with_items(app, &[&open, &updates, &sep, &quit])?
            };
            // The macOS icon is a template the menu bar tints; Linux panels
            // draw icons as is, so they get the colored one.
            let icon = if cfg!(target_os = "macos") {
                Image::from_bytes(include_bytes!("../icons/tray.png"))?
            } else {
                Image::from_bytes(include_bytes!("../icons/64x64.png"))?
            };
            TrayIconBuilder::with_id("clove")
                .icon(icon)
                .icon_as_template(true)
                .tooltip("clove")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "open" => show_main(app),
                    "quick" => toggle_popover(app),
                    "check-updates" => check_updates(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_popover(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(false) if window.label() == "popover" => {
                if window.is_visible().unwrap_or(false) {
                    let _ = window.hide();
                    if let Ok(mut t) = window.app_handle().state::<PopoverState>().0.lock() {
                        *t = Some(Instant::now());
                    }
                }
            }
            // The app lives in the menu bar (the tray on Linux); closing the
            // window only hides it.
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("build tauri app");

    app.run(|_app, _event| {
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = _event {
            show_main(_app);
        }
    });
}
