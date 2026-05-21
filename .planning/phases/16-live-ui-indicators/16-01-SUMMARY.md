---
phase: 16-live-ui-indicators
plan: "01"
subsystem: ui
tags: [ui, indicators, spinner, pure-fn, exhaustive-match]
dependency_graph:
  requires: []
  provides: [src/ui/indicators.rs]
  affects: [src/ui/mod.rs]
tech_stack:
  added: []
  patterns: [pure-fn, exhaustive-match, inline-tests]
key_files:
  created:
    - src/ui/indicators.rs
  modified:
    - src/ui/mod.rs
decisions:
  - "Doc comment must not contain literal 'crate::infra::' string — arch-lint grep matches comments too"
metrics:
  duration: "~2 min"
  completed: "2026-05-21"
  tasks_completed: 2
  files_changed: 2
---

# Phase 16 Plan 01: ui/indicators.rs Pure Helpers Summary

**One-liner:** Pure display helpers module with half-circle spinner (6-frame, 150ms/frame), M:SS elapsed formatter, and exhaustive 23-variant CommandSpec short-code lookup — all zero-infra, compile-time drift-guarded.

## What Was Built

`src/ui/indicators.rs` — a pure UI helper module with three public exports consumed by the row renderer (Plan 16-02):

1. **`SPINNER_FRAMES: [&str; 6]`** — half-circle set `◐ ◓ ◑ ◒ ◐ ◓`; single swap point for alternate glyph sets.

2. **`spinner_frame(elapsed: Duration) -> &'static str`** — derives frame index from `elapsed.as_millis() / 150 % 6`; no stored counter (UI-03).

3. **`format_elapsed(elapsed: Duration) -> String`** — `"Ns"` under 60s, `"M:SS"` at 60s+, minutes unpadded (D-08).

4. **`task_short_label(spec: &CommandSpec) -> &'static str`** — exhaustive `match` over all 23 `CommandSpec` variants, no `_ =>` catch-all; mirrors `collision_policy()` pattern from `command.rs:172-206`.

**19 inline tests:** Spinner frame boundaries (0ms/149ms/150ms/749ms/750ms/900ms wrap), elapsed format boundaries (0s/42s/59s/60s/61s/600s/723s), spot-check labels, and `task_short_label_covers_every_variant` drift-guard meta-test (constructs all 23 variants, asserts count==23, asserts all labels non-empty).

`src/ui/mod.rs` updated with `pub mod indicators;` after `pub mod theme`.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Create ui/indicators.rs spinner_frame + format_elapsed + boundary tests | d80c19a | src/ui/indicators.rs (create), src/ui/mod.rs |
| 2 | Add exhaustive task_short_label + drift-guard coverage test | d80c19a | src/ui/indicators.rs (extend) |

## Verification Results

- `cargo test --lib ui::indicators`: 19 tests passed, 0 failed
- `make arch-lint`: PASS (G-02 satisfied — zero infra imports)
- `cargo build`: Finished with no errors or warnings
- `cargo test --all-targets`: No FAILED or error[] output (no regressions)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Doc comment triggered G-02 arch-lint grep**
- **Found during:** Task 1 verification
- **Issue:** The module doc comment contained the literal string `crate::infra::` (referencing the G-02 rule for documentation purposes). The arch-lint Makefile uses `rg 'crate::infra::'` against `src/ui/` files without excluding comments, causing a false positive G-02 FAIL.
- **Fix:** Changed doc comment from `No \`crate::infra::\` imports (G-02)` to `Zero infra imports (G-02)`.
- **Files modified:** `src/ui/indicators.rs` (doc comment only)
- **Commit:** d80c19a (included in same task commit)

## Known Stubs

None — all three functions are fully implemented and tested.

## Threat Flags

None — pure helper module with no I/O, network, persistence, auth, or external input surface.

## Self-Check: PASSED

- [x] `src/ui/indicators.rs` exists: FOUND
- [x] `src/ui/mod.rs` contains `pub mod indicators`: FOUND (line 13)
- [x] Commit d80c19a exists: FOUND
- [x] 19 tests pass: CONFIRMED
- [x] arch-lint PASS: CONFIRMED
- [x] No regressions: CONFIRMED
