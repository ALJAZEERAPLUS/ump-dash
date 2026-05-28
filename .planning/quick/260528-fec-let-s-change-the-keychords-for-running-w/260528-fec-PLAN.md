---
status: in_progress
quick_id: 260528-fec
date: 2026-05-28
---

# Quick Task 260528-fec: UMP run keychords

## Goal

Change run keychords to match `~/aljazeera/ump` package scripts instead of generic React Native commands.

## Tasks

1. Add UMP run command coverage.
   - Files: `src/domain/command.rs`, `src/domain/pipeline.rs`, `src/ui/indicators.rs`, `src/app/dispatch_tests.rs`
   - Action: represent Android/iOS UMP run commands with `local`, `dev`, and `prod` variants and assert generated `yarn android:*` / `yarn ios:*` argv.
   - Verify: targeted Cargo tests fail before implementation, then pass after implementation.

2. Change the run modal flow.
   - Files: `src/app/keybindings.rs`, `src/app/update.rs`, `src/app/handle_key.rs`, `src/ui/modals.rs`
   - Action: keep top-level `a`/`i` palettes; map `r` in each palette to target selection, then a run-type picker ordered `local`, `dev`, `prod`.
   - Verify: dispatch tests cover palette resolution, target-first behavior, run-type selection, and modal cancellation.

3. Switch Metro spawn to Rozenite.
   - Files: `src/infra/process.rs`
   - Action: spawn Metro with `yarn start:rozenite --reset-cache` so the UMP script supplies Rozenite/client-log flags while preserving reset-cache.
   - Verify: unit test locks the Metro spawn argv.
