#!/usr/bin/env bash
# Sends a zip or dmg to Apple's notary service with the App Store Connect API
# key and waits for the verdict; prints Apple's log and fails unless accepted.
set -euo pipefail
file="$1"
key="$RUNNER_TEMP/notary-key.p8"
printf '%s' "$APPLE_API_KEY" > "$key"
auth=(--key "$key" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER")
out=$(xcrun notarytool submit "$file" "${auth[@]}" --wait --output-format json)
echo "$out"
status=$(echo "$out" | jq -r .status)
if [ "$status" != "Accepted" ]; then
  xcrun notarytool log "$(echo "$out" | jq -r .id)" "${auth[@]}" || true
  exit 1
fi
