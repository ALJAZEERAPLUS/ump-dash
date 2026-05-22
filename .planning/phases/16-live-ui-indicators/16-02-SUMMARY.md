---
phase: 16-live-ui-indicators
plan: "02"
subsystem: ui
tags: [ui, indicators, spinner, render, worktree-table, config]
dependency_graph:
  requires: [src/ui/indicators.rs]
  provides: [worktree-table-live-indicators]
  affects: [src/ui/panels.rs, src/domain/dash_config.rs, config.example.toml]
tech_stack:
  added: []
  patterns: [render-reads-instant, per-row-task-lookup, config-driven-glyph]
key_files:
  created: []
  modified:
    - src/ui/panels.rs
    - src/ui/indicators.rs
    - src/domain/dash_config.rs
    - config.example.toml
decisions:
  - "Spinner glyph set made configurable (SpinnerStyle enum) per user request during checkpoint; default = circles"
  - "Half-circles kept as default; braille offered as opt-in for terminals where ambiguous-width glyphs don't align under Y/P"
metrics:
  duration: "~1 checkpoint cycle"
  completed: "2026-05-22"
  tasks_completed: 3
  files_changed: 4
---

# Phase 16 Plan 02: Live Worktree-Table Indicators Summary

**One-liner:** Wired the Plan 16-01 helpers into `render_worktree_table` — split the merged `Y/P` icon into independent yellow-spinner cells, added a rightmost task column (`spinner label elapsed`) for non-install tasks, and live elapsed from `started_at.elapsed()` with zero new `AppState` — then made the spinner glyph set user-configurable (default circles).

## What Was Built

`src/ui/panels.rs` — `render_worktree_table` now reflects live per-worktree task state every 250ms frame:

1. **UI-01 — split Y/P cells (D-01/D-03):** the merged `Y/P` icon became independent `Y` cell + single-space separator + `P` cell. Slash dropped — renders `Y P`.
2. **UI-02 — per-install spinner (D-02/D-06/D-09):** `Y` cell shows a yellow spinner only while `CommandSpec::YarnInstall` runs; `P` cell only while `YarnPodInstall` runs. Every other running task renders `<spinner> <label> <elapsed>` in a new rightmost task column (D-04). Idle cells fall back to staleness color (`wt.stale`/`wt.stale_pods`).
3. **UI-03 — live elapsed (no state):** elapsed computed from `record.started_at.elapsed()` in the render path each frame. `git diff src/app/` = 0 lines — no `AppState` field, no tick counter.
4. **Pitfall 1 fixed:** the `Table::new` Constraint array gained a 5th entry (`Length(20)` task column); the metro detail row gained a matching 5th `Cell::from("")` so columns stay aligned.

**Post-checkpoint addition (user request) — configurable spinner glyphs:**
- `ui::indicators::SpinnerStyle { Circles, Braille }` with two const frame sets (`◐◓◑◒` default, `⠋⠙⠹⠸⠼⠴`). `spinner_frame(elapsed, style)` selects the set.
- New `DashConfig.spinner_style` key (`config.toml`), default `"circles"`; `"braille"`/`"dots"` → braille, anything else → circles. `panels.rs` reads it once per render.
- Rationale: half-circles are east_asian_width=Ambiguous and didn't sit flush under the width-1 `Y`/`P` letters in the user's terminal; braille (all-Narrow) does. User chose configurable with circles as default.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Split Y/P into independent cells with per-install spinner | 76a294a | src/ui/panels.rs |
| 2 | Add task column + fix Constraint array and metro detail row | 136a2cc | src/ui/panels.rs |
| 3 | Human-verify terminal alignment + live animation (checkpoint) | — | (visual; resolved "approved") |

**Post-checkpoint follow-ups (user-directed):**

| Change | Commit | Files |
|--------|--------|-------|
| Swap spinner to braille for width-1 alignment | 7cef87d | src/ui/indicators.rs, src/ui/panels.rs |
| Make spinner glyph set configurable, default circles | 9f3c154 | src/ui/indicators.rs, src/ui/panels.rs, src/domain/dash_config.rs, config.example.toml |

## Verification Results

- `cargo test --lib ui::indicators`: 22 passed, 0 failed
- `cargo test --lib`: 137 passed, 0 failed
- `make arch-lint`: PASS (G-02 — `ui/` imports zero infra; reads domain `DashConfig` only)
- `cargo build`: Finished, no errors/warnings
- `git diff src/app/`: empty (UI-03 invariant — no `AppState` change)
- `rg '"/P"' src/ui/panels.rs`: no match (slash dropped)
- Human checkpoint: user ran the TUI; spinners same width, table alignment intact; approved.

## Deviations from Plan

**Scope addition (user-directed at checkpoint):** the plan locked a single half-circle set (D-05) with braille as a fallback only if alignment broke. The user requested the glyph set be made configurable instead, with circles as the default. Implemented as `SpinnerStyle` + `DashConfig.spinner_style`. No locked requirement (UI-01/02/03) was weakened — spinner remains 6-frame, yellow, `elapsed`-driven.

## Known Stubs

None.

## Threat Flags

None — render-only over trusted in-process state; new config field is a non-secret display preference parsed with a safe default.

## Out-of-Scope Issue Surfaced (not addressed here)

**Latent orphan bug (Phase 14/15 task system):** dispatching a *different* command type on a worktree that already has a running task overwrites `slice.task` without aborting the previous task — the old process keeps running, its handle is lost (uncancellable), and its output is silently dropped (`update.rs:612-616`). Same-type dispatch is handled correctly by `collision_policy`. Tracked for a follow-up fix; outside Phase 16's render-only scope.

## Self-Check: PASSED

- [x] `src/ui/panels.rs` renders independent Y/P cells + task column: CONFIRMED
- [x] Commits 76a294a, 136a2cc, 7cef87d, 9f3c154 exist: FOUND
- [x] 137 lib tests pass: CONFIRMED
- [x] arch-lint PASS: CONFIRMED
- [x] No `AppState` change (UI-03): CONFIRMED (empty src/app/ diff)
- [x] Human checkpoint approved: CONFIRMED
