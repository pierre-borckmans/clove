# Limits

[← README](../README.md)

Everywhere:

- **Mounts don't survive a reboot.** Run `clove mount --all-repos`, or install the service.
- **Absolute paths don't move.** Python virtualenvs and cargo's `target/` fingerprints record the path they were built at, so they rebuild in a clone. Bazel keeps one output base per path.
- **Daemons started by warm steps** (a Bazel server, git's fsmonitor) are stopped when the golden image unmounts, and clove says which.

On macOS:

- **Each clone is its own mounted volume**, so a machine with many clones has many mounts.

On Linux:

- **Clones share one volume per repo.** `clove unmount` without names unmounts it if nothing uses it; a single clone can't be unmounted. `clone_root` must be a folder of its own per repo (the default).
- **Freed space comes back a few seconds late**, once btrfs cleans up a removed clone.
- **No end-to-end test in CI yet**; unit tests run on Linux.
- **The app's tray icon needs AppIndicator support.** KDE, Ubuntu's GNOME and most other desktops have it; stock GNOME needs the AppIndicator extension. Without it the app still works: closing the window hides it, and launching the app again brings it back.
- **The app can't add the `/etc/fstab` line** that `clove init` offers at a terminal; it needs sudo. `clove doctor` and the app's Doctor page show the command.
