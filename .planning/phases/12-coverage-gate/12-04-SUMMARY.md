---
phase: 12-coverage-gate
plan: 04
subsystem: testing
tags: [cargo-llvm-cov, coverage-baseline, thresholds, rust, llvm, ratchet, phase-12-final]

# Dependency graph
requires:
  - phase: 12-coverage-gate
    plan: 00
    provides: "Makefile cov-baseline target writing to .planning/phases/12-coverage-gate/BASELINE-COVERAGE.json (D-02/D-03); lib+bin crate layout enabling integration tests"
  - phase: 12-coverage-gate
    plan: 01
    provides: "Metro single-instance characterization tests (register_twice_panics + register_once_then_clear_allows_second_register inline; metro_start_while_running_triggers_restart_not_double_spawn integration) — reason src/domain/metro.rs baseline is 70% and not 0%"
  - phase: 12-coverage-gate
    plan: 02
    provides: "Process-group-kill integration test (killing_pgid_reaps_child_tree) — reason the tests/ harness is linked and included in the coverage run"
  - phase: 12-coverage-gate
    plan: 03
    provides: "17 inline dispatch tests (command-queue drain / modal dismissal / palette resolution) — reason src/app.rs baseline is 11.31% and not 0%"
provides:
  - ".planning/phases/12-coverage-gate/BASELINE-COVERAGE.json — LLVM coverage export (machine-readable, diffable) that Phase 13+ `make cov-check` compares against"
  - ".planning/phases/12-coverage-gate/BASELINE-COVERAGE.md — human-readable per-file table + prerequisites + Pitfall-7 toolchain note"
  - ".planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md — ratchet table with threshold = floor(baseline, 5) for every src/ file (D-04/D-05)"
  - "Phase 12 exit gate — all four COVER-NN are green, Phase 13 refactor can now begin"
affects: [13-refactor, 14-task-system, 15-shell-command-cancellation, 16-release-polish]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Coverage baseline = one diffable JSON + two human-readable MDs in .planning/phases/12-coverage-gate/. Phase 13+ regenerates the JSON via `make cov-baseline`, diffs it against this baseline, and uses COVERAGE-THRESHOLDS.md as the row-by-row ratchet."
    - "floor(baseline, 5) policy avoids inventing aspirational numbers — the threshold is whatever reality measured, rounded down to the nearest 5%. Gives ~5 pct-point slack for rustc/llvm toolchain drift."
    - "Absolute paths produced by cargo-llvm-cov are stripped to repo-relative via `.filename | sub(\"^/Users/cubicme/aljazeera/dashboard/\"; \"\")` in jq extraction — future machines running on different paths need to update this prefix or use a relative-path variant."

key-files:
  created:
    - ".planning/phases/12-coverage-gate/BASELINE-COVERAGE.json"
    - ".planning/phases/12-coverage-gate/BASELINE-COVERAGE.md"
    - ".planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md"
  modified: []

key-decisions:
  - "D-04/D-05 applied literally — 27 per-file threshold rows, each `floor(baseline_lines_pct, 5)`. Totals threshold is 10% (from 12.84% baseline), function 20% (from 20.82%), region 5% (from 9.89%)."
  - "Zero-coverage files treated as accepted reality, not a blocker — 20 of 27 files sit at 0% (most of ui/*, most of infra/*, tui.rs, event.rs, domain/worktree.rs, main.rs). Their ratchet-at-0 means Phase 13+ cannot go below 0, which is vacuously satisfied. Adding tests for them is a Phase-13+ task, not a COVER-04 one."
  - "Toolchain header embeds rustc 1.94.1 (e408947bf 2026-03-25) + cargo-llvm-cov 0.8.5 in BASELINE-COVERAGE.md so Phase 13+ can spot-check drift if thresholds appear violated."

patterns-established:
  - "Phase baselines live at .planning/phases/NN-name/BASELINE-COVERAGE.{json,md} + COVERAGE-THRESHOLDS.md — a future phase-NN can add its own baseline without conflicting with the Phase-12 one by using a phase-scoped filename prefix."
  - "`git add -f` is required to commit new files under .planning/ because .gitignore line 5 (`.planning/`) blocks auto-tracking. Existing tracked .planning files continue to be tracked; only new-file addition needs the force flag."

requirements-completed:
  - COVER-04

# Metrics
duration: 3m 9s
completed: 2026-04-23
---

# Phase 12 Plan 04: Post-Wave-2 Coverage Baseline Summary

**Ran `make cov-baseline` after 12-01/02/03 tests landed; committed a machine-readable LLVM JSON + two human-readable Markdown tables (BASELINE-COVERAGE.md + COVERAGE-THRESHOLDS.md) as the Phase-13 regression ratchet.**

## Performance

- **Duration:** 3 min 9 s
- **Started:** 2026-04-23T18:59:33Z
- **Completed:** 2026-04-23T19:02:42Z
- **Tasks:** 4 planned (1 human-action prereq, 1 auto JSON, 1 auto MD, 1 human-verify gate)
- **Tasks executed autonomously:** 2 auto tasks + 2 auto-approved checkpoints (YOLO mode)
- **Files created:** 3 (BASELINE-COVERAGE.json, BASELINE-COVERAGE.md, COVERAGE-THRESHOLDS.md)
- **Files modified:** 0

## Accomplishments

- Generated a full cargo-llvm-cov baseline of the workspace (46 lib tests + 2 metro_single_instance integration tests + 1 process_group_kill integration test) with `--json --summary-only`. Output is well-formed LLVM export (`type = "llvm.coverage.json.export"`, version `"3.0.1"`, 27 `src/` files, totals block with line/function/region percentages).
- Produced a human-readable per-file baseline table with one row per src/ file (`filename | lines % | functions % | regions %`), totals section, prerequisites section, Pitfall-7 reproducibility note, and a "how to regenerate" block.
- Produced a per-file threshold table applying `floor(baseline_pct, 5)` to every row per D-04, totals-level thresholds, explicit invariants Phase-13+ must preserve, and a changelog section seeded with the initial Phase-12 entry per D-05.
- Confirmed `tests/*.rs` source files are excluded from the coverage report (cargo-llvm-cov default behavior); only `src/` files appear.
- Verified the HTML report at `target/llvm-cov/html/*` is not tracked (`/target/llvm-cov*` in `.gitignore` from Plan 12-00).
- Ran the full verification suite: `cargo test --quiet` (46 lib + 3 integration tests pass), `cargo clippy --all-targets -- -D warnings` (clean).

## Coverage Highlights

- **Total line coverage:** 445 / 3465 = **12.84%** → threshold **10%**
- **Total function coverage:** 56 / 269 = **20.82%** → threshold **20%**
- **Total region coverage:** 589 / 5958 = **9.89%** → threshold **5%**

**Highest-covered modules (≥ 50% lines):**
- `src/domain/refresh.rs` — 100% (17 Phase-11 inline tests)
- `src/domain/metro.rs` — 70% (COVER-01 + pre-existing inline)
- `src/infra/jira.rs` — 70.18% (six pre-existing `extracts_key*` tests)
- `src/infra/android_prefs.rs` — 58.82% (pre-existing inline tests)

**Mid-coverage modules (< 15% lines but non-zero):**
- `src/app.rs` — 11.31% (12-03 dispatch tests exist; app.rs is ~3000 LOC so small share)
- `src/infra/config.rs` — 8.70%
- `src/domain/command.rs` — 8.54%

**Zero-coverage modules (accepted by floor-to-5 at 0% threshold):**
- All of `src/ui/*` (7 files: error_overlay, footer, help_overlay, mod, modals, panels, theme) — TUI render code, no unit tests
- Most of `src/infra/*` (10 files: command_runner, devices, jira_cache, multiplexer, port, process, sim_history, tmux, worktrees) — infrastructure adapters
- `src/tui.rs`, `src/event.rs`, `src/main.rs`, `src/domain/worktree.rs` — 0% each

The 20 zero-coverage files are accepted at 0% threshold: the floor-to-5 policy means Phase 13+ cannot ratchet below 0%, so the ratchet is effectively "do not remove the tests we added" (vacuously true for files with no tests). Adding unit tests for these modules is a Phase-13+ concern, not a COVER-04 one.

## Task Commits

1. **Task 04.1: Install cargo-llvm-cov + llvm-tools-preview** — no commit (prerequisite already installed by orchestrator; verified `cargo-llvm-cov 0.8.5` + `llvm-tools-aarch64-apple-darwin` + `jq 1.8.1` present before running).

2. **Task 04.2 + 04.3: Generate baseline JSON + MD artifacts** — `5cc75ae` (chore)
   - Ran `make cov-baseline` producing `.planning/phases/12-coverage-gate/BASELINE-COVERAGE.json` (27 files, 3465 lines, 12.84% total line coverage).
   - Validated JSON schema: `type == "llvm.coverage.json.export"`, version string present, totals numeric, files >= 20, no `tests/*.rs` entries.
   - Generated `BASELINE-COVERAGE.md` with totals table, 27-row per-file table, prerequisites section, Pitfall-7 reproducibility note, "how to regenerate" block.
   - Generated `COVERAGE-THRESHOLDS.md` with totals-level thresholds, 27-row per-file threshold table (`floor(baseline, 5)` for every row), explicit Phase-13+ invariants block, and initial changelog entry.

3. **Task 04.4: Human sanity-check of baseline** — no commit (auto-approved in YOLO mode; all spot-checks pass — `domain/metro.rs` at 70% confirms 12-01 tests ran under coverage, `domain/refresh.rs` at 100% confirms Phase-11 tests intact, `src/app.rs` at 11.31% confirms 12-03 dispatch tests ran).

## Files Created/Modified

Created (committed in `5cc75ae`):
- `.planning/phases/12-coverage-gate/BASELINE-COVERAGE.json` — 12,367 bytes, LLVM JSON export v3.0.1, 27 `src/` files, totals block
- `.planning/phases/12-coverage-gate/BASELINE-COVERAGE.md` — Totals + per-file table + Prerequisites + Reproducibility Note + How-to-Regenerate
- `.planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md` — Totals thresholds + per-file thresholds + Phase-13+ invariants + Changelog

Modified: none.

## Decisions Made

- **27 baseline rows, not ≥ 20.** The plan's acceptance criterion said `>= 20`; the actual src/ tree has 27 files. All are included; none are filtered out.
- **Repo-relative paths in Markdown tables.** `cargo-llvm-cov --json` writes absolute paths (`/Users/cubicme/aljazeera/dashboard/src/app.rs`). Extraction stripped the prefix via `sub("^/Users/cubicme/aljazeera/dashboard/"; "")` so table rows are `| src/app.rs | …` — diffable across machines after Phase-13 contributors update the prefix or use a relative-path alternative.
- **Percentages rounded to 2 decimal places in Markdown**, not kept at 15-digit precision. The JSON retains full precision; the MD is for humans.
- **Float threshold formula.** `((.summary.lines.percent / 5) | floor) * 5` — jq's `floor` floors floats to integer, multiplied back by 5 yields the nearest-5 floor. For a 70.00% baseline this is 70; for 8.70% it is 5; for 11.31% it is 10. Verified row-by-row via awk post-check.
- **Sections required by acceptance criteria** explicitly present with `##` headings: `## Prerequisites`, `## Totals`, `## Per-File Baseline`, `## Reproducibility Note (Pitfall 7...)`, `## How to Regenerate` in BASELINE-COVERAGE.md; totals thresholds + per-file thresholds + invariants + Changelog in COVERAGE-THRESHOLDS.md.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `git add -f` required for new files under .planning/**
- **Found during:** Task 04.2 (staging baseline artifacts)
- **Issue:** `.gitignore` line 5 (`.planning/`) excludes new untracked files under that tree. `git status --short .planning/…/BASELINE-COVERAGE.json` returned empty, and `git check-ignore` confirmed `.gitignore:5` was matching. The plan's acceptance criterion requires the three artifacts to be **committed** under `.planning/phases/12-coverage-gate/`.
- **Fix:** Used `git add -f` to force-stage the three artifacts. This matches the historical convention for .planning/ content (all existing .planning files — e.g., 12-00-PLAN.md through 12-04-PLAN.md, 12-CONTEXT.md, etc. — were presumably force-added at creation and then continued as tracked files). No .gitignore edit was required — existing tracked files stay tracked.
- **Files modified:** none (only changed the `git add` invocation)
- **Commit:** `5cc75ae`

### Human-Action Checkpoint (Task 04.1)

The plan contains one `checkpoint:human-action` for installing `cargo-llvm-cov` + `llvm-tools-preview`. Per the orchestrator's objective, those prereqs were already installed on this machine:

```bash
$ cargo llvm-cov --version
cargo-llvm-cov 0.8.5
$ rustup component list --installed | grep llvm-tools
llvm-tools-aarch64-apple-darwin
```

Note: the component's modern name is `llvm-tools-aarch64-apple-darwin` (host-triple-suffixed) rather than the legacy alias `llvm-tools-preview`. Both refer to the same LLVM tooling and either satisfies `cargo-llvm-cov`. This was surfaced in BASELINE-COVERAGE.md's Prerequisites section.

### Human-Verify Checkpoint (Task 04.4)

Auto-approved under YOLO mode. Spot-checks that PASSED:
- All three artifact files present at expected paths.
- Every threshold row has `threshold ≤ baseline` (awk check: 27 rows validated, 0 violations).
- `src/domain/metro.rs` at 70% — confirms COVER-01's register-once / register-twice tests executed under coverage.
- `src/domain/refresh.rs` at 100% — confirms Phase-11's 17 inline tests still pass and are measured.
- `src/app.rs` at 11.31% — confirms COVER-03's dispatch_tests module (17 tests) executed under coverage. Not exceptional coverage because app.rs is ~3000 LOC, but non-zero confirms the wiring works.
- HTML report at `target/llvm-cov/html/` not tracked (gitignored).

## Coverage Observations Worth Flagging for Phase 13

These are not blockers for COVER-04 (per the floor-to-5 policy, 0% baseline → 0% threshold is accepted), but are relevant to Phase 13's refactor scoping:

- `src/infra/process.rs` at 0% despite Phase-12 process-group-kill test — the integration test in `tests/process_group_kill.rs` spawns `bash` via `tokio::process::Command` directly; it does NOT route through `src/infra/process.rs`. If that wrapper exists to be the single subprocess-spawning surface (per Phase-11 findings), Phase 13 should either delete the wrapper or write a characterization test that actually drives it.
- `src/infra/command_runner.rs` at 0% — the same note applies; Phase 13 should evaluate whether this is dead code or needs a test.
- `src/domain/worktree.rs` at 0% — the domain layer should have unit tests; this is a Phase-13+ candidate.
- `src/app.rs` at 11.31% is low relative to its importance (command dispatch) — expanding dispatch coverage is a Phase-13 refactor candidate (split app.rs into smaller files per Phase-11 audit findings).

These are **flagged, not blocking**. Phase 12's scope ended at "lock the ratchet"; expanding coverage is a Phase-13+ concern.

## Issues Encountered

- None during execution. The single Rule-3 fix (`-f` flag on `git add`) was a staging-mechanics issue, not a code issue.

## User Setup Required

None post-execution. Prerequisites (`cargo install cargo-llvm-cov`, `rustup component add llvm-tools-preview` or equivalent) remain documented in BASELINE-COVERAGE.md's Prerequisites section for future contributors and CI setup.

## Phase 12 Exit Gate — ALL FOUR COVER-NN GREEN

- **COVER-01 (Plan 12-01):** ✓ 3 metro inline tests + 2 metro_single_instance integration tests pass
- **COVER-02 (Plan 12-02):** ✓ killing_pgid_reaps_child_tree integration test passes in < 1 s on macOS
- **COVER-03 (Plan 12-03):** ✓ 17 dispatch_tests pass under `cargo test --lib dispatch_tests`
- **COVER-04 (this plan):** ✓ BASELINE-COVERAGE.json + BASELINE-COVERAGE.md + COVERAGE-THRESHOLDS.md committed, all 27 threshold rows satisfy `threshold ≤ baseline`, HTML report gitignored
- **Global:** ✓ `cargo test --quiet` exits 0, `cargo clippy --all-targets -- -D warnings` exits 0

**Phase 12 is COMPLETE. Phase 13 (architecture refactor) can now begin.**

## Next Phase Readiness

Phase 13 entry criteria per ROADMAP:
- ✓ Phase 12 baseline committed (this plan)
- ✓ Phase 11 audit findings available at `.planning/phases/11-architecture-audit/AUDIT.md`
- ✓ `make cov-check` target exists (from Plan 12-00) for Phase 13 regression detection
- ✓ `COVERAGE-THRESHOLDS.md` is the row-by-row ratchet Phase 13 must not drop below

Phase 13 workflow for any refactor PR:
1. Make refactor changes.
2. Run `make cov-baseline` (regenerates JSON).
3. Diff new JSON against committed baseline.
4. For each row in `COVERAGE-THRESHOLDS.md`: verify new baseline ≥ threshold.
5. If any row drops below threshold: either fix (restore coverage) or document a structural rationale in COVERAGE-THRESHOLDS.md's Changelog and lower the threshold. Never ratchet down without written rationale (D-05).

## Self-Check: PASSED

File existence:
- FOUND: .planning/phases/12-coverage-gate/BASELINE-COVERAGE.json
- FOUND: .planning/phases/12-coverage-gate/BASELINE-COVERAGE.md
- FOUND: .planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md

Commit existence:
- FOUND: 5cc75ae (chore(12-04): commit post-wave-2 coverage baseline artifacts)

Verification:
- `jq -e '.type == "llvm.coverage.json.export"' BASELINE-COVERAGE.json` — exit 0
- `jq -e '.data[0].files | length >= 20' BASELINE-COVERAGE.json` — exit 0 (actual: 27)
- `jq -e '.data[0].totals.lines.percent > 0 and .data[0].totals.lines.percent <= 100' BASELINE-COVERAGE.json` — exit 0 (actual: 12.84)
- `grep -c '^| src/' BASELINE-COVERAGE.md` — 27
- `grep -c '^| src/' COVERAGE-THRESHOLDS.md` — 27
- `grep -q 'Prerequisites' BASELINE-COVERAGE.md` — present
- `grep -q 'threshold' COVERAGE-THRESHOLDS.md` — present
- `grep -q 'Pitfall 7' BASELINE-COVERAGE.md` — present
- `awk` per-row `threshold <= baseline` check — 27/27 validated, 0 violations
- `cargo test --quiet` — exit 0
- `cargo clippy --all-targets -- -D warnings` — exit 0
- `jq -r '.data[0].files[].filename' BASELINE-COVERAGE.json | grep -E '^tests/'` — empty (tests/ excluded)

---
*Phase: 12-coverage-gate*
*Completed: 2026-04-23*
*Phase 12 complete — all 5 plans landed, all 4 COVER-NN requirements green.*
