# Exploration: checkpoints and forks

Branch `explore/checkpoints`. A prototype to play with, in the CLI and the desktop app, macOS tested. Nothing here is committed.

## What it is

A clone is already a copy-on-write copy of the golden image. The same trick works on a clone itself:

- **Checkpoint.** Freeze a clone as it is right now. That covers tracked files, uncommitted edits, untracked files, ignored files (`node_modules`, `dist`), and the clone's git state (HEAD, index, reflog, rebase state). It takes about 0.05s.
- **Rewind.** Put a checkpoint back in about 1.5s, branch commit included.
- **Fork.** Make new clones from another clone's current state, mid-work, each on its own branch. Give three agents three forks of the same state, compare, keep the winner.

## Commands

```
clove checkpoint <clone> [name]          # default name cp-1, cp-2, ...; `.` = the clone you're in
clove checkpoint <clone> <name> --drop   # delete one
clove checkpoints [clone]                # NAME, AGE, HEAD, OWN (disk only it holds), TOOK
clove rewind <clone> [name]              # default: newest checkpoint you made
        --force                          #   unmount even with open files (a shell inside then needs `cd $PWD`)
        --no-save                        #   don't keep the replaced state as `before-rewind`
clove fork <clone> <name>... [--branch b | --detach]
```

Every command takes `--json`. The core API is in `crates/clove-core/src/checkpoint.rs` (`Clove::checkpoint`, `checkpoints`, `drop_checkpoint`, `rewind`, `fork`), so the TUI and app can call it the same way the CLI does.

Measured on the demo repo (18k files in `node_modules`, M-series Mac, debug build):

| Step | Time |
|---|---|
| `clove new` | 0.7-0.9s |
| `checkpoint` | 0.02-0.07s |
| `rewind` | 1.2-1.7s |
| `fork` into 3 | 1.3-1.8s |

File count doesn't matter. Each operation copies one image file or snapshots one subvolume.

## How it works

Layout: `~/.clove/repos/<id>/clones/<name>/checkpoints/<id>/{meta.json, git/, image.asif}`.

Six new `Store` methods in `backend/mod.rs`: `checkpoint_create`, `checkpoint_restore`, `checkpoint_rename`, `checkpoint_delete`, `checkpoint_bytes` and `clone_fork`.

### macOS (APFS images)

- **Checkpoint and fork** call `clonefile(2)` on the clone's image file while it stays mounted. First, `F_FULLFSYNC` on the mount root forces APFS to commit and flush to the image's backing file (`sys_macos::flush`, ~30ms). I tested this before writing it. Without the flush, a clone of the image taken right after writing 3000 files had none of them. With it, five runs of 5000 files plus a 30MB file and a rename all came through intact.
- **Rewind** detaches the clone, moves its image aside, `clonefile`s the checkpoint into place, and attaches. If anything fails it puts the old image back. The detach and attach account for nearly all of the ~1.5s.
- Forks mount in parallel threads, so three forks take about as long as one.

### Linux (btrfs)

Checkpoints are read-only snapshots at `<clone_root>/.clove-checkpoints/<clone>/<id>`. Rewind renames the live subvolume aside, snapshots the checkpoint into its place, then deletes the old one. No unmount, so processes inside keep the old files until they close them. A fork is a writable snapshot of the source subvolume.

**This code has never been compiled.** There's no Linux target on this machine. Expect small fixes.

### Git state

The clone's HEAD and index live in the main repo's `.git/worktrees/clove-<name>/`, outside the image. So:

- A checkpoint copies that admin dir (minus `gitdir`, `commondir` and `locked`) into `checkpoints/<id>/git/`. It records HEAD's branch and commit, and writes `refs/clove/checkpoints/<clone>/<id>` so `git gc` can't drop the commit.
- Rewind restores the admin dir and moves the branch back with `git update-ref -m "clove rewind ..."`. Commits made after the checkpoint stay in the reflog and in the `before-rewind` checkpoint's ref.
- Git state gets copied *before* the image freezes. If a commit lands in between, the image comes out newer than HEAD. That shows up as uncommitted changes, never as lost ones.
- A fork copies the admin dir minus `logs/`, gets its own `gitdir`, rewrites the `.git` file inside the copied image (it still pointed at the source), and creates its branch at the source's HEAD. Inodes and mtimes survive the copy, so the copied index stays valid and `git status` rehashes nothing.
- Fork reruns the toolchain path fixups (`venv::relocate`), as `clove new` does. Venvs embed the source clone's absolute path.

## Decisions

- **Rewind keeps what it replaces** as `before-rewind`, so `clove rewind x before-rewind` undoes it. Only one is kept, and each rewind replaces it. `--no-save` skips it. Rewind with no name picks the newest checkpoint you made and skips `before-rewind`.
- **Busy clones.** Checkpoint and fork work on a clone in use, and the copy matches what a power cut at that instant would leave. Data still buffered inside a process isn't in it. Rewind refuses while anything has files open in the clone and names the processes, before saving anything. `--force` unmounts anyway. Fork stops git's fsmonitor and orphan daemons first, like `rm` and `unmount` do. When run from inside the clone, the CLI moves its own cwd out of the way.
- **Fork branch names.** Each fork gets `<source branch>-<fork name>` by default, e.g. `feat/login-a`. A detached source gives `<fork name>`. `--branch` works for a single fork only. Fork refuses an existing branch or clone name, and refuses a source stopped mid-rebase or mid-merge, since the copied rebase state would finish onto the source's branch.
- **Disk.** OWN is APFS's private size of the checkpoint image (btrfs: qgroup exclusive bytes), meaning what dropping it frees. It starts at 0 and grows as the clone drifts away from it. The flip side: right after a checkpoint, the clone's own OWN in `clove ls` drops toward 0 too, because its blocks are now shared. Checkpoints don't show in `clove ls` or `clove status` yet.
- **Removal.** `clove rm` and `clove gc` delete a clone's checkpoints and refs with it (`forget_checkpoints`).
- **Forks are ordinary clones.** Their `meta.json` is the source's (same golden generation, base, `setup_commit`) with a new name, branch and path, plus `forked_from: <source>`. `CloneInfo` carries `forked_from` and a `checkpoints` count (a directory count, no image reads).

## Open questions

- **Rewind needs an unmount on macOS.** A shell sitting in the clone blocks it without `--force`, and after `--force` the shell needs `cd $PWD`. A gentler variant would mount the restored image next to the old one and swap the mount points with `rename` (the spare clone already does that). Processes inside would keep the old volume until they leave. That's nicer for shells but sneakier for dev servers, which would keep serving the old files.
- **Should checkpoints be automatic?** E.g. before `clove update`, or each time an agent finishes a turn (a Claude Code hook). At ~0.05s and ~0 bytes up front, that's affordable.
- **Index-only blobs.** A file staged but never committed is reachable only from the index. If the checkpoint is older than gc's prune window (2 weeks), those blobs could go. A ref to a tree written from the saved index would fix it.
- **Checkpoints of a clone made from an older golden generation** keep that image alive. They already do the same via the clone itself, so nothing new, but `clove gc` doesn't mention them.
- **Compare forks.** `clove diff a b`, or `clove ls --forks-of x`, would make the "keep the winner" step easier. Today you compare branches with git.
- **TUI parity.** The app has it all now (below); the TUI has none of it yet. That's the parity work left: a checkpoints count and `forked from` on the clone row, Checkpoint / Rewind… / Fork in its actions, and a rewind picker.
- **Agents guide.** `agents::GUIDE` got a short section, because a CLI test requires every command in it. That changes the installed-instructions text if this merges.

## In the app

The map is a garlic: the golden image is the bulb, clones are cloves on rings.

- **Fork.** Hold Option and pull a clove off a clone. A pale copy tears free on a stem back to it; the clone stays. Let go past tearing ("let go to fork") and the fork appears on `<clone>-a`, branch `<branch>-a` (first letter free for both, `Clove::fork_auto`). Without Option the pull still rips the clone off (remove, 5s undo). Also "Fork" in the clone's ring menu and list menu.
- **Lineage.** A fork sits right after its parent on the same ring (forks share their parent's generation), one step outside it, joined to the parent clove by a stem instead of to the bulb. If the parent is gone it falls back to the bulb edge. The list and the map's bottom bar say "forked from X".
- **Checkpoints are skins.** A clove with checkpoints wears 1 to 3 thin rings, with a small count past 3. "Checkpoint" in the ring menu saves one (~50ms, a toast). "Rewind…" opens a picker (name, age, size, branch@sha; `before-rewind` included, drop on hover). While the rewind runs, the skins peel off the clove and it turns over.
- **Busy clone.** A rewind refused because something has files open shows a toast naming the processes, with "Rewind anyway" (`force`).
- Tauri: commands `list_checkpoints`, `checkpoint_clone`, `drop_checkpoint`; ops `fork` and `rewind` (progress events, Activity cards). The browser mock (`pnpm dev`) has a clone with 2 checkpoints, a fork of it, and a busy clone for the "Rewind anyway" path.

## Dev and demo runs never touch the real install

`CLOVE_NO_SERVICE=1` makes `service::install/sync/uninstall` no-ops (they log a skip), makes `service::cli_exe` fail so nothing spawns the installed CLI (spare fill, `cli_version`, the app's upgrade hand-off), hides brew (`brew_bin`, `brew_prefix`), refuses `clove upgrade`, and leaves shell rc files alone. `CLOVE_NO_AGENT_HOOKS=1` makes `agents::install_with_report` and `agents::remove` no-ops. `explore/demo.sh` exports both, and runs `init --json`.

An earlier demo run rewrote the real `~/Library/LaunchAgents/dev.clove.*.plist` to point at the debug build and the demo `CLOVE_HOME`: plain `clove init` calls `service::sync()` when the service is installed. That's what the guard is for.

Run the app on the demo state:

```
cd apps/desktop
env $(grep '^export' /tmp/clove-explore-checkpoints/env.sh | sed 's/^export //') pnpm tauri dev
```

## Tests

- `crates/clove-core/tests/checkpoints.rs` covers the whole flow on real disk images. It checkpoints mid-work state right after writing (exercising the flush), wrecks the clone, checks that a busy rewind is refused and saves nothing, then rewinds and checks files, index, staged changes and branch. It also covers undo via `before-rewind`, rewind with no name, three forks (own branch, own registration, same uncommitted state, divergence), name clashes, `--branch`, and `clove rm` cleaning up checkpoints and refs. It checks `forked_from`, the checkpoint count and `fork_auto`'s generated name, and ends with `gc` finding nothing left behind.
- A unit test in `checkpoint.rs` checks the admin-dir copy.
- `cargo test -p clove-core` and `cargo test -p clove-cli` pass. `cargo clippy -p clove-core -p clove-cli --all-targets -- -D warnings` is clean.

## Play with it

```
explore/demo.sh       # builds target/debug/clove, makes /tmp/clove-explore-checkpoints, runs the scenario (CLOVE_NO_SERVICE set)
```

The demo uses its own `CLOVE_HOME`, clone root, user config and git config under `/tmp/clove-explore-checkpoints`, and never touches `~/.clove`, your repos or the service. It builds a repo whose install step writes 18k files and sleeps 2s, then runs `init`, `new feat/login`, makes a commit plus uncommitted, untracked and build-output changes, checkpoints, wrecks it (deletes `node_modules` and `dist`, commits garbage), rewinds, lists checkpoints, forks into `a b c`, and shows the forks diverging. It prints timings at each step.

Then, by hand:

```
source /tmp/clove-explore-checkpoints/env.sh    # sets the isolated env, defines clove(), cd into the repo
clove checkpoints feat-login
clove rewind feat-login before-rewind           # undo the demo's rewind
clove checkpoint b mine                         # then break b however you like
clove rewind b                                  # refused while a shell sits inside b...
clove rewind b --force                          # ...unless forced; that shell then needs `cd $PWD`
clove fork b b2 b3                              # forks of a fork
clove rm c --force
```

When you're done:

```
explore/cleanup.sh    # detaches every image mounted under the demo dir, deletes it
```
