mod brand;
mod complete;
mod render;
mod shell;

use anstream::{eprintln, println};
use anyhow::{Context, Result, bail};
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::engine::ArgValueCompleter;
use clove_core::doctor::{self, Level};
use clove_core::opencode::{self, managed_repo};
use clove_core::{Clove, Exec, NewOptions, RemoveOptions, Silent, git};
use render::*;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

const AFTER_HELP: &str = "\
Getting started:
  clove init                 build the golden image for this repo (once, slow)
  clove new feat/login       a ready worktree in ~1s, deps and env included
  clove ls                   list clones
  clove rm feat/login        unmount and delete it
  clove prune                remove clones (and plain git worktrees) whose PR is merged
  clove adopt ../old-wt      turn a plain git worktree into a clone on its branch
  clove repos                every repo clove manages
  clove repos --candidates   repos you could add (then clove init -C <path>)
  clove                      all of the above, interactively (also: clove tui)

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
    /// Build the golden image: checkout of the base branch, warmed. Shows what
    /// it will do and asks first in a terminal.
    Init {
        /// Don't ask; start right away.
        #[arg(short, long)]
        yes: bool,
    },
    /// Create a clone on a branch (new, local, or from origin).
    #[command(visible_alias = "add")]
    New(NewArgs),
    /// List clones, then the repo's other git worktrees (ones clove didn't make).
    #[command(visible_alias = "ls")]
    List {
        /// Skip git status (faster with many clones).
        #[arg(long)]
        fast: bool,
        /// Only the plain git worktrees. With --json, their list (plain `ls
        /// --json` stays the clones' list).
        #[arg(long)]
        worktrees: bool,
    },
    /// Turn a plain git worktree into a clone on the same branch. The worktree
    /// must be clean; it is removed (`git worktree remove`) once the clone exists.
    Adopt {
        /// Path, folder name or branch of the worktree.
        #[arg(add = ArgValueCompleter::new(complete::worktrees))]
        worktree: String,
        /// Clone name (default: the branch with / turned into -).
        #[arg(short, long)]
        name: Option<String>,
        /// Skip setup steps.
        #[arg(long)]
        no_setup: bool,
        /// Stay in the current directory (with shell integration, adopt cds by default).
        #[arg(long)]
        no_cd: bool,
    },
    /// Remove clones: unmount, delete the image and the git registration.
    /// A plain git worktree (path or folder name) goes with `git worktree
    /// remove`; with uncommitted changes only when --force (they're listed).
    /// Locked ones and commits on no branch always stay.
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
    /// A clone's changes against its base: committed, staged, unstaged and
    /// untracked. Pages the patch in a terminal; `--stat` counts per file.
    Diff {
        /// The clone (default: the one you are in).
        #[arg(add = ArgValueCompleter::new(complete::clones))]
        clone: Option<String>,
        /// Changed lines per file instead of the patch.
        #[arg(long)]
        stat: bool,
        /// Counts per package (the nearest package.json, Cargo.toml, go.mod…).
        #[arg(long)]
        packages: bool,
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
        /// Also check which clones' PRs are merged, for `clove ls` (the service uses this).
        #[arg(long, hide = true)]
        prs: bool,
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
    /// Bring clones up to the latest base: fetch it, rebase onto it (or
    /// merge), then rerun the setup steps whose inputs changed. Stops on
    /// conflicts with the files listed; --continue or --abort after.
    Update {
        /// Clones (default: the one you are in).
        #[arg(add = ArgValueCompleter::new(complete::clones))]
        clones: Vec<String>,
        /// Every clone of this repo; one with a conflict doesn't stop the others.
        #[arg(long, conflicts_with = "clones")]
        all: bool,
        /// Merge the base instead of rebasing (default: the `update.strategy` setting).
        #[arg(long, conflicts_with = "rebase")]
        merge: bool,
        /// Rebase onto the base (overrides `update.strategy = "merge"`).
        #[arg(long)]
        rebase: bool,
        /// Set uncommitted changes aside and reapply them after.
        #[arg(long)]
        autostash: bool,
        /// After fixing and `git add`-ing the conflicts: finish the update.
        #[arg(long = "continue", conflicts_with_all = ["abort", "all"])]
        continue_: bool,
        /// Undo a stopped update: the branch goes back to where it was.
        #[arg(long, conflicts_with = "all")]
        abort: bool,
        /// Use the base as last fetched.
        #[arg(long)]
        no_fetch: bool,
        /// On conflicts, start the resolver (`update.resolver`, claude by
        /// default) without asking; on a clone already stopped, start it now.
        #[arg(long, conflicts_with_all = ["abort", "all"])]
        resolve: bool,
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
    /// Remove clones whose pull request is merged (asks GitHub through `gh`),
    /// and plain git worktrees whose PR is merged and that are clean and unlocked.
    /// Lists them unless --apply, or you confirm the prompt.
    Prune {
        /// Remove them and their branches without asking.
        #[arg(long)]
        apply: bool,
    },
    /// Clones and worktrees clove removed: when, by what (rm, prune,
    /// auto-prune, Claude Code, OpenCode), why, and how to get the branch back.
    /// Codex and Pi use the regular clove commands.
    Log {
        /// How many, newest first.
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
        /// Every repo clove manages, not just this one.
        #[arg(long)]
        all_repos: bool,
    },
    /// How fast clones come: median and p90, where the time goes, which
    /// clones reran setup, golden image builds, and the recent creations.
    Stats {
        /// Recent creations listed, newest first.
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,
        /// Every repo clove manages, not just this one.
        #[arg(long)]
        all_repos: bool,
    },
    /// Find leftovers from interrupted operations.
    Gc {
        /// Remove them (default: only list).
        #[arg(long)]
        apply: bool,
    },
    /// Show the resolved config and where it comes from, or edit one layer.
    #[command(args_conflicts_with_subcommands = true)]
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
        /// Write .clove.toml with the current steps so the team can share them.
        #[arg(long)]
        write: bool,
        /// Open a config file in $EDITOR, creating it if missing: you (this repo,
        /// only you), team (.clove.toml) or all (every repo, only you).
        #[arg(long, value_enum, num_args = 0..=1, default_missing_value = "you")]
        edit: Option<Layer>,
    },
    /// Every repo clove manages: clones, golden image state, merged clones.
    Repos {
        /// Instead, git repos on this machine clove doesn't manage yet, most
        /// recently used first (looks in the usual code folders).
        #[arg(long)]
        candidates: bool,
    },
    /// Refresh the PR cache `clove ls` shows (run in the background by ls).
    #[command(hide = true)]
    CheckPrs,
    /// Wait for clones marked "remove when free" and remove them (run in the
    /// background by `clove rm`).
    #[command(hide = true)]
    RemovePending,
    /// Make the spare clone `new` claims, mounted ahead of time (run in the
    /// background after new, init, refresh and at login). macOS.
    #[command(hide = true)]
    Spare {
        /// Detach and delete it instead.
        #[arg(long)]
        discard: bool,
    },
    /// Delete the golden image and clove's state for this repo.
    Deinit,
    /// Background agents: remount clones at login, keep golden images fresh.
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Teach Claude Code, Codex, OpenCode and Pi to use clove: skills, global
    /// instructions, and hooks or extensions where supported.
    Agents {
        #[command(subcommand)]
        action: AgentsAction,
    },
    /// How to use clove, written for coding agents (what `clove agents install` points them to).
    Guide,
    /// Hooks for Claude Code, OpenCode and Pi. Codex uses instructions only.
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
    /// Shell integration (cd into clones, completions). `--install` adds it to
    /// your rc file; without it, prints the code for `eval "$(clove shell-init zsh)"`.
    ShellInit {
        /// Default: your $SHELL.
        shell: Option<shell::Shell>,
        /// Add the line to ~/.zshrc, ~/.bashrc or fish's config.fish.
        #[arg(long)]
        install: bool,
    },
    /// Terminal UI: repos, clones, live progress. Bare `clove` opens it too.
    #[command(alias = "ui")]
    Tui,
    /// Upgrade clove (and the app) with Homebrew, then restart the service.
    /// Other installs get the download link.
    Upgrade {
        /// Only ask GitHub for the latest release and say.
        #[arg(long)]
        check: bool,
        /// A quiet `--check` for the hint other commands print.
        #[arg(long, hide = true, requires = "check")]
        background: bool,
        /// From the app: wait for this pid to quit, upgrade, open the app again.
        #[arg(long, hide = true, value_name = "PID")]
        relaunch_app: Option<u32>,
    },
}

#[derive(Subcommand)]
enum HookAction {
    /// Print the settings.json block that makes Claude Code use clove for worktrees.
    Claude,
    /// Print the OpenCode plugin (worktree strategy and context hint).
    Opencode,
    /// Print the Pi extension (managed-repo prompt hint).
    Pi,
    /// Claude Code WorktreeCreate hook: JSON on stdin, prints the clone path.
    WorktreeCreate,
    /// Claude Code WorktreeRemove hook: JSON on stdin with worktree_path.
    WorktreeRemove,
    #[command(hide = true)]
    Check {
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// OpenCode worktree create: JSON on stdin; prints the resulting directory as JSON.
    OpencodeWorktreeCreate,
    /// OpenCode worktree remove: JSON on stdin with directory and force.
    OpencodeWorktreeRemove,
    /// Roll back only the clone reserved by an interrupted OpenCode create.
    #[command(hide = true)]
    OpencodeWorktreeAbort,
    /// OpenCode worktree list: JSON on stdin; prints clove's worktree inventory as JSON.
    OpencodeWorktreeList,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show each setting, its value and the file it comes from (or one value).
    Get {
        #[arg(value_parser = setting_keys())]
        key: Option<String>,
    },
    /// Set one setting in one config file, keeping the rest of the file as is.
    Set {
        #[arg(value_parser = setting_keys())]
        key: String,
        value: String,
        /// Which file: you (this repo, only you), team (.clove.toml) or all
        /// (every repo, only you).
        #[arg(long, value_enum, default_value = "you")]
        layer: Layer,
    },
    /// Remove a setting from one config file so a lower layer or the default applies.
    Unset {
        #[arg(value_parser = setting_keys())]
        key: String,
        #[arg(long, value_enum, default_value = "you")]
        layer: Layer,
    },
}

fn setting_keys() -> clap::builder::PossibleValuesParser {
    clap::builder::PossibleValuesParser::new(
        clove_core::settings::FIELDS
            .iter()
            .map(|f| clap::builder::PossibleValue::new(f.key).help(f.help)),
    )
}

#[derive(Subcommand)]
enum AgentsAction {
    /// Write the skill, instructions and hooks. Running it again updates them.
    Install,
    /// Take out everything `install` wrote, leaving the rest of each file as it was.
    Remove,
    /// What is installed where.
    Status,
}

#[derive(Subcommand)]
enum ServiceAction {
    #[cfg_attr(target_os = "macos", doc = "Install the launchd agents.")]
    #[cfg_attr(target_os = "linux", doc = "Install the systemd user units.")]
    Install {
        /// Fallback check for repos nobody fetches in, e.g. 1h, 4h. Fetches and
        /// pulls trigger a check right away.
        #[arg(long, default_value = "4h")]
        every: String,
    },
    #[cfg_attr(target_os = "macos", doc = "Remove the launchd agents.")]
    #[cfg_attr(target_os = "linux", doc = "Remove the systemd user units.")]
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
    #[arg(add = ArgValueCompleter::new(complete::removable), required_unless_present = "all")]
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
    /// Plain git worktrees: lift a `git worktree lock` to remove it (the
    /// lock goes back if removal fails).
    #[arg(long)]
    unlock: bool,
}

fn main() {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    let json = cli.json;
    let hint = wants_upgrade_hint(&cli);
    if let Err(e) = run(cli) {
        if json {
            println!("{}", serde_json::json!({ "error": format!("{e:#}") }));
        } else {
            eprintln!("{} {e:#}", paint(RED, "error:"));
        }
        std::process::exit(1);
    }
    if hint && let Some(v) = clove_core::upgrade::hint() {
        eprintln!(
            "{}",
            paint(DIM, format!("clove v{v} is available: clove upgrade"))
        );
    }
}

/// Commands a person typed in a terminal get the "new version" hint; ones
/// that scripts, shells and hooks run, or that print paths, don't.
fn wants_upgrade_hint(cli: &Cli) -> bool {
    let quiet_cmd = matches!(
        cli.cmd,
        None | Some(
            Cmd::Upgrade { .. }
                | Cmd::Hook { .. }
                | Cmd::ShellInit { .. }
                | Cmd::CheckPrs
                | Cmd::RemovePending
                | Cmd::Spare { .. }
                | Cmd::Cd { .. }
                | Cmd::Path { .. }
                | Cmd::Run { .. }
                | Cmd::Guide
                | Cmd::Tui
        )
    );
    !quiet_cmd
        && !cli.json
        && std::env::var_os("CLOVE_NO_UPDATE_CHECK").is_none()
        && std::io::stdin().is_terminal()
        && std::io::stderr().is_terminal()
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
        // In a terminal, the bare command is the TUI (like lazygit, k9s); piped,
        // it prints help. stdout may be captured: `cd "$(clove)"`.
        let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
        // With no repo to show, the TUI opens on its add-repo screen.
        if interactive && !cli.json {
            return run_tui(&cli);
        }
        return home(&cli);
    };
    match cmd {
        Cmd::Init { yes } => {
            let c = repo(&cli).map_err(|e| {
                anyhow::anyhow!(
                    "{e:#}\n  `clove init -C <dir>` sets up another repo; `clove repos --candidates` lists them"
                )
            })?;
            let preview = clove_core::preview::preview_of(&c, doctor::machine());
            if !cli.json {
                print_plan(&preview);
            }
            let interactive = !cli.json && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
            let asking = interactive && !yes && !preview.initialized;
            if !cli.json && !asking {
                eprintln!();
            }
            if asking {
                let go = ask(
                    "Build the golden image now?",
                    &["Runs the warm steps once; every clone after it takes about a second.".to_string()],
                    true,
                )?;
                if !go {
                    eprintln!("{} nothing changed", paint(DIM, "·"));
                    return Ok(());
                }
            }
            fstab_prompt(&c, cli.json)?;
            let meta = c.init(r)?;
            c.spawn_spare_fill();
            if cli.json {
                return emit(&meta);
            }
            timing_bars(&meta.timings);
            eprintln!(
                "\n{} golden image ready in {}. Next: {}",
                paint(GREEN, "●"),
                paint(BOLD, secs(total_ms(&meta.timings))),
                paint(BOLD, "clove new <branch>")
            );
            after_init()?;
        }
        Cmd::Refresh {
            all_repos,
            if_stale,
            full,
            prs,
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
                if *prs {
                    // Background only: unmounted clones are not mounted to look inside.
                    if c.config.prune.auto {
                        // Checks too, and records each removal in the history.
                        match c.auto_prune(r) {
                            Ok(gone) if !gone.is_empty() && !cli.json => {
                                eprintln!("{} auto-pruned {}", paint(GREEN, "✓"), gone.join(", "))
                            }
                            _ => {}
                        }
                        // Held ones (an open TUI or app) were marked; they go once let go.
                        if !c.pending_removals().is_empty() {
                            spawn_pending_waiter(c);
                        }
                    } else {
                        let _ = c.merge_checks(false);
                    }
                }
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
                        c.spawn_spare_fill();
                        if !cli.json {
                            timing_bars(&meta.timings);
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
        Cmd::Update {
            clones,
            all,
            merge,
            rebase,
            autostash,
            continue_,
            abort,
            no_fetch,
            resolve,
        } => {
            let strategy = if *merge {
                Some(clove_core::update::Strategy::Merge)
            } else if *rebase {
                Some(clove_core::update::Strategy::Rebase)
            } else {
                None
            };
            let mode = if *continue_ {
                UpdateMode::Continue
            } else if *abort {
                UpdateMode::Abort
            } else {
                UpdateMode::Run
            };
            cmd_update(
                &cli, clones, *all, mode, strategy, *autostash, !*no_fetch, *resolve, r,
            )?;
        }
        Cmd::Diff {
            clone,
            stat,
            packages,
        } => {
            let c = repo(&cli)?;
            let name = c.resolve(clone.as_deref().unwrap_or("."))?;
            cmd_diff(&cli, &c, &name, *stat, *packages)?;
        }
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
        Cmd::List { fast, worktrees } => cmd_list(&cli, !fast, *worktrees)?,
        Cmd::Adopt {
            worktree,
            name,
            no_setup,
            no_cd,
        } => cmd_adopt(&cli, worktree, name.clone(), *no_setup, *no_cd, r)?,
        Cmd::Remove(a) => {
            let c = repo(&cli)?;
            // Clones first; what isn't one may be a plain git worktree.
            let mut plain = vec![];
            let names = if a.all {
                c.clone_names()?
            } else {
                let mut names = vec![];
                for q in &a.clones {
                    match c.resolve(q) {
                        Ok(n) => names.push(n),
                        Err(e) => match c.resolve_worktree(q) {
                            Ok(w) => plain.push(w),
                            Err(_) => return Err(e),
                        },
                    }
                }
                names
            };
            let mut removed_worktrees = vec![];
            for w in &plain {
                if !cli.json {
                    eprintln!(
                        "{} {} {}",
                        paint(MAGENTA, "−"),
                        paint(BOLD, &w.name),
                        paint(DIM, "(plain git worktree: git worktree remove)")
                    );
                }
                let opts = RemoveOptions {
                    force: a.force,
                    delete_branch: a.delete_branch,
                    branch_merged: false,
                    unlock: a.unlock,
                    ..Default::default()
                };
                if !cli.json && w.locked && a.unlock {
                    let why = w.lock_reason.as_deref().unwrap_or("no reason given");
                    eprintln!("  {}", paint(DIM, format!("unlocking (locked: {why})")));
                }
                if !cli.json && w.dirty == Some(true) && !w.changes.is_empty() {
                    let verb = if a.force { "discarding" } else { "uncommitted" };
                    eprintln!("  {}", paint(DIM, format!("{verb}:")));
                    for l in &w.changes {
                        eprintln!("    {}", paint(DIM, l));
                    }
                    if w.changes_total > w.changes.len() {
                        eprintln!(
                            "    {}",
                            paint(DIM, format!("and {} more", w.changes_total - w.changes.len()))
                        );
                    }
                }
                c.remove_worktree(&w.path, &opts, r)?;
                if std::env::current_dir().is_ok_and(|d| d.starts_with(&w.path)) {
                    shell::request_cd(&c.root);
                }
                removed_worktrees.push(w.path.clone());
            }
            // The shell's cwd keeps a volume busy. With shell integration, the
            // wrapper cds to the main checkout and then removes that clone.
            let cwd = std::env::current_dir().ok();
            let inside = |n: &String| cwd.as_ref().is_some_and(|d| d.starts_with(c.clone_mount(n)));
            let mut removed = vec![];
            let mut when_free = std::collections::BTreeMap::new();
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
                    // No shell integration (an agent, a script): the clone
                    // is busy, so it goes once whatever runs in it exits.
                }
                if !cli.json {
                    eprintln!("{} {}", paint(MAGENTA, "−"), paint(BOLD, n));
                }
                let res = c.remove_or_defer(
                    n,
                    &RemoveOptions {
                        force: a.force,
                        delete_branch: a.delete_branch,
                        branch_merged: false,
                        unlock: false,
                        ..Default::default()
                    },
                    r,
                )?;
                match res {
                    clove_core::Removal::Removed => {
                        if inside(n) {
                            shell::request_cd(&c.root);
                        }
                        removed.push(n.clone());
                    }
                    clove_core::Removal::WhenFree(users) => {
                        if !cli.json {
                            eprintln!(
                                "  {} in use by {}: removing it as soon as {} exit{} (--force to pull it now)",
                                paint(CYAN, "↻"),
                                users.join(", "),
                                if users.len() == 1 { "it" } else { "they" },
                                if users.len() == 1 { "s" } else { "" },
                            );
                        }
                        when_free.insert(n.clone(), users);
                    }
                }
            }
            if !when_free.is_empty() {
                spawn_pending_waiter(&c);
            }
            if cli.json {
                return emit(&serde_json::json!({
                    "removed": removed,
                    "when_free": when_free,
                    "removed_worktrees": removed_worktrees,
                }));
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
            } else if shell::outdated() {
                hint_shell_reload();
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
                // At login (--all-repos) spares are gone with the reboot.
                c.spawn_spare_fill();
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
            if clones.is_empty() && c.shared_volume() {
                c.unmount_volume(*force)?;
                let root = c.clone_root();
                if cli.json {
                    return emit(&serde_json::json!({ "unmounted": [root] }));
                }
                eprintln!("  {} {}", paint(DIM, "○"), tilde(&root));
                return Ok(());
            }
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
        Cmd::Prune { apply } => cmd_prune(&cli, *apply, r)?,
        Cmd::Log { limit, all_repos } => cmd_log(&cli, *limit, *all_repos)?,
        Cmd::Stats { limit, all_repos } => cmd_stats(&cli, *limit, *all_repos)?,
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
                eprintln!("\nrun {} to clean these up", paint(BOLD, "clove gc --apply"));
            }
        }
        Cmd::Repos { candidates: true } => cmd_candidates(&cli)?,
        Cmd::Repos { candidates: false } => cmd_repos(&cli)?,
        Cmd::CheckPrs => {
            repo(&cli)?.merge_checks(false)?;
        }
        Cmd::RemovePending => {
            repo(&cli)?.wait_and_remove_pending(std::time::Duration::from_secs(12 * 3600), &Silent)?;
        }
        Cmd::Spare { discard } => {
            let c = repo(&cli)?;
            if *discard {
                c.discard_spare(false)?;
            } else {
                c.fill_spare(true)?;
            }
            let state = format!("{:?}", c.spare_state());
            if cli.json {
                return emit(&serde_json::json!({ "spare": state }));
            }
            eprintln!("{} spare: {state}", paint(DIM, "·"));
        }
        Cmd::Config {
            action: Some(action), ..
        } => cmd_config_action(&cli, action)?,
        Cmd::Config {
            action: None,
            write,
            edit,
        } => {
            let c = repo(&cli)?;
            if let Some(layer) = edit {
                let paths = clove_core::config::layer_paths(&c.root, &c.name);
                let path = &paths[*layer as usize].1;
                clove_core::config::ensure_file(path)?;
                edit_file(path)?;
                // Parse it back so a typo shows now, not at the next `clove new`.
                Clove::open(&c.root).with_context(|| format!("{} has a problem", tilde(path)))?;
                eprintln!("{} {}", paint(GREEN, "✓"), tilde(path));
                if layer_is_team(*layer) {
                    eprintln!("{}", paint(DIM, "commit .clove.toml to share it with the team"));
                }
                return Ok(());
            }
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
                // Where settings come from, then the resolved result as TOML.
                eprintln!("{}", paint(DIM, "config files, later ones win:"));
                let layers = clove_core::config::layer_paths(&c.root, &c.name);
                let pw = layers
                    .iter()
                    .map(|(_, p)| tilde(p).chars().count())
                    .max()
                    .unwrap_or(0);
                for ((label, path), flag) in layers.iter().zip(["--edit all", "--edit team", "--edit"]) {
                    let mark = if path.exists() {
                        paint(GREEN, "●")
                    } else {
                        paint(DIM, "○")
                    };
                    eprintln!(
                        "  {mark} {:<15} {:<pw$}  {}",
                        label,
                        tilde(path),
                        paint(DIM, format!("clove config {flag}"))
                    );
                }
                let svc = clove_core::service::installed().iter().all(|(_, on)| *on);
                let shell = std::env::var_os("CLOVE_CD_FILE").is_some();
                eprintln!(
                    "{}",
                    paint(
                        DIM,
                        format!(
                            "service {} · shell integration {} · state {}\n",
                            if svc { "on" } else { "off (clove service install)" },
                            if shell {
                                "on"
                            } else {
                                "off (clove shell-init --install)"
                            },
                            tilde(&clove_core::state_home())
                        )
                    )
                );
                print!("{}", c.config.to_repo_toml());
                if c.config.steps_detected {
                    eprintln!(
                        "{}",
                        paint(
                            DIM,
                            "# steps detected; `clove config --write` saves them to .clove.toml"
                        )
                    );
                }
            }
        }
        Cmd::Deinit => {
            let c = repo(&cli)?;
            c.deinit(r)?;
            clove_core::service::sync()?;
            if cli.json {
                return emit(&serde_json::json!({ "deinit": c.name }));
            }
        }
        Cmd::Service { action } => match action {
            ServiceAction::Install { every } => {
                let secs = clove_core::config::parse_duration(every)?.max(300);
                for p in clove_core::service::install(secs)? {
                    eprintln!("{} {}", paint(GREEN, "✓"), tilde(&p));
                }
                eprintln!(
                    "clones remount at login; stale golden images refresh when you fetch or pull, and every {} otherwise",
                    clove_core::util::human_age(secs)
                );
            }
            ServiceAction::Uninstall => {
                for p in clove_core::service::uninstall()? {
                    eprintln!("{} removed {}", paint(GREEN, "✓"), tilde(&p));
                }
            }
            ServiceAction::Status => {
                let st = clove_core::service::installed();
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
        Cmd::Guide => print!("{}", clove_core::agents::GUIDE),
        Cmd::Agents { action } => cmd_agents(&cli, action)?,
        Cmd::ShellInit { shell: sh, install } => {
            let sh = sh
                .or_else(shell::detect)
                .context("can't tell your shell from $SHELL; pass zsh, bash or fish")?;
            if *install {
                let (path, changed) = shell::install(sh)?;
                if changed {
                    eprintln!("{} added to {}", paint(GREEN, "✓"), tilde(&path));
                    eprintln!("open a new terminal (or `source {}`) to use it", tilde(&path));
                } else {
                    eprintln!("{} already in {}", paint(GREEN, "✓"), tilde(&path));
                }
            } else {
                print!("{}", shell::init(sh));
            }
        }
        Cmd::Tui => run_tui(&cli)?,
        Cmd::Upgrade {
            check,
            background,
            relaunch_app,
        } => cmd_upgrade(&cli, *check, *background, *relaunch_app, r)?,
    }
    Ok(())
}

fn cmd_upgrade(
    cli: &Cli,
    check: bool,
    background: bool,
    relaunch_app: Option<u32>,
    r: &dyn clove_core::Reporter,
) -> Result<()> {
    use clove_core::upgrade;
    if background {
        upgrade::background_check();
        return Ok(());
    }
    if check {
        let st = upgrade::check(true);
        if cli.json {
            return emit(&st);
        }
        if let Some(e) = &st.error {
            bail!("couldn't ask GitHub for the latest release: {e}");
        }
        match &st.latest {
            Some(l) if st.available => {
                eprintln!(
                    "{} clove v{} is available (you have {})",
                    paint(YELLOW, "↑"),
                    l.version,
                    st.current
                );
                eprintln!("  {}", paint(DIM, &l.url));
                if st.cli == upgrade::Source::Brew {
                    eprintln!("  upgrade: {}", paint(BOLD, "clove upgrade"));
                }
            }
            _ => eprintln!("{} clove {} is up to date", paint(GREEN, "✓"), st.current),
        }
        return Ok(());
    }
    let opts = upgrade::UpgradeOptions { relaunch_app };
    let up = upgrade::upgrade(r, &opts)?;
    if cli.json {
        return emit(&up);
    }
    match up.to.as_deref() {
        Some(to) if upgrade::newer(to, &up.from) => {
            eprintln!("{} clove {} → {to}", paint(GREEN, "✓"), up.from);
        }
        Some(to) => eprintln!("{} clove {to}: no newer CLI in the tap", paint(GREEN, "✓")),
        None => eprintln!("{} upgraded", paint(GREEN, "✓")),
    }
    if up.app && relaunch_app.is_none() {
        eprintln!(
            "  {}",
            paint(
                DIM,
                "the app was upgraded too; it opens on the new version next time"
            )
        );
    }
    Ok(())
}

/// First-run setup the golden image alone doesn't cover: shell integration and
/// the background service. Asks only in an interactive terminal, so scripts and
/// agents get a hint instead of a prompt.
fn after_init() -> Result<()> {
    let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if !shell::active() {
        match shell::detect() {
            Some(sh) if interactive => {
                let yes = ask(
                    "Let clove cd you into new clones and set up their environment?",
                    &[
                        "Inside a clone, your shell gets the clone's environment (e.g. pnpm's store), so `pnpm i` reuses what's installed.".to_string(),
                        format!("Adds one line to {}:", tilde(&shell::rc_path(sh))),
                        shell::rc_line(sh).to_string(),
                    ],
                    true,
                )?;
                if yes {
                    let (path, _) = shell::install(sh)?;
                    eprintln!(
                        "{} added; open a new terminal (or `source {}`)",
                        paint(GREEN, "✓"),
                        tilde(&path)
                    );
                } else {
                    eprintln!("{} skipped; `clove shell-init --install` later", paint(DIM, "·"));
                }
            }
            _ => eprintln!("{} {}", paint(DIM, "·"), shell::HINT),
        }
    }
    if clove_core::service::installed().iter().all(|(_, on)| *on) {
        // Add this repo's FETCH_HEAD to the watch list.
        clove_core::service::sync()?;
        return Ok(());
    }
    if !interactive {
        eprintln!(
            "{} run {} to refresh stale golden images when you pull and remount clones at login",
            paint(DIM, "·"),
            paint(BOLD, "clove service install")
        );
        return Ok(());
    }
    let yes = ask(
        "Keep golden images fresh in the background?",
        &[
            "Refreshes a stale golden image when you fetch or pull, and every 4h.".to_string(),
            format!(
                "Remounts clones after a reboot. Installs {}.",
                clove_core::service::AGENTS
            ),
        ],
        true,
    )?;
    if yes {
        for p in clove_core::service::install(clove_core::service::DEFAULT_EVERY)? {
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

/// Linux: offers the /etc/fstab line that lets clove mount the repo's image
/// without sudo. Asks only in an interactive terminal; otherwise prints it.
fn fstab_prompt(c: &Clove, json: bool) -> Result<()> {
    let Some(line) = c.missing_fstab_line() else {
        return Ok(());
    };
    let cmd = format!("printf '\\n%s\\n' '{line}' | sudo tee -a /etc/fstab");
    let interactive = !json && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if !interactive {
        eprintln!(
            "{} to mount without sudo, add this line to /etc/fstab:\n  {line}\n  {}",
            paint(DIM, "·"),
            paint(BOLD, &cmd)
        );
        return Ok(());
    }
    let yes = ask(
        "Mount clove's image without sudo?",
        &[
            "Adds one line to /etc/fstab (asks for your password once):".to_string(),
            line.clone(),
        ],
        true,
    )?;
    if yes {
        match c.add_fstab_line(&line) {
            Ok(()) => eprintln!("{} added to /etc/fstab: {line}", paint(GREEN, "✓")),
            Err(e) => eprintln!("{} {e:#}; later: {}", paint(YELLOW, "!"), paint(BOLD, &cmd)),
        }
    } else {
        eprintln!(
            "{} skipped; clove will mount through udisks (needs a desktop session). Later: {}",
            paint(DIM, "·"),
            paint(BOLD, &cmd)
        );
    }
    Ok(())
}

fn print_plan(p: &clove_core::InitPreview) {
    eprintln!(
        "{} {}  {}",
        paint(BOLD, "clove init"),
        p.name,
        paint(DIM, tilde(&p.root))
    );
    if p.initialized {
        eprintln!(
            "  {}",
            paint(DIM, "already has a golden image; `clove refresh` updates it")
        );
        eprintln!();
        return;
    }
    if p.base_found {
        eprintln!("  base    {}", p.base);
    } else {
        eprintln!(
            "  base    {} {}",
            p.base,
            paint(YELLOW, "(not found yet; init fetches it)")
        );
    }
    if p.steps.is_empty() {
        eprintln!(
            "  steps   {}",
            paint(
                DIM,
                "none detected: clones get the checkout only (add [[step]] to .clove.toml)"
            )
        );
    } else {
        let src = if p.steps_detected {
            "detected"
        } else {
            "from config"
        };
        eprintln!("  steps   {}", paint(DIM, src));
        let w = p.steps.iter().map(|s| s.name.len()).max().unwrap_or(0);
        for s in &p.steps {
            let cmd = s.warm.as_deref().unwrap_or("(setup only)");
            eprintln!("    {:<w$}  {}", s.name, paint(DIM, cmd));
        }
    }
    if !p.nested_skipped.is_empty() {
        let shown: Vec<_> = p.nested_skipped.iter().take(4).map(String::as_str).collect();
        let more = p.nested_skipped.len() - shown.len();
        eprintln!(
            "  nested  {} projects not warmed: {}{}",
            p.nested_skipped.len(),
            shown.join(", "),
            if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            }
        );
    }
    if !p.copy.is_empty() {
        eprintln!("  copy    {}", p.copy.join(", "));
    }
    if p.exec != Exec::None {
        eprintln!("  env     {}", format!("{:?}", p.exec).to_lowercase());
    }
    eprintln!("  clones  {}", tilde(&p.clone_root.join("<name>")));
    eprintln!(
        "  image   {} {}",
        tilde(&p.state),
        paint(DIM, format!("(sparse, up to {})", p.image_size))
    );
    for t in &p.missing_tools {
        eprintln!("  {} {t} isn't on PATH; its step will fail", paint(YELLOW, "!"));
    }
    for m in &p.machine {
        let color = if m.level == Level::Fail { RED } else { YELLOW };
        eprintln!("  {} {}: {}", paint(color, "!"), m.name, m.detail);
    }
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
    c.spawn_spare_fill();
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
    if !c.pending_removals().is_empty() {
        spawn_pending_waiter(&c);
    }
    if !a.no_cd && !cli.json {
        if !shell::request_cd(&info.path) {
            if std::io::stdout().is_terminal() {
                hint_shell_init();
            }
        } else if shell::outdated() {
            hint_shell_reload();
        }
    }
    Ok(())
}

/// Starts `clove refresh --if-stale` in the background when the golden image is
/// stale, so the next clone starts from a fresh one. Never blocks `new`.
/// Starts a detached waiter, outside any clone, that removes clones marked
/// "remove when free" once nothing uses them. One per repo (it takes a lock).
fn spawn_pending_waiter(c: &Clove) {
    let Ok(exe) = std::env::current_exe() else { return };
    let log = c.logs_dir().join("remove-pending.log");
    let _ = std::fs::create_dir_all(c.logs_dir());
    let out = std::fs::File::create(&log).ok();
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("-C")
        .arg(&c.root)
        .arg("remove-pending")
        // Its own cwd must not hold the clone it waits for.
        .current_dir(&c.root)
        .stdin(std::process::Stdio::null())
        .process_group(0);
    if let Some(o) = out {
        if let Ok(e) = o.try_clone() {
            cmd.stderr(e);
        }
        cmd.stdout(o);
    }
    let _ = cmd.spawn();
}

/// When the PR cache is older than a few minutes, refresh it in a detached
/// process so the next `clove ls` shows merges other tools made.
fn maybe_background_pr_check(c: &Clove) {
    // A waiter that died (reboot) is restarted by the next command.
    if !c.pending_removals().is_empty() {
        spawn_pending_waiter(c);
    }
    let fresh = c
        .prune_cache()
        .is_some_and(|p| clove_core::util::now_unix().saturating_sub(p.checked_at) < 300);
    let nothing = c.clone_names().map(|n| n.is_empty()).unwrap_or(true)
        && c.other_worktrees(false).map(|w| w.is_empty()).unwrap_or(true);
    if fresh || nothing {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    use std::os::unix::process::CommandExt;
    let _ = std::process::Command::new(exe)
        .arg("-C")
        .arg(&c.root)
        .arg("check-prs")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn();
}

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

/// This shell loaded clove's integration before an upgrade: cd works, but
/// newer parts (each clone's environment) need a fresh shell.
fn hint_shell_reload() {
    eprintln!(
        "  {} {}",
        paint(YELLOW, "!"),
        paint(
            YELLOW,
            "this shell has clove's integration from an older version, so the clone's environment won't load: open a new terminal (or run `exec $SHELL`)"
        )
    );
}

fn hint_shell_init() {
    eprintln!(
        "{}",
        paint(
            DIM,
            "tip: run `clove shell-init --install` so clove can cd for you (and complete names)"
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

fn cmd_list(cli: &Cli, status: bool, only_worktrees: bool) -> Result<()> {
    let c = repo(cli)?;
    let others = c.other_worktrees(status)?;
    if only_worktrees {
        maybe_background_pr_check(&c);
        if cli.json {
            return emit(&others);
        }
        if others.is_empty() {
            eprintln!("no other git worktrees");
        }
        print_worktrees(&others, status, false);
        return Ok(());
    }
    let clones = c.list(status)?;
    maybe_background_pr_check(&c);
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
        print_worktrees(&others, status, true);
        print_recent_auto_prunes(&c);
        return Ok(());
    }
    // PRs from the last `clove prune` or background check; never asks GitHub here.
    let prs: std::collections::BTreeMap<String, clove_core::MergeCheck> = c
        .prune_cache()
        .map(|p| p.checks.into_iter().map(|m| (m.clone.clone(), m)).collect())
        .unwrap_or_default();
    let mut headers = vec!["", "NAME", "BRANCH", "CHANGES"];
    if !prs.is_empty() {
        headers.push("PR");
    }
    headers.extend(["AGE", "OWN", "PATH"]);
    let mut t = Table::new(&headers);
    let mut prunable = 0;
    let mut broken = 0;
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
        let changes = if let Some(issue) = &i.issue {
            broken += 1;
            styled(YELLOW, issue.clone())
        } else if let Some(op) = i.operation {
            styled(YELLOW, format!("{}: conflicts", op.word()))
        } else if i.removing_when_free {
            styled(CYAN, "removing when free")
        } else if changes.is_empty() && i.mounted && status {
            styled(DIM, "clean")
        } else if !i.mounted {
            styled(DIM, "-")
        } else {
            styled(YELLOW, changes.trim().to_string())
        };
        let mut row = vec![dot, styled(BOLD, i.name.clone()), branch, changes];
        if !prs.is_empty() {
            let m = prs.get(&i.name);
            if m.is_some_and(|m| m.verdict == clove_core::Verdict::Prune) {
                prunable += 1;
            }
            row.push(match m.and_then(|m| m.pr.as_ref()) {
                Some(p) => {
                    let (style, word) = match p.state {
                        clove_core::PrState::Merged => (GREEN, "merged"),
                        clove_core::PrState::Open => (CYAN, "open"),
                        clove_core::PrState::Closed => (DIM, "closed"),
                    };
                    styled(style, format!("#{} {word}", p.number))
                }
                None => styled(DIM, "-"),
            });
        }
        row.extend([
            cell(clove_core::util::human_age(now.saturating_sub(i.created_at))),
            cell(clove_core::util::human_bytes(i.own_bytes)),
            styled(DIM, tilde(&i.path)),
        ]);
        t.row(row);
    }
    t.print();
    print_worktrees(&others, status, true);
    if broken > 0 {
        eprintln!(
            "\n{} {broken} clone{} broken outside clove: {} finishes removing {}",
            paint(YELLOW, "!"),
            if broken == 1 { " was" } else { "s were" },
            paint(BOLD, "clove gc --apply"),
            if broken == 1 { "it" } else { "them" }
        );
    }
    if prunable > 0 {
        eprintln!(
            "\n{} {prunable} merged clone{} can go: {}",
            paint(GREEN, "✓"),
            if prunable == 1 { "" } else { "s" },
            paint(BOLD, "clove prune")
        );
    }
    print_recent_auto_prunes(&c);
    Ok(())
}

/// Clones the service removed in the last day, so nobody wonders where they went.
fn print_recent_auto_prunes(c: &Clove) {
    let now = clove_core::util::now_unix();
    let recent: Vec<_> = c
        .history()
        .into_iter()
        .take_while(|e| now.saturating_sub(e.at) < 86_400)
        .filter(|e| e.by == clove_core::history::By::AutoPrune)
        .collect();
    if recent.is_empty() {
        return;
    }
    eprintln!();
    for e in &recent {
        eprintln!("{} {}", paint(YELLOW, "−"), paint(DIM, e.summary(now)));
    }
    eprintln!("{}", paint(DIM, "branches and how to get them back: clove log"));
}

/// The "other git worktrees" section: dimmer than clones, after them.
fn print_worktrees(others: &[clove_core::OtherWorktree], status: bool, heading: bool) {
    if others.is_empty() {
        return;
    }
    if heading {
        eprintln!(
            "\n{} {}",
            paint(DIM, "other git worktrees"),
            paint(
                DIM,
                "(not clones: `clove adopt` makes one a clone, `clove rm` removes it)"
            )
        );
    }
    let mut t = Table::new(&["", "NAME", "BRANCH", "STATE", "PR", "PATH"]);
    for w in others {
        let branch = match &w.branch {
            Some(b) => styled(MAGENTA, b.clone()),
            None => styled(DIM, format!("({})", w.head.as_deref().unwrap_or("detached"))),
        };
        let mut state = vec![];
        if w.missing {
            state.push("missing".to_string());
        }
        if w.locked {
            state.push(match &w.lock_reason {
                Some(r) => format!("locked: {r}"),
                None => "locked".into(),
            });
        }
        if w.dirty == Some(true) {
            state.push("dirty".into());
        }
        if let Some(n) = w.detached_commits.filter(|n| *n > 0) {
            state.push(format!(
                "{n} commit{} on no branch",
                if n == 1 { "" } else { "s" }
            ));
        }
        let state = if state.is_empty() {
            styled(DIM, if status { "clean" } else { "-" })
        } else {
            styled(YELLOW, state.join(", "))
        };
        let pr = match &w.pr {
            Some(p) => {
                let (style, word) = match p.state {
                    clove_core::PrState::Merged => (GREEN, "merged"),
                    clove_core::PrState::Open => (CYAN, "open"),
                    clove_core::PrState::Closed => (DIM, "closed"),
                };
                styled(style, format!("#{} {word}", p.number))
            }
            None => styled(DIM, "-"),
        };
        t.row(vec![
            styled(DIM, "◇"),
            cell(w.name.clone()),
            branch,
            state,
            pr,
            styled(DIM, tilde(&w.path)),
        ]);
    }
    t.print();
}

fn cmd_adopt(
    cli: &Cli,
    query: &str,
    name: Option<String>,
    no_setup: bool,
    no_cd: bool,
    r: &dyn clove_core::Reporter,
) -> Result<()> {
    let c = repo(cli)?;
    let w = c.resolve_worktree(query)?;
    if !cli.json {
        eprintln!(
            "{} {} {}",
            paint(GREEN, "+"),
            paint(BOLD, &w.name),
            paint(DIM, format!("(git worktree at {}) → clone", tilde(&w.path)))
        );
    }
    let was_inside = std::env::current_dir().is_ok_and(|d| d.starts_with(&w.path));
    let info = c.adopt(&w.path, &clove_core::AdoptOptions { name, no_setup }, r)?;
    c.spawn_spare_fill();
    if cli.json {
        return emit(&info);
    }
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
    // The old folder is gone; a shell standing in it follows the branch.
    if (!no_cd || was_inside) && !shell::request_cd(&info.path) && std::io::stdout().is_terminal() {
        hint_shell_init();
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum UpdateMode {
    Run,
    Continue,
    Abort,
}

#[allow(clippy::too_many_arguments)]
fn cmd_update(
    cli: &Cli,
    clones: &[String],
    all: bool,
    mode: UpdateMode,
    strategy: Option<clove_core::update::Strategy>,
    autostash: bool,
    fetch: bool,
    resolve: bool,
    r: &dyn clove_core::Reporter,
) -> Result<()> {
    use clove_core::update::{Update, UpdateOptions};
    let c = repo(cli)?;
    // `--resolve` on a clone already stopped on conflicts: straight to the resolver.
    if resolve && matches!(mode, UpdateMode::Run) && clones.len() <= 1 {
        let name = c.resolve(clones.first().map_or(".", String::as_str))?;
        if c.info(&name, false)?.operation.is_some() {
            return run_resolver(&c, &name);
        }
    }
    let names: Vec<String> = if all {
        c.clone_names()?
    } else if clones.is_empty() {
        vec![c.resolve(".")?]
    } else {
        clones.iter().map(|q| c.resolve(q)).collect::<Result<_>>()?
    };
    // One fetch for all of them.
    if fetch && matches!(mode, UpdateMode::Run) && names.len() > 1 {
        c.fetch()?;
    }
    let opts = UpdateOptions {
        strategy,
        autostash,
        fetch: fetch && names.len() == 1,
    };
    let mut results = vec![];
    let mut trouble = false;
    for name in &names {
        if !cli.json && names.len() > 1 {
            eprintln!("{} {}", paint(BOLD, "update"), name);
        }
        if let UpdateMode::Abort = mode {
            match c.update_abort(name) {
                Ok(()) => {
                    if !cli.json {
                        eprintln!(
                            "{} {name}: back where it was before the update",
                            paint(GREEN, "✓")
                        );
                    }
                    results.push(serde_json::json!({ "clone": name, "aborted": true }));
                }
                Err(e) => {
                    trouble = true;
                    if !cli.json {
                        eprintln!("{} {name}: {e:#}", paint(RED, "✗"));
                    }
                    results.push(serde_json::json!({ "clone": name, "error": format!("{e:#}") }));
                }
            }
            continue;
        }
        let res = match mode {
            UpdateMode::Continue => c.update_continue(name, r),
            _ => c.update(name, &opts, r),
        };
        match res {
            Ok(u) => {
                let conflict = matches!(u, Update::Conflict { .. });
                if !cli.json {
                    print_update(name, &u);
                }
                results.push(serde_json::json!({ "clone": name, "update": u }));
                // Offered only to a person at a terminal, one clone at a time.
                let interactive =
                    !cli.json && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
                if conflict && names.len() == 1 && interactive && c.config.update.resolver.is_some() {
                    let who =
                        clove_core::update::resolver_name(c.config.update.resolver.as_deref().unwrap_or(""));
                    if resolve || ask(&format!("Resolve with {who}?"), &[], false)? {
                        return run_resolver(&c, name);
                    }
                }
                if conflict {
                    trouble = true;
                }
            }
            Err(e) => {
                trouble = true;
                if !cli.json {
                    eprintln!("{} {name}: {e:#}", paint(RED, "✗"));
                }
                results.push(serde_json::json!({ "clone": name, "error": format!("{e:#}") }));
            }
        }
    }
    if cli.json {
        emit(&results)?;
    }
    if trouble {
        std::process::exit(1);
    }
    Ok(())
}

/// Runs the resolver in the clone, in this terminal, then says where the
/// update stands: finished, or still stopped on conflicts.
fn run_resolver(c: &Clove, name: &str) -> Result<()> {
    let (who, mut cmd) = c.resolver_command(name)?;
    eprintln!("{} starting {who} in {name}", paint(MAGENTA, "→"));
    let status = cmd.status().with_context(|| format!("starting {who}"))?;
    match c.info(name, false)?.operation {
        None => {
            eprintln!("{} {name}: update finished", paint(GREEN, "✓"));
            Ok(())
        }
        Some(op) => {
            eprintln!(
                "{} {name}: the {} is still stopped on conflicts ({who} exited {status}); fix them, `git add` them, then {} (or {})",
                paint(YELLOW, "!"),
                op.word(),
                paint(BOLD, format!("clove update --continue {name}")),
                format!("clove update --abort {name}")
            );
            std::process::exit(1);
        }
    }
}

fn print_update(name: &str, u: &clove_core::update::Update) {
    use clove_core::update::Update;
    match u {
        Update::UpToDate { base } => eprintln!("{} {name}: up to date with {base}", paint(GREEN, "✓")),
        Update::Updated {
            base,
            strategy,
            commits,
            hint,
            failed_steps,
        } => {
            let what = match strategy {
                clove_core::update::Strategy::Rebase => "rebased onto",
                clove_core::update::Strategy::Merge => "merged",
            };
            let n = commits
                .map(|n| format!(" ({n} new commit{})", if n == 1 { "" } else { "s" }))
                .unwrap_or_default();
            eprintln!("{} {name}: {what} {base}{n}", paint(GREEN, "✓"));
            if !failed_steps.is_empty() {
                eprintln!(
                    "  {} setup failed: {}",
                    paint(YELLOW, "!"),
                    failed_steps.join(", ")
                );
            }
            if let Some(h) = hint {
                eprintln!("  {}", paint(DIM, h));
            }
        }
        Update::Conflict { strategy, files, .. } => {
            eprintln!(
                "{} {name}: {} stopped on conflicts in {} file{}:",
                paint(YELLOW, "!"),
                strategy.word(),
                files.len(),
                if files.len() == 1 { "" } else { "s" }
            );
            for f in files {
                eprintln!("    {f}");
            }
            eprintln!(
                "  fix them and `git add` them, then {} (or {}); {} hands them to Claude",
                paint(BOLD, format!("clove update --continue {name}")),
                format!("clove update --abort {name}"),
                format!("clove update --resolve {name}")
            );
        }
    }
}

/// `clove stats`: per repo, how long clones take and where the time goes.
fn cmd_stats(cli: &Cli, limit: usize, all_repos: bool) -> Result<()> {
    use clove_core::stats::{FAST_MS, RERAN_MS};
    let repos: Vec<Clove> = if all_repos {
        clove_core::known_repos()
            .into_iter()
            .filter_map(|r| Clove::open(&r.root).ok())
            .collect()
    } else {
        vec![repo(cli)?]
    };
    if cli.json {
        let v: Vec<serde_json::Value> = repos
            .iter()
            .map(|c| {
                let mut created = c.created();
                created.truncate(limit);
                serde_json::json!({
                    "repo": c.name,
                    "root": c.root,
                    "stats": c.stats(),
                    "created": created,
                    "golden_builds": c.golden_builds(),
                })
            })
            .collect();
        return emit(&v);
    }
    let now = clove_core::util::now_unix();
    for (i, c) in repos.iter().enumerate() {
        if i > 0 {
            println!();
        }
        let created = c.created();
        let s = clove_core::stats::stats(&created, &c.golden_builds());
        if s.clones == 0 {
            println!("{}  {}", paint(BOLD, &c.name), paint(DIM, "no clones made yet"));
            continue;
        }
        println!(
            "{}  {} clone{} · median {} · p90 {} · {} of {} under {}",
            paint(BOLD, &c.name),
            s.clones,
            if s.clones == 1 { "" } else { "s" },
            paint(BOLD, secs(s.median_ms)),
            secs(s.p90_ms),
            s.fast,
            s.clones,
            secs(FAST_MS),
        );
        if let Some(m) = s.clean_median_ms {
            println!(
                "  {} median {} when no setup step had to rerun",
                paint(GREEN, "clove itself:"),
                paint(BOLD, secs(m)),
            );
        }
        if s.reran > 0 {
            println!(
                "  {} {} of {} reran setup (over {}): the branch changed what the golden image had set up",
                paint(YELLOW, "setup:"),
                s.reran,
                s.clones,
                secs(RERAN_MS),
            );
        }
        if let Some(g) = s.golden_median_ms {
            println!(
                "  {} {} build{} logged · median {}",
                paint(DIM, "golden:"),
                s.golden_builds,
                if s.golden_builds == 1 { "" } else { "s" },
                secs(g),
            );
        }
        println!(
            "\n  {}",
            paint(DIM, "WHERE THE TIME GOES (median, when the step ran)")
        );
        // Bars are on the scale of a clone that reran nothing, so clove's own
        // steps stay readable; a rerun step past it overflows with ▸.
        let scale = s.clean_median_ms.unwrap_or(s.median_ms).max(1.0);
        let name_w = s.steps.iter().map(|x| x.step.len()).max().unwrap_or(4);
        for x in &s.steps {
            if x.max_ms < 5.0 {
                continue;
            }
            let cells = ((x.median_ms / scale) * 24.0).round().max(1.0) as usize;
            let bar = if cells > 24 {
                format!("{}▸", "█".repeat(24))
            } else {
                "█".repeat(cells)
            };
            let runs = if x.runs < s.clones {
                paint(DIM, format!("  in {} of {}", x.runs, s.clones))
            } else {
                String::new()
            };
            println!(
                "  {:<name_w$}  {:>6}  {}  {}{runs}",
                x.step,
                secs(x.median_ms),
                paint(DIM, format!("p90 {:>6}", secs(x.p90_ms))),
                paint(
                    if x.step.starts_with("setup ") {
                        YELLOW
                    } else {
                        CYAN
                    },
                    bar
                ),
            );
        }
        println!("\n  {}", paint(DIM, "RECENT"));
        let mut t = Table::new(&["WHEN", "NAME", "TOTAL", "MOUNT", "CHECKOUT", "SETUP", "OTHER"]);
        for e in created.iter().take(limit) {
            let step = |name: &str| e.timings.iter().find(|t| t.step == name).map(|t| t.ms);
            let fmt = |ms: Option<f64>| ms.map(secs).unwrap_or_else(|| "–".into());
            let total_style = if e.total_ms < FAST_MS {
                GREEN
            } else if e.reran() {
                YELLOW
            } else {
                RED
            };
            let other = e.untimed_ms()
                + e.timings
                    .iter()
                    .filter(|t| {
                        !["mount", "checkout"].contains(&t.step.as_str()) && !t.step.starts_with("setup ")
                    })
                    .map(|t| t.ms)
                    .sum::<f64>();
            t.row(vec![
                styled(DIM, clove_core::history::ago(now.saturating_sub(e.at))),
                styled(BOLD, e.name.clone()),
                styled(total_style, secs(e.total_ms)),
                cell(fmt(step("mount"))),
                cell(fmt(step("checkout"))),
                if e.reran() {
                    styled(YELLOW, secs(e.setup_ms()))
                } else {
                    cell(secs(e.setup_ms()))
                },
                styled(DIM, secs(other)),
            ]);
        }
        t.print();
    }
    Ok(())
}

fn cmd_log(cli: &Cli, limit: usize, all_repos: bool) -> Result<()> {
    let repos: Vec<Clove> = if all_repos {
        clove_core::known_repos()
            .into_iter()
            .filter_map(|r| Clove::open(&r.root).ok())
            .collect()
    } else {
        vec![repo(cli)?]
    };
    let mut rows: Vec<(String, clove_core::history::Removed)> = repos
        .iter()
        .flat_map(|c| c.history().into_iter().map(|e| (c.name.clone(), e)))
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.at));
    rows.truncate(limit);
    if cli.json {
        let v: Vec<serde_json::Value> = rows
            .iter()
            .map(|(repo, e)| {
                let mut v = serde_json::to_value(e).unwrap_or_default();
                v["repo"] = repo.clone().into();
                v["restore"] = e.restore().into();
                v
            })
            .collect();
        return emit(&v);
    }
    if rows.is_empty() {
        eprintln!("nothing removed yet");
        return Ok(());
    }
    let now = clove_core::util::now_unix();
    let mut head = vec!["WHEN", "WHAT", "NAME", "BRANCH", "WHY", "BRANCH BACK"];
    if all_repos {
        head.insert(2, "REPO");
    }
    let mut t = Table::new(&head);
    for (repo, e) in &rows {
        let what = match e.by {
            clove_core::history::By::AutoPrune => styled(YELLOW, e.by.label()),
            _ => cell(e.by.label()),
        };
        let mut name = e.name.clone();
        if e.kind == clove_core::history::Kind::Worktree {
            name.push_str(" (git worktree)");
        }
        let mut row = vec![
            styled(DIM, clove_core::history::ago(now.saturating_sub(e.at))),
            what,
            styled(BOLD, name),
            styled(MAGENTA, e.branch.clone().unwrap_or_else(|| "(detached)".into())),
            cell(e.reason.clone().unwrap_or_default()),
            styled(
                DIM,
                e.restore().unwrap_or_else(|| {
                    if e.branch_deleted || e.branch.is_none() {
                        String::new()
                    } else {
                        "kept".into()
                    }
                }),
            ),
        ];
        if all_repos {
            row.insert(2, cell(repo.clone()));
        }
        t.row(row);
    }
    t.print();
    Ok(())
}

fn cmd_prune(cli: &Cli, apply: bool, r: &dyn clove_core::Reporter) -> Result<()> {
    use clove_core::Verdict;
    let c = repo(cli)?;
    let clove_core::Checks {
        clones: checks,
        worktrees,
        warning,
    } = c.all_merge_checks(true)?;
    let go: Vec<&clove_core::MergeCheck> = checks.iter().filter(|m| m.verdict == Verdict::Prune).collect();
    let go_wt: Vec<&clove_core::WorktreeCheck> =
        worktrees.iter().filter(|m| m.verdict == Verdict::Prune).collect();
    let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if !cli.json {
        if let Some(w) = &warning {
            eprintln!("{} {w}", paint(YELLOW, "!"));
        }
        if checks.is_empty() && worktrees.is_empty() {
            eprintln!("no clones yet.");
            return Ok(());
        }
        let mut t = Table::new(&["", "NAME", "BRANCH", "WHY", "OWN"]);
        for m in &checks {
            let (mark, why) = match m.verdict {
                Verdict::Prune => (styled(GREEN, "✓"), styled(GREEN, m.reason.clone())),
                Verdict::Review => (styled(YELLOW, "!"), styled(YELLOW, m.reason.clone())),
                Verdict::Keep => (styled(DIM, "·"), styled(DIM, m.reason.clone())),
            };
            t.row(vec![
                mark,
                styled(BOLD, m.clone.clone()),
                styled(MAGENTA, m.branch.clone().unwrap_or_else(|| "(detached)".into())),
                why,
                cell(clove_core::util::human_bytes(m.own_bytes)),
            ]);
        }
        if !checks.is_empty() {
            t.print();
        }
        if !worktrees.is_empty() {
            eprintln!(
                "\n{}",
                paint(
                    DIM,
                    "other git worktrees (removed with `git worktree remove`, never forced)"
                )
            );
            let mut t = Table::new(&["", "NAME", "BRANCH", "WHY", "PATH"]);
            for m in &worktrees {
                let (mark, why) = match m.verdict {
                    Verdict::Prune => (styled(GREEN, "✓"), styled(GREEN, m.reason.clone())),
                    Verdict::Review => (styled(YELLOW, "!"), styled(YELLOW, m.reason.clone())),
                    Verdict::Keep => (styled(DIM, "·"), styled(DIM, m.reason.clone())),
                };
                t.row(vec![
                    mark,
                    cell(m.name.clone()),
                    styled(MAGENTA, m.branch.clone().unwrap_or_else(|| "(detached)".into())),
                    why,
                    styled(DIM, tilde(&m.path)),
                ]);
            }
            t.print();
        }
    }
    let freed: u64 = go.iter().map(|m| m.own_bytes).sum();
    let mut confirmed = apply;
    if go.is_empty() && go_wt.is_empty() {
        if !cli.json {
            eprintln!("\nnothing to prune");
        }
    } else if !apply && !cli.json {
        let mut parts = vec![];
        if !go.is_empty() {
            parts.push(format!(
                "{} clone{} ({})",
                go.len(),
                if go.len() == 1 { "" } else { "s" },
                clove_core::util::human_bytes(freed)
            ));
        }
        if !go_wt.is_empty() {
            parts.push(format!(
                "{} git worktree{}",
                go_wt.len(),
                if go_wt.len() == 1 { "" } else { "s" }
            ));
        }
        let n = go.len() + go_wt.len();
        let what = format!(
            "{} and {}",
            parts.join(" and "),
            if n == 1 { "its branch" } else { "their branches" }
        );
        if interactive {
            confirmed = ask(&format!("Remove {what}?"), &[], false)?;
        } else {
            eprintln!("\n{what} can go: {}", paint(BOLD, "clove prune --apply"));
        }
    }
    let mut removed = vec![];
    let mut removed_worktrees: Vec<std::path::PathBuf> = vec![];
    if confirmed {
        // The shell's cwd keeps a volume busy: leave first, then prune again.
        let cwd = std::env::current_dir().ok();
        let inside = go.iter().any(|m| {
            cwd.as_ref()
                .is_some_and(|d| d.starts_with(c.clone_mount(&m.clone)))
        });
        if inside {
            if shell::request_cd_then(&c.root, &["prune".to_string(), "--apply".to_string()]) {
                eprintln!("{}", paint(DIM, "pruning after leaving this clone"));
                return Ok(());
            }
            bail!("you are inside a clone to prune; cd out first (or set up `clove shell-init`)");
        }
        for m in &go {
            if !cli.json {
                eprintln!("{} {}", paint(MAGENTA, "−"), paint(BOLD, &m.clone));
            }
            let opts = clove_core::prune::prune_options(clove_core::history::By::Prune, &m.reason);
            let res = c.remove(&m.clone, &opts, r);
            match res {
                Ok(()) => removed.push(m.clone.clone()),
                Err(e) => eprintln!("{} kept {}: {e:#}", paint(YELLOW, "!"), m.clone),
            }
        }
        for m in &go_wt {
            if !cli.json {
                eprintln!(
                    "{} {} {}",
                    paint(MAGENTA, "−"),
                    paint(BOLD, &m.name),
                    paint(DIM, "(git worktree)")
                );
            }
        }
        // Failures come back in `kept`, printed below.
        let (gone, kept) = c.remove_merged_worktrees(&worktrees, &Silent);
        for k in &kept {
            if !cli.json {
                eprintln!("{} {k}", paint(YELLOW, "!"));
            }
        }
        if std::env::current_dir().is_ok_and(|d| gone.iter().any(|g| d.starts_with(g))) {
            shell::request_cd(&c.root);
        }
        removed_worktrees = gone;
    }
    if cli.json {
        return emit(&serde_json::json!({
            "checks": checks,
            "worktrees": worktrees,
            "warning": warning,
            "removed": removed,
            "removed_worktrees": removed_worktrees,
        }));
    }
    Ok(())
}

fn cmd_config_action(cli: &Cli, action: &ConfigAction) -> Result<()> {
    use clove_core::settings;
    let c = repo(cli)?;
    match action {
        ConfigAction::Get { key } => {
            let states = settings::fields(&c.root, &c.name, &c.config)?;
            if let Some(k) = key {
                let s = states
                    .into_iter()
                    .find(|s| s.field.key == k.as_str())
                    .context("unknown setting")?;
                if cli.json {
                    return emit(&s);
                }
                println!("{}", s.effective);
                return Ok(());
            }
            if cli.json {
                return emit(&states);
            }
            // Paths under home print as ~/..., and long values end in an ellipsis.
            const MAX: usize = 48;
            let shown = |s: &settings::FieldState| {
                let v = if s.effective.starts_with('/') {
                    render::tilde(std::path::Path::new(&s.effective))
                } else {
                    s.effective.clone()
                };
                if v.chars().count() > MAX {
                    format!("{}…", v.chars().take(MAX - 1).collect::<String>())
                } else {
                    v
                }
            };
            let kw = states.iter().map(|s| s.field.key.len()).max().unwrap_or(0);
            let vw = states.iter().map(|s| shown(s).chars().count()).max().unwrap_or(0);
            for s in &states {
                let from = s.source.map(|l| l.label()).unwrap_or("default");
                println!("{:<kw$}  {:<vw$}  {}", s.field.key, shown(s), paint(DIM, from));
            }
        }
        ConfigAction::Set { key, value, layer } => {
            let l = settings::Layer::ALL[*layer as usize];
            let path = l.path(&c.root, &c.name);
            let shown = settings::set(&path, key, value)?;
            if cli.json {
                return emit(&serde_json::json!({ "key": key, "value": shown, "path": path }));
            }
            eprintln!(
                "{} {key} = {shown}  {}",
                paint(GREEN, "✓"),
                paint(DIM, tilde(&path))
            );
            shadowed_note(&c, key, l)?;
            if layer_is_team(*layer) {
                eprintln!("{}", paint(DIM, "commit .clove.toml to share it with the team"));
            }
        }
        ConfigAction::Unset { key, layer } => {
            let l = settings::Layer::ALL[*layer as usize];
            let path = l.path(&c.root, &c.name);
            let removed = settings::unset(&path, key)?;
            if cli.json {
                return emit(&serde_json::json!({ "key": key, "removed": removed, "path": path }));
            }
            if removed {
                eprintln!("{} unset {key}  {}", paint(GREEN, "✓"), paint(DIM, tilde(&path)));
            } else {
                eprintln!("{}", paint(DIM, format!("{key} is not set in {}", tilde(&path))));
            }
        }
    }
    Ok(())
}

/// Warns when a later layer still overrides what was just written.
fn shadowed_note(c: &Clove, key: &str, wrote: clove_core::settings::Layer) -> Result<()> {
    let c = Clove::open(&c.root)?;
    let states = clove_core::settings::fields(&c.root, &c.name, &c.config)?;
    if let Some(s) = states.iter().find(|s| s.field.key == key)
        && let Some(src) = s.source
        && src.index() > wrote.index()
    {
        eprintln!(
            "{} {} sets {key} = {}, which wins",
            paint(YELLOW, "!"),
            src.label(),
            s.effective
        );
    }
    Ok(())
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Layer {
    /// ~/.config/clove/config.toml: every repo, only you.
    All = 0,
    /// .clove.toml in the repo: shared with the team.
    Team = 1,
    /// ~/.config/clove/repos/<repo>.toml: this repo, only you.
    You = 2,
}

fn layer_is_team(l: Layer) -> bool {
    matches!(l, Layer::Team)
}

/// Opens a file in $VISUAL/$EDITOR, or the default text editor on macOS.
fn edit_file(path: &std::path::Path) -> Result<()> {
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.trim().is_empty());
    let status = match editor {
        // $EDITOR may carry flags ("code -w"), so let the shell split it.
        Some(ed) => std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{ed} \"$1\""))
            .arg("sh")
            .arg(path)
            .status()?,
        None if cfg!(target_os = "macos") => {
            std::process::Command::new("open").arg("-t").arg(path).status()?
        }
        None => bail!("set $EDITOR, or edit {} yourself", path.display()),
    };
    if !status.success() {
        bail!("editor exited with {status}");
    }
    Ok(())
}

fn cmd_repos(cli: &Cli) -> Result<()> {
    let repos: Vec<(clove_core::RepoRecord, Option<clove_core::RepoSummary>)> = clove_core::known_repos()
        .into_iter()
        .map(|r| {
            let s = Clove::open(&r.root).ok().map(|c| c.summary());
            (r, s)
        })
        .collect();
    if cli.json {
        let out: Vec<_> = repos
            .iter()
            .map(|(r, s)| serde_json::json!({ "name": r.name, "root": r.root, "summary": s }))
            .collect();
        return emit(&out);
    }
    if repos.is_empty() {
        eprintln!("no repos yet. {} in one", paint(BOLD, "clove init"));
        return Ok(());
    }
    let here = Clove::discover(&std::env::current_dir().unwrap_or_default())
        .ok()
        .map(|c| c.root);
    let mut t = Table::new(&["", "REPO", "CLONES", "GOLDEN", "MERGED", "PATH"]);
    for (r, s) in &repos {
        let dot = if here.as_ref() == Some(&r.root) {
            styled(CYAN, "@")
        } else {
            styled(DIM, " ")
        };
        let (clones, golden, merged) = match s {
            None => (styled(DIM, "-"), styled(RED, "can't open"), styled(DIM, "-")),
            Some(s) => (
                cell(format!("{} ({} mounted)", s.clones, s.mounted)),
                match (s.generation, s.refreshing, s.stale) {
                    (None, _, _) => styled(YELLOW, "not built"),
                    (Some(g), true, _) => styled(CYAN, format!("gen {g} · refreshing")),
                    (Some(g), _, true) => styled(YELLOW, format!("gen {g} · stale")),
                    (Some(g), _, _) => styled(GREEN, format!("gen {g} · fresh")),
                },
                if s.prunable > 0 {
                    styled(GREEN, s.prunable.to_string())
                } else {
                    styled(DIM, "-")
                },
            ),
        };
        t.row(vec![
            dot,
            styled(BOLD, r.name.clone()),
            clones,
            golden,
            merged,
            styled(DIM, tilde(&r.root)),
        ]);
    }
    t.print();
    Ok(())
}

fn cmd_candidates(cli: &Cli) -> Result<()> {
    let found = clove_core::candidates::find();
    if cli.json {
        return emit(&found);
    }
    if found.is_empty() {
        let roots: Vec<String> = clove_core::candidates::default_roots()
            .iter()
            .map(|r| tilde(r))
            .collect();
        eprintln!(
            "no unmanaged git repos found in {}. {} sets one up",
            if roots.is_empty() {
                "the usual folders".to_string()
            } else {
                roots.join(", ")
            },
            paint(BOLD, "clove init -C <dir>")
        );
        return Ok(());
    }
    let now = clove_core::util::now_unix();
    let mut t = Table::new(&["REPO", "BRANCH", "ACTIVE", "PATH"]);
    for c in &found {
        t.row(vec![
            styled(BOLD, c.name.clone()),
            match &c.branch {
                Some(b) => styled(MAGENTA, b.clone()),
                None => styled(DIM, "(detached)"),
            },
            cell(clove_core::util::human_age(now.saturating_sub(c.active))),
            styled(DIM, tilde(&c.root)),
        ]);
    }
    t.print();
    eprintln!("\n{} sets one up", paint(BOLD, "clove init -C <path>"));
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
        println!(
            "  local   {} {}",
            s.copy.join(", "),
            paint(DIM, "(gitignored, copied into each clone)")
        );
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
    if s.steps_detected {
        let plan = clove_core::detect::plan(&c.root, c.config.nested_installs);
        let covered = |d: &String| {
            let prefix = format!("{d}/");
            c.config
                .steps
                .iter()
                .any(|st| st.watch.iter().any(|w| w.starts_with(&prefix)))
        };
        let skipped: Vec<String> = plan
            .skipped_installs
            .iter()
            .filter(|d| !covered(d))
            .cloned()
            .collect();
        if !skipped.is_empty() {
            println!(
                "  {} {} nested project{} without dependencies in clones: {} {}",
                paint(YELLOW, "!"),
                skipped.len(),
                if skipped.len() == 1 { "" } else { "s" },
                skipped.join(", "),
                paint(DIM, "(clove doctor)")
            );
        }
    }
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

/// Agent worktree hooks. Progress goes to stderr; stdout carries the protocol response.
fn cmd_hook(action: &HookAction) -> Result<()> {
    let read_input = || -> Result<serde_json::Value> {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
        serde_json::from_str(&buf).context("hook input is not JSON")
    };
    let required = |input: &serde_json::Value, key: &str| {
        input[key]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .with_context(|| format!("hook input requires a non-empty {key}"))
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
        HookAction::Opencode | HookAction::Pi => {
            let (body, location) = match action {
                HookAction::Opencode => (
                    clove_core::agents::OPENCODE_PLUGIN,
                    "~/.config/opencode/plugins/clove.js (or .opencode/plugins/clove.js in a repo)",
                ),
                _ => (
                    clove_core::agents::PI_EXTENSION,
                    "~/.pi/agent/extensions/clove.js (or .pi/extensions/clove.js in a repo)",
                ),
            };
            print!("{body}");
            eprintln!(
                "{}",
                paint(DIM, format!("save to {location}, or run `clove agents install`"))
            );
        }
        HookAction::WorktreeCreate => {
            let input = read_input()?;
            let name = input["name"].as_str().context("hook input has no name")?;
            let cwd = input["cwd"]
                .as_str()
                .map(PathBuf::from)
                .unwrap_or(std::env::current_dir()?);
            // Claude Code has no fallback once this hook exists, so repos
            // clove doesn't manage get what Claude would have made.
            let Some(c) = managed_repo(&cwd) else {
                println!("{}", plain_worktree_add(&cwd, name)?.display());
                return Ok(());
            };
            let info = c.new_clone(
                &NewOptions {
                    branch: Some(name.to_string()),
                    ..Default::default()
                },
                &TermReporter::new(false),
            )?;
            c.spawn_spare_fill();
            println!("{}", info.path.display());
        }
        HookAction::WorktreeRemove => {
            let input = read_input()?;
            let path = PathBuf::from(
                input["worktree_path"]
                    .as_str()
                    .context("hook input has no worktree_path")?,
            );
            let c = Clove::discover(&path)
                .or_else(|_| Clove::discover(&input["cwd"].as_str().map(PathBuf::from).unwrap_or_default()));
            let clone = c
                .as_ref()
                .ok()
                .and_then(|c| Some((c, c.resolve(&path.to_string_lossy()).ok()?)));
            let Some((c, name)) = clone else {
                return plain_worktree_remove(&path);
            };
            // Keeps the branch when it has commits its base does not.
            c.remove(
                &name,
                &RemoveOptions {
                    force: false,
                    delete_branch: true,
                    branch_merged: false,
                    unlock: false,
                    by: clove_core::history::By::ClaudeHook,
                    reason: None,
                },
                &TermReporter::new(false),
            )?;
        }
        HookAction::Check { dir } => {
            let dir = match dir {
                Some(dir) => dir.clone(),
                None => std::env::current_dir()?,
            };
            let output = match managed_repo(&dir) {
                Some(c) => serde_json::json!({ "managed": true, "root": c.root }),
                None => serde_json::json!({ "managed": false }),
            };
            println!("{output}");
        }
        HookAction::OpencodeWorktreeCreate => {
            let input = read_input()?;
            let source = required(&input, "sourceDirectory")?;
            let directory = required(&input, "directory")?;
            let branch = match &input["branch"] {
                serde_json::Value::Null => None,
                serde_json::Value::String(s) => Some(s.as_str()).filter(|s| !s.is_empty()),
                _ => bail!("hook input branch must be a starting ref string"),
            };
            let request_id = match &input["requestId"] {
                serde_json::Value::Null => None,
                v => Some(v.as_str().context("hook input requestId must be a string")?),
            };
            let directory = opencode::create_worktree(
                &source,
                &directory,
                branch,
                request_id,
                &TermReporter::new(false),
            )?;
            println!("{}", serde_json::json!({ "directory": directory }));
        }
        HookAction::OpencodeWorktreeRemove => {
            let input = read_input()?;
            let path = required(&input, "directory")?;
            let project = required(&input, "projectDirectory")?;
            let force = input["force"].as_bool().unwrap_or(false);
            opencode::remove_worktree(&project, &path, force, &TermReporter::new(false))?;
        }
        HookAction::OpencodeWorktreeAbort => {
            let input = read_input()?;
            let project = required(&input, "projectDirectory")?;
            let id = input["requestId"]
                .as_str()
                .context("hook input requestId must be a string")?;
            let c = Clove::discover(&project)?;
            c.require_record()?;
            c.abort_clone_by_request(
                id,
                clove_core::history::By::OpencodeHook,
                &TermReporter::new(false),
            )?;
        }
        HookAction::OpencodeWorktreeList => {
            let input = read_input()?;
            let source = required(&input, "sourceDirectory")?;
            let entries = opencode::list_worktrees(&source)?;
            println!("{}", serde_json::json!(entries));
        }
    }
    Ok(())
}

fn cmd_agents(cli: &Cli, action: &AgentsAction) -> Result<()> {
    use clove_core::agents::{self, State};
    let (ts, verb, no_targets) = match action {
        AgentsAction::Install => {
            let report = agents::install_with_report()?;
            if report.opencode_skipped {
                eprintln!(
                    "OpenCode skipped: the clove plugin requires OpenCode V2 (stable 2.x) on PATH. \
                     Update OpenCode and rerun `clove agents install`."
                );
            }
            (report.changed, "installed", !report.has_targets)
        }
        AgentsAction::Remove => (agents::remove()?, "removed", false),
        AgentsAction::Status => {
            let ts = agents::status()?;
            let no_targets = ts.is_empty();
            (ts, "", no_targets)
        }
    };
    if cli.json {
        return emit(&ts);
    }
    if no_targets {
        eprintln!(
            "no supported coding agent found (looked for ~/.claude, ~/.codex, ~/.pi/agent, \
             and opencode or pi on PATH)"
        );
        return Ok(());
    }
    if ts.is_empty() && !verb.is_empty() {
        eprintln!("{} nothing to change", paint(GREEN, "✓"));
    }
    for t in &ts {
        let mark = match (verb, t.state) {
            ("", State::Installed) => paint(GREEN, "✓"),
            ("", State::Outdated) => paint(YELLOW, "!"),
            ("", State::Missing) => paint(DIM, "·"),
            _ => paint(GREEN, "✓"),
        };
        let what = match (verb, t.state) {
            ("", State::Installed) => "installed",
            ("", State::Outdated) => "outdated",
            ("", State::Missing) => "not installed",
            (v, _) => v,
        };
        eprintln!(
            "{mark} {:<28} {:<14} {}",
            t.what,
            what,
            paint(DIM, tilde(&t.path))
        );
    }
    match (action, agents::summary(&ts)) {
        (AgentsAction::Status, State::Installed) => {}
        (AgentsAction::Status, _) => eprintln!(
            "
{}",
            paint(BOLD, "clove agents install")
        ),
        (AgentsAction::Install, _) if !ts.is_empty() => eprintln!(
            "
{}",
            paint(
                DIM,
                "agents now reach for clove in repos it manages; `clove guide` is what they read. \
                 Restart running agent sessions to pick it up."
            )
        ),
        _ => {}
    }
    Ok(())
}

/// Claude Code's own worktree: `.claude/worktrees/<name>` at the repo root,
/// on a new `worktree-<name>` branch from the remote default branch (local
/// HEAD when there is none).
fn plain_worktree_add(cwd: &Path, name: &str) -> Result<PathBuf> {
    let top = PathBuf::from(clove_core::git::out(cwd, &["rev-parse", "--show-toplevel"])?.trim());
    let path = top.join(".claude").join("worktrees").join(name);
    let branch = format!("worktree-{name}");
    let base = clove_core::git::out(
        &top,
        &["rev-parse", "--verify", "--quiet", "refs/remotes/origin/HEAD"],
    )
    .map(|_| "origin/HEAD".to_string())
    .unwrap_or_else(|_| "HEAD".into());
    let p = path.to_string_lossy().to_string();
    clove_core::git::run_ok(&top, &["worktree", "add", "-b", &branch, &p, &base])
        .context("git worktree add")?;
    Ok(path)
}

/// Removes a worktree clove didn't make, as Claude Code would: never forced,
/// so uncommitted changes stop it. The branch stays.
fn plain_worktree_remove(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let path = path.canonicalize()?;
    let mut cmd = git::cmd(&path);
    cmd.args(["worktree", "remove"]);
    clove_core::util::output(cmd.arg("--").arg(&path)).context("git worktree remove")?;
    Ok(())
}

/// `clove diff`: the patch (paged in a terminal), per-file or per-package
/// counts, or with --json everything a script or agent needs.
fn cmd_diff(cli: &Cli, c: &Clove, name: &str, stat: bool, packages: bool) -> Result<()> {
    use clove_core::patch::{self, Change};
    let d = c.diff(name)?;
    let files = patch::parse(&d.patch);
    if cli.json {
        let list: Vec<_> = files
            .iter()
            .map(|f| {
                serde_json::json!({
                    "path": f.path,
                    "old_path": f.old_path,
                    "change": f.change,
                    "added": f.added,
                    "removed": f.removed,
                    "binary": f.binary,
                    "fold": patch::fold(f, patch::attrs_of(&d, &f.path)),
                    "group": patch::group_of(&f.path, &d.packages).0,
                })
            })
            .collect();
        let mut v = serde_json::to_value(&d)?;
        v["files"] = serde_json::json!(list);
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    let tty = std::io::stdout().is_terminal();
    if files.is_empty() {
        eprintln!("{name} matches {}: nothing committed, staged or modified", d.base);
        return Ok(());
    }
    if stat || packages {
        let (a, r) = files
            .iter()
            .fold((0, 0), |(a, r), f| (a + f.added, r + f.removed));
        let nums = |a: u32, r: u32| {
            format!(
                "{} {}",
                paint(GREEN, format!("{:>6}", format!("+{a}"))),
                paint(RED, format!("{:<6}", format!("−{r}")))
            )
        };
        if packages {
            let mut groups: Vec<(String, u32, u32, usize)> = vec![];
            for f in &files {
                let key = patch::group_of(&f.path, &d.packages).0;
                match groups.iter_mut().find(|g| g.0 == key) {
                    Some(g) => {
                        g.1 += f.added;
                        g.2 += f.removed;
                        g.3 += 1;
                    }
                    None => groups.push((key, f.added, f.removed, 1)),
                }
            }
            groups.sort_by_key(|g| std::cmp::Reverse(g.1 + g.2));
            for (key, a, r, n) in &groups {
                let label = if key.is_empty() { "(repository root)" } else { key };
                println!(
                    "{} {label} {}",
                    nums(*a, *r),
                    paint(DIM, format!("{n} file{}", if *n == 1 { "" } else { "s" }))
                );
            }
        } else {
            for f in &files {
                let mark = match f.change {
                    Change::Added => paint(GREEN, "A"),
                    Change::Deleted => paint(RED, "D"),
                    Change::Renamed => paint(CYAN, "R"),
                    Change::Modified => paint(YELLOW, "M"),
                };
                let from = f
                    .old_path
                    .as_ref()
                    .map(|o| paint(DIM, format!(" ← {o}")))
                    .unwrap_or_default();
                let bin = if f.binary {
                    paint(DIM, " binary")
                } else {
                    String::new()
                };
                println!("{} {mark} {}{from}{bin}", nums(f.added, f.removed), f.path);
            }
        }
        println!(
            "{} {}",
            nums(a, r),
            paint(
                DIM,
                format!(
                    "{} files vs {} (from {})",
                    files.len(),
                    d.base,
                    &d.merge_base[..d.merge_base.len().min(10)]
                )
            )
        );
        return Ok(());
    }
    if !tty {
        // `clove diff | head` closes the pipe early: that is not an error.
        let _ = std::io::Write::write_all(&mut std::io::stdout(), d.patch.as_bytes());
        return Ok(());
    }
    // In a terminal: colour it like git, and page it like git.
    let mut out = String::with_capacity(d.patch.len() + d.patch.len() / 8);
    for line in d.patch.lines() {
        let style = if line.starts_with("diff --git") || line.starts_with("+++ ") || line.starts_with("--- ")
        {
            BOLD
        } else if line.starts_with("@@") {
            CYAN
        } else if line.starts_with('+') {
            GREEN
        } else if line.starts_with('-') {
            RED
        } else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        out.push_str(&paint(style, line));
        out.push('\n');
    }
    page(&out)
}

fn stdout_write(text: &str) -> std::io::Result<()> {
    std::io::Write::write_all(&mut std::io::stdout(), text.as_bytes())
}

/// Writes through `$PAGER` (default `less -FRX`), or straight out without one.
fn page(text: &str) -> Result<()> {
    use std::io::Write;
    let pager = std::env::var("PAGER").ok().filter(|p| !p.trim().is_empty());
    let mut cmd = match &pager {
        Some(p) => {
            let mut c = std::process::Command::new("sh");
            c.arg("-c").arg(p);
            c
        }
        None => {
            let mut c = std::process::Command::new("less");
            c.arg("-FRX");
            c
        }
    };
    match cmd.stdin(std::process::Stdio::piped()).spawn() {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                // A pager quit early closes the pipe: not an error.
                let _ = stdin.write_all(text.as_bytes());
            }
            child.wait()?;
        }
        Err(_) => {
            let _ = stdout_write(text);
        }
    }
    Ok(())
}

/// `clove` with no command: the mark, where this repo stands, and what to type next.
fn tui_dir(cli: &Cli) -> Result<PathBuf> {
    Ok(match &cli.dir {
        Some(d) => d.clone(),
        None => std::env::current_dir()?,
    })
}

fn run_tui(cli: &Cli) -> Result<()> {
    clove_tui::run(&tui_dir(cli)?)
}

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
                hint("clove tui", "browse and manage them");
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
            hint("clove tui", "browse repos you already set up");
        }
    }
    println!();
    println!("  {}", paint(DIM, "clove --help for every command"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Agents learn clove from `clove guide`; a command missing there is one
    /// they won't use.
    #[test]
    fn the_guide_covers_every_command() {
        let cmd = Cli::command();
        let missing: Vec<String> = cmd
            .get_subcommands()
            .filter(|s| !s.is_hide_set())
            .map(|s| s.get_name().to_string())
            .filter(|n| {
                ![
                    "help",
                    "hook",
                    "shell-init",
                    "tui",
                    "completions",
                    "guide",
                    "agents",
                    "service",
                    "upgrade",
                ]
                .contains(&n.as_str())
            })
            .filter(|n| {
                let aliases: Vec<String> = cmd
                    .find_subcommand(n)
                    .map(|s| s.get_all_aliases().map(str::to_string).collect())
                    .unwrap_or_default();
                !std::iter::once(n.clone())
                    .chain(aliases)
                    .any(|a| clove_core::agents::GUIDE.contains(&format!("clove {a}")))
            })
            .collect();
        assert!(missing.is_empty(), "add to agents::GUIDE: {missing:?}");
    }
}
