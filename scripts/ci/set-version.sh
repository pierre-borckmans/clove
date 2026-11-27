#!/usr/bin/env bash
# Stamps a release's version into the build: the workspace version in
# Cargo.toml, and the clove crates' entries in both lockfiles, so --locked
# builds still pass. The tag is the source of truth, not main.
#   set-version.sh 0.2.0      (or v0.2.0)
set -euo pipefail
v="${1#v}"
[[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || {
  echo "not a version: $1" >&2
  exit 1
}
cd "$(dirname "$0")/../.."

# [workspace.package] holds the first `version =` line.
awk -v v="$v" '!done && /^version = "/ { print "version = \"" v "\""; done = 1; next } { print }' \
  Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml

# The version line after each clove crate's `name =` line.
for lock in Cargo.lock apps/desktop/src-tauri/Cargo.lock; do
  awk -v v="$v" '
    /^name = "clove-(cli|core|tui)"$/ { mine = 1; print; next }
    mine && /^version = "/ { print "version = \"" v "\""; mine = 0; next }
    { mine = 0; print }
  ' "$lock" > "$lock.tmp" && mv "$lock.tmp" "$lock"
done

grep -q "^version = \"$v\"" Cargo.toml || { echo "Cargo.toml not stamped" >&2; exit 1; }
echo "version $v"
