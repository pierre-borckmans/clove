#!/usr/bin/env bash
# Imports the Developer ID certificate (base64 .p12 in APPLE_CERTIFICATE) into
# a throwaway keychain and exports SIGN_ID, the identity codesign uses.
set -euo pipefail
trap 'echo "import-cert: failed at line $LINENO: $BASH_COMMAND" >&2' ERR
step() { echo "import-cert: $*"; }

[ -n "${APPLE_CERTIFICATE:-}" ] || { echo "APPLE_CERTIFICATE is empty"; exit 1; }
[ -n "${APPLE_CERTIFICATE_PASSWORD:-}" ] || { echo "APPLE_CERTIFICATE_PASSWORD is empty"; exit 1; }

kc="$RUNNER_TEMP/signing.keychain-db"
pw=$(uuidgen)
step "creating a temporary keychain"
security create-keychain -p "$pw" "$kc"
security set-keychain-settings -lut 21600 "$kc"
security unlock-keychain -p "$pw" "$kc"

step "decoding the certificate ($(printf '%s' "$APPLE_CERTIFICATE" | wc -c | tr -d ' ') base64 chars)"
p12="$RUNNER_TEMP/cert.p12"
# Line breaks or spaces from copying would break the decode.
printf '%s' "$APPLE_CERTIFICATE" | tr -d ' \r\n' | base64 -D > "$p12"
step "decoded $(wc -c < "$p12" | tr -d ' ') bytes"

step "importing"
security import "$p12" -k "$kc" -f pkcs12 -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
rm -f "$p12"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$pw" "$kc" > /dev/null
# Searched first, without dropping the login keychain.
security list-keychains -d user -s "$kc" $(security list-keychains -d user | tr -d '"')

step "identities:"
security find-identity -v -p codesigning "$kc"
id=$(security find-identity -v -p codesigning "$kc" | grep -o '"Developer ID Application: [^"]*"' | head -1 | tr -d '"' || true)
[ -n "$id" ] || { echo "no Developer ID Application identity in APPLE_CERTIFICATE"; exit 1; }
echo "SIGN_ID=$id" >> "$GITHUB_ENV"
step "signing as: $id"
