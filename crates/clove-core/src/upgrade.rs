//! New releases: the latest one on GitHub, how this clove was installed, and
//! the Homebrew upgrade. One cache file (`upgrade-check.json` in the state
//! folder) serves the CLI, the TUI and the app, so they agree and GitHub is
//! asked at most once an hour between them.

use crate::events::{Event, Reporter};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The version of the code running now (the CLI's, or the app's clove-core).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO: &str = "pierre-borckmans/clove";
pub const TAP: &str = "pierre-borckmans/tap";
pub const FORMULA: &str = "pierre-borckmans/tap/clove";
pub const CASK: &str = "clove-app";
/// The macOS app's bundle id, to relaunch it.
pub const APP_ID: &str = "dev.clove.desktop";
/// Automatic checks wait this long after the last one; forced ones don't.
pub const CHECK_EVERY: u64 = 3600;
/// The CLI's "new version" hint shows at most this often per version.
const HINT_EVERY: u64 = 24 * 3600;
const API: &str = "https://api.github.com/repos/pierre-borckmans/clove/releases/latest";

pub fn releases_url() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Release {
    /// Without the leading `v`.
    pub version: String,
    /// The release page.
    pub url: String,
    /// Release notes (markdown).
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub published_at: Option<String>,
}

/// What `upgrade-check.json` holds.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cache {
    /// Unix time of the last request, answered or not.
    #[serde(default)]
    pub checked_at: u64,
    /// GitHub's ETag for `latest`: an unchanged release costs a 304.
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub latest: Option<Release>,
    /// Why the last request failed; None once one succeeds.
    #[serde(default)]
    pub error: Option<String>,
    /// The last upgrade that failed, for the app to show after it relaunches.
    #[serde(default)]
    pub upgrade_error: Option<String>,
    /// Version and time the CLI last printed its hint.
    #[serde(default)]
    pub hinted: Option<(String, u64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Homebrew formula (CLI) or cask (app): `clove upgrade` handles it.
    Brew,
    /// Anything else: the user downloads the release.
    Manual,
}

/// Where this clove stands, for every surface.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub current: String,
    pub latest: Option<Release>,
    /// `latest` is newer than `current`.
    pub available: bool,
    pub checked_at: u64,
    pub error: Option<String>,
    pub upgrade_error: Option<String>,
    /// How the CLI on PATH was installed.
    pub cli: Source,
    pub cli_path: Option<PathBuf>,
}

fn cache_path() -> PathBuf {
    crate::state_home().join("upgrade-check.json")
}

pub fn load_cache(path: &Path) -> Cache {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_cache(path: &Path, c: &Cache) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::util::write_atomic(path, &serde_json::to_vec_pretty(c)?)
}

/// The last known answer, without asking GitHub.
pub fn status() -> Status {
    status_of(&load_cache(&cache_path()), VERSION)
}

fn status_of(c: &Cache, current: &str) -> Status {
    let (cli, cli_path) = cli_source();
    Status {
        current: current.to_string(),
        available: c.latest.as_ref().is_some_and(|l| newer(&l.version, current)),
        latest: c.latest.clone(),
        checked_at: c.checked_at,
        error: c.error.clone(),
        upgrade_error: c.upgrade_error.clone(),
        cli,
        cli_path,
    }
}

/// Asks GitHub for the latest release, unless someone did within the hour.
/// `force` asks anyway. A failed request is recorded in the status, not
/// returned as an error, so callers keep the last known release.
pub fn check(force: bool) -> Status {
    let path = cache_path();
    let c = check_at(&path, crate::util::now_unix(), force, &curl_latest);
    status_of(&c, VERSION)
}

/// Whether an automatic check is due.
pub fn stale() -> bool {
    is_stale(&load_cache(&cache_path()), crate::util::now_unix())
}

fn is_stale(c: &Cache, now: u64) -> bool {
    // Never checked, or a clock that went backwards, counts as stale too.
    c.checked_at == 0 || now < c.checked_at || now - c.checked_at >= CHECK_EVERY
}

/// One HTTP answer: status code, ETag, body.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub code: u16,
    pub etag: Option<String>,
    pub body: String,
}

type Fetch<'a> = &'a dyn Fn(Option<&str>) -> Result<Response>;

fn check_at(path: &Path, now: u64, force: bool, fetch: Fetch) -> Cache {
    let mut c = load_cache(path);
    if !force && !is_stale(&c, now) {
        return c;
    }
    // Without a release to fall back on, a 304 would leave nothing to show.
    let etag = c.latest.as_ref().and(c.etag.clone());
    c.checked_at = now;
    match fetch(etag.as_deref()) {
        Ok(r) if r.code == 304 => c.error = None,
        Ok(r) if r.code == 200 => match parse_release(&r.body) {
            Ok(rel) => {
                c.latest = Some(rel);
                c.etag = r.etag;
                c.error = None;
            }
            Err(e) => c.error = Some(format!("{e:#}")),
        },
        Ok(r) => c.error = Some(format!("GitHub answered {}", r.code)),
        Err(e) => c.error = Some(format!("{e:#}")),
    }
    let _ = save_cache(path, &c);
    c
}

fn parse_release(body: &str) -> Result<Release> {
    let v: serde_json::Value = serde_json::from_str(body).context("release JSON")?;
    let tag = v["tag_name"].as_str().context("release without a tag")?;
    let version = tag.trim_start_matches('v').to_string();
    parse_version(&version).with_context(|| format!("not a version: {tag}"))?;
    Ok(Release {
        url: v["html_url"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| format!("https://github.com/{REPO}/releases/tag/{tag}")),
        notes: v["body"].as_str().unwrap_or_default().to_string(),
        published_at: v["published_at"].as_str().map(str::to_string),
        version,
    })
}

/// `curl`, like most machines have: clove carries no HTTP client.
fn curl_latest(etag: Option<&str>) -> Result<Response> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "-L", "--max-time", "10", "-D", "-"])
        .args(["-H", "Accept: application/vnd.github+json"])
        .args(["-H", "User-Agent: clove-update-check"]);
    if let Some(e) = etag {
        cmd.arg("-H").arg(format!("If-None-Match: {e}"));
    }
    let out = crate::util::output(cmd.arg(API).stdin(Stdio::null()))?;
    parse_response(&String::from_utf8_lossy(&out.stdout))
}

/// Splits `curl -D -` output into the last response's status, ETag and body
/// (redirects print one header block each).
pub fn parse_response(raw: &str) -> Result<Response> {
    let mut rest = raw;
    loop {
        let (head, body) = rest
            .split_once("\r\n\r\n")
            .or_else(|| rest.split_once("\n\n"))
            .unwrap_or((rest, ""));
        if body.starts_with("HTTP/") {
            rest = body;
            continue;
        }
        let mut lines = head.lines();
        let code = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|c| c.parse().ok())
            .context("no HTTP status in the answer")?;
        let etag = lines.find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("etag")
                .then(|| v.trim().to_string())
        });
        return Ok(Response {
            code,
            etag,
            body: body.to_string(),
        });
    }
}

// ---- versions -------------------------------------------------------------

/// (major, minor, patch, pre-release) of `1.2.3` or `v1.2.3-rc.1`.
fn parse_version(v: &str) -> Option<(u64, u64, u64, Option<&str>)> {
    let v = v.trim().trim_start_matches('v');
    let v = v.split('+').next()?;
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let mut n = core.split('.').map(|p| p.parse::<u64>().ok());
    let out = (n.next()??, n.next()??, n.next()??, pre);
    n.next().is_none().then_some(out)
}

/// Semver order; a pre-release comes before its release. Unparsable
/// versions compare equal, so they never offer an upgrade.
pub fn compare(a: &str, b: &str) -> Ordering {
    let (Some(a), Some(b)) = (parse_version(a), parse_version(b)) else {
        return Ordering::Equal;
    };
    (a.0, a.1, a.2)
        .cmp(&(b.0, b.1, b.2))
        .then_with(|| match (a.3, b.3) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(x), Some(y)) => compare_pre(x, y),
        })
}

fn compare_pre(a: &str, b: &str) -> Ordering {
    let mut a = a.split('.');
    let mut b = b.split('.');
    loop {
        return match (a.next(), b.next()) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(x), Some(y)) => {
                let o = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    _ => x.cmp(y),
                };
                if o == Ordering::Equal {
                    continue;
                }
                o
            }
        };
    }
}

/// `latest` is a newer version than `current`.
pub fn newer(latest: &str, current: &str) -> bool {
    compare(latest, current) == Ordering::Greater
}

// ---- the CLI's hint -----------------------------------------------------------

/// The one-line hint to print after a command, from the cache only: Some at
/// most once a day per version. Starts a background check when the cache is
/// older than an hour, for the next command to show.
pub fn hint() -> Option<String> {
    let path = cache_path();
    let mut c = load_cache(&path);
    let now = crate::util::now_unix();
    if is_stale(&c, now) {
        // Marked checked first, so commands meanwhile don't start more.
        c.checked_at = now;
        if save_cache(&path, &c).is_ok() {
            spawn_background_check();
        }
    }
    let v = c.latest.as_ref().map(|l| l.version.clone())?;
    if !newer(&v, VERSION) {
        return None;
    }
    if c.hinted
        .as_ref()
        .is_some_and(|(hv, at)| *hv == v && now.saturating_sub(*at) < HINT_EVERY)
    {
        return None;
    }
    c.hinted = Some((v.clone(), now));
    let _ = save_cache(&path, &c);
    Some(v)
}

/// `clove upgrade --check --background`, detached, so no command waits on
/// the network.
fn spawn_background_check() {
    let Ok(exe) = std::env::current_exe() else { return };
    if exe.file_name().is_none_or(|n| n != "clove") {
        return;
    }
    use std::os::unix::process::CommandExt;
    let _ = Command::new(exe)
        .args(["upgrade", "--check", "--background"])
        .env("CLOVE_NO_UPDATE_CHECK", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
}

/// The detached check: forced, since `hint` already marked the cache checked.
pub fn background_check() {
    let _ = check(true);
}

// ---- install source -----------------------------------------------------------

fn brew_bin() -> Option<PathBuf> {
    crate::util::which("brew").or_else(|| {
        [
            "/opt/homebrew/bin/brew",
            "/usr/local/bin/brew",
            "/home/linuxbrew/.linuxbrew/bin/brew",
        ]
        .map(PathBuf::from)
        .into_iter()
        .find(|p| p.is_file())
    })
}

/// `brew --prefix`, asked once per process.
pub fn brew_prefix() -> Option<PathBuf> {
    static PREFIX: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    PREFIX
        .get_or_init(|| {
            if let Some(p) = std::env::var_os("HOMEBREW_PREFIX").map(PathBuf::from)
                && p.is_dir()
            {
                return Some(p);
            }
            let out = crate::util::run(Command::new(brew_bin()?).arg("--prefix")).ok()?;
            (!out.is_empty()).then(|| PathBuf::from(out))
        })
        .clone()
}

/// A binary that resolves into `<prefix>/Cellar/clove/` came from the formula.
pub fn from_formula(exe: &Path, prefix: &Path) -> bool {
    let real = exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf());
    let cellar = prefix.join("Cellar/clove");
    let cellar = cellar.canonicalize().unwrap_or(cellar);
    real.starts_with(cellar)
}

/// How the clove on PATH (this one, from the CLI) was installed, and where it is.
pub fn cli_source() -> (Source, Option<PathBuf>) {
    let Ok(exe) = crate::service::cli_exe() else {
        return (Source::Manual, None);
    };
    let brew = brew_prefix().is_some_and(|p| from_formula(&exe, &p));
    (if brew { Source::Brew } else { Source::Manual }, Some(exe))
}

/// Whether the app came from the `clove-app` cask (its Caskroom folder is
/// there; `brew list --cask` says the same, slower).
pub fn app_from_cask() -> bool {
    cfg!(target_os = "macos") && brew_prefix().is_some_and(|p| p.join("Caskroom").join(CASK).is_dir())
}

/// What `clove --version` of the CLI on PATH says: it may differ from the
/// running code after an upgrade, or next to an app.
pub fn cli_version() -> Option<String> {
    let exe = crate::service::cli_exe().ok()?;
    let out = crate::util::run(
        Command::new(exe)
            .arg("--version")
            .env("CLOVE_NO_UPDATE_CHECK", "1"),
    )
    .ok()?;
    out.split_whitespace()
        .last()
        .filter(|v| parse_version(v).is_some())
        .map(str::to_string)
}

// ---- upgrade -----------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct UpgradeOptions {
    /// The app's pid: wait for it to quit before replacing it, then open it again.
    pub relaunch_app: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Upgraded {
    pub from: String,
    /// The CLI's version afterwards.
    pub to: Option<String>,
    /// The app was upgraded too.
    pub app: bool,
    /// The newer release whose app the tap doesn't have yet (Apple still
    /// notarizing it), when that left the app behind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_pending: Option<String>,
}

/// Upgrades the Homebrew installs: refreshes our tap only (not every tap,
/// which `brew update` would), upgrades the formula, restarts the service
/// on the new binary, then the cask when installed. Manual installs get the
/// download page in the error.
pub fn upgrade(r: &dyn Reporter, opts: &UpgradeOptions) -> Result<Upgraded> {
    // The app quits right after starting us; its bundle must not be replaced
    // while it runs.
    let app_quit = opts
        .relaunch_app
        .map(|pid| wait_for_exit(pid, Duration::from_secs(60)));
    let res = upgrade_inner(r, app_quit);
    let path = cache_path();
    let mut c = load_cache(&path);
    c.upgrade_error = match &res {
        Err(e) => Some(format!("{e:#}")),
        // Shown by the relaunched app, which is still the old one.
        Ok(u) => u
            .app_pending
            .as_ref()
            .map(|v| format!("the CLI is upgraded; the app's v{v} isn't in the tap yet (Apple is still notarizing it): update again later")),
    };
    let _ = save_cache(&path, &c);
    if app_quit.is_some() {
        // Even after a failure: the old app opens and shows what went wrong.
        relaunch_app(r);
    }
    res
}

/// `app_quit`: None from a terminal; from the app, whether it quit in time.
fn upgrade_inner(r: &dyn Reporter, app_quit: Option<bool>) -> Result<Upgraded> {
    let (source, exe) = cli_source();
    let latest = check(true);
    if source == Source::Manual {
        let at = exe.map(|p| format!(" ({})", p.display())).unwrap_or_default();
        let what = latest
            .latest
            .as_ref()
            .filter(|_| latest.available)
            .map(|l| format!("v{} from {}", l.version, l.url))
            .unwrap_or_else(|| format!("it from {}", releases_url()));
        bail!("clove wasn't installed with Homebrew{at}: download {what}");
    }
    let brew = brew_bin().context("brew not found")?;
    if let Some(e) = &latest.error {
        r.event(Event::Warn {
            message: format!("couldn't ask GitHub for the latest release ({e}); upgrading anyway"),
        });
    }
    let mut timings = vec![];
    let tap = crate::util::run(Command::new(&brew).args(["--repository", TAP]))
        .map(PathBuf::from)
        .ok()
        .filter(|p| p.join(".git").exists());
    let auto_update = match &tap {
        Some(dir) => {
            crate::events::timed(r, &mut timings, "update tap", || {
                stream(
                    r,
                    "update tap",
                    Command::new(crate::git::bin())
                        .arg("-C")
                        .arg(dir)
                        .args(["pull", "--ff-only"]),
                )
            })?;
            false
        }
        None => {
            r.event(Event::Skip {
                step: "update tap".into(),
                reason: format!("{TAP} isn't a git checkout; brew updates itself"),
            });
            true
        }
    };
    let brew_cmd = |args: &[&str]| {
        let mut c = Command::new(&brew);
        c.args(args);
        if !auto_update {
            c.env("HOMEBREW_NO_AUTO_UPDATE", "1");
        }
        c
    };
    crate::events::timed(r, &mut timings, "upgrade clove", || {
        stream(r, "upgrade clove", &mut brew_cmd(&["upgrade", FORMULA]))
    })?;
    if crate::service::installed().iter().all(|(_, on)| *on) {
        crate::events::timed(r, &mut timings, "restart service", || {
            crate::service::sync().map(|_| ())
        })?;
    }
    let mut app = false;
    let cask = cfg!(target_os = "macos")
        && brew_cmd(&["list", "--cask", CASK])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
    if cask {
        if app_quit == Some(false) {
            bail!("the app didn't quit, so it wasn't upgraded: quit it and run clove upgrade");
        }
        if app_quit.is_none() && app_running() {
            r.event(Event::Skip {
                step: "upgrade app".into(),
                reason: "the app is open: update it from its menu (Check for Updates…)".into(),
            });
        } else {
            crate::events::timed(r, &mut timings, "upgrade app", || {
                stream(r, "upgrade app", &mut brew_cmd(&["upgrade", "--cask", CASK]))
            })?;
            app = true;
        }
    }
    // The formula lands minutes after a release, the cask only once Apple
    // has notarized the app: say so rather than claim it's up to date.
    let app_pending = cask
        .then(|| {
            let have = crate::util::run(&mut brew_cmd(&["list", "--cask", "--versions", CASK])).ok()?;
            let have = have.split_whitespace().nth(1)?.to_string();
            let want = latest.latest.as_ref()?.version.clone();
            newer(&want, &have).then_some(want)
        })
        .flatten();
    if let Some(v) = &app_pending {
        app = false;
        r.event(Event::Warn {
            message: format!(
                "the app's v{v} isn't in the tap yet (Apple is still notarizing it): update again later"
            ),
        });
    }
    Ok(Upgraded {
        from: VERSION.to_string(),
        to: cli_version(),
        app,
        app_pending,
    })
}

/// Runs a command, its output lines as events of `step`; the error carries
/// the last lines.
fn stream(r: &dyn Reporter, step: &str, cmd: &mut Command) -> Result<()> {
    let desc = crate::util::describe(cmd);
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("running {desc}"))?;
    let tail = Mutex::new(VecDeque::<String>::new());
    let streams: [Box<dyn std::io::Read + Send>; 2] = [
        Box::new(child.stdout.take().expect("piped")),
        Box::new(child.stderr.take().expect("piped")),
    ];
    std::thread::scope(|s| {
        for out in streams {
            let tail = &tail;
            s.spawn(move || {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    {
                        let mut t = tail.lock().unwrap_or_else(|e| e.into_inner());
                        t.push_back(line.clone());
                        if t.len() > 8 {
                            t.pop_front();
                        }
                    }
                    r.event(Event::Output {
                        step: step.to_string(),
                        line,
                    });
                }
            });
        }
    });
    let status = child.wait()?;
    if !status.success() {
        let t = tail.lock().unwrap_or_else(|e| e.into_inner());
        let last: Vec<&str> = t.iter().map(String::as_str).collect();
        bail!("{desc} failed ({status})\n{}", last.join("\n"));
    }
    Ok(())
}

fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// Waits up to `limit` for `pid` to exit. True once it has.
fn wait_for_exit(pid: u32, limit: Duration) -> bool {
    let start = Instant::now();
    while alive(pid) {
        if start.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    true
}

/// The macOS app is running (its binary lives in `clove.app`).
fn app_running() -> bool {
    cfg!(target_os = "macos")
        && Command::new("pgrep")
            .args(["-f", "clove.app/Contents/MacOS/"])
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
}

fn relaunch_app(r: &dyn Reporter) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let ok = Command::new("open")
        .args(["-b", APP_ID])
        .status()
        .is_ok_and(|s| s.success())
        || Command::new("open")
            .args(["-a", "clove"])
            .status()
            .is_ok_and(|s| s.success());
    if !ok {
        r.event(Event::Warn {
            message: "couldn't open the app again; start it yourself".into(),
        });
    }
}

/// Forgets a failed upgrade once the app has shown it.
pub fn clear_upgrade_error() -> Result<()> {
    let path = cache_path();
    let mut c = load_cache(&path);
    if c.upgrade_error.take().is_some() {
        save_cache(&path, &c)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn versions_order_like_semver() {
        assert!(newer("0.1.23", "0.1.22"));
        assert!(newer("v0.2.0", "0.1.99"));
        assert!(newer("1.0.0", "0.99.99"));
        assert!(!newer("0.1.22", "0.1.22"));
        assert!(!newer("0.1.21", "0.1.22"));
        // A release beats its pre-releases; pre-releases order by fields.
        assert!(newer("0.2.0", "0.2.0-rc.1"));
        assert!(!newer("0.2.0-rc.1", "0.2.0"));
        assert!(newer("0.2.0-rc.10", "0.2.0-rc.2"));
        assert!(newer("0.2.0-rc.1", "0.2.0-beta.3"));
        assert!(newer("0.2.0-rc.1.1", "0.2.0-rc.1"));
        // Build metadata is ignored; junk never offers an upgrade.
        assert_eq!(compare("1.0.0+abc", "1.0.0"), Ordering::Equal);
        assert!(!newer("latest", "0.1.0"));
        assert!(!newer("1.2", "0.1.0"));
    }

    fn release_json(tag: &str) -> String {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/{REPO}/releases/tag/{tag}"),
            "body": "## What's new\n- faster",
            "published_at": "2026-11-20T10:00:00Z",
        })
        .to_string()
    }

    fn ok(tag: &str, etag: &str) -> Response {
        Response {
            code: 200,
            etag: Some(etag.into()),
            body: release_json(tag),
        }
    }

    #[test]
    fn checks_hourly_with_the_etag_and_keeps_the_release_on_304() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("upgrade-check.json");
        let sent: RefCell<Vec<Option<String>>> = RefCell::default();
        let answers: RefCell<VecDeque<Response>> = RefCell::new(
            [
                ok("v0.2.0", "\"e1\""),
                Response {
                    code: 304,
                    etag: Some("\"e1\"".into()),
                    body: String::new(),
                },
                ok("v0.3.0", "\"e2\""),
            ]
            .into(),
        );
        let fetch = |etag: Option<&str>| -> Result<Response> {
            sent.borrow_mut().push(etag.map(str::to_string));
            Ok(answers.borrow_mut().pop_front().unwrap())
        };

        let c = check_at(&path, 1000, false, &fetch);
        assert_eq!(c.latest.as_ref().unwrap().version, "0.2.0");
        assert_eq!(c.latest.as_ref().unwrap().notes, "## What's new\n- faster");
        // Within the hour: from the cache, no request.
        let c = check_at(&path, 1000 + CHECK_EVERY - 1, false, &fetch);
        assert_eq!(sent.borrow().len(), 1);
        assert_eq!(c.checked_at, 1000);
        // Due: conditional request; 304 keeps the release.
        let c = check_at(&path, 1000 + CHECK_EVERY, false, &fetch);
        assert_eq!(sent.borrow()[1].as_deref(), Some("\"e1\""));
        assert_eq!(c.latest.as_ref().unwrap().version, "0.2.0");
        assert_eq!(c.checked_at, 1000 + CHECK_EVERY);
        // Forced: asks at once.
        let c = check_at(&path, 1000 + CHECK_EVERY + 5, true, &fetch);
        assert_eq!(sent.borrow().len(), 3);
        assert_eq!(c.latest.as_ref().unwrap().version, "0.3.0");
        assert_eq!(load_cache(&path).etag.as_deref(), Some("\"e2\""));
        assert!(status_of(&c, "0.2.5").available);
        assert!(!status_of(&c, "0.3.0").available);
    }

    #[test]
    fn a_failed_check_is_recorded_and_keeps_the_last_release() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("upgrade-check.json");
        check_at(&path, 10, false, &|_| Ok(ok("v1.0.0", "x")));
        let c = check_at(&path, 10, true, &|_| anyhow::bail!("offline"));
        assert_eq!(c.error.as_deref(), Some("offline"));
        assert_eq!(c.latest.unwrap().version, "1.0.0");
        let c = check_at(&path, 11, true, &|_| {
            Ok(Response {
                code: 403,
                etag: None,
                body: String::new(),
            })
        });
        assert_eq!(c.error.as_deref(), Some("GitHub answered 403"));
        // The next success clears it.
        let c = check_at(&path, 12, true, &|_| Ok(ok("v1.0.1", "y")));
        assert_eq!(c.error, None);
    }

    #[test]
    fn no_etag_is_sent_without_a_release_to_fall_back_on() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("upgrade-check.json");
        save_cache(
            &path,
            &Cache {
                etag: Some("\"stale\"".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let seen = RefCell::new(Some(String::new()));
        check_at(&path, 5, true, &|e| {
            *seen.borrow_mut() = e.map(str::to_string);
            Ok(ok("v1.0.0", "z"))
        });
        assert_eq!(*seen.borrow(), None);
    }

    #[test]
    fn a_clock_going_back_makes_the_cache_stale() {
        let c = Cache {
            checked_at: 5000,
            ..Default::default()
        };
        assert!(!is_stale(&c, 5000 + 10));
        assert!(is_stale(&c, 100));
        assert!(is_stale(&c, 5000 + CHECK_EVERY));
    }

    #[test]
    fn parses_curl_output_after_redirects() {
        let raw = "HTTP/1.1 301 Moved Permanently\r\nLocation: x\r\n\r\n\
                   HTTP/2 200\r\ncontent-type: application/json\r\nETag: W/\"abc\"\r\n\r\n{\"a\":1}";
        let r = parse_response(raw).unwrap();
        assert_eq!(r.code, 200);
        assert_eq!(r.etag.as_deref(), Some("W/\"abc\""));
        assert_eq!(r.body, "{\"a\":1}");
        let r = parse_response("HTTP/2 304\r\netag: \"q\"\r\n\r\n").unwrap();
        assert_eq!((r.code, r.body.as_str()), (304, ""));
        assert!(parse_response("garbage").is_err());
    }

    #[test]
    fn rejects_releases_without_a_version_tag() {
        assert!(parse_release(&release_json("nightly")).is_err());
        assert_eq!(parse_release(&release_json("v0.4.1")).unwrap().version, "0.4.1");
    }

    #[test]
    fn a_binary_in_the_cellar_came_from_the_formula() {
        let t = tempfile::tempdir().unwrap();
        let prefix = t.path();
        let cellar = prefix.join("Cellar/clove/0.1.14/bin");
        std::fs::create_dir_all(&cellar).unwrap();
        std::fs::write(cellar.join("clove"), "").unwrap();
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        std::os::unix::fs::symlink(cellar.join("clove"), prefix.join("bin/clove")).unwrap();
        assert!(from_formula(&prefix.join("bin/clove"), prefix));
        assert!(from_formula(&cellar.join("clove"), prefix));
        // A cargo build, or another formula's Cellar, is not ours.
        let other = prefix.join("Cellar/clove-nightly/bin");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("clove"), "").unwrap();
        assert!(!from_formula(&other.join("clove"), prefix));
        let cargo = prefix.join(".cargo/bin");
        std::fs::create_dir_all(&cargo).unwrap();
        std::fs::write(cargo.join("clove"), "").unwrap();
        assert!(!from_formula(&cargo.join("clove"), prefix));
    }
}
