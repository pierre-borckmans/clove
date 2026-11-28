#!/usr/bin/env bash
# Detaches every disk image the demo mounted and deletes the demo folder.
set -uo pipefail
DEMO=${DEMO:-/tmp/clove-explore-checkpoints}
[ -e "$DEMO" ] || { echo "nothing at $DEMO"; exit 0; }
DEMO=$(cd "$DEMO" && pwd -P)
case "$DEMO" in
  /private/tmp/*|/tmp/*) ;;
  *) echo "refusing to clean $DEMO: not under /tmp" >&2; exit 1 ;;
esac
# Deepest mount points first.
mount | sed -n "s#^\(/dev/[^ ]*\) on \($DEMO/.*\) (.*#\1 \2#p" | sort -k2 -r |
while read -r dev point; do
  echo "detach $point"
  diskutil unmount force "$point" >/dev/null 2>&1
  diskutil eject "$dev" >/dev/null 2>&1
done
if mount | grep -q " on $DEMO/"; then
  echo "still mounted under $DEMO:" >&2
  mount | grep " on $DEMO/" >&2
  exit 1
fi
rm -rf "$DEMO"
echo "removed $DEMO"
