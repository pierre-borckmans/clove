#!/usr/bin/env bash
# clove checkpoints and forks, end to end, on a throwaway repo.
# Everything lives under $DEMO with its own CLOVE_HOME: your real clove state,
# repos and service are never touched. explore/cleanup.sh undoes it all.
set -euo pipefail

DEMO=${DEMO:-/tmp/clove-explore-checkpoints}
SRC=$(cd "$(dirname "$0")/.." && pwd)
PACKAGES=${PACKAGES:-300}        # fake node_modules: PACKAGES x 60 files

if [ -e "$DEMO" ]; then
  echo "$DEMO exists; run $SRC/explore/cleanup.sh first" >&2
  exit 1
fi
mkdir -p "$DEMO"
DEMO=$(cd "$DEMO" && pwd -P)    # /tmp is /private/tmp on macOS

bold() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
now() { perl -MTime::HiRes=time -e 'printf "%.3f", time'; }
# timed <label> <command...>: runs it, prints how long it took
timed() {
  local label=$1; shift
  local t0 t1; t0=$(now)
  "$@"
  t1=$(now)
  printf '\033[32m   -> %s took %.2fs\033[0m\n' "$label" "$(echo "$t1 - $t0" | bc)"
}

bold "build clove from $SRC"
cargo build -q -p clove-cli --manifest-path "$SRC/Cargo.toml"
CLOVE_BIN=$SRC/target/debug/clove

cat > "$DEMO/env.sh" <<EOF
# source me: points clove at the demo state, never at ~/.clove
export CLOVE_HOME=$DEMO/state
export CLOVE_CLONE_ROOT=$DEMO/app.clove
export CLOVE_CONFIG=$DEMO/no-user-config.toml
export CLOVE_NO_UPDATE_CHECK=1
export CLOVE_NO_AUTO_REFRESH=1
# Never touch the real launchd service, agent hooks, shell rc or brew.
export CLOVE_NO_SERVICE=1
export CLOVE_NO_AGENT_HOOKS=1
export GIT_CONFIG_GLOBAL=$DEMO/gitconfig
unset CLOVE_CD_FILE CLOVE_SHELL
clove() { "$CLOVE_BIN" "\$@"; }
cd "$DEMO/app" 2>/dev/null || true
EOF
cat > "$DEMO/gitconfig" <<EOF
[user]
  name = demo
  email = demo@example.com
[init]
  defaultBranch = main
[advice]
  detachedHead = false
EOF
# shellcheck disable=SC1091
source "$DEMO/env.sh"
cd "$DEMO"

bold "a demo repo with a slow, file-heavy install step"
git init -q --bare origin.git
git clone -q origin.git seed 2>/dev/null
cd seed
mkdir -p src scripts
cat > package.json <<'EOF'
{ "name": "demo", "version": "1.0.0" }
EOF
echo "lockfile v1 packages=$PACKAGES" > package-lock.json
cat > src/app.js <<'EOF'
export function greet(name) {
  return `hello ${name}`;
}
EOF
cat > scripts/fake-install.sh <<'EOF'
#!/bin/sh
# Stands in for `npm ci`: slow, and thousands of files in node_modules.
set -e
n=$(sed 's/.*packages=//' package-lock.json)
rm -rf node_modules && mkdir node_modules
i=0
while [ $i -lt "$n" ]; do
  d=node_modules/pkg-$i/lib
  mkdir -p "$d"
  j=0; while [ $j -lt 60 ]; do echo "module.exports = $i * $j" > "$d/f$j.js"; j=$((j+1)); done
  i=$((i+1))
done
sleep 2   # the network
echo "installed $n packages" > node_modules/.installed
EOF
chmod +x scripts/fake-install.sh
cat > .gitignore <<'EOF'
node_modules/
dist/
EOF
cat > .clove.toml <<'EOF'
image_size = "4G"
spare = false
fetch = false
[refresh]
auto = false
[[step]]
name = "install"
warm = "./scripts/fake-install.sh"
setup = "./scripts/fake-install.sh"
watch = ["package-lock.json"]
EOF
git add -A && git commit -qm "init" && git push -q origin main
cd "$DEMO"
git clone -q origin.git app
cd app

bold "clove init (builds the golden image once: runs the install)"
timed "init" clove init --yes --json >/dev/null

bold "clove new feat/login"
timed "new" clove new feat/login --no-cd
WT=$(clove path feat-login)
echo "   clone at $WT"

bold "work in it: a commit, an uncommitted edit, an untracked file, build output"
cd "$WT"
echo "// login form" >> src/app.js && git commit -qam "login: start"
cat >> src/app.js <<'EOF'
export function login(user) {
  return greet(user) + ", welcome back";
}
EOF
echo "TODO: rate limit" > NOTES.md
mkdir -p dist && echo "bundle" > dist/app.js
show() {
  echo "   branch $(git branch --show-current) at $(git log -1 --format='%h %s')"
  echo "   node_modules: $(find node_modules -type f 2>/dev/null | wc -l | tr -d ' ') files, dist: $(ls dist 2>/dev/null | wc -l | tr -d ' ') files"
  git status --short | sed 's/^/   /'
}
show

bold "clove checkpoint feat-login good"
timed "checkpoint" clove checkpoint feat-login good

bold "wreck it: delete node_modules and dist, break app.js, commit the damage"
rm -rf node_modules dist NOTES.md
echo "garbage" > src/app.js
git commit -qam "oops"
show

bold "clove rewind feat-login good"
cd "$DEMO/app"     # a shell inside the clone would hold it (or: --force, then cd \$PWD)
timed "rewind" clove rewind feat-login good
cd "$WT"
show
echo "   uncommitted login() in app.js: $(grep -c 'function login' src/app.js) match"

bold "clove checkpoints feat-login (OWN = disk only that checkpoint holds)"
clove checkpoints feat-login

bold "clove fork feat-login a b c: three copies of this exact state"
cd "$DEMO/app"
timed "fork x3" clove fork feat-login a b c
for f in a b c; do
  cd "$(clove path $f)"
  echo "// approach $f" >> src/app.js
  git commit -qam "approach $f"
done

bold "they diverge; the source is untouched"
for f in feat-login a b c; do
  cd "$(clove path $f)"
  printf '   %-10s %-18s %s | NOTES.md: %s | node_modules: %s files\n' "$f" \
    "$(git branch --show-current)" "$(git log -1 --format=%s)" \
    "$(test -f NOTES.md && echo yes || echo no)" \
    "$(find node_modules -type f | wc -l | tr -d ' ')"
done

bold "clove ls"
cd "$DEMO/app"
clove ls

cat <<EOF

$(printf '\033[1m')Play with it$(printf '\033[0m'):
  source $DEMO/env.sh        # isolated CLOVE_HOME, a clove() function, cd into the repo
  clove checkpoints feat-login
  clove rewind feat-login before-rewind    # undo the rewind
  clove fork b b2 && clove rm a --force    # keep the winner
Clean up: $SRC/explore/cleanup.sh
EOF
