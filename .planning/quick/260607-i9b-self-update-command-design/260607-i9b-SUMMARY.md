---
quick_id: 260607-i9b
status: complete
completed: 2026-06-07
commit: 8100144
---

# Quick Task 260607-i9b Summary

## Goal

Add `ump-dash update` as a pre-TUI self-update command for GitHub Release
binary installs.

## Completed

- Added pre-TUI CLI dispatch for no args, `update`, `--version`/`-V`, and
  unknown-arg usage errors.
- Added `src/infra/self_update.rs` with GitHub CLI release querying, stable
  semver release selection, release-note display, platform asset mapping,
  `SHA256SUMS` parsing, SHA-256 verification, safe `.tar.gz` archive
  extraction, source-checkout refusal, writable-install checks, and
  same-directory temp/backup executable replacement.
- Added focused tests for release selection, platform mapping, source-checkout
  detection, checksum parsing/verification, archive validation, replacement
  path planning, and CLI dispatch.
- Updated the release workflow to upload `SHA256SUMS` with the platform
  tarballs.
- Updated `README.md` and `RELEASING.md` for updater usage, `gh`
  authentication, source checkout behavior, curated release bodies, and
  checksum publishing.

## Verification

- `cargo test self_update`
- `cargo test cli_dispatches`
- `cargo check`
- `cargo run -- --version`
- `cargo run -- update` fails as expected for `target/debug/ump-dash`
- `cargo test`
- `make arch-lint`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml")'`

## Commit

Implementation commit: `8100144`
