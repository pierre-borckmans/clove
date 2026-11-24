#!/usr/bin/env bash
# Full-page screenshots of the built site, desktop and mobile, light and dark.
# Needs `pnpm build` first. Usage: scripts/screenshots.sh [out-dir]
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:-screenshots}"
mkdir -p "$out"
pnpm exec vite preview --port 4317 --strictPort >/dev/null 2>&1 &
server=$!
trap 'kill $server' EXIT
for _ in $(seq 50); do curl -sf http://localhost:4317 >/dev/null && break; sleep 0.1; done
shot() {
  pnpm dlx playwright@1 screenshot --browser chromium --full-page --wait-for-timeout 8000 \
    --viewport-size "$2" --color-scheme "$3" http://localhost:4317/ "$out/$1-$3.png" >/dev/null
}
shot desktop 1440,900 light
shot desktop 1440,900 dark
shot mobile 390,844 light
shot mobile 390,844 dark
ls -1 "$out"
