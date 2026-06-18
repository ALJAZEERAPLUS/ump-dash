---
quick_id: 260618-rst
status: planned
created: 2026-06-18
---

# Quick Task 260618-rst: Worktree Shortcut Cleanup

## Goal

Remove the visible shell `!` command, revise worktree shortcuts so checkout is `w c` and new branch is `w n`, rename the `switch` group label to `metro`, and move Claude/Tmux/Jira open actions under lowercase `o` chords.

## Tasks

1. Add focused shortcut characterization tests.
   - Files: `src/app/dispatch_tests.rs` or existing keybinding tests.
   - Action: Add red tests for removed shell exposure, worktree `c`/`n`, `s` no longer resolving to checkout, footer/help text, and lowercase open chords.
   - Verify: Run focused tests and observe expected failures before implementation.

2. Update keybinding registry and routing.
   - Files: `src/app/keybindings.rs`, possibly `src/app/handle_key.rs`, `src/ui/footer.rs`, `src/ui/help_overlay.rs`.
   - Action: Change key rows/action mappings through the centralized registry so footer and help derive the new labels automatically.
   - Verify: Focused tests pass.

3. Run guards and record summary.
   - Files: `.planning/STATE.md`, quick summary.
   - Action: Run focused tests, architecture lint, and clippy/cargo tests as practical for the scope; update quick task state and summary.
   - Verify: Commit code changes atomically, then commit GSD artifacts.
