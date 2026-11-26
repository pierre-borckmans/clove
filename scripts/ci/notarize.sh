#!/usr/bin/env bash
# Apple's notary service, with the App Store Connect API key.
#   notarize.sh submit FILE   upload; prints the submission id last
#   notarize.sh wait ID       wait for the verdict; Apple's log if rejected
#   notarize.sh FILE          both
# Network errors (runners drop connections during long waits) are retried.
set -euo pipefail
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

submit() {
  local out id
  out=$(retry xcrun notarytool submit "$1" "${auth[@]}" --output-format json)
  echo "$out" >&2
  id=$(echo "$out" | jq -r .id)
  [ -n "$id" ] && [ "$id" != null ] || { echo "notarize: no submission id" >&2; exit 1; }
  echo "notarize: submitted $1 as $id" >&2
  echo "$id"
}

wait_for() {
  local id="$1" out status
  # First submissions from a new team can take hours; wait in bounded slices.
  for _ in $(seq 1 34); do
    out=$(xcrun notarytool wait "$id" "${auth[@]}" --timeout 10m --output-format json 2>/dev/null || true)
    status=$(echo "$out" | jq -r '.status // empty' 2>/dev/null || true)
    [ -z "$status" ] && status=$(xcrun notarytool info "$id" "${auth[@]}" --output-format json 2>/dev/null | jq -r '.status // empty' || true)
    echo "notarize: $id ${status:-unknown}"
    case "$status" in
      Accepted) return 0 ;;
      Invalid|Rejected)
        retry xcrun notarytool log "$id" "${auth[@]}" || true
        return 1 ;;
    esac
    sleep 20
  done
  echo "notarize: gave up waiting on $id"
  return 1
}

case "${1:-}" in
  submit) submit "$2" ;;
  wait) wait_for "$2" ;;
  *) wait_for "$(submit "$1")" ;;
esac
