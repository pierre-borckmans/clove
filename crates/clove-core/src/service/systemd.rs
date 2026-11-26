//! Linux: systemd user units.
//!   clove-mount.service    `clove mount --all-repos` at login (default.target)
//!   clove-refresh.service  `clove refresh --all-repos --if-stale --prs`, run by:
//!   clove-refresh.timer    every `every` seconds
//!   clove-refresh.path     PathChanged= on each known repo's FETCH_HEAD

use super::{DEFAULT_EVERY, watch_paths};
use crate::util::home;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const MOUNT: &str = "clove-mount.service";
const REFRESH: &str = "clove-refresh.service";
const TIMER: &str = "clove-refresh.timer";
const PATH: &str = "clove-refresh.path";

fn unit_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
        .join("systemd/user")
}

fn unit_path(name: &str) -> PathBuf {
    unit_dir().join(name)
}

fn systemctl(args: &[&str]) {
    let _ = Command::new("systemctl").arg("--user").args(args).output();
}

/// `%` starts a specifier in every unit setting.
fn esc(s: &str) -> String {
    s.replace('%', "%%")
}

/// A quoted word for Environment=.
fn quote(s: &str) -> String {
    format!("\"{}\"", esc(s).replace('\\', "\\\\").replace('"', "\\\""))
}

/// A quoted word for ExecStart=, which also expands `$VAR`.
fn quote_exec(s: &str) -> String {
    quote(s).replace('$', "$$")
}

/// What the units run with: the binary and the environment steps need.
struct Ctx {
    exe: PathBuf,
    log: PathBuf,
    env: Vec<(String, String)>,
}

impl Ctx {
    fn current() -> Result<Self> {
        let exe = super::cli_exe()?;
        let log = crate::state_home().join("logs");
        std::fs::create_dir_all(&log)?;
        // Steps need the user's tools (nix, direnv, pnpm); systemd starts with a bare PATH.
        let mut env = vec![
            ("PATH".into(), std::env::var("PATH").unwrap_or_default()),
            ("HOME".into(), home().to_string_lossy().into_owned()),
        ];
        // A custom state folder has to reach the units, or they would read ~/.clove.
        if let Ok(h) = std::env::var("CLOVE_HOME") {
            env.push(("CLOVE_HOME".into(), h));
        }
        Ok(Self { exe, log, env })
    }

    fn service(&self, description: &str, args: &[&str], log: &str, extra: &str) -> String {
        let exec: Vec<String> = std::iter::once(self.exe.to_string_lossy().into_owned())
            .chain(args.iter().map(|s| s.to_string()))
            .map(|a| quote_exec(&a))
            .collect();
        let env: String = self
            .env
            .iter()
            .map(|(k, v)| format!("Environment={}\n", quote(&format!("{k}={v}"))))
            .collect();
        let log = esc(&self.log.join(log).to_string_lossy());
        format!(
            "[Unit]\nDescription={description}\n\n[Service]\nType=oneshot\nExecStart={}\n{env}\
             StandardOutput=append:{log}\nStandardError=append:{log}\n{extra}",
            exec.join(" "),
        )
    }
}

fn mount_unit(ctx: &Ctx) -> String {
    ctx.service(
        "clove: mount clones at login",
        &["mount", "--all-repos"],
        "dev.clove.mount.log",
        "\n[Install]\nWantedBy=default.target\n",
    )
}

fn refresh_unit(ctx: &Ctx) -> String {
    ctx.service(
        "clove: refresh stale golden images",
        &["refresh", "--all-repos", "--if-stale", "--prs"],
        "dev.clove.refresh.log",
        "Nice=10\nIOSchedulingClass=idle\n",
    )
}

fn timer_unit(every: u64) -> String {
    format!(
        "[Unit]\nDescription=clove: check golden images every {}\n\n\
         [Timer]\nOnActiveSec={every}s\nOnUnitActiveSec={every}s\nUnit={REFRESH}\n\n\
         [Install]\nWantedBy=timers.target\n",
        crate::util::human_age(every)
    )
}

fn path_unit(watch: &[PathBuf]) -> String {
    let paths: String = watch
        .iter()
        .map(|p| format!("PathChanged={}\n", esc(&p.to_string_lossy())))
        .collect();
    format!(
        "[Unit]\nDescription=clove: refresh when a repo is fetched\n\n\
         [Path]\n{paths}Unit={REFRESH}\n\n[Install]\nWantedBy=paths.target\n"
    )
}

/// The timer's interval, from an installed timer unit.
fn parse_every(timer: &str) -> Option<u64> {
    timer
        .lines()
        .find_map(|l| l.strip_prefix("OnUnitActiveSec="))
        .and_then(|v| v.trim().trim_end_matches('s').parse().ok())
}

fn write(path: &Path, body: &str) -> Result<()> {
    std::fs::create_dir_all(path.parent().context("unit dir")?)?;
    std::fs::write(path, body).with_context(|| format!("write {}", path.display()))
}

/// Installs the units and starts them. The refresh service runs `refresh
/// --all-repos --if-stale` when any repo's FETCH_HEAD changes and every
/// `every` seconds; fresh repos cost one fetch at most.
pub fn install(every: u64) -> Result<Vec<PathBuf>> {
    let ctx = Ctx::current()?;
    let watch = watch_paths();
    let mut units = vec![
        (MOUNT, mount_unit(&ctx)),
        (REFRESH, refresh_unit(&ctx)),
        (TIMER, timer_unit(every)),
    ];
    // A path unit with no paths is invalid; drop it until a repo exists.
    if watch.is_empty() {
        systemctl(&["disable", "--now", PATH]);
        let _ = std::fs::remove_file(unit_path(PATH));
    } else {
        units.push((PATH, path_unit(&watch)));
    }
    let mut written = vec![];
    for (name, body) in &units {
        let p = unit_path(name);
        write(&p, body)?;
        written.push(p);
    }
    systemctl(&["daemon-reload"]);
    let mut enable = vec!["enable", "--now", "--no-block", MOUNT, TIMER];
    if !watch.is_empty() {
        enable.push(PATH);
    }
    systemctl(&enable);
    // Pick up a changed interval or watch list.
    let mut restart = vec!["restart", "--no-block", TIMER];
    if !watch.is_empty() {
        restart.push(PATH);
    }
    systemctl(&restart);
    Ok(written)
}

/// Rewrites installed units so the watch list matches the known repos (after
/// `init` or `deinit`). Keeps the installed interval. No-op when the service
/// is not installed.
pub fn sync() -> Result<bool> {
    let Ok(body) = std::fs::read_to_string(unit_path(TIMER)) else {
        return Ok(false);
    };
    install(parse_every(&body).unwrap_or(DEFAULT_EVERY))?;
    Ok(true)
}

pub fn uninstall() -> Result<Vec<PathBuf>> {
    systemctl(&["disable", "--now", MOUNT, TIMER, PATH, REFRESH]);
    let mut removed = vec![];
    for name in [MOUNT, REFRESH, TIMER, PATH] {
        let p = unit_path(name);
        if p.exists() {
            std::fs::remove_file(&p)?;
            removed.push(p);
        }
    }
    systemctl(&["daemon-reload"]);
    Ok(removed)
}

pub fn installed() -> Vec<(String, bool)> {
    [MOUNT, TIMER]
        .iter()
        .map(|n| (n.to_string(), unit_path(n).exists()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        Ctx {
            exe: "/opt/my tools/clove".into(),
            log: "/home/u/.clove/logs".into(),
            env: vec![
                ("PATH".into(), "/bin:/a%b".into()),
                ("HOME".into(), "/home/u".into()),
            ],
        }
    }

    #[test]
    fn mount_unit_quotes_and_installs_at_login() {
        let u = mount_unit(&ctx());
        assert!(u.contains("ExecStart=\"/opt/my tools/clove\" \"mount\" \"--all-repos\"\n"));
        assert!(u.contains("Environment=\"PATH=/bin:/a%%b\"\n"));
        assert!(u.contains("StandardOutput=append:/home/u/.clove/logs/dev.clove.mount.log\n"));
        assert!(u.contains("WantedBy=default.target"));
    }

    #[test]
    fn refresh_runs_low_priority_without_install_section() {
        let u = refresh_unit(&ctx());
        assert!(u.contains("\"refresh\" \"--all-repos\" \"--if-stale\" \"--prs\""));
        assert!(u.contains("IOSchedulingClass=idle"));
        assert!(!u.contains("[Install]"));
    }

    #[test]
    fn timer_interval_round_trips() {
        let t = timer_unit(7200);
        assert!(t.contains("OnUnitActiveSec=7200s"));
        assert_eq!(parse_every(&t), Some(7200));
    }

    #[test]
    fn path_unit_watches_each_fetch_head() {
        let u = path_unit(&[
            "/src/a/.git/FETCH_HEAD".into(),
            "/src/100%/.git/FETCH_HEAD".into(),
        ]);
        assert!(u.contains("PathChanged=/src/a/.git/FETCH_HEAD\n"));
        assert!(u.contains("PathChanged=/src/100%%/.git/FETCH_HEAD\n"));
        assert!(u.contains("Unit=clove-refresh.service"));
    }

    #[test]
    fn quoting_escapes_specials() {
        assert_eq!(quote_exec(r#"a"b\c$d"#), r#""a\"b\\c$$d""#);
        assert_eq!(quote("PATH=$x"), r#""PATH=$x""#);
    }
}
