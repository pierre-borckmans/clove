#!/usr/bin/env bash
# Sends a zip or dmg to Apple's notary service with the App Store Connect API
# key and waits for the verdict; prints Apple's log and fails unless accepted.
# Network errors (runners drop connections during long waits) are retried.
set -euo pipefail
file="$1"
key="$RUNNER_TEMP/notary-key.p8"
printf '%s' "$APPLE_API_KEY" > "$key"
auth=(--key "$key" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER")

retry() {
  local n
  for n in 1 2 3 4 5; do
    "$@" && return 0
    echo "notarize: attempt $n failed, retrying in $((n * 30))s" >&2
    sleep $((n * 30))
  done
  return 1
}

out=$(retry xcrun notarytool submit "$file" "${auth[@]}" --output-format json)
echo "$out"
id=$(echo "$out" | jq -r .id)
[ -n "$id" ] && [ "$id" != null ] || { echo "notarize: no submission id"; exit 1; }
echo "notarize: submission $id"

# First submissions from a new team can take an hour; wait in bounded slices.
for _ in $(seq 1 30); do
  out=$(xcrun notarytool wait "$id" "${auth[@]}" --timeout 10m --output-format json 2>/dev/null || true)
  status=$(echo "$out" | jq -r '.status // empty' 2>/dev/null || true)
  [ -z "$status" ] && status=$(xcrun notarytool info "$id" "${auth[@]}" --output-format json 2>/dev/null | jq -r '.status // empty' || true)
  echo "notarize: status ${status:-unknown}"
  case "$status" in
    Accepted) exit 0 ;;
    Invalid|Rejected)
      retry xcrun notarytool log "$id" "${auth[@]}" || true
      exit 1 ;;
  esac
  sleep 20
done
echo "notarize: gave up waiting on $id"
exit 1
