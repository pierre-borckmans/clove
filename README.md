<picture>
  <source media="(prefers-color-scheme: dark)" srcset="brand/mark-on-dark.svg">
  <img src="brand/mark.svg" alt="" width="72">
</picture>

# clove

[![ci](https://github.com/pierre-borckmans/clove/actions/workflows/ci.yml/badge.svg)](https://github.com/pierre-borckmans/clove/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/pierre-borckmans/clove)](https://github.com/pierre-borckmans/clove/releases/latest)
[![license](https://img.shields.io/github/license/pierre-borckmans/clove)](LICENSE)

A ready git worktree in about a second, with `node_modules`, the dev shell and every other ignored file already there.

```
$ clove new feat/login
+ feat/login
  ✓ clone image              0.00s
  ✓ mount                    0.70s
  ✓ checkout                 0.21s
  ✓ setup mise-trust         0.02s
  ✓ setup direnv-allow       0.02s
  · setup direnv             unchanged since golden
  · setup pnpm               unchanged since golden

● feat-login ready in 1.0s ~/src/mono.clove/feat-login/mono
```

| On a 35k-file monorepo | `git worktree add` + setup | `clove new` |
|---|---|---|
| Time to a usable checkout | ~72s | 1.0 to 1.2s |
| New disk per worktree | a few GB | ~6 MB |

## Install

The CLI and terminal UI, on macOS and Linux:

```
brew install pierre-borckmans/tap/clove
clove shell-init --install       # cd into new clones, completions (zsh, bash, fish)
```

The Mac app, with a menu-bar popover:

```
brew install --cask pierre-borckmans/tap/clove-app
```

Without Homebrew:

- **Prebuilt binaries** for macOS and Linux are on the [releases page](https://github.com/pierre-borckmans/clove/releases). The macOS ones are signed and notarized.
- **From source:** `cargo install --git https://github.com/pierre-borckmans/clove clove-cli`

Then, in a repository:

```
clove init                       # shows what it will set up, then builds the golden image (once)
clove new feat/login             # a ready worktree on that branch
clove service install            # remount clones at login, keep golden images fresh
clove agents install             # teach Claude Code and Codex to use clove (optional)
```

### macOS

Needs APFS, the default filesystem. Tested on macOS 26, which has ASIF disk images; older versions fall back to `hdiutil` sparse images, which are implemented but untested.

### Linux

Needs btrfs-progs (`sudo apt install btrfs-progs`) and loop devices. The host disk can be any filesystem. Kernel 6.7 or later gives exact clone sizes (simple quotas); older kernels use full qgroups, or report no sizes.

`clove init` offers to add one line to `/etc/fstab`, with sudo, once per repo:

```
~/.clove/repos/<repo>-<hash>/pool.btrfs ~/src/<repo>.clove btrfs loop,user,noauto,exec,user_subvol_rm_allowed 0 0
```

With it, clove mounts the repo's image as you, at `<repo>.clove`, from a desktop or over SSH. Without it, clove mounts through udisks2 if installed, which works only from a desktop session and puts clones under `/run/media/$USER/` (or `/media/$USER/`). `sudo -n` is the last resort. `clove doctor` says which one a repo uses.

## Why

`git worktree add` gives you the tracked files and nothing else. On a monorepo with 35k files, 4,470 npm packages and a nix flake dev shell, a fresh worktree takes about 72 seconds before anything runs: 41s for direnv to evaluate the flake, 30s for `pnpm install --offline`, and a few GB of new files.

Tools that copy the ignored files over (worktrunk's `copy-ignored`, `cp -c`, the git-cow family) copy-on-write file by file. That saves disk, but time grows with the file count: that repo's 549k ignored files take minutes, and `clonefile` on the root `node_modules` alone takes 15s.

clove copies one disk image instead of half a million files, so the cost doesn't depend on how many files the repo has.

## How it works

Each repo gets one **golden** image: a worktree of the base branch with dependencies installed and dev environments warmed. A clone is a copy-on-write copy of it, registered as a regular git worktree with a copy of the golden index. Inodes and timestamps are kept, so git rehashes nothing. clove then checks out your branch and runs only the setup steps whose inputs changed since the golden commit: if the lockfile is the same, `pnpm install` doesn't run.

Clones are isolated. Adding a dependency or editing the nix flake in one doesn't affect the others. File watchers, Docker bind mounts, Go and Rust builds all work inside a clone.

### On macOS

The golden image is an APFS volume in a disk image file.

1. `clove new` copies the image *file* with `clonefile(2)`: one syscall whatever is inside, and no disk until something is written.
2. It mounts the copy as its own volume, in about 0.7s.
3. It registers the worktree and runs the changed steps.

Five runs on the monorepo above: 1.04 to 1.16s per clone, 6.1 MB of disk each. `clove init` took 1m18s once.

### On Linux

Each repo has one btrfs image file, mounted once. The golden worktree is a subvolume in it, and a clone is a snapshot of that subvolume, so there is no mount per clone. A snapshot of a 185k-file golden takes about 8 ms, and the first `git status` in the clone 45 to 61 ms, with no rehash.

## Use

```
clove new feat/login             # branch from origin/main, or check out an existing branch
clove new feat/login -x claude   # and start an agent in it
clove new pr:123                 # check out pull request #123 as branch pr/123
clove ls                         # clones, branch, dirty/ahead/behind, own disk
clove cd feat-login              # jump to a clone; `clove cd` goes back to the main checkout
clove run feat-login -- pnpm test   # run inside the clone's dev environment
clove rm feat-login -D           # unmount, delete, delete the branch
clove prune                      # remove clones whose PR is merged, and their branches
clove log                        # what clove removed, when, why, and how to get the branch back
clove adopt ../old-worktree      # turn a plain git worktree into a clone on its branch
clove setup                      # in a clone, after a pull: rerun the setup steps that changed
clove refresh                    # bring the golden image to the latest base (incremental)
clove doctor                     # check the setup, with a fix for each problem
clove repos                      # every repo clove manages, with golden state and merged clones
clove repos --candidates         # git repos in your code folders clove could manage
clove config get                 # each setting, its value and which file it comes from
clove config set fetch false     # change one setting (--layer team|all; default: you, this repo)
clove config --edit              # open your settings for this repo (--edit team, --edit all)
```

Every feature is in three places at once:

- **The CLI**, above. Every command takes `--json`; `clove new --json` prints the clone's path, branch and timings.
- **The terminal UI:** `clove` on its own (or `clove tui`), with mouse support. A sidebar lists every repo, its clones and whether its golden image is fresh. `s` opens settings (Enter edits or toggles, ←/→ picks the file it saves to, `x` resets), `g` cleans up, `L` shows what was removed, `A` adds a repo, and Enter quits into the selected clone.
- **The Mac app,** with the same sidebar, settings, cleanup and history, plus a menu-bar popover.

**Shell integration** (`clove shell-init --install`) lets `clove new`, `clove cd` and the terminal UI change your directory, and completes clone and branch names. It also gives your shell each clone's environment while you're in it: clove writes a `.clove-env` next to the clone's worktree, outside git, with the variables its steps use (pnpm's store, your config's `env`), and the shell loads it on `cd` and restores your variables on the way out. Without it, a plain `pnpm i` in a clone would start an empty store and rebuild `node_modules`. Only files clove wrote are loaded: each carries a per-user token, so a repo can't ship one. Shells without the integration can use `clove run <clone> -- <command>`.

**Plain git worktrees** (made before clove, or by an agent's `git worktree add`) show up in `clove ls` too. `clove rm <path or folder name>` removes one with `git worktree remove`, never forced, so a locked or modified worktree stays, and so does a detached one holding commits on no branch. `clove adopt` turns one into a clone on the same branch, and puts it back if that fails. Ignored files in it, like build output or `.env`, aren't carried over.

### Merged PRs

`clove prune` asks GitHub (through `gh`) for each clone's pull request. A clone goes when its PR is merged, its tip is the commit GitHub merged (or one the PR contains), and it has no uncommitted changes. That catches squash merges, which `git branch --merged` misses. A clone with commits after the merge or local edits is listed for review and kept, as is any branch without a PR. It lists first and asks; `--apply` skips the question.

With `prune.auto` on (off by default), the service prunes on its own, but only clones proven merged and clean that git has been quiet in for an hour. It never touches plain git worktrees, and does nothing when GitHub can't be asked. A clone something still has open is removed as soon as it's free.

Every removal, by hand, by prune or by the service, goes in a history. `clove log` shows what went, when, by what, why, and the `git branch` command that brings a deleted branch back. A command on a removed clone says what happened to it.

```toml
[prune]
auto = true
```

### Coding agents

`clove agents install` teaches Claude Code and Codex to use clove:

- a Claude Code skill (`~/.claude/skills/clove`),
- a short block in `~/.claude/CLAUDE.md` and `~/.codex/AGENTS.md`, between markers, leaving the rest of each file as it was,
- Claude Code's worktree hooks in `~/.claude/settings.json`, so `claude --worktree` and subagent worktrees become clones. In repos clove doesn't manage, the hooks make the same plain worktree Claude would.

Agents then read `clove guide`, written for them and matching the installed version. `clove agents status` shows what's installed, `clove agents remove` takes it out, and `clove doctor` flags it when missing or outdated.

## Staying fresh

The golden image is a recipe, not a snapshot of your checkout: the base branch, plus the files listed in `copy`, plus the `warm` steps.

- **Refresh is incremental.** `clove refresh` clones the current golden image, checks out the latest base and reruns only the warm steps whose `watch` paths or definition changed. A refresh where nothing relevant changed takes about 4s on the monorepo above. `--full` reruns everything.
- **Stale means a clone would redo work.** The image is stale once a step's inputs changed on the base, because a new clone would rerun that step. `clove status` names it, e.g. `build-deps inputs changed in 3 commits`. Dozens of commits behind with no step inputs touched is still fresh. `max_age` and `max_behind` are a safety net.
- **Stale images refresh themselves.** `clove new` starts a background refresh when it finds the image stale, without waiting for it.
- **The service follows your pulls.** `clove service install` adds two launchd agents on macOS, or systemd user units on Linux. One remounts clones at login. The other runs `clove refresh --all-repos --if-stale --prs` whenever git rewrites a repo's `FETCH_HEAD` (any fetch or pull in the main checkout) and every 4h otherwise, which also checks clones' PRs. A fresh repo costs at most one `git fetch`.
- **Refreshing never touches existing clones.** The next generation is built next to the current one and swapped in atomically; `clove new` keeps working meanwhile.
- **Clones catch up with `clove setup`.** After a pull or rebase in a clone, it copies your local files again and reruns only the setup steps whose inputs changed.

## Configure

clove detects steps from the repo: mise, direnv (including nix-direnv's cache), pnpm, bun, yarn, npm and uv. `clove config` shows what it found, and `clove config --write` saves it to `.clove.toml` so the team gets the same setup.

```toml
base = "origin/main"

[[step]]
name = "pnpm"
warm = "pnpm install --frozen-lockfile"                 # in the golden image, on init/refresh
setup = "pnpm install --prefer-offline --frozen-lockfile" # in each new clone...
watch = ["pnpm-lock.yaml", "pnpm-workspace.yaml"]       # ...only if these changed

[[step]]
name = "codegen"
warm = "make generate"
setup = "make generate"
watch = ["proto/"]
```

Files that exist only in your checkout, like `.env` or `.envrc.local`, never reach a fresh checkout. clove detects the ones git ignores (`**/.env`, `**/.env.*`, `**/.envrc.local`, `**/.dev.vars`) and copies them into the golden image before warming, so codegen can read them, and again into each new clone, so it gets their current contents. List them yourself to override:

```toml
copy = [".env", "apps/*/.env.local", "config/dev.secrets.json"]

[refresh]
auto = true        # refresh in the background when `clove new` finds the image stale
max_age = "24h"     # safety net; steps' watch paths are the main signal
max_behind = 100   # commits

[prune]
auto = false       # the service removes merged, clean, idle clones; `clove log` lists them
```

Steps run inside the repo's environment (`direnv exec` or `mise exec`). `[[step]]` replaces the detected steps; `[[add_step]]` appends to them. Settings layer in this order, last wins:

1. `~/.config/clove/config.toml`: personal defaults for every repo
2. `.clove.toml` in the repo: shared with the team
3. `~/.config/clove/repos/<repo>.toml`: personal overrides for one repo

For example, a monorepo whose workspace libraries must be built before the app typechecks:

```toml
# ~/.config/clove/repos/mono.toml
[[add_step]]
name = "build-deps"
warm = "pnpm run build-deps"
setup = "pnpm run build-deps"
watch = ["common/javascript", "packages/inkwell", "pnpm-lock.yaml"]
```

## Limits

Everywhere:

- **Mounts don't survive a reboot.** Run `clove mount --all-repos`, or install the service.
- **Absolute paths don't move.** Python virtualenvs and cargo's `target/` fingerprints record the path they were built at, so they rebuild in a clone. Bazel keeps one output base per path.
- **Daemons started by warm steps** (a Bazel server, git's fsmonitor) are stopped when the golden image unmounts, and clove says which.

On macOS:

- **Each clone is its own mounted volume**, so a machine with many clones has many mounts.

On Linux:

- **Clones share one volume per repo.** `clove unmount` without names unmounts it if nothing uses it; a single clone can't be unmounted. `clone_root` must be a folder of its own per repo, which is the default.
- **Freed space comes back a few seconds late**, once btrfs cleans up a removed clone.
- **No end-to-end test in CI yet**; unit tests run on Linux.

## Layout

```
~/.clove/repos/<repo>-<hash>/
  golden/current -> gen-N        the golden image, HEAD and index snapshot, meta.json
  clones/<name>/                 each clone's image and meta.json
  history.jsonl                  what clove removed (clove log)
<repo parent>/<repo>.clove/<name>/<repo>   where a clone is mounted
```

The worktree sits one level inside the volume, so the volume's own files stay out of `git status` and the folder keeps the repo's name.

On Linux, `~/.clove/repos/<repo>-<hash>/pool.btrfs` is the repo's one image, mounted at `<repo>.clove` (or where udisks put it, recorded in `mount` next to it). Inside it, `.clove-golden/` is the golden worktree, `.clove-gens/gen-N` its read-only snapshots, and `<name>/<repo>` each clone.

## Develop

```
cargo test --workspace           # unit tests + an end-to-end run on real disk images (macOS)
scripts/smoke.zsh                # the CLI through the zsh integration, on a throwaway repo
```

Releasing and signing: [RELEASING.md](RELEASING.md).

MIT licensed.
