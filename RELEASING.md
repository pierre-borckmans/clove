# Releasing

1. Bump `version` in `Cargo.toml` (`[workspace.package]`) and commit.
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.

`.github/workflows/release.yml` then builds the CLI for macOS (arm64, Intel) and Linux (arm64, x86_64), publishes a GitHub release with `checksums.txt`, and renders `packaging/homebrew/clove.rb.in` into `Formula/clove.rb` in [pierre-borckmans/homebrew-tap](https://github.com/pierre-borckmans/homebrew-tap). It pushes with the `TAP_DEPLOY_KEY` secret, a write deploy key on the tap.

The workflow stops if the tag doesn't match the crate version.
