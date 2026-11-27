//! Updates. The check and its hourly cache live in clove-core, shared with
//! the CLI and TUI; the upgrade itself is `clove upgrade --relaunch-app`,
//! started detached because it replaces this app, which quits first.

use crate::commands::CmdResult;
use crate::ops::block;
use clove_core::upgrade::{self, Source, Status};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::AppHandle;

#[derive(Serialize)]
pub struct UpgradeState {
    #[serde(flatten)]
    pub status: Status,
    /// `clove --version` of the CLI on PATH; None when there is none.
    pub cli_version: Option<String>,
    /// How this app was installed: only the cask can upgrade in place.
    pub app_source: Source,
    /// A newer app sits on disk than the one running (a manual brew upgrade).
    pub restart_to: Option<String>,
}

fn state(status: Status) -> UpgradeState {
    UpgradeState {
        cli_version: upgrade::cli_version(),
        app_source: if upgrade::app_from_cask() {
            Source::Brew
        } else {
            Source::Manual
        },
        restart_to: bundle_version().filter(|v| upgrade::newer(v, &status.current)),
        status,
    }
}

/// The version in the running bundle's Info.plist, which `brew upgrade`
/// rewrites under a running app.
fn bundle_version() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    // clove.app/Contents/MacOS/clove
    let plist = exe.parent()?.parent()?.join("Info.plist");
    plist_version(&std::fs::read_to_string(plist).ok()?)
}

fn plist_version(xml: &str) -> Option<String> {
    let after = xml.split("<key>CFBundleShortVersionString</key>").nth(1)?;
    let v = after.split("<string>").nth(1)?.split("</string>").next()?;
    Some(v.trim().to_string())
}

/// The last known state, without the network.
#[tauri::command]
pub async fn upgrade_status() -> CmdResult<UpgradeState> {
    block(|| Ok(state(upgrade::status()))).await
}

/// Asks GitHub: at once when `force`, else only when the shared cache is an
/// hour old.
#[tauri::command]
pub async fn check_upgrade(force: bool) -> CmdResult<UpgradeState> {
    block(move || Ok(state(upgrade::check(force)))).await
}

#[tauri::command]
pub async fn clear_upgrade_error() -> CmdResult<()> {
    block(upgrade::clear_upgrade_error).await
}

/// Hands the upgrade to the CLI, detached, and quits: the CLI waits for this
/// process to exit, upgrades formula and cask, then opens the app again.
/// Output goes to `~/.clove/logs/upgrade.log`; a failure lands in the cache
/// for the relaunched app to show.
#[tauri::command]
pub async fn start_upgrade(app: AppHandle) -> CmdResult<()> {
    block(move || {
        let cli = clove_core::service::cli_exe()?;
        let log = clove_core::state_home().join("logs/upgrade.log");
        spawn_detached(&cli, &log)?;
        // Let the command answer before the webview goes away.
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            app.exit(0);
        });
        Ok(())
    })
    .await
}

fn spawn_detached(cli: &Path, log: &PathBuf) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let out = std::fs::File::create(log)?;
    Command::new(cli)
        .args(["upgrade", "--relaunch-app", &std::process::id().to_string()])
        .env("CLOVE_NO_UPDATE_CHECK", "1")
        .stdin(Stdio::null())
        .stderr(out.try_clone()?)
        .stdout(out)
        .process_group(0)
        .spawn()?;
    Ok(())
}

/// Starts this app again: after a brew upgrade under it, the new version.
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

/// Opens a release page in the browser. Only GitHub pages of clove.
#[tauri::command]
pub async fn open_release(url: String) -> CmdResult<()> {
    block(move || {
        let ours = format!("https://github.com/{}/", upgrade::REPO);
        if !url.starts_with(&ours) {
            anyhow::bail!("not a clove release page: {url}");
        }
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        clove_core::util::output(Command::new(opener).arg(&url))?;
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_bundle_version() {
        let xml = "<dict>\n  <key>CFBundleName</key>\n  <string>clove</string>\n  \
                   <key>CFBundleShortVersionString</key>\n  <string>0.2.1</string>\n</dict>";
        assert_eq!(super::plist_version(xml).as_deref(), Some("0.2.1"));
        assert_eq!(super::plist_version("<dict></dict>"), None);
    }
}
