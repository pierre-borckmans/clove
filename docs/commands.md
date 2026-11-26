# Commands and interfaces

[← README](../README.md)

## Commands

```
# create
clove new feat/login             # branch from origin/main, or check out an existing branch
clove new feat/login -x claude   # ...and start an agent in it
clove new pr:123                 # check out pull request #123 as branch pr/123
clove adopt ../old-worktree      # turn a plain git worktree into a clone on its branch

# work
clove ls                         # clones, branch, dirty/ahead/behind, own disk
clove cd feat-login              # jump to a clone; `clove cd` alone goes back to the main checkout
clove run feat-login -- pnpm test   # run inside the clone's dev environment
clove setup                      # in a clone, after a pull: rerun the setup steps that changed

# clean up
clove rm feat-login -D           # unmount, delete, and delete the branch
clove prune                      # remove clones whose PR is merged, and their branches
clove log                        # what clove removed, when, why, and how to get the branch back

# maintain
clove refresh                    # bring the golden image to the latest base (incremental)
clove doctor                     # check the setup, with a fix for each problem
clove repos                      # every repo clove manages, golden state, merged clones
clove repos --candidates         # git repos in your code folders clove could manage

# configure
clove config get                 # each setting, its value, and the file it comes from
clove config set fetch false     # change one setting (--layer team|all; default: you, this repo)
clove config --edit              # open your settings for this repo (--edit team, --edit all)
```

## Interfaces

Every feature is in three places:

- **The CLI** above. Every command takes `--json`; `clove new --json` prints the clone's path, branch and timings.
- **The terminal UI:** `clove` on its own (or `clove tui`), with mouse support.
  - A sidebar lists every repo, its clones, and whether its golden image is fresh.
  - `s` settings (Enter edits or toggles, ←/→ picks the file it saves to, `x` resets) · `g` clean up · `L` removal history · `A` add a repo · Enter quits into the selected clone.
- **The Mac app:** the same sidebar, settings, cleanup and history, plus a menu-bar popover.

## Shell integration

`clove shell-init --install`:

- lets `clove new`, `clove cd` and the terminal UI change your directory
- completes clone and branch names
- gives your shell each clone's environment while you're in it:
  - clove writes a `.clove-env` next to the clone's worktree, outside git, with the variables its steps use (pnpm's store, your config's `env`)
  - the shell loads it on `cd` and restores your variables on the way out
  - without it, a plain `pnpm i` in a clone would start an empty store and rebuild `node_modules`
  - only files clove wrote are loaded: each carries a per-user token, so a repo can't ship one

Shells without the integration can use `clove run <clone> -- <command>`.

## Plain git worktrees

Worktrees made before clove, or by an agent's `git worktree add`, show up in `clove ls` too.

- **`clove rm <path or folder name>`** removes one with `git worktree remove`, never forced. A locked or modified worktree stays, and so does a detached one holding commits on no branch.
- **`clove adopt`** turns one into a clone on the same branch, and puts it back if that fails. Ignored files in it (build output, `.env`) aren't carried over.
