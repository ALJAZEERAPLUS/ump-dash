---
quick_id: 260622-hrv
status: complete
commit: 57d19bc
---

# Quick Task 260622-hrv Summary

## Result

Fixed the open submenu so selecting `c`, `e`, or `t` returns the menu to the root, matching existing command and modal submenu behavior. The debugger option already cleared the palette; the regression test now covers it with the other open actions and representative actions from the Android, iOS, Yarn, Git, and Worktree submenus.

## Root Cause

`Action::OpenClaudeCode`, `Action::OpenShellTab`, and `Action::OpenEditor` emitted effects or errors without clearing `state.modal_stack.palette_mode`. Command-based submenu actions already cleared the palette through `Action::CommandRun`, so the open submenu was the inconsistent path.

## Files Changed

- `src/app/update.rs`
- `src/app/dispatch_tests.rs`

## Verification

- `cargo test submenu_option_selection_returns_to_root_after_update` passed
- `cargo test palette_resolution` passed
- `cargo test` passed
- `make arch-lint` passed
- `cargo clippy --all-targets -- -D warnings` passed
- `make arch-report` passed
