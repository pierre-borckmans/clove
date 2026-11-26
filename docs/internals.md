# Internals

[← README](../README.md)

## macOS: one APFS volume per clone

The golden image is an APFS volume in a disk image file. `clove new`:

1. copies the image *file* with `clonefile(2)`: one syscall whatever is inside, and no disk used until something is written
2. mounts the copy as its own volume (~0.7s)
3. registers the worktree and runs the changed steps

On the monorepo above: ~1.1s and ~6 MB of disk per clone. `clove init` took ~1m20s, once.

## Linux: one btrfs volume per repo

- Each repo has one btrfs image file, mounted once.
- The golden worktree is a subvolume in it, and a clone is a snapshot of that subvolume, so there's no mount per clone.
- On a ~185k-file golden: the snapshot takes ~8 ms, and the first `git status` in the clone ~50 ms, with no rehash.

## Layout on disk

```
~/.clove/repos/<repo>-<hash>/
  golden/current -> gen-N        the golden image, HEAD and index snapshot, meta.json
  clones/<name>/                 each clone's image and meta.json
  history.jsonl                  what clove removed (clove log)
<repo parent>/<repo>.clove/<name>/<repo>   where a clone is mounted
```

- The worktree sits one level inside the volume, so the volume's own files stay out of `git status` and the folder keeps the repo's name.
- On Linux, `~/.clove/repos/<repo>-<hash>/pool.btrfs` is the repo's one image, mounted at `<repo>.clove` (or where udisks put it, recorded in `mount` next to it). Inside it:
  - `.clove-golden/` is the golden worktree
  - `.clove-gens/gen-N` are its read-only snapshots
  - `<name>/<repo>` is each clone
