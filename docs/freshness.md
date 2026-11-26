# Staying fresh

[← README](../README.md)

The golden image is a recipe, not a snapshot of your checkout: the base branch, plus the files listed in `copy`, plus the `warm` steps.

- **Refresh is incremental.** `clove refresh` clones the current golden image, checks out the latest base and reruns only the warm steps whose `watch` paths or definition changed. ~4s on the monorepo above when nothing relevant changed. `--full` reruns everything.
- **Stale means a new clone would redo work.**
  - The image is stale once a step's inputs changed on the base, because a new clone would rerun that step. `clove status` names it, e.g. `build-deps inputs changed in 3 commits`.
  - Dozens of commits behind with no step inputs touched is still fresh.
  - `max_age` and `max_behind` are a safety net.
- **Stale images refresh themselves.** `clove new` starts a background refresh when it finds the image stale, without waiting for it.
- **The service follows your pulls.** `clove service install` adds two launchd agents on macOS, or systemd user units on Linux:
  - one remounts clones at login
  - one runs `clove refresh --all-repos --if-stale --prs` whenever git rewrites a repo's `FETCH_HEAD` (any fetch or pull in the main checkout), and every 4h otherwise, which also checks clones' PRs. A fresh repo costs at most one `git fetch`.
- **Refreshing never touches existing clones.** The next generation is built next to the current one and swapped in atomically; `clove new` keeps working meanwhile.
- **Clones catch up with `clove setup`.** After a pull or rebase in a clone, it copies your local files again and reruns only the setup steps whose inputs changed.
