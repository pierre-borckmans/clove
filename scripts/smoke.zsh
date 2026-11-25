#!/usr/bin/env zsh
# CLI smoke test through the real shell integration, on a throwaway repo.
# Usage: scripts/smoke.zsh [path/to/clove]
set -euo pipefail

CLOVE_BIN=${1:-${0:A:h:h}/target/debug/clove}
T=$(mktemp -d "${TMPDIR:-/tmp}/clove-smoke.XXXXXX"); T=${T:A}
export CLOVE_HOME=$T/state CLOVE_CLONE_ROOT=$T/clones CLOVE_CONFIG=$T/none.toml
export GIT_CONFIG_GLOBAL=/dev/null GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
path=(${CLOVE_BIN:h} $path)
trap 'cd /; command clove -C $T/app rm --all --force >/dev/null 2>&1 || true; command clove -C $T/app deinit >/dev/null 2>&1 || true; rm -rf $T' EXIT

pass() { print -P "%F{green}✓%f $1" }
fail() { print -P "%F{red}✗ $1%f"; exit 1 }

git init -q --bare -b main $T/remote.git
git clone -q $T/remote.git $T/seed 2>/dev/null
cd $T/seed
print 'image_size = "2G"\n[refresh]\nmax_behind = 0\n[[step]]\nname = "deps"\nwarm = "mkdir -p deps && touch deps/ok"\nsetup = "touch deps/setup"\nwatch = ["lock.txt"]' > .clove.toml
print 'deps/' > .gitignore; print v1 > lock.txt
git add -A && git commit -qm init && git push -q origin main
git clone -q $T/remote.git $T/app 2>/dev/null
cd $T/app

eval "$(command clove shell-init zsh)"
whence -w clove | grep -q function || fail "shell-init defines a function"
pass "shell-init"

clove init >/dev/null 2>&1 || fail "init"
pass "init"

clove new feat/one >/dev/null 2>&1 || fail "new"
[[ $PWD == $CLOVE_CLONE_ROOT/feat-one/app ]] || fail "new cds into the clone (pwd=$PWD)"
[[ -f deps/ok ]] || fail "golden deps present"
[[ ! -f deps/setup ]] || fail "setup skipped when lock unchanged"
[[ $(git branch --show-current) == feat/one ]] || fail "on branch"
pass "new + cd ($PWD)"

out=$(command clove new --json --no-fetch two 2>/dev/null)
p=$(print -r -- $out | python3 -c 'import json,sys; print(json.load(sys.stdin)["path"])')
[[ -d $p ]] || fail "--json path"
pass "new --json"

clove ls --json | python3 -c 'import json,sys; assert len(json.load(sys.stdin)) == 2' || fail "ls --json"
pass "ls --json"

[[ $(command clove run two -- pwd) == $p ]] || fail "run"
pass "run"

clove cd two; [[ $PWD == $p ]] || fail "cd"
clove cd; [[ $PWD == $T/app ]] || fail "cd (no arg) to main checkout"
pass "cd"

clove cd feat-one
clove rm feat-one >/dev/null 2>&1 || fail "rm from inside the clone"
[[ $PWD == $T/app ]] || fail "rm left us in the main checkout (pwd=$PWD)"
command clove ls --json | python3 -c 'import json,sys; assert [c["name"] for c in json.load(sys.stdin)] == ["two"]' || fail "rm removed it"
pass "rm from inside"

if [[ $(uname) == Linux ]]; then
  # Clones share the repo's volume: `clove unmount` without names unmounts it.
  clove unmount two >/dev/null 2>&1 && fail "unmount of one clone should refuse on Linux"
  clove unmount >/dev/null 2>&1 && clove mount >/dev/null 2>&1 || fail "unmount/mount"
else
  clove unmount two >/dev/null 2>&1 && clove mount >/dev/null 2>&1 || fail "unmount/mount"
fi
pass "unmount + mount"

hp=$(print '{"name":"agent-x","cwd":"'$T/app'"}' | command clove hook worktree-create 2>/dev/null)
[[ -d $hp && $(git -C $hp branch --show-current) == agent-x ]] || fail "claude WorktreeCreate hook (got $hp)"
print '{"worktree_path":"'$hp'","cwd":"'$T/app'"}' | command clove hook worktree-remove >/dev/null 2>&1 || fail "claude WorktreeRemove hook"
[[ ! -d $hp ]] || fail "hook removed the clone"
git -C $T/app rev-parse --verify --quiet refs/heads/agent-x >/dev/null && fail "merged branch deleted by remove hook"
pass "claude worktree hooks"

# a stale golden image refreshes itself in the background after `new`
(cd $T/seed && print x >> lock.txt && git commit -qam bump && git push -q origin main)
gen() { command clove status --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["golden"]["generation"])' }
g0=$(gen)
clove new --fetch --no-cd auto-refresh >/dev/null 2>&1 || fail "new on a stale golden"
for i in {1..60}; do [[ $(gen) -gt $g0 ]] && break; sleep 0.5; done
[[ $(gen) -gt $g0 ]] || fail "background refresh did not produce a new generation"
for i in {1..20}; do command clove status --json | grep -q '"refreshing": null' && break; sleep 0.5; done
pass "auto refresh in the background (gen $g0 -> $(gen))"

clove rm --all >/dev/null 2>&1 || fail "rm --all"
[[ -z $(command clove gc --json | python3 -c 'import json,sys; print(*json.load(sys.stdin)["actions"])') ]] || fail "gc clean"
clove deinit >/dev/null 2>&1 || fail "deinit"
pass "rm --all, gc, deinit"
