---
quick_id: 260529-fs9
slug: let-s-add-support-for-ghostty-the-same-w
status: complete
date: 2026-05-29
commit: 2839c35
---

# Quick Task 260529-fs9 Summary

## Completed

- Added `GhosttyAdapter` to the terminal surface abstraction.
- Detects Ghostty sessions via `GHOSTTY_RESOURCES_DIR`, after tmux and zellij priority.
- Opens Ghostty surfaces using AppleScript on macOS and `ghostty +new-window` on non-macOS platforms.
- Updated user-facing tmux/zellij wording to include Ghostty.

## Verification

- Red test confirmed Ghostty detection was missing before implementation.
- `cargo test ghostty` passed.
- `rustfmt --check src/infra/multiplexer.rs` passed.
- `cargo test` passed.
- `cargo clippy --all-targets -- -D warnings` passed.

## Notes

- `cargo fmt --check` across the whole repo still reports pre-existing formatting drift in untouched files, so only the modified adapter file was format-checked.
