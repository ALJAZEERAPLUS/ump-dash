---
quick_id: 260529-hhm
slug: the-origin-has-changed-now-it-is-github-
status: complete
created: 2026-05-29
---

# Quick Task 260529-hhm: Move release/docs to UMP Dash origin

Update the project from the old `cubicme/rn-dash` public naming to the new UMP-specific `ALJAZEERAPLUS/ump-dash` origin, adjust README/release/CI references, and cut the next release.

## Tasks

1. Rename user-facing package, binary, config, and docs references from `rn-dash` / `RN Dash` to `ump-dash` / `UMP Dash` where current behavior should follow the new UMP-specific project identity.
2. Update release automation references so GitHub Actions packages the new binary name and release helper output points at `github.com/ALJAZEERAPLUS/ump-dash`.
3. Verify with Cargo, commit the update and changelog, set `origin` to the new repo, push `main`, then finalize release `v1.3.0`.

## Verification

- `cargo check`
- `cargo test`
- `cargo clippy -- -D warnings`
- Release tag pushed and GitHub Actions release workflow started for `v1.3.0`
