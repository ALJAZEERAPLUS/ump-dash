# Phase 12 Coverage Thresholds (per D-04 / D-05)

**Policy:** `threshold % = floor(baseline %, 5)`. No aspirational numbers.
**Enforcement:** human-checked this phase (D-05). An enforcement script is a post-milestone concern.
**When to update:** Phase 13+ may LOWER a row's baseline (refactor removes code) — threshold adjusts
to `floor(new baseline, 5)` only if the lower baseline is a justified structural change, never because
tests regressed. Phases may NEVER ratchet the threshold down below a prior threshold without written
rationale in this file's changelog section.

## Totals Threshold

Total line-coverage baseline: **12.84%** → threshold **10%**

Total function-coverage baseline: **20.82%** → threshold **20%**

Total region-coverage baseline: **9.89%** → threshold **5%**

## Per-File Thresholds (line coverage)

| File | Baseline Lines % | Threshold % (floor,5) |
|------|------------------|-----------------------|
| src/app/mod.rs | 0.00% | 0% |
| src/app/state.rs | ~15% (inherits Default + helpers from old src/app.rs) | 10% |
| src/app/update.rs | ~12% (inherits dispatch_tests exercise from old src/app.rs) | 10% |
| src/app/handle_key.rs | ~20% (inherits palette + modal dismiss tests) | 15% |
| src/app/runtime.rs | 0.00% (async event loop not unit-tested) | 0% |
| src/app/effect_runner.rs | 0.00% (stub, populated in Plan 13-08) | 0% |
| src/app/adapters.rs | 0.00% (stub, populated in Plan 13-08) | 0% |
| src/domain/action.rs | 0.00% | 0% |
| src/domain/command.rs | 8.54% | 5% |
| src/domain/jira.rs | 100.00% | 100% |
| src/domain/metro.rs | 70.00% | 70% |
| src/domain/ports/jira_port.rs | 0.00% | 0% |
| src/domain/ports/mod.rs | 0.00% | 0% |
| src/domain/ports/multiplexer_port.rs | 0.00% | 0% |
| src/domain/ports/process_port.rs | 0.00% | 0% |
| src/domain/refresh.rs | 100.00% | 100% |
| src/domain/worktree.rs | 0.00% | 0% |
| src/event.rs | 0.00% | 0% |
| src/infra/android_prefs.rs | 58.82% | 55% |
| src/infra/command_runner.rs | 0.00% | 0% |
| src/infra/config.rs | 8.70% | 5% |
| src/infra/devices.rs | 0.00% | 0% |
| src/infra/jira.rs | structural | 0% (see note) |
| src/infra/jira_cache.rs | 0.00% | 0% |
| src/infra/multiplexer.rs | 0.00% | 0% |
| src/infra/port.rs | 0.00% | 0% |
| src/infra/process.rs | 0.00% | 0% |
| src/infra/sim_history.rs | 0.00% | 0% |
| src/infra/tmux.rs | 0.00% | 0% |
| src/infra/worktrees.rs | 0.00% | 0% |
| src/main.rs | 0.00% | 0% |
| src/tui.rs | 0.00% | 0% |
| src/ui/error_overlay.rs | 0.00% | 0% |
| src/ui/footer.rs | 0.00% | 0% |
| src/ui/help_overlay.rs | 0.00% | 0% |
| src/ui/mod.rs | 0.00% | 0% |
| src/ui/modals.rs | 0.00% | 0% |
| src/ui/panels.rs | 0.00% | 0% |
| src/ui/theme.rs | 0.00% | 0% |

> **Note on `src/infra/jira.rs`:** Per Plan 13-01, the 6 `extract_jira_key*` inline tests moved to
> `src/domain/jira.rs` (where they still cover 100% of the pure function) and the pure function
> itself moved with them. `infra/jira.rs` now contains only the HTTP client + `is_inside_tmux`
> helper, none of which are unit-tested by current tests. The 70.18% → new-baseline drop is a
> structural change (code + tests moved together, no test regression). New threshold is 0%
> under floor-to-5 policy; the covering tests now enforce the floor on `domain/jira.rs`.

## Invariants Phase 13+ MUST Preserve

- `src/domain/refresh.rs >= 100%` — dropping any of the 17 inline tests in Phase 13 refactor is a ratchet violation.
- `src/domain/metro.rs >= 70%` — the register-once / register-twice / update-level `MetroStart` characterization tests (COVER-01) must keep passing under coverage.
- `src/domain/jira.rs >= 100%` — the six `extracts_key*` / `returns_none*` inline tests (relocated from `infra/jira.rs` in Phase 13 Plan 13-01) must keep passing.
- `src/infra/android_prefs.rs >= 55%` — pre-existing inline tests must keep passing.
- `src/app/update.rs >= 10%` AND `src/app/handle_key.rs >= 15%` — 12-03's dispatch-tests module (command-queue drain, modal dismissal, palette resolution) was split from the monolithic `src/app.rs` into these two files in Plan 13-06; the coverage that previously bound on `src/app.rs` now binds across these two rows. Neither may regress below its threshold.
- `src/domain/command.rs >= 5%` and `src/infra/config.rs >= 5%` — whatever minimal coverage exists today must not regress.
- Total line coverage `>= 10%`, total function coverage `>= 20%`, total region coverage `>= 5%`.

Any Phase 13+ PR that drops a row below its threshold requires:
1. Explanation of why the drop is a structural change (e.g., deleted dead code), not a test regression.
2. An entry in the Changelog section below updating the threshold.

## Changelog

| Date | Phase | Change | Rationale |
|------|-------|--------|-----------|
| 2026-04-23 | 12 | Initial thresholds | Baseline after COVER-01/02/03 landed (Waves 1+2). Full workspace line coverage = 12.84%; highest coverage is `domain/refresh.rs` (100%); lowest non-zero is `domain/command.rs` (8.54%). Twenty modules at 0% accepted per D-04 floor-to-5 policy — the ratchet for those is "do not remove tests that currently cover them," which is vacuously satisfied at 0%. |
| 2026-04-24 | 13 | Plan 13-01 — action.rs moved to domain; 3 traits + extract_jira_key relocated; per-file ratchet rows updated per structural-change policy. | `src/action.rs` deleted → replaced by new row `src/domain/action.rs` 0% (trait+enum moved verbatim, no new executable code). `src/infra/jira.rs` 70.18% → 0% is a structural split: the 6 `extract_jira_key*` inline tests migrated to new row `src/domain/jira.rs` 100% (same tests, new location). The 70% threshold invariant now binds to `domain/jira.rs`. Three new trait-only port files (`domain/ports/process_port.rs`, `jira_port.rs`, `multiplexer_port.rs`) added at 0% — trait definitions have no executable region and are floor-exempt. No test was removed; this is a pure file-move refactor. |
| 2026-04-24 | 13 | Plan 13-06 — F-200 structural split of src/app.rs (2522 LOC) into src/app/{mod,state,update,handle_key,runtime,effect_runner,adapters}.rs. Per-file ratchet rows updated per structural-change policy. | `src/app.rs` row DELETED → replaced by 7 new rows for the submodules. The 17 COVER-03 dispatch tests exercise handle_key (palette resolution + modal dismissal) and update (command queue drain); the former 11.31% src/app.rs coverage now distributes across `src/app/update.rs` and `src/app/handle_key.rs`. `src/app/runtime.rs` at 0% because the async event loop is not unit-tested — same as the corresponding `pub async fn run` block that previously lived at src/app.rs:2097-2246 (which contributed 0% to the src/app.rs baseline). `src/app/effect_runner.rs` and `src/app/adapters.rs` are STUBS (6-7 LOC each) — populated in Plans 13-07/13-08. `src/app/mod.rs` is re-exports only (0% floor-exempt). Zero behavior change — verbatim lift-and-shift. No test was removed. |

## Cross-Reference

- Human-readable baseline: [`BASELINE-COVERAGE.md`](./BASELINE-COVERAGE.md)
- Raw JSON (diffable): [`BASELINE-COVERAGE.json`](./BASELINE-COVERAGE.json)
- Phase decisions: [`12-CONTEXT.md`](./12-CONTEXT.md) §Threshold Policy (D-04, D-05)
