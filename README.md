# clove

A ready git worktree in about a second, with `node_modules`, the dev shell and every other ignored file already there.

```
$ clove new feat/login
+ feat/login
  ✓ clone image              0.00s
  ✓ mount                    0.70s
  ✓ checkout                 0.21s
  ✓ setup mise               0.02s
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

Clones are isolated. Adding a dependency or editing the nix flake in one clone doesn't affect the others. File watchers, Docker bind mounts, Go and Rust builds all work inside a clone.

## Install

macOS with APFS. Built and tested on macOS 26, which has the faster ASIF images. Older versions fall back to `hdiutil` sparse images, which is written but untested.

```
cargo install --git https://github.com/pierre-borckmans/clove clove-cli
eval "$(clove shell-init zsh)"     # in ~/.zshrc; bash works too, fish: clove shell-init fish | source
```

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
clove setup                   # in a clone, after a pull: rerun the setup steps that changed
clove refresh                 # bring the golden image to the latest base (incremental)
clove doctor                  # check the setup, with a fix for each problem
```

Every command takes `--json`. `clove new --json` prints the clone's path, branch and timings, so an agent can create its own worktree:

```
path=$(clove new --json agent/task-42 | jq -r .path)
```

`clove new pr:123` checks out a pull request as branch `pr/123`.

To make Claude Code put its worktrees (`claude --worktree`, subagents) in clones, merge the block from `clove hook claude` into `~/.claude/settings.json`. The create hook makes a clone named after the worktree, and the remove hook deletes it, along with the branch if the branch has no commits of its own.

## Staying fresh

The golden image is a recipe, not a snapshot of your checkout: the base branch, plus the files listed in `copy`, plus the `warm` steps. clove rebuilds it from that recipe and keeps it current.

- **Refresh is incremental.** `clove refresh` clones the current golden image, checks out the latest base and reruns only the warm steps whose `watch` paths changed since the previous golden commit, or whose definition changed. On the monorepo, a refresh where nothing relevant changed takes 4.3s. `--full` reruns everything.
- **Stale images refresh themselves.** The golden image is stale when it's older than `max_age` or more than `max_behind` commits behind the base. When `clove new` finds it stale, it starts a refresh in the background and doesn't wait, so the next clone starts fresh. `clove status` shows how fresh it is and whether a refresh is running.
- **The service checks hourly.** `clove service install` adds two launchd agents: one remounts clones at login, the other runs `clove refresh --all-repos --if-stale` every hour. A repo that is still fresh costs one `git fetch`.
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
max_age = "24h"
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

- **macOS only.** It needs APFS `clonefile` and disk images. Linux could do the same with a reflinked image file on btrfs or XFS, but that isn't written yet.
- **Each clone is a mounted volume.** Mounts don't survive a reboot. Run `clove mount --all-repos`, or install the service.
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

## Develop

```
cargo test --workspace        # unit tests + an end-to-end run on real disk images
scripts/smoke.zsh             # the CLI through the zsh integration, on a throwaway repo
```

MIT licensed.
