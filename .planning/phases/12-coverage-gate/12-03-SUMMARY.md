---
phase: 12-coverage-gate
plan: 03
subsystem: testing
tags: [rust, tea, table-driven-tests, characterization-test, handle-key, update, palette, modal, command-queue, cover-03]

# Dependency graph
requires:
  - phase: 12-coverage-gate
    provides: Plan 12-00 bin+lib scaffolding — src/lib.rs with pub mod app/domain/infra/ui; Cargo.toml [dev-dependencies] tokio with macros/rt-multi-thread/process/time/sync features. Without these, the 11 #[tokio::test] functions in modal_dismissal + command_queue sub-modules could not compile.
provides:
  - src/app/dispatch_tests.rs — 17 characterization tests covering TEA dispatch for handle_key (palette→Action resolution across 5 PaletteMode variants + the CleanToggle entry/exit flow) and update() (8 ModalState dismissals + CommandQueuePush append + CommandExited drain for empty + non-empty queue).
  - Regression trip-wire for Phase 13 F-201 (Effect enum refactor of update) and F-208 (unified keybinding registry) — any silently dropped palette key, modal that no longer dismisses, or queue that no longer drains fails CI before landing.
  - Pattern 4 template from 12-RESEARCH.md realized inline with unrecognized-key fallback assertions — key('z') assertions in each palette test guard against new palette additions that silently drop keys.
affects: [13-refactor, 12-04-baseline-coverage]

# Tech tracking
tech-stack:
  added: []  # All deps already present from 12-00 (tokio dev-deps, ratatui, crossterm)
  patterns:
    - "Table-driven TEA characterization: one #[test] fn per PaletteMode variant, each asserting every declared key → expected Action pair verbatim from src/app.rs:333-381 (re-declaring the table IS the characterization)"
    - "Unrecognized-key fallback regression-guard: key('z') → Some(Action::ModalCancel) assertion in every palette test, converting 'forgot to add a key case' into a test-caught regression when future additions bypass the _ fallback arm"
    - "Child module under src/app/ rather than inline in app.rs: app.rs is 2425 lines (above the 2000-line Ousterhout threshold); adding 600 test lines inline would push it past 3000. Sub-module split is Claude's Discretion recommendation per 12-RESEARCH.md §Validation Architecture"
    - "Two-phase modal dismiss assertion: (1) handle_key(state, key) → expected Action, (2) update(state, action) → state.modal == None. Catches both the key→action resolution bug AND the update-handler-forgot-to-clear-modal bug independently"
    - "dispatch_command early-return avoidance: the CommandExited non-empty-queue drain test seeds one worktree into state.worktrees so dispatch_command does NOT bail at src/app.rs:497 — without this seed the test would pop_front but never set running_command, masking the invariant"
    - "tokio::spawn bypass in MetroStart path: the sync_before_metro_modal_dismisses test sets state.skip_external_metro_check = true before invoking SyncBeforeMetroDecline, routing MetroStart through the synchronous channel-send path (src/app.rs:595-598) instead of the external-metro-detect tokio::spawn"

key-files:
  created:
    - "src/app/dispatch_tests.rs"
  modified:
    - "src/app.rs"

key-decisions:
  - "Placed tests under src/app/dispatch_tests.rs (new sub-module) rather than inline in app.rs, per D-08 and Pattern 4 of 12-RESEARCH.md. This file is 2425 lines — inlining 600 test lines would push past 3000, well beyond the 2000-line Ousterhout threshold identified in Phase 11."
  - "Split 17 tests into 3 sub-modules (palette_resolution, modal_dismissal, command_queue) for navigability — each module is a single TEA surface. Test names are self-describing (no _works suffix) so failing assertions at CI time read naturally."
  - "Regression-guard via key('z') → Some(Action::ModalCancel) assertions in every palette test. Re-declaring the palette table (verbatim from src/app.rs:333-381) IS the characterization; the fallback assertion catches the future-addition-drops-a-key class of bug."
  - "[Rule 1 - Bug] Fixed clippy field_reassign_with_default warning: the original helper `base_state()` did `AppState::default()` followed by `s.focused_panel = FocusedPanel::WorktreeTable`. Since FocusedPanel::WorktreeTable is the #[default] variant, this is a no-op that clippy flags under `-D warnings`. Simplified to `AppState::default()` alone with a doc-comment explaining the implicit default."
  - "For CommandQueue drain test: seeded one worktree into state.worktrees so dispatch_command actually runs (it early-returns on empty worktrees per src/app.rs:497). This lets the test assert the expected post-condition: running_command becomes the popped spec, the remaining spec stays in the queue."
  - "Interpreted phase-description's `palette x` as CleanToggle modal confirm key per Research Assumption A2. The `x` is NOT a PaletteMode variant — only 5 exist (a/i/y/g/w). Test covers BOTH transitions: Yarn 'c' → Action::OpenCleanMenu (entry), and CleanToggle 'x' → Action::CleanConfirm (exit)."

patterns-established:
  - "When a TEA update() handler transitively spawns tokio tasks (e.g. Action::MetroStart at src/app.rs:602), set pre-condition flags (skip_external_metro_check) to route through the synchronous code path in tests rather than requiring a full tokio runtime with background-task cleanup."
  - "When testing dispatch_command post-conditions, seed at least one worktree so the function does not early-return at src/app.rs:497. Without a worktree the function logs a warning and returns — assertion about running_command state would then silently reflect the empty-queue case rather than the drain case."

requirements-completed: [COVER-03]

# Metrics
duration: 3min
completed: 2026-04-23
---

# Phase 12 Plan 03: TEA Dispatch Coverage Summary

**17 table-driven characterization tests lock in handle_key's palette/modal resolution and update()'s modal-dismiss + command-queue routing — Phase 13's Effect enum refactor and keybinding registry work now have a deterministic trip-wire for every declared (PaletteMode, key) pair, every ModalState dismissal, and both CommandExited drain cases.**

## Performance

- **Duration:** ~3 min (read plan + verify types + write tests + 1 clippy fix + verify + commit)
- **Started:** 2026-04-23T18:49:14Z
- **Completed:** 2026-04-23T18:52:48Z
- **Tasks:** 1 (Task 03.1)
- **Files:** 1 created (src/app/dispatch_tests.rs, 603 lines), 1 modified (src/app.rs — +3 lines for mod declaration)

## Accomplishments

### Test file layout (3 sub-modules, 17 tests)

| Sub-module | Tests | Attribute | Covers |
|------------|-------|-----------|--------|
| `palette_resolution` | 6 | `#[test]` | 5 `PaletteMode` variants + CleanToggle entry/exit flow |
| `modal_dismissal` | 8 | `#[tokio::test]` | 8 `ModalState` variants × documented dismiss key(s) |
| `command_queue` | 3 | `#[tokio::test]` | `CommandQueuePush` append, `CommandExited` drain (non-empty + empty) |

**Total: 17 tests, all passing in 0.01s (full `cargo test --lib` suite: 46 pass, 0 fail).**

### COVER-03 must-haves — all satisfied

- [x] For each of 5 `PaletteMode` variants (Android, Ios, Yarn, Git, Worktree), every declared key resolves to the exact `Action` documented at src/app.rs:333-381. Verbatim.
- [x] Yarn palette 'c' → `Action::OpenCleanMenu` (entry), AND `ModalState::CleanToggle` 'x' → `Action::CleanConfirm` (exit). Both transitions covered by `yarn_c_opens_clean_toggle_then_x_confirms` — this is the phase-description's "palette x" per Research A2.
- [x] 8 `ModalState` variants dismiss on documented keys; after `update()` applies the dismiss action, `state.modal == None` for each.
- [x] `update(state, Action::CommandQueuePush(spec), …)` appends `spec` to `state.command_queue` (front/back order verified).
- [x] `update(state, Action::CommandExited, …)` with non-empty queue pops the front AND sets `running_command` to the popped spec (confirmed via `dispatch_command` path when a worktree is seeded).
- [x] `update(state, Action::CommandExited, …)` with empty queue clears `running_command` without panic and without spurious state changes.
- [x] Unrecognized palette keys produce `Action::ModalCancel` (regression-guard against future additions that silently drop keys) — `key('z')` assertion in every palette test.

### Phase-description `palette x` interpretation

Per Research Assumption A2, "palette x" is the confirm key inside `ModalState::CleanToggle`, not a sixth top-level palette. There are only 5 `PaletteMode` variants (a/i/y/g/w) — confirmed by the `PaletteMode` enum at src/app.rs:44-55. The `yarn_c_opens_clean_toggle_then_x_confirms` test covers BOTH halves of the flow:

1. **Entry:** `PaletteMode::Yarn` + `Char('c')` → `Action::OpenCleanMenu`
2. **Exit:** `ModalState::CleanToggle` + `Char('x')` → `Action::CleanConfirm`
3. **Cancel:** `ModalState::CleanToggle` + `Esc` → `Action::ModalCancel`

### `src/app.rs` wiring

One-line addition at EOF (after `metro_http_post`):

```rust
#[cfg(test)]
mod dispatch_tests;
```

Guarded by `#[cfg(test)]` so it imposes zero cost on release builds.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Clippy `field_reassign_with_default` on `base_state()` helper**

- **Found during:** Task 03.1 verification (`cargo clippy --all-targets -- -D warnings` after tests first passed)
- **Issue:** Plan's pattern `let mut s = AppState::default(); s.focused_panel = FocusedPanel::WorktreeTable; s` triggers `-D clippy::field_reassign_with_default` because `FocusedPanel::WorktreeTable` is already the `#[default]` variant (src/app.rs:13-18). The reassignment is a no-op that clippy flags.
- **Fix:** Simplified `base_state()` to `AppState::default()` alone with a doc comment noting that `focused_panel` defaults to `WorktreeTable` implicitly. Behavior unchanged; clippy clean.
- **Files modified:** `src/app/dispatch_tests.rs`
- **Commit:** squashed into 108ec84 (same-task fix before commit)

No Rule 2/Rule 3/Rule 4 deviations. Plan executed as written apart from the clippy polish.

## Verification

```
$ cargo build --tests
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.58s

$ cargo test --lib dispatch_tests
running 17 tests
test app::dispatch_tests::palette_resolution::git_palette_resolves_every_key ... ok
test app::dispatch_tests::palette_resolution::ios_palette_resolves_every_key ... ok
test app::dispatch_tests::palette_resolution::yarn_c_opens_clean_toggle_then_x_confirms ... ok
test app::dispatch_tests::palette_resolution::worktree_palette_resolves_every_key ... ok
test app::dispatch_tests::palette_resolution::android_palette_resolves_every_key ... ok
test app::dispatch_tests::modal_dismissal::text_input_modal_dismisses_on_esc ... ok
test app::dispatch_tests::command_queue::command_exited_with_empty_queue_clears_running_command ... ok
test app::dispatch_tests::modal_dismissal::sync_before_run_modal_dismisses_on_n_and_esc ... ok
test app::dispatch_tests::modal_dismissal::sync_before_metro_modal_dismisses_on_n_and_esc ... ok
test app::dispatch_tests::modal_dismissal::device_picker_modal_dismisses_on_esc ... ok
test app::dispatch_tests::modal_dismissal::clean_toggle_modal_dismisses_on_esc ... ok
test app::dispatch_tests::command_queue::command_queue_push_appends_to_back ... ok
test app::dispatch_tests::modal_dismissal::confirm_modal_dismisses_on_n_and_esc ... ok
test app::dispatch_tests::modal_dismissal::branch_picker_modal_dismisses_on_esc ... ok
test app::dispatch_tests::modal_dismissal::external_metro_conflict_dismisses_on_n_and_esc ... ok
test app::dispatch_tests::palette_resolution::yarn_palette_resolves_every_key ... ok
test app::dispatch_tests::command_queue::command_exited_with_nonempty_queue_pops_and_dispatches_front ... ok
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out; finished in 0.01s

$ cargo test --lib
test result: ok. 46 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.72s
```

All 17 new tests pass, full lib suite (46 tests) green, clippy `-D warnings` clean.

## Action / CommandSpec Variant Verification

All action/type references used in tests were cross-checked against source truth during implementation; no mismatches found:

- `Action::{CommandRun, OpenCleanMenu, CleanConfirm, ModalCancel, WorktreeAdd, WorktreeRemove, WorktreeAddNewBranch, StartSetAndroidMode, SyncBeforeRunDecline, SyncBeforeMetroDecline, CommandQueuePush, CommandExited, KillExternalMetro}` all present in `src/action.rs`.
- `CommandSpec::{ShellCommand, RnRunAndroid, RnReleaseBuild, RnRunIosDevice, RnRunIos, YarnPodInstall, YarnInstall, YarnUnitTests, YarnCheckTypes, YarnJest, YarnLint, GitFetch, GitPull, GitPush, GitResetHardFetch, GitCheckout, GitCheckoutNew, GitRebase}` all present in `src/domain/command.rs`.
- `ModalState::{Confirm, TextInput, DevicePicker, CleanToggle, SyncBeforeRun, SyncBeforeMetro, ExternalMetroConflict, BranchPicker}` — all 8 variants covered (`src/domain/command.rs:193-241`).
- `CleanOptions::default()` — struct field names (`node_modules`, `pods`, `android`, `sync_after`) unused by the test; default fine.

## Self-Check

Files verified via `ls` / `git show`:

- **FOUND:** `src/app/dispatch_tests.rs` (603 lines)
- **FOUND:** `src/app.rs` line 2427-2428 contains `#[cfg(test)]\nmod dispatch_tests;`
- **FOUND:** commit `108ec84` on main

## Self-Check: PASSED
