//! macOS: two launchd agents.

use super::{DEFAULT_EVERY, watch_paths};
use crate::util::home;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

const MOUNT: &str = "dev.clove.mount";
const REFRESH: &str = "dev.clove.refresh";

fn plist_path(label: &str) -> PathBuf {
    home().join("Library/LaunchAgents").join(format!("{label}.plist"))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn plist(label: &str, args: &[&str], trigger: &str) -> Result<String> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let log = crate::state_home().join("logs").join(format!("{label}.log"));
    std::fs::create_dir_all(log.parent().context("log dir")?)?;
    // Steps need the user's tools (nix, direnv, pnpm); launchd starts with a bare PATH.
    let path = std::env::var("PATH").unwrap_or_default();
    let args: String = std::iter::once(exe.to_string_lossy().to_string())
        .chain(args.iter().map(|s| s.to_string()))
        .map(|a| format!("    <string>{}</string>\n", xml_escape(&a)))
        .collect();
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key>
  <array>
{args}  </array>
{trigger}
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key><string>{path}</string>
    <key>HOME</key><string>{home}</string>
{clove_home}  </dict>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
  <key>ProcessType</key><string>Background</string>
  <key>LowPriorityIO</key><true/>
</dict>
</plist>
"#,
        path = xml_escape(&path),
        home = xml_escape(&home().to_string_lossy()),
        // A custom state folder has to reach the agent, or it would read ~/.clove.
        clove_home = std::env::var("CLOVE_HOME")
            .map(|h| format!("    <key>CLOVE_HOME</key><string>{}</string>\n", xml_escape(&h)))
            .unwrap_or_default(),
        log = xml_escape(&log.to_string_lossy()),
    ))
}

fn launchctl(args: &[&str]) {
    let _ = Command::new("launchctl").args(args).output();
}

fn uid() -> String {
    // SAFETY: getuid has no preconditions.
    unsafe { libc::getuid() }.to_string()
}

/// Installs both agents. The refresh agent runs `refresh --all-repos --if-stale`
/// when any repo's FETCH_HEAD changes and every `every` seconds; fresh repos
/// cost one fetch at most (none within `fetch_interval` of the last one).
pub fn install(every: u64) -> Result<Vec<PathBuf>> {
    let mount = plist(MOUNT, &["mount", "--all-repos"], "  <key>RunAtLoad</key><true/>")?;
    let watch: String = watch_paths()
        .iter()
        .map(|p| format!("    <string>{}</string>\n", xml_escape(&p.to_string_lossy())))
        .collect();
    let refresh = plist(
        REFRESH,
        &["refresh", "--all-repos", "--if-stale", "--prs"],
        &format!(
            "  <key>StartInterval</key><integer>{every}</integer>\n  \
             <key>ThrottleInterval</key><integer>30</integer>\n  \
             <key>WatchPaths</key>\n  <array>\n{watch}  </array>"
        ),
    )?;
    let mut written = vec![];
    for (label, body) in [(MOUNT, mount), (REFRESH, refresh)] {
        let p = plist_path(label);
        std::fs::create_dir_all(p.parent().context("LaunchAgents")?)?;
        launchctl(&["bootout", &format!("gui/{}/{label}", uid())]);
        std::fs::write(&p, body)?;
        launchctl(&["bootstrap", &format!("gui/{}", uid()), &p.to_string_lossy()]);
        written.push(p);
    }
    Ok(written)
}

/// Rewrites an installed service so its watch list matches the known repos
/// (after `init` or `deinit`). Keeps the installed interval. No-op when the
/// service is not installed.
pub fn sync() -> Result<bool> {
    let Ok(body) = std::fs::read_to_string(plist_path(REFRESH)) else {
        return Ok(false);
    };
    let every = body
        .split("<key>StartInterval</key><integer>")
        .nth(1)
        .and_then(|s| s.split('<').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_EVERY);
    install(every)?;
    Ok(true)
}

pub fn uninstall() -> Result<Vec<PathBuf>> {
    let mut removed = vec![];
    for label in [MOUNT, REFRESH] {
        launchctl(&["bootout", &format!("gui/{}/{label}", uid())]);
        let p = plist_path(label);
        if p.exists() {
            std::fs::remove_file(&p)?;
            removed.push(p);
        }
    }
    Ok(removed)
}

pub fn installed() -> Vec<(String, bool)> {
    [MOUNT, REFRESH]
        .iter()
        .map(|l| (l.to_string(), plist_path(l).exists()))
        .collect()
}
