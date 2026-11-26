# Releasing

1. Bump `version` in `Cargo.toml` (`[workspace.package]`) and commit.
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.

`.github/workflows/release.yml` then builds the CLI for macOS (arm64, Intel) and Linux (arm64, x86_64), publishes a GitHub release with `checksums.txt`, and renders `packaging/homebrew/clove.rb.in` into `Formula/clove.rb` in [pierre-borckmans/homebrew-tap](https://github.com/pierre-borckmans/homebrew-tap). It pushes with the `TAP_DEPLOY_KEY` secret, a write deploy key on the tap.

The workflow stops if the tag doesn't match the crate version.

## Signing (macOS)

With these repository secrets set, the release signs the macOS CLI with Developer ID and sends it to Apple's notary service, and builds the Mac app (universal, signed, notarized, stapled `.dmg`), which the tap ships as the `clove-app` cask. Without them, the CLI keeps its ad-hoc signature and the app job is skipped.

Apple's queue never holds the release. The CLI and formula go out as soon as the binaries are signed: a bare binary can't carry a stapled ticket, so macOS looks it up online on first run. The `notarized` job then waits for Apple's verdicts and fails if one is a rejection. The `app` job starts after the release, waits for Apple as long as it takes (first submissions from a new team took over an hour), then attaches the `.dmg` to the release and updates the cask.

| Secret | What |
|---|---|
| `APPLE_CERTIFICATE` | Developer ID Application certificate and key, `.p12`, base64 |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password |
| `APPLE_TEAM_ID` | team ID from Membership details |
| `APPLE_API_KEY_ID` | App Store Connect API key ID (Developer role) |
| `APPLE_API_ISSUER` | its issuer ID |
| `APPLE_API_KEY` | the `.p8` file's contents |

Helpers: `scripts/ci/import-cert.sh` (temporary keychain, exports `SIGN_ID`) and `scripts/ci/notarize.sh` (submits, waits, prints Apple's log on rejection).
