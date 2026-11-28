# Checkpoints, rewind and fork

[← README](../../README.md)

A clone is a copy-on-write copy of the golden image. The same trick works on a clone itself:

- **Checkpoint** freezes a clone as it is now: tracked, uncommitted, untracked and ignored files (`node_modules`, `dist`), and its git state (HEAD, index, reflog, rebase state). About 0.05s; it holds no disk until the clone drifts away from it.
- **Rewind** puts a checkpoint back, branch commit included, in about 1.5s. What it replaces is kept as `before-rewind`, so `clove rewind <clone> before-rewind` undoes it (one is kept; each rewind replaces it; `--no-save` skips it).
- **Fork** makes new clones from another clone's current state, mid-work, each on its own branch (`<branch>-<suffix>` by default). Give three agents three forks of the same state, compare, keep the winner.

```
clove checkpoint <clone> [name]          # default name cp-1, cp-2, ...; `.` = the clone you're in
clove checkpoint <clone> <name> --drop   # delete one
clove checkpoints [clone]                # NAME, AGE, HEAD, OWN (disk only it holds), TOOK
clove rewind <clone> [name]              # default: the newest one you made
        --force                          #   unmount even with files open (a shell inside then needs `cd $PWD`)
        --no-save                        #   don't keep the replaced state as `before-rewind`
clove fork <clone> <name>... [--branch b | --detach]
```

The core API is `Clove::checkpoint`, `checkpoints`, `drop_checkpoint`, `rewind`, `fork` and `fork_auto` (`crates/clove-core/src/checkpoint.rs`), shared by the CLI and the app.

## How it works

State lives at `~/.clove/repos/<id>/clones/<name>/checkpoints/<id>/{meta.json, git/, image}`. The backends implement `checkpoint_create`, `checkpoint_restore`, `checkpoint_rename`, `checkpoint_delete`, `checkpoint_bytes` and `clone_fork`.

- **macOS (APFS images).** Checkpoint and fork `clonefile(2)` the clone's image while it stays mounted, after an `F_FULLFSYNC` on the mount root so APFS flushes to the image file (without it, a copy taken right after writing thousands of files missed them). Rewind detaches the clone, swaps the image, and attaches; on failure it puts the old image back. Forks mount in parallel.
- **Linux (btrfs).** Checkpoints are read-only snapshots at `<clone_root>/.clove-checkpoints/<clone>/<id>`. Rewind renames the live subvolume aside, snapshots the checkpoint into its place, and deletes the old one; there's no unmount, so processes inside keep the old files until they close them. A fork is a writable snapshot.
- **Git state** lives outside the image, in the main repo's `.git/worktrees/clove-<name>/`. A checkpoint copies that admin dir (minus `gitdir`, `commondir`, `locked`), records HEAD's branch and commit, and writes `refs/clove/checkpoints/<clone>/<id>` so `git gc` keeps the commit. Rewind restores it and moves the branch with `git update-ref`. Git state is copied before the image freezes, so a commit landing in between shows as uncommitted changes, never lost ones. A fork gets its own admin dir and `.git` file, and its branch at the source's HEAD; inodes survive the copy, so the copied index stays valid. Fork reruns the toolchain path fixups, as `clove new` does.
- A fork records `forked_from` in its `meta.json`; `CloneInfo` carries it and a `checkpoints` count.

## Rules

- **Busy clones.** Checkpoint and fork work on a clone in use; the copy is what a power cut at that instant would leave (data still buffered in a process isn't in it). Rewind refuses while something has files open in the clone and names the processes, before saving anything; `--force` unmounts anyway (the app offers "Rewind anyway").
- **Removing a parent.** `clove rm` on a clone with forks asks in a terminal (default: remove them too, deepest first); a script or `--json` must say `--with-forks` or `--keep-forks`. Kept forks become plain clones of the image.
- **Fork names.** `<source branch>-<suffix>`, where the suffix is the fork's name minus the source clone's `<name>-` prefix (`feat-search-b` forked from `feat-search` on `feat/search` gets `feat/search-b`); a name without that prefix is used whole. A detached source gives `<name>`. Fork refuses an existing branch or clone name, and a source stopped mid-rebase or mid-merge. `fork_auto` (the app) picks `<clone>-a`, `-b`, ... free for both clone and branch.
- **Disk.** OWN is what dropping a checkpoint frees. Right after a checkpoint, the clone's own OWN drops toward 0 too, because its blocks are now shared.
- **Removal.** `clove rm` and `clove gc` delete a clone's checkpoints and refs with it.

## In the app

The map is a garlic: the golden image is the bulb in the centre, clones are cloves on a ring, joined to it by stems.

- **Radius means lineage.** Every clone that isn't a fork sits on one ring, in a wedge sized by its forks; forks grow outward from their parent, fanned so stems never cross. A lineage with more than 3 forks, or deeper than 2, folds into a bulb (`+N`); open and closed lineages are shared with the list, which shows the same tree.
- **Generation is in the bulb.** Its rim is the current image; each older one is a dashed contour inside it, and a clone's stem roots on its generation's contour. Older clones' skins are paler and papery.
- **Checkpoints are skins** following the clove's outline (up to 3, then a count). Rewind peels them off.
- **Gestures.** Pulling a clove outward (a bigger radius from the bulb) rips it off, with its forks, behind a 5s undo; sideways or inward it only budges. Option-pulling outward forks it. Pulling out of the bulb makes a new clone.
- **Ripping** keeps the stem attached while you pull, dashed and reddening with the radius gained, fully red past the tear point; let go there and the stem snaps back to its root as the clove (and its forks) drift off, behind a 5s undo. Elsewhere it springs back.
- **Refreshing** the image draws a new skin round the bulb while the label shows the live step; when it lands, the new rim settles and the old one moves inward to become the first contour.
- **The list** shows the same fork tree: forks under their parent with connectors, a chevron and "+N forks" per parent (Left/Right, `*`, Option-click, and an Expand/Fold forks toggle), the same open or folded state as the map's bulbs. The map's bottom bar has the same toggle, next to the zoom controls; it refits the camera when the map grows or shrinks by a quarter. Removing a parent offers "Also remove N forks".
- **One panel at the map's foot.** At rest it is a slim legend row (with Expand/Fold forks, zoom and fit). Picking something grows it upward into details: the bulb shows the image (commit, base, size, age, warm steps); a clove its branch, PR, changes, checkpoints and lineage (only when it has some), setup timings, disk and path; a plain worktree its branch, path and changes. Escape or a background click shrinks it back. The camera glides so the picked node, or its open ring menu, stays above the panel.
- **Nothing extra without forks or checkpoints.** A repo that uses neither looks as it did: no tree gutter in the list or `clove ls`, no lineage section, no Rewind action until a clone has a checkpoint.
- One SVG scene on one animation clock (`map/scene.ts`): the layout is a pure function, a spring per node eases toward it, and every stem, label and badge is drawn from the springs each frame.

## Dev and demo runs

`CLOVE_NO_SERVICE=1` makes service install, sync and uninstall no-ops, stops anything from running the installed `clove` CLI (spare fill, version check, the app's upgrade hand-off), hides brew, refuses `clove upgrade`, and leaves shell rc files alone. `CLOVE_NO_AGENT_HOOKS=1` leaves coding agents' files alone. Use both (with your own `CLOVE_HOME`) for a debug build or a demo, so it can't rewrite the real launchd service to point at itself.

## Known limits

- **btrfs is untested.** The Linux backend's checkpoint code has never run, and is only checked by reading.
- **Rewind unmounts on macOS**, so a shell sitting in the clone blocks it without `--force`, and needs `cd $PWD` after.
- **Staged-only blobs.** A file staged but never committed is reachable only from the saved index; past git's prune window (2 weeks) its blob could go.
- **Old generations stay.** Checkpoints of a clone made from an older golden image keep that image alive, as the clone does.
- **The terminal UI** doesn't show checkpoints or forks yet.
