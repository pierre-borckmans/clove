#!/usr/bin/env bash
# Imports the Developer ID certificate (base64 .p12 in APPLE_CERTIFICATE) into
# a throwaway keychain and exports SIGN_ID, the identity codesign uses.
set -euo pipefail
kc="$RUNNER_TEMP/signing.keychain-db"
pw=$(uuidgen)
security create-keychain -p "$pw" "$kc"
security set-keychain-settings -lut 21600 "$kc"
security unlock-keychain -p "$pw" "$kc"
p12="$RUNNER_TEMP/cert.p12"
printf '%s' "$APPLE_CERTIFICATE" | base64 --decode > "$p12"
security import "$p12" -k "$kc" -f pkcs12 -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
rm -f "$p12"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$pw" "$kc" > /dev/null
# Searched first, without dropping the login keychain.
security list-keychains -d user -s "$kc" $(security list-keychains -d user | tr -d '"')
id=$(security find-identity -v -p codesigning "$kc" | grep -o '"Developer ID Application: [^"]*"' | head -1 | tr -d '"')
[ -n "$id" ] || { echo "no Developer ID Application identity in APPLE_CERTIFICATE"; exit 1; }
echo "SIGN_ID=$id" >> "$GITHUB_ENV"
echo "signing as: $id"
