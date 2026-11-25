mod brand;
mod complete;
mod render;
mod service;
mod shell;

use anstream::{eprint, eprintln, println};
use anyhow::{Context, Result, bail};
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::engine::ArgValueCompleter;
use clove_core::doctor::{self, Level};
use clove_core::{Clove, Exec, NewOptions, RemoveOptions, Silent};
use render::*;
use std::io::IsTerminal;
use std::path::PathBuf;

const AFTER_HELP: &str = "\
Getting started:
  clove init                 build the golden image for this repo (once, slow)
  clove new feat/login       a ready worktree in ~1s, deps and env included
  clove ls                   list clones
  clove rm feat/login        unmount and delete it
  clove ui                   all of the above, interactively

Shell integration (cd into new clones, completions):
  eval \"$(clove shell-init zsh)\"     # or bash; fish: clove shell-init fish | source

Agents: every command takes --json; `clove new --json` prints the path.";

/// Copy-on-write git worktrees with dependencies already installed.
#[derive(Parser)]
#[command(name = "clove", version, about, after_help = AFTER_HELP, max_term_width = 100)]
struct Cli {
    /// Run as if started in <DIR>.
    #[arg(short = 'C', global = true, value_name = "DIR")]
    dir: Option<PathBuf>,
    /// Machine-readable output on stdout.
    #[arg(long, global = true)]
    json: bool,
    /// Show every line of step output.
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build the golden image: checkout of the base branch, warmed.
    Init,
    /// Create a clone on a branch (new, local, or from origin).
    #[command(visible_alias = "add")]
    New(NewArgs),
    /// List clones.
    #[command(visible_alias = "ls")]
    List {
        /// Skip git status (faster with many clones).
        #[arg(long)]
        fast: bool,
    },
    /// Remove clones: unmount, delete the image and the git registration.
    #[command(visible_alias = "rm")]
    Remove(RemoveArgs),
    /// Change directory to a clone (needs shell integration). No argument: the main checkout.
    Cd {
        #[arg(add = ArgValueCompleter::new(complete::clones))]
        clone: Option<String>,
    },
    /// Print a clone's worktree path, mounting it if needed.
    Path {
        #[arg(add = ArgValueCompleter::new(complete::clones))]
        clone: String,
    },
    /// Run a command inside a clone, in its dev environment (direnv/mise).
    #[command(visible_alias = "exec")]
    Run {
        #[arg(add = ArgValueCompleter::new(complete::clones))]
        clone: String,
        /// Command and arguments.
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Update the golden image to the latest base. Existing clones are untouched.
    Refresh {
        /// Every repo clove knows about (used by the service).
        #[arg(long)]
        all_repos: bool,
        /// Only if stale: a step's inputs changed on the base, or past [refresh] max_age / max_behind.
        #[arg(long)]
        if_stale: bool,
        /// Rerun every warm step, not only those whose inputs changed.
        #[arg(long)]
        full: bool,
    },
    /// Re-sync a clone after its branch moved: copy local files, rerun changed setup steps.
    Setup {
        /// Clone (default: the one you are in).
        #[arg(add = ArgValueCompleter::new(complete::clones))]
        clone: Option<String>,
        /// Rerun every setup step.
        #[arg(long)]
        all: bool,
    },
    /// Golden image and clone summary for this repo.
    #[command(visible_alias = "info")]
    Status,
    /// Mount clones (after a reboot). No argument: all of them.
    Mount {
        #[arg(add = ArgValueCompleter::new(complete::clones))]
        clones: Vec<String>,
        /// Every repo clove knows about.
        #[arg(long)]
        all_repos: bool,
    },
    /// Unmount clones without deleting them. No argument: all of them.
    Unmount {
        #[arg(add = ArgValueCompleter::new(complete::clones))]
        clones: Vec<String>,
        /// Unmount even if files are open.
        #[arg(short, long)]
        force: bool,
    },
    /// Check the machine and repo setup.
    Doctor,
    /// Find leftovers from interrupted operations.
    Gc {
        /// Remove them (default: only list).
        #[arg(long)]
        apply: bool,
    },
    /// Show the resolved config, or write it to .clove.toml.
    Config {
        /// Write .clove.toml with the current steps so the team can share them.
        #[arg(long)]
        write: bool,
    },
    /// Delete the golden image and clove's state for this repo.
    Deinit,
    /// Background agents: remount clones at login, keep golden images fresh.
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Hooks for other tools. `clove hook claude` prints the Claude Code settings.
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
    /// Print shell integration: `eval "$(clove shell-init zsh)"`.
    ShellInit { shell: shell::Shell },
    /// Interactive terminal UI.
    Ui,
}

#[derive(Subcommand)]
enum HookAction {
    /// Print the settings.json block that makes Claude Code use clove for worktrees.
    Claude,
    /// Claude Code WorktreeCreate hook: JSON on stdin, prints the clone path.
    WorktreeCreate,
    /// Claude Code WorktreeRemove hook: JSON on stdin with worktree_path.
    WorktreeRemove,
}

#[derive(Subcommand)]
enum ServiceAction {
    /// Install the launchd agents.
    Install {
        /// Fallback check for repos nobody fetches in, e.g. 1h, 4h. Fetches and
        /// pulls trigger a check right away.
        #[arg(long, default_value = "4h")]
        every: String,
    },
    /// Remove the launchd agents.
    Uninstall,
    /// Show whether the agents are installed.
    Status,
}

#[derive(Args)]
struct NewArgs {
    /// Branch to check out. Created from --base if it does not exist.
    /// `pr:123` checks out a pull request. Omit for a generated name.
    #[arg(add = ArgValueCompleter::new(complete::branches))]
    branch: Option<String>,
    /// Start point for a new branch.
    #[arg(short, long, add = ArgValueCompleter::new(complete::refs))]
    base: Option<String>,
    /// Clone name (default: branch with / turned into -).
    #[arg(short, long)]
    name: Option<String>,
    /// Detached HEAD at --base, no branch.
    #[arg(long, conflicts_with = "branch")]
    detach: bool,
    /// Skip setup steps.
    #[arg(long)]
    no_setup: bool,
    /// Do not fetch the base first.
    #[arg(long, conflicts_with = "fetch")]
    no_fetch: bool,
    /// Fetch the base even if the repo was fetched recently.
    #[arg(long)]
    fetch: bool,
    /// Stay in the current directory (with shell integration, `new` cds by default).
    #[arg(long)]
    no_cd: bool,
    /// Run this command in the clone afterwards, e.g. `-x claude`.
    #[arg(short = 'x', long = "exec", value_name = "CMD", num_args = 1.., allow_hyphen_values = true)]
    exec: Option<Vec<String>>,
}

#[derive(Args)]
struct RemoveArgs {
    #[arg(add = ArgValueCompleter::new(complete::clones), required_unless_present = "all")]
    clones: Vec<String>,
    /// Every clone of this repo.
    #[arg(long, conflicts_with = "clones")]
    all: bool,
    /// Discard uncommitted changes and force-unmount busy volumes.
    #[arg(short, long)]
    force: bool,
    /// Also delete the branch.
    #[arg(short = 'D', long)]
    delete_branch: bool,
}

fn main() {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    let json = cli.json;
    if let Err(e) = run(cli) {
        if json {
            println!("{}", serde_json::json!({ "error": format!("{e:#}") }));
        } else {
            eprintln!("{} {e:#}", paint(RED, "error:"));
        }
        std::process::exit(1);
    }
}

fn repo(cli: &Cli) -> Result<Clove> {
    let dir = match &cli.dir {
        Some(d) => d.clone(),
        None => std::env::current_dir()?,
    };
    Clove::discover(&dir)
}

fn emit<T: serde::Serialize>(v: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    let reporter = TermReporter::new(cli.verbose);
    let r: &dyn clove_core::Reporter = if cli.json { &Silent } else { &reporter };
    let Some(cmd) = &cli.cmd else {
        return home(&cli);
    };
    match cmd {
        Cmd::Init => {
            let c = repo(&cli)?;
            if !cli.json {
                print_plan(&c);
            }
            let meta = c.init(r)?;
            if cli.json {
                return emit(&meta);
            }
            eprintln!(
                "\n{} golden image ready in {}. Next: {}",
                paint(GREEN, "●"),
                secs(total_ms(&meta.timings)),
                paint(BOLD, "clove new <branch>")
            );
            after_init()?;
        }
        Cmd::Refresh {
            all_repos,
            if_stale,
            full,
        } => {
            let targets: Vec<Clove> = if *all_repos {
                clove_core::known_repos()
                    .into_iter()
                    .filter_map(|r| Clove::open(&r.root).ok())
                    .collect()
            } else {
                vec![repo(&cli)?]
            };
            let mut metas = vec![];
            let mut failed = false;
            for c in &targets {
                if *if_stale {
                    let _ = c.fetch_if_due();
                    let fresh = c.freshness().ok().flatten();
                    let skip = match &fresh {
                        None => Some("no golden image".to_string()),
                        Some(f) if f.refreshing.is_some() => Some("already refreshing".into()),
                        Some(f) if !f.stale => Some("fresh".into()),
                        _ => None,
                    };
                    if let Some(why) = skip {
                        if !cli.json {
                            eprintln!("{} {} {}", paint(DIM, "·"), c.name, paint(DIM, &why));
                        }
                        metas.push(serde_json::json!({ "repo": c.name, "skipped": why }));
                        continue;
                    }
                }
                if !cli.json {
                    eprintln!("{} {}", paint(BOLD, "refresh"), c.name);
                }
                match c.refresh_with(&clove_core::RefreshOptions { full: *full }, r) {
                    Ok(meta) => {
                        if !cli.json {
                            eprintln!(
                                "{} golden gen {} at {} ({})\n",
                                paint(GREEN, "●"),
                                meta.generation,
                                &meta.commit[..10],
                                secs(total_ms(&meta.timings))
                            );
                        }
                        metas.push(serde_json::json!({ "repo": c.name, "golden": meta }));
                    }
                    Err(e) if *all_repos => {
                        failed = true;
                        eprintln!("{} {}: {e:#}", paint(RED, "error:"), c.name);
                        metas.push(serde_json::json!({ "repo": c.name, "error": format!("{e:#}") }));
                    }
                    Err(e) => return Err(e),
                }
            }
            if cli.json {
                emit(&if *all_repos {
                    serde_json::json!(metas)
                } else {
                    metas[0]["golden"].clone()
                })?;
            }
            if failed {
                std::process::exit(1);
            }
        }
        Cmd::New(a) => cmd_new(&cli, a, r)?,
        Cmd::Setup { clone, all } => {
            let c = repo(&cli)?;
            let name = c.resolve(clone.as_deref().unwrap_or("."))?;
            if !cli.json {
                eprintln!("{} {}", paint(BOLD, "setup"), name);
            }
            let info = c.setup_clone(&name, *all, r)?;
            if cli.json {
                return emit(&info);
            }
            if !info.failed_steps.is_empty() {
                bail!("setup failed: {}", info.failed_steps.join(", "));
            }
        }
        Cmd::List { fast } => cmd_list(&cli, !fast)?,
        Cmd::Remove(a) => {
            let c = repo(&cli)?;
            let names = if a.all {
                c.clone_names()?
            } else {
                a.clones.iter().map(|q| c.resolve(q)).collect::<Result<_>>()?
            };
            // The shell's cwd keeps a volume busy. With shell integration, the
            // wrapper cds to the main checkout and then removes that clone.
            let cwd = std::env::current_dir().ok();
            let inside = |n: &String| cwd.as_ref().is_some_and(|d| d.starts_with(c.clone_mount(n)));
            let mut removed = vec![];
            for n in &names {
                if inside(n) && !a.force {
                    let mut args = vec!["rm".to_string()];
                    if a.delete_branch {
                        args.push("-D".into());
                    }
                    args.push(n.clone());
                    if shell::request_cd_then(&c.root, &args) {
                        eprintln!(
                            "{} {} {}",
                            paint(MAGENTA, "−"),
                            paint(BOLD, n),
                            paint(DIM, "(after leaving it)")
                        );
                        continue;
                    }
                    bail!(
                        "you are inside `{n}`; cd out first (or set up `clove shell-init`, which does it for you)"
                    );
                }
                if !cli.json {
                    eprintln!("{} {}", paint(MAGENTA, "−"), paint(BOLD, n));
                }
                c.remove(
                    n,
                    &RemoveOptions {
                        force: a.force,
                        delete_branch: a.delete_branch,
                    },
                    r,
                )?;
                if inside(n) {
                    shell::request_cd(&c.root);
                }
                removed.push(n.clone());
            }
            if cli.json {
                return emit(&serde_json::json!({ "removed": removed }));
            }
        }
        Cmd::Cd { clone } => {
            let c = repo(&cli)?;
            let path = match clone {
                Some(q) => c.mount(&c.resolve(q)?)?,
                None => c.root.clone(),
            };
            if cli.json {
                return emit(&serde_json::json!({ "path": path }));
            }
            if !shell::request_cd(&path) {
                println!("{}", path.display());
                hint_shell_init();
            }
        }
        Cmd::Path { clone } => {
            let c = repo(&cli)?;
            let path = c.mount(&c.resolve(clone)?)?;
            if cli.json {
                return emit(&serde_json::json!({ "path": path }));
            }
            println!("{}", path.display());
        }
        Cmd::Run { clone, command } => {
            let c = repo(&cli)?;
            let name = c.resolve(clone)?;
            let path = c.mount(&name)?;
            exec_in(&c, &name, &path, command)?;
        }
        Cmd::Status => cmd_status(&cli)?,
        Cmd::Mount { clones, all_repos } => {
            let targets: Vec<Clove> = if *all_repos {
                clove_core::known_repos()
                    .into_iter()
                    .filter_map(|r| Clove::open(&r.root).ok())
                    .collect()
            } else {
                vec![repo(&cli)?]
            };
            let mut mounted = vec![];
            for c in &targets {
                let names = if clones.is_empty() {
                    c.clone_names()?
                } else {
                    clones.iter().map(|q| c.resolve(q)).collect::<Result<_>>()?
                };
                for n in names {
                    match c.mount(&n) {
                        Ok(p) => mounted.push(p),
                        Err(e) => eprintln!("{} {n}: {e:#}", paint(YELLOW, "!")),
                    }
                }
            }
            if cli.json {
                return emit(&serde_json::json!({ "mounted": mounted }));
            }
            for p in mounted {
                eprintln!("  {} {}", paint(GREEN, "●"), tilde(&p));
            }
        }
        Cmd::Unmount { clones, force } => {
            let c = repo(&cli)?;
            let names = if clones.is_empty() {
                c.clone_names()?
            } else {
                clones.iter().map(|q| c.resolve(q)).collect::<Result<_>>()?
            };
            for n in &names {
                c.unmount(n, *force)?;
                if !cli.json {
                    eprintln!("  {} {n}", paint(DIM, "○"));
                }
            }
            if cli.json {
                return emit(&serde_json::json!({ "unmounted": names }));
            }
        }
        Cmd::Doctor => cmd_doctor(&cli)?,
        Cmd::Gc { apply } => {
            let c = repo(&cli)?;
            let actions = c.gc(*apply)?;
            if cli.json {
                return emit(&serde_json::json!({ "applied": apply, "actions": actions }));
            }
            if actions.is_empty() {
                eprintln!("{} nothing to clean", paint(GREEN, "✓"));
            }
            for a in &actions {
                eprintln!(
                    "  {} {a}",
                    if *apply {
                        paint(GREEN, "✓")
                    } else {
                        paint(DIM, "·")
                    }
                );
            }
            if !apply && !actions.is_empty() {
                eprintln!("\nrun {} to remove them", paint(BOLD, "clove gc --apply"));
            }
        }
        Cmd::Config { write } => {
            let c = repo(&cli)?;
            if *write {
                let path = c.root.join(clove_core::config::REPO_FILE);
                if path.exists() {
                    bail!("{} already exists", path.display());
                }
                std::fs::write(&path, c.config.to_repo_toml())?;
                eprintln!("{} wrote {}", paint(GREEN, "✓"), tilde(&path));
            } else if cli.json {
                return emit(&c.config);
            } else {
                print!("{}", c.config.to_repo_toml());
                if !c.config.sources.is_empty() {
                    eprintln!(
                        "{}",
                        paint(
                            DIM,
                            format!(
                                "# from {}",
                                c.config
                                    .sources
                                    .iter()
                                    .map(|p| tilde(p))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        )
                    );
                } else {
                    eprintln!(
                        "{}",
                        paint(DIM, "# detected; `clove config --write` saves it to .clove.toml")
                    );
                }
            }
        }
        Cmd::Deinit => {
            let c = repo(&cli)?;
            c.deinit(r)?;
            service::sync()?;
            if cli.json {
                return emit(&serde_json::json!({ "deinit": c.name }));
            }
        }
        Cmd::Service { action } => match action {
            ServiceAction::Install { every } => {
                let secs = clove_core::config::parse_duration(every)?.max(300);
                for p in service::install(secs)? {
                    eprintln!("{} {}", paint(GREEN, "✓"), tilde(&p));
                }
                eprintln!(
                    "clones remount at login; stale golden images refresh when you fetch or pull, and every {} otherwise",
                    clove_core::util::human_age(secs)
                );
            }
            ServiceAction::Uninstall => {
                for p in service::uninstall()? {
                    eprintln!("{} removed {}", paint(GREEN, "✓"), tilde(&p));
                }
            }
            ServiceAction::Status => {
                let st = service::installed();
                if cli.json {
                    return emit(&st);
                }
                for (label, on) in st {
                    let dot = if on {
                        paint(GREEN, "●")
                    } else {
                        paint(DIM, "○")
                    };
                    println!("  {dot} {label}");
                }
            }
        },
        Cmd::Hook { action } => cmd_hook(action)?,
        Cmd::ShellInit { shell } => print!("{}", shell::init(*shell)),
        Cmd::Ui => {
            let dir = match &cli.dir {
                Some(d) => d.clone(),
                None => std::env::current_dir()?,
            };
            clove_tui::run(&dir)?;
        }
    }
    Ok(())
}

/// First-run setup the golden image alone doesn't cover: shell integration and
/// the background service. Asks only in an interactive terminal, so scripts and
/// agents get a hint instead of a prompt.
fn after_init() -> Result<()> {
    if std::env::var_os("CLOVE_CD_FILE").is_none() {
        let sh = std::env::var("SHELL").unwrap_or_default();
        let line = match sh.rsplit('/').next() {
            Some("fish") => "clove shell-init fish | source".to_string(),
            Some(s @ ("bash" | "zsh")) => format!("eval \"$(clove shell-init {s})\""),
            _ => "eval \"$(clove shell-init zsh)\"".to_string(),
        };
        eprintln!(
            "{} add {} to your shell rc so `clove new` and `clove cd` can change directory",
            paint(DIM, "·"),
            paint(BOLD, &line)
        );
    }
    if service::installed().iter().all(|(_, on)| *on) {
        // Add this repo's FETCH_HEAD to the watch list.
        service::sync()?;
        return Ok(());
    }
    let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if !interactive {
        eprintln!(
            "{} run {} to refresh stale golden images when you pull and remount clones at login",
            paint(DIM, "·"),
            paint(BOLD, "clove service install")
        );
        return Ok(());
    }
    eprint!(
        "\nKeep golden images fresh in the background? Installs two launchd agents: \
         one refreshes a stale image when you fetch or pull (and every 4h), one remounts \
         clones at login. [Y/n] "
    );
    let mut answer = String::new();
    // End of input (Ctrl-D, a closed pipe) counts as no; only Enter takes the default.
    let read = std::io::stdin().read_line(&mut answer)?;
    if read > 0 && matches!(answer.trim().to_ascii_lowercase().as_str(), "" | "y" | "yes") {
        for p in service::install(service::DEFAULT_EVERY)? {
            eprintln!("{} {}", paint(GREEN, "✓"), tilde(&p));
        }
    } else {
        eprintln!(
            "{} skipped; {} later if you change your mind",
            paint(DIM, "·"),
            paint(BOLD, "clove service install")
        );
    }
    Ok(())
}

fn print_plan(c: &Clove) {
    eprintln!(
        "{} {}  {}",
        paint(BOLD, "clove init"),
        c.name,
        paint(DIM, tilde(&c.root))
    );
    eprintln!("  base   {}", c.config.base);
    if c.config.steps.is_empty() {
        eprintln!(
            "  steps  {}",
            paint(DIM, "none detected (add [[step]] to .clove.toml)")
        );
    } else {
        let names: Vec<_> = c.config.steps.iter().map(|s| s.name.as_str()).collect();
        let src = if c.config.steps_detected {
            " (detected)"
        } else {
            ""
        };
        eprintln!("  steps  {}{}", names.join(", "), paint(DIM, src));
    }
    let copy = c.resolve_copy();
    if !copy.is_empty() {
        let src = if c.config.copy.is_none() {
            " (detected, not in git)"
        } else {
            ""
        };
        eprintln!("  copy   {}{}", copy.join(", "), paint(DIM, src));
    }
    if c.config.exec != Exec::None {
        eprintln!("  env    {}", format!("{:?}", c.config.exec).to_lowercase());
    }
    eprintln!();
}

fn cmd_new(cli: &Cli, a: &NewArgs, r: &dyn clove_core::Reporter) -> Result<()> {
    let c = repo(cli)?;
    let opts = NewOptions {
        branch: a.branch.clone(),
        base: a.base.clone(),
        name: a.name.clone(),
        detach: a.detach,
        no_setup: a.no_setup,
        fetch: if a.no_fetch {
            Some(false)
        } else if a.fetch {
            Some(true)
        } else {
            None
        },
    };
    if !cli.json {
        let label = a
            .branch
            .clone()
            .or(a.name.clone())
            .unwrap_or_else(|| "new clone".into());
        eprintln!("{} {}", paint(GREEN, "+"), paint(BOLD, label));
    }
    let info = c.new_clone(&opts, r)?;
    if cli.json {
        emit(&info)?;
    } else {
        eprintln!(
            "\n{} {} {} {}",
            paint(GREEN, "●"),
            paint(BOLD, &info.name),
            paint(DIM, format!("ready in {}", secs(info.total_ms))),
            tilde(&info.path)
        );
        if !info.failed_steps.is_empty() {
            eprintln!(
                "  {} setup failed: {} (the clone is usable; see the log above)",
                paint(YELLOW, "!"),
                info.failed_steps.join(", ")
            );
        }
    }
    if let Some(cmd) = &a.exec {
        return exec_in(&c, &info.name, &info.path, cmd);
    }
    maybe_background_refresh(&c, cli.json);
    if !a.no_cd && !cli.json && !shell::request_cd(&info.path) && std::io::stdout().is_terminal() {
        hint_shell_init();
    }
    Ok(())
}

/// Starts `clove refresh --if-stale` in the background when the golden image is
/// stale, so the next clone starts from a fresh one. Never blocks `new`.
fn maybe_background_refresh(c: &Clove, quiet: bool) {
    if !c.config.refresh.auto || std::env::var_os("CLOVE_NO_AUTO_REFRESH").is_some() {
        return;
    }
    let Ok(Some(f)) = c.freshness() else { return };
    if !f.stale || f.refreshing.is_some() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let log = c.logs_dir().join("refresh.log");
    let _ = std::fs::create_dir_all(c.logs_dir());
    let Ok(out) = std::fs::File::create(&log) else {
        return;
    };
    let err = out.try_clone().ok();
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("-C")
        .arg(&c.root)
        .args(["refresh", "--if-stale"])
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .process_group(0);
    if let Some(e) = err {
        cmd.stderr(e);
    }
    if cmd.spawn().is_ok() && !quiet {
        eprintln!(
            "  {} golden image is {}; refreshing in the background ({})",
            paint(DIM, "↻"),
            f.reason.unwrap_or_default(),
            paint(DIM, tilde(&log))
        );
    }
}

fn hint_shell_init() {
    eprintln!(
        "{}",
        paint(
            DIM,
            "tip: eval \"$(clove shell-init zsh)\" lets clove cd for you and enables completions"
        )
    );
}

/// Replaces this process with `command` inside the clone's environment.
fn exec_in(c: &Clove, name: &str, path: &std::path::Path, command: &[String]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let (prog, args): (String, Vec<String>) = match c.config.exec {
        Exec::Direnv => (
            "direnv".into(),
            [vec!["exec".into(), path.display().to_string()], command.to_vec()].concat(),
        ),
        Exec::Mise => (
            "mise".into(),
            [vec!["exec".into(), "--".into()], command.to_vec()].concat(),
        ),
        _ => (command[0].clone(), command[1..].to_vec()),
    };
    let err = std::process::Command::new(&prog)
        .args(&args)
        .current_dir(path)
        .envs(c.step_env(path, name))
        .exec();
    Err(err).with_context(|| format!("exec {prog}"))
}

fn cmd_list(cli: &Cli, status: bool) -> Result<()> {
    let c = repo(cli)?;
    let clones = c.list(status)?;
    if cli.json {
        return emit(&clones);
    }
    if clones.is_empty() {
        let hint = if c.record()?.is_some() {
            "clove new <branch>"
        } else {
            "clove init"
        };
        eprintln!("no clones yet. {}", paint(BOLD, hint));
        return Ok(());
    }
    let mut t = Table::new(&["", "NAME", "BRANCH", "CHANGES", "AGE", "OWN", "PATH"]);
    let now = clove_core::util::now_unix();
    let cwd = std::env::current_dir().unwrap_or_default();
    for i in &clones {
        let here = cwd.starts_with(i.path.parent().unwrap_or(&i.path));
        let dot = if !i.mounted {
            styled(DIM, "○")
        } else if here {
            styled(CYAN, "@")
        } else {
            styled(GREEN, "●")
        };
        let branch = match &i.branch {
            Some(b) => styled(MAGENTA, b.clone()),
            None => styled(DIM, format!("({})", i.head.as_deref().unwrap_or("detached"))),
        };
        let mut changes = String::new();
        if i.dirty == Some(true) {
            changes.push_str("dirty ");
        }
        if let Some(a) = i.ahead.filter(|a| *a > 0) {
            changes.push_str(&format!("↑{a} "));
        }
        if let Some(b) = i.behind.filter(|b| *b > 0) {
            changes.push_str(&format!("↓{b}"));
        }
        let changes = if changes.is_empty() && i.mounted && status {
            styled(DIM, "clean")
        } else if !i.mounted {
            styled(DIM, "-")
        } else {
            styled(YELLOW, changes.trim().to_string())
        };
        t.row(vec![
            dot,
            styled(BOLD, i.name.clone()),
            branch,
            changes,
            cell(clove_core::util::human_age(now.saturating_sub(i.created_at))),
            cell(clove_core::util::human_bytes(i.own_bytes)),
            styled(DIM, tilde(&i.path)),
        ]);
    }
    t.print();
    Ok(())
}

fn cmd_status(cli: &Cli) -> Result<()> {
    let c = repo(cli)?;
    let s = c.status()?;
    if cli.json {
        return emit(&s);
    }
    println!("{}  {}", paint(BOLD, &s.name), paint(DIM, tilde(&s.root)));
    match &s.golden {
        Some(g) => {
            let behind = match g.behind {
                Some(0) => paint(GREEN, format!("up to date with {}", g.meta.base)),
                Some(n) => paint(YELLOW, format!("{n} behind {}", g.meta.base)),
                None => paint(DIM, "?"),
            };
            println!(
                "  golden  gen {} · {} · {} old · {} · {}",
                g.meta.generation,
                &g.meta.commit[..10],
                clove_core::util::human_age(g.age_secs),
                behind,
                clove_core::util::human_bytes(g.image_bytes)
            );
        }
        None => println!("  golden  {}", paint(YELLOW, "not built (clove init)")),
    }
    if let Some(f) = &s.freshness {
        let p = &c.config.refresh;
        let policy = paint(
            DIM,
            format!(
                "(stale when a step's inputs change, or after {} or {} commits; auto refresh {})",
                clove_core::util::human_age(p.max_age_secs),
                p.max_behind,
                if p.auto { "on" } else { "off" }
            ),
        );
        let state = match (&f.refreshing, &f.reason) {
            (Some(pid), _) => paint(CYAN, format!("refreshing now (pid {pid})")),
            (None, Some(why)) => paint(YELLOW, format!("stale: {why}")),
            (None, None) => paint(GREEN, "fresh"),
        };
        println!("  fresh   {state} {policy}");
    }
    if !s.copy.is_empty() {
        println!("  copies  {}", s.copy.join(", "));
    }
    let names: Vec<_> = s.steps.iter().map(|x| x.name.as_str()).collect();
    println!(
        "  steps   {}{}",
        if names.is_empty() {
            "none".into()
        } else {
            names.join(", ")
        },
        paint(DIM, if s.steps_detected { " (detected)" } else { "" })
    );
    println!(
        "  clones  {} ({} mounted) in {}",
        s.clones,
        s.mounted,
        paint(DIM, tilde(&s.clone_root))
    );
    Ok(())
}

fn cmd_doctor(cli: &Cli) -> Result<()> {
    let mut checks = doctor::machine();
    if let Ok(c) = repo(cli) {
        checks.extend(doctor::repo(&c));
    }
    if cli.json {
        return emit(&checks);
    }
    for ch in &checks {
        let (sym, style) = match ch.level {
            Level::Ok => ("✓", GREEN),
            Level::Warn => ("!", YELLOW),
            Level::Fail => ("✗", RED),
        };
        println!("  {} {:<14} {}", paint(style, sym), ch.name, ch.detail);
        if let Some(fix) = &ch.fix {
            println!("    {} {}", paint(DIM, "fix:"), paint(BOLD, fix));
        }
    }
    if checks.iter().any(|c| c.level == Level::Fail) {
        std::process::exit(1);
    }
    Ok(())
}

const CLAUDE_HOOKS: &str = r#"{
  "hooks": {
    "WorktreeCreate": [
      { "hooks": [{ "type": "command", "command": "clove hook worktree-create" }] }
    ],
    "WorktreeRemove": [
      { "hooks": [{ "type": "command", "command": "clove hook worktree-remove" }] }
    ]
  }
}"#;

/// Claude Code worktree hooks. Progress goes to stderr; stdout carries only the path.
fn cmd_hook(action: &HookAction) -> Result<()> {
    let read_input = || -> Result<serde_json::Value> {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
        serde_json::from_str(&buf).context("hook input is not JSON")
    };
    match action {
        HookAction::Claude => {
            println!("{CLAUDE_HOOKS}");
            eprintln!(
                "{}",
                paint(
                    DIM,
                    "merge into ~/.claude/settings.json (or .claude/settings.json in a repo); \
                     `claude --worktree <name>` and subagent worktrees then become clove clones"
                )
            );
        }
        HookAction::WorktreeCreate => {
            let input = read_input()?;
            let name = input["name"].as_str().context("hook input has no name")?;
            let cwd = input["cwd"]
                .as_str()
                .map(PathBuf::from)
                .unwrap_or(std::env::current_dir()?);
            let c = Clove::discover(&cwd)?;
            let info = c.new_clone(
                &NewOptions {
                    branch: Some(name.to_string()),
                    ..Default::default()
                },
                &TermReporter::new(false),
            )?;
            println!("{}", info.path.display());
        }
        HookAction::WorktreeRemove => {
            let input = read_input()?;
            let path = PathBuf::from(
                input["worktree_path"]
                    .as_str()
                    .context("hook input has no worktree_path")?,
            );
            let c = Clove::discover(&path).or_else(|_| {
                Clove::discover(&input["cwd"].as_str().map(PathBuf::from).unwrap_or_default())
            })?;
            let name = c.resolve(&path.to_string_lossy())?;
            // Keeps the branch when it has commits its base does not.
            c.remove(
                &name,
                &RemoveOptions {
                    force: false,
                    delete_branch: true,
                },
                &TermReporter::new(false),
            )?;
        }
    }
    Ok(())
}

/// `clove` with no command: the mark, where this repo stands, and what to type next.
fn home(cli: &Cli) -> Result<()> {
    if !std::io::stdout().is_terminal() || cli.json {
        Cli::command().print_help()?;
        return Ok(());
    }
    println!();
    for line in brand::banner_lines() {
        println!("  {line}");
    }
    println!(
        "  {}",
        paint(DIM, "copy-on-write git worktrees, ready in a second")
    );
    println!();
    let hint =
        |cmd: &str, what: &str| println!("  {}  {}", paint(BOLD, format!("{cmd:<22}")), paint(DIM, what));
    match repo(cli) {
        Ok(c) => match c.status() {
            Ok(s) if s.golden.is_some() => {
                let fresh = match &s.freshness {
                    Some(f) if f.refreshing.is_some() => paint(CYAN, "refreshing"),
                    Some(f) if f.stale => paint(YELLOW, "stale"),
                    _ => paint(GREEN, "fresh"),
                };
                println!(
                    "  {}  golden {fresh} · {} clone{} ({} mounted)",
                    paint(BOLD, &s.name),
                    s.clones,
                    if s.clones == 1 { "" } else { "s" },
                    s.mounted
                );
                println!();
                hint("clove new <branch>", "a ready worktree");
                hint("clove ls", "your clones");
                hint("clove ui", "browse and manage them");
            }
            _ => {
                println!(
                    "  {}  {}",
                    paint(BOLD, &c.name),
                    paint(DIM, "no golden image yet")
                );
                println!();
                hint("clove init", "build it once (a few minutes on a big repo)");
                hint("clove doctor", "check the setup first");
            }
        },
        Err(_) => {
            hint("cd <repo> && clove init", "set up a repository");
            hint("clove ui", "browse repos you already set up");
        }
    }
    println!();
    println!("  {}", paint(DIM, "clove --help for every command"));
    Ok(())
}
