---
status: in_progress
quick_id: 260528-gku
date: 2026-05-28
---

# Quick Task 260528-gku: Run key follow-up

## Goal

Follow up quick task `260528-fec`: remove the old run keys now that the new UMP keys exist, and add uppercase `R` to repeat the last run config using the same workspace-scoped persistence model as other workspace state.

## Tasks

1. Lock the keybinding behavior with failing tests.
   - Files: `src/app/dispatch_tests.rs`, likely `src/app/keybindings.rs`
   - Action: assert legacy run keys no longer resolve, and uppercase `R` repeats the last target + flavor for the current workspace.
   - Verify: targeted Cargo tests fail before implementation.

2. Implement workspace-scoped last run config.
   - Files: `src/app/update.rs`, `src/app/state.rs`, `src/domain/*` as needed
   - Action: persist the last run target + flavor per workspace when a run is dispatched, and route uppercase `R` to that saved config.
   - Verify: targeted tests pass.

3. Run the repo checks required for app-state/keybinding changes.
   - Verify: `cargo test`, `make arch-lint`, and `cargo check`.
