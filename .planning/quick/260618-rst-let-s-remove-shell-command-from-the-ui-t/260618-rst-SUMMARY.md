---
status: complete
quick_id: 260618-rst
date: 2026-06-18
commit: e1197a9
---

# Quick Task 260618-rst Summary

## Completed

- Removed the visible/dispatch `!` shell-command keybinding from the worktree table.
- Changed the worktree palette so `w c` adds/checks out a worktree and `w n` creates a new-branch worktree; old `w w` and `w b` now fall back to palette cancel behavior.
- Added an `o` Open palette with lowercase `o c`, `o t`, and `o j` bindings for Claude Code, shell tab, and Metro debugger.
- Changed the visible worktree-table Enter hint from `switch` to `metro` and updated user-facing help/README wording.
- Added dispatch/footer/help characterization tests for the new shortcut layout.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `make arch-lint`
- `make arch-report`

## Commit

- `e1197a9` - `fix(260618-rst): update worktree shortcuts`
