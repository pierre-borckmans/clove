<picture>
  <source media="(prefers-color-scheme: dark)" srcset="brand/mark-on-dark.svg">
  <img src="brand/mark.svg" alt="" width="72">
</picture>

# clove

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

## Why

`git worktree add` gives you the tracked files and nothing else. On the monorepo I work in (35k files, 4,470 npm packages, a nix flake dev shell) a fresh worktree took about 72 seconds before I could run anything: 41s for direnv to evaluate the flake, 30s for `pnpm install --offline`, and a few GB of new files.

Tools that copy the ignored files over (worktrunk's `copy-ignored`, `cp -c`, and the git-cow family) use copy-on-write per file. That saves disk, but the time grows with the file count. Copying that repo's 549k ignored files took minutes, and `clonefile` on the root `node_modules` alone took 15s.

## How

clove keeps one **golden** disk image per repo: an APFS volume holding a worktree of the base branch, with dependencies installed and dev environments warmed.

1. `clove new` clones the image *file* with `clonefile(2)`. That's one syscall, no matter how many files are inside, and costs no disk until something is written.
2. It mounts the clone, about 0.7s.
3. It registers the volume as a regular git worktree, using a copy of the golden index. The clone keeps inodes and timestamps, so git doesn't rehash anything.
4. It checks out your branch and runs only the setup steps whose inputs changed since the golden commit. If the lockfile is the same, `pnpm install` doesn't run.

Measured on that monorepo over five runs: 1.04 to 1.16s per clone, 6.1 MB of disk each. `clove init` took 1m18s once.

On Linux the golden image is a btrfs image file instead, mounted once per repo. The golden worktree is a subvolume in it, and a clone is a snapshot of that subvolume, so there is no mount per clone. A snapshot of a 185k-file golden took about 8 ms, and the first `git status` in the clone 45 to 61 ms, with no rehash.

Clones are isolated. Adding a dependency or editing the nix flake in one clone doesn't affect the others. File watchers, Docker bind mounts, Go and Rust builds all work inside a clone.

## Install

macOS with APFS. Built and tested on macOS 26, which has the faster ASIF images. Older versions fall back to `hdiutil` sparse images, which is written but untested.

Linux needs btrfs-progs (`sudo apt install btrfs-progs`) and loop devices. The host disk can be any filesystem. Kernel 6.7 or later gives exact clone sizes (simple quotas). On older kernels the image uses full qgroups, or reports no sizes. `clove init` offers to add one line to `/etc/fstab`, with sudo, once per repo:

```
~/.clove/repos/<repo>-<hash>/pool.btrfs ~/src/<repo>.clove btrfs loop,user,noauto,exec,user_subvol_rm_allowed 0 0
```

With it, clove mounts the image as you, at `<repo>.clove`, from a desktop or over SSH. Without it, clove mounts through udisks2 if it's installed, which works only from a desktop session and puts clones under `/run/media/$USER/` (or `/media/$USER/`). `sudo -n` is the last resort. `clove doctor` says which one a repo uses.

```
brew install pierre-borckmans/tap/clove
clove shell-init --install         # adds the shell integration to ~/.zshrc (bash and fish too)
```

Homebrew works on macOS and Linux. Prebuilt binaries for both are on the [releases page](https://github.com/pierre-borckmans/clove/releases). From source: `cargo install --git https://github.com/pierre-borckmans/clove clove-cli`.

Shell integration lets `clove new` and `clove cd` change your directory, and adds completions for clone and branch names.

## Use

```
clove init                    # build the golden image (once; minutes on a big repo)
clove new feat/login          # branch from origin/main, or check out an existing branch
clove new feat/login -x claude   # and start an agent in it
clove ls                      # clones, branch, dirty/ahead/behind, own disk
clove cd feat-login           # jump to a clone; `clove cd` goes back to the main checkout
clove run feat-login -- pnpm test   # run inside the clone's dev environment
clove rm feat-login -D        # unmount, delete, delete the branch
clove prune                   # remove clones whose PR is merged, and their branches
clove setup                   # in a clone, after a pull: rerun the setup steps that changed
clove refresh                 # bring the golden image to the latest base (incremental)
clove doctor                  # check the setup, with a fix for each problem
clove repos                   # every repo clove manages, with golden state and merged clones
clove config --edit           # open your settings for this repo (--edit team, --edit all)
```

The same features are in three places, and each one gets new features at the same time:

- **The CLI**, above.
- **`clove ui`**, a terminal UI with mouse support. A sidebar lists every repo, with its clones and whether its golden image is fresh. `s` opens settings (config files, steps, service), `g` cleans up merged clones and leftovers, and Enter quits into the selected clone.
- **The Mac app** (`apps/desktop`), with the same sidebar, settings and cleanup, plus a menu-bar popover.

Every command takes `--json`. `clove new --json` prints the clone's path, branch and timings, so an agent can create its own worktree:

```
path=$(clove new --json agent/task-42 | jq -r .path)
```

`clove new pr:123` checks out a pull request as branch `pr/123`.

`clove prune` asks GitHub (through `gh`) for each clone's pull request. A clone goes when its PR is merged, its tip is the commit GitHub merged (or one the PR contains), and it has no uncommitted changes. That works for squash merges, which `git branch --merged` misses. A clone with commits after the merge, or with local edits, is listed for review and kept, as is any branch without a PR, since that may be fresh work. It lists first and asks; `--apply` skips the question. Run from inside a merged clone, it leaves the clone first. `clove ls` shows each clone's PR from the last check, which the background service refreshes.

To make Claude Code put its worktrees (`claude --worktree`, subagents) in clones, merge the block from `clove hook claude` into `~/.claude/settings.json`. The create hook makes a clone named after the worktree, and the remove hook deletes it, along with the branch if the branch has no commits of its own.

## Staying fresh

The golden image is a recipe, not a snapshot of your checkout: the base branch, plus the files listed in `copy`, plus the `warm` steps. clove rebuilds it from that recipe and keeps it current.

- **Refresh is incremental.** `clove refresh` clones the current golden image, checks out the latest base and reruns only the warm steps whose `watch` paths changed since the previous golden commit, or whose definition changed. On the monorepo, a refresh where nothing relevant changed takes 4.3s. `--full` reruns everything.
- **Stale means a clone would redo work.** The golden image is stale as soon as a step's `watch` paths changed on the base, or the step's definition changed, because a new clone would rerun that step. `clove status` names the step, e.g. `build-deps inputs changed in 3 commits`. Dozens of commits behind with no step inputs touched is still fresh. `max_age` and `max_behind` are a safety net on top.
- **Stale images refresh themselves.** When `clove new` finds the image stale, it starts a refresh in the background and doesn't wait, so the next clone starts fresh.
- **The service follows your pulls.** `clove init` offers it in an interactive terminal, or run `clove service install`. It adds two launchd agents (systemd user units on Linux: `clove-mount.service`, and `clove-refresh` as a service, a timer and a path unit): one remounts clones at login, the other runs `clove refresh --all-repos --if-stale` whenever git rewrites a repo's `FETCH_HEAD` (any `git fetch` or `git pull` in the main checkout), and every 4h for repos nobody fetches in. A fresh repo costs at most one `git fetch`.
- **Refreshing never touches existing clones.** It builds the next generation next to the current one and swaps it in atomically, and `clove new` keeps working while it runs.
- **Clones catch up with `clove setup`.** After a `git pull` or a rebase in a clone, it copies your local files again and reruns only the setup steps whose inputs changed since they last ran.

## Configure

clove detects steps from the repo: mise, direnv (including nix-direnv's cache), pnpm, bun, yarn, npm and uv. `clove config` shows what it found, and `clove config --write` saves it to `.clove.toml` so your team gets the same setup.

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

Files that only exist in your checkout, like `.env` or `.envrc.local`, never reach a fresh checkout. clove detects the ones git ignores (`**/.env`, `**/.env.*`, `**/.envrc.local`, `**/.dev.vars`). It copies them into the golden image before warming, so codegen can read them, and again into each new clone, so the clone gets their current contents. List them yourself to override:

```toml
copy = [".env", "apps/*/.env.local", "config/dev.secrets.json"]

[refresh]
auto = true        # refresh in the background when `clove new` finds the image stale
max_age = "24h"     # safety net; steps' watch paths are the main signal
max_behind = 100   # commits
```

Steps run inside the repo's environment (`direnv exec`, or `mise exec`). `[[step]]` replaces the detected steps; `[[add_step]]` appends to them. Settings layer in this order, last wins:

1. `~/.config/clove/config.toml`: personal defaults (`clone_root`, `image_size`, `fetch`, `fetch_interval`)
2. `.clove.toml` in the repo: shared with the team
3. `~/.config/clove/repos/<repo>.toml`: personal overrides for one repo, for steps you don't want to commit

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

- **macOS and Linux.** macOS needs APFS (`clonefile` and disk images). Linux needs btrfs-progs and loop devices, and an `/etc/fstab` line to mount without a desktop session (see Install). Linux has no end-to-end test in CI yet.
- **On Linux, clones share one volume per repo.** `clove unmount` without names unmounts it, if nothing uses it. Unmounting a single clone isn't possible. `clone_root` must be a folder of its own for each repo, which is the default. Space a removed clone held comes back a few seconds later, when btrfs cleans up.
- **Each clone is a mounted volume** (on Linux, the repo's one volume). Mounts don't survive a reboot. Run `clove mount --all-repos`, or install the service.
- **Absolute paths don't move.** Python virtualenvs and cargo's `target/` fingerprints record the path they were built at, so they rebuild in a clone. Bazel keeps one output base per path.
- **Daemons started by warm steps** (a Bazel server, git's fsmonitor) get stopped when the golden image unmounts. clove says which ones.

## Layout

```
~/.clove/repos/<repo>-<hash>/
  golden/current -> gen-N      the image, HEAD and index snapshot, meta.json
  clones/<name>/               the clone's image and meta.json
<repo parent>/<repo>.clove/<name>/<repo>   where a clone is mounted
```

The worktree sits one level inside the volume, so the volume's own files stay out of `git status` and the folder keeps the repo's name.

On Linux, `~/.clove/repos/<repo>-<hash>/pool.btrfs` is the repo's one image, mounted at `<repo>.clove` (or where udisks put it, recorded in `mount` next to it). Inside it, `.clove-golden/` is the golden worktree, `.clove-gens/gen-N` its read-only snapshots, and `<name>/<repo>` each clone.

## Develop

```
cargo test --workspace        # unit tests + an end-to-end run on real disk images
scripts/smoke.zsh             # the CLI through the zsh integration, on a throwaway repo
```

MIT licensed.
