//! launchd agents: remount clones at login, refresh golden images nightly.

use anyhow::{Context, Result};
use clove_core::util::home;
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
    let log = clove_core::state_home().join("logs").join(format!("{label}.log"));
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
  </dict>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
  <key>ProcessType</key><string>Background</string>
  <key>LowPriorityIO</key><true/>
</dict>
</plist>
"#,
        path = xml_escape(&path),
        home = xml_escape(&home().to_string_lossy()),
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

pub fn install(hour: u8, minute: u8) -> Result<Vec<PathBuf>> {
    let mount = plist(MOUNT, &["mount", "--all-repos"], "  <key>RunAtLoad</key><true/>")?;
    let refresh = plist(
        REFRESH,
        &["refresh", "--all-repos"],
        &format!(
            "  <key>StartCalendarInterval</key>\n  <dict><key>Hour</key><integer>{hour}</integer><key>Minute</key><integer>{minute}</integer></dict>"
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
