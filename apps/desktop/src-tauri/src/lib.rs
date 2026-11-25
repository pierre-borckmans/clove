pub mod commands;
pub mod ops;

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
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
    let _ = w.move_window(Position::TrayBottomCenter);
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
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let emitter = app.clone();
    let Ok(watcher) = clove_core::watch::Watcher::new(move || {
        let _ = emitter.emit("repos-changed", ());
        let _ = tx.send(());
    }) else {
        return;
    };
    std::thread::spawn(move || {
        loop {
            watcher.sync(clove_core::watch::all_targets());
            match rx.recv_timeout(Duration::from_secs(15)) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_positioner::init())
        .manage(PopoverState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::repos,
            commands::repo_status,
            commands::clones,
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
            commands::repo_config,
            commands::edit_config,
            commands::config_set,
            commands::service_status,
            commands::service_set,
            commands::prs,
            commands::check_prs,
            commands::branch_candidates,
            commands::cleanup,
            commands::shell_status,
            commands::shell_install,
            ops::start_op,
            open_main,
            hide_popover,
            quit,
        ])
        .setup(|app| {
            watch_repos(app.handle().clone());
            let open = MenuItem::with_id(app, "open", "Open clove", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit clove", true, Some("CmdOrCtrl+Q"))?;
            let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])?;
            TrayIconBuilder::with_id("clove")
                .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .tooltip("clove")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "open" => show_main(app),
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
            // The app lives in the menu bar; closing the window only hides it.
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("build tauri app");

    app.run(|app, event| {
        if let RunEvent::Reopen { .. } = event {
            show_main(app);
        }
    });
}
