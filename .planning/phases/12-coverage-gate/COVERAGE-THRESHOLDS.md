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
| src/app.rs | 11.31% | 10% |
| src/domain/command.rs | 8.54% | 5% |
| src/domain/metro.rs | 70.00% | 70% |
| src/domain/refresh.rs | 100.00% | 100% |
| src/domain/worktree.rs | 0.00% | 0% |
| src/event.rs | 0.00% | 0% |
| src/infra/android_prefs.rs | 58.82% | 55% |
| src/infra/command_runner.rs | 0.00% | 0% |
| src/infra/config.rs | 8.70% | 5% |
| src/infra/devices.rs | 0.00% | 0% |
| src/infra/jira.rs | 70.18% | 70% |
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

## Invariants Phase 13+ MUST Preserve

- `src/domain/refresh.rs >= 100%` — dropping any of the 17 inline tests in Phase 13 refactor is a ratchet violation.
- `src/domain/metro.rs >= 70%` — the register-once / register-twice / update-level `MetroStart` characterization tests (COVER-01) must keep passing under coverage.
- `src/infra/jira.rs >= 70%` — the six `extracts_key*` / `returns_none*` inline tests must keep passing.
- `src/infra/android_prefs.rs >= 55%` — pre-existing inline tests must keep passing.
- `src/app.rs >= 10%` — 12-03's dispatch-tests module (command-queue drain, modal dismissal, palette resolution) must keep passing under coverage.
- `src/domain/command.rs >= 5%` and `src/infra/config.rs >= 5%` — whatever minimal coverage exists today must not regress.
- Total line coverage `>= 10%`, total function coverage `>= 20%`, total region coverage `>= 5%`.

Any Phase 13+ PR that drops a row below its threshold requires:
1. Explanation of why the drop is a structural change (e.g., deleted dead code), not a test regression.
2. An entry in the Changelog section below updating the threshold.

## Changelog

| Date | Phase | Change | Rationale |
|------|-------|--------|-----------|
| 2026-04-23 | 12 | Initial thresholds | Baseline after COVER-01/02/03 landed (Waves 1+2). Full workspace line coverage = 12.84%; highest coverage is `domain/refresh.rs` (100%); lowest non-zero is `domain/command.rs` (8.54%). Twenty modules at 0% accepted per D-04 floor-to-5 policy — the ratchet for those is "do not remove tests that currently cover them," which is vacuously satisfied at 0%. |

## Cross-Reference

- Human-readable baseline: [`BASELINE-COVERAGE.md`](./BASELINE-COVERAGE.md)
- Raw JSON (diffable): [`BASELINE-COVERAGE.json`](./BASELINE-COVERAGE.json)
- Phase decisions: [`12-CONTEXT.md`](./12-CONTEXT.md) §Threshold Policy (D-04, D-05)
