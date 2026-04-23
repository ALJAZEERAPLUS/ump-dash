# Phase 12 Baseline Coverage Report

**Generated:** 2026-04-23T19:00:49Z
**Toolchain:** rustc 1.94.1 (e408947bf 2026-03-25)
**cargo-llvm-cov:** cargo-llvm-cov 0.8.5
**Command:** `make cov-baseline` (alias for `cargo llvm-cov --workspace --json --summary-only --output-path .planning/phases/12-coverage-gate/BASELINE-COVERAGE.json`)
**Raw JSON:** `BASELINE-COVERAGE.json` (adjacent file)

## Prerequisites (one-time per dev machine)

```bash
cargo install cargo-llvm-cov --locked      # ~2 min
rustup component add llvm-tools-preview    # seconds
```

Without both, `make cov-baseline` fails with `command "llvm-profdata" not found` or
`unknown subcommand "llvm-cov"`. On newer rustup installations the component is named
`llvm-tools-aarch64-apple-darwin` (or the equivalent host triple) — both names refer to
the same LLVM tooling and either will satisfy `cargo-llvm-cov`.

## Totals

| Metric | Count | Covered | Percent |
|--------|-------|---------|---------|
| Lines | 3465 | 445 | 12.84% |
| Functions | 269 | 56 | 20.82% |
| Regions | 5958 | 589 | 9.89% |

## Per-File Baseline

| File | Lines % | Functions % | Regions % |
|------|---------|-------------|-----------|
| src/app.rs | 11.31% | 6.85% | 9.71% |
| src/domain/command.rs | 8.54% | 33.33% | 4.58% |
| src/domain/metro.rs | 70.00% | 63.16% | 70.29% |
| src/domain/refresh.rs | 100.00% | 100.00% | 100.00% |
| src/domain/worktree.rs | 0.00% | 0.00% | 0.00% |
| src/event.rs | 0.00% | 0.00% | 0.00% |
| src/infra/android_prefs.rs | 58.82% | 75.00% | 52.38% |
| src/infra/command_runner.rs | 0.00% | 0.00% | 0.00% |
| src/infra/config.rs | 8.70% | 9.09% | 7.06% |
| src/infra/devices.rs | 0.00% | 0.00% | 0.00% |
| src/infra/jira.rs | 70.18% | 72.73% | 75.86% |
| src/infra/jira_cache.rs | 0.00% | 0.00% | 0.00% |
| src/infra/multiplexer.rs | 0.00% | 0.00% | 0.00% |
| src/infra/port.rs | 0.00% | 0.00% | 0.00% |
| src/infra/process.rs | 0.00% | 0.00% | 0.00% |
| src/infra/sim_history.rs | 0.00% | 0.00% | 0.00% |
| src/infra/tmux.rs | 0.00% | 0.00% | 0.00% |
| src/infra/worktrees.rs | 0.00% | 0.00% | 0.00% |
| src/main.rs | 0.00% | 0.00% | 0.00% |
| src/tui.rs | 0.00% | 0.00% | 0.00% |
| src/ui/error_overlay.rs | 0.00% | 0.00% | 0.00% |
| src/ui/footer.rs | 0.00% | 0.00% | 0.00% |
| src/ui/help_overlay.rs | 0.00% | 0.00% | 0.00% |
| src/ui/mod.rs | 0.00% | 0.00% | 0.00% |
| src/ui/modals.rs | 0.00% | 0.00% | 0.00% |
| src/ui/panels.rs | 0.00% | 0.00% | 0.00% |
| src/ui/theme.rs | 0.00% | 0.00% | 0.00% |

Notes:
- `src/domain/refresh.rs` at 100% reflects the 17 inline tests canonized in Phase 11
  as the exemplary deep-module reference.
- `src/domain/metro.rs` at 70% reflects 12-01's register-once + register-twice
  characterization tests plus pre-existing inline tests.
- `src/infra/jira.rs` at 70% reflects the six existing `extracts_key*` /
  `returns_none*` inline tests.
- `src/app.rs` at 11.31% is low because the file is ~3000 LOC and 12-03's dispatch
  tests cover only the command-queue, modal-dismissal, and palette-resolution
  surfaces. Expanding dispatch coverage is a Phase-13+ concern, not a COVER-04 one.
- Modules at 0% (most of `ui/*`, most of `infra/*`, `tui.rs`, `event.rs`,
  `domain/worktree.rs`, `main.rs`) have no unit tests and are exercised only via
  manual TUI driving. These lines are accepted at 0% by the floor-to-5 policy
  (threshold = 0%) — Phase 13+ cannot regress below 0, so the ratchet is effectively
  a "do not remove the tests we added" guarantee for these modules, not a coverage
  target.

## Reproducibility Note (Pitfall 7 from 12-RESEARCH.md)

Different `rustc` versions can produce subtly different line/region counts for
the same Rust source. This baseline was measured on `rustc 1.94.1 (e408947bf 2026-03-25)`.
Phase 13+ coverage checks should be run on the same toolchain when possible. The
`floor(baseline, 5)` threshold policy (see `COVERAGE-THRESHOLDS.md`) has 5 percentage
points of slack built in to absorb minor toolchain drift. If a coverage regression
appears that cannot be explained by the diff under review, verify toolchain parity
before treating it as a bug.

## How to Regenerate

```bash
# Ensure prerequisites are installed (see above).
make cov-baseline          # regenerates BASELINE-COVERAGE.json
# Then re-run the jq extraction scripts documented in 12-RESEARCH.md §Code Examples B
# to rebuild this file and COVERAGE-THRESHOLDS.md.
```

Quick per-file extraction one-liner (for re-populating the table above):

```bash
jq -r '.data[0].files[]
       | "| \(.filename | sub("^.*/dashboard/"; "")) | \(.summary.lines.percent)% | \(.summary.functions.percent)% | \(.summary.regions.percent)% |"' \
  .planning/phases/12-coverage-gate/BASELINE-COVERAGE.json | sort
```

## Cross-Reference

- Threshold table: [`COVERAGE-THRESHOLDS.md`](./COVERAGE-THRESHOLDS.md)
- Phase decisions: [`12-CONTEXT.md`](./12-CONTEXT.md) §Threshold Policy (D-04, D-05)
- Research: [`12-RESEARCH.md`](./12-RESEARCH.md) §Pattern verified, §Code Examples, §Pitfall 7
