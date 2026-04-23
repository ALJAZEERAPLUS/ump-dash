---
phase: 12-coverage-gate
plan: 00
subsystem: testing
tags: [cargo-llvm-cov, rust, tokio, integration-tests, coverage, rust-2024]

# Dependency graph
requires:
  - phase: 11-architecture-audit
    provides: Clean architectural baseline with domain/infra/app/ui separation confirmed; AUDIT.md findings inform which src/ modules need the tightest coverage ratchet in 12-04
provides:
  - bin+lib dual-crate layout — src/lib.rs re-exports all seven modules as `pub mod` so integration tests in tests/*.rs can `use rn_dash::domain::metro::MetroManager`
  - [dev-dependencies] scope with tokio macros/rt-multi-thread/process/time/io-util/sync features + anyhow for #[tokio::test] subprocess tests
  - Makefile with cov / cov-html / cov-baseline / cov-check targets wrapping cargo-llvm-cov (D-02 local-only invocation surface)
  - tests/common/mod.rs with fake_metro_handle(pid, worktree) factory for 12-01's MetroManager invariant tests
  - .gitignore entry /target/llvm-cov* so HTML coverage output is never committed (D-03)
affects: [12-01-metro-single-instance, 12-02-process-group-kill, 12-03-dispatch-tests, 12-04-baseline-coverage, 13-refactor]

# Tech tracking
tech-stack:
  added:
    - "cargo-llvm-cov (via Makefile wrappers; installed one-time per dev via `cargo install cargo-llvm-cov --locked`)"
    - "llvm-tools-preview rustup component (one-time per dev machine)"
    - "tokio dev-feature subset (macros, rt-multi-thread, process, time, io-util, sync) in a separate resolver scope from runtime tokio"
    - "anyhow dev-dep for integration-test error propagation"
  patterns:
    - "Bin+lib dual layout — main.rs stays thin, lib.rs is the shared module surface. Integration tests compile as a separate crate that depends on rn-dash via [lib]"
    - "Makefile recipe-as-documentation — each target carries D-xx decision references in comments so a future reader can trace the invocation back to the phase-12 context doc"
    - "tests/common/mod.rs submodule pattern — Rust-book convention for sharing code across tests/*.rs binaries without creating a standalone `tests/common.rs` binary"

key-files:
  created:
    - "src/lib.rs"
    - "Makefile"
    - "tests/common/mod.rs"
  modified:
    - "src/main.rs"
    - "Cargo.toml"
    - ".gitignore"
    - "src/domain/metro.rs"

key-decisions:
  - "[Rule 3 - Blocking] Added `impl Default for MetroManager` because clippy's new_without_default lint fires once domain::metro is publicly exposed via `pub mod domain` — previously suppressed because the module was private inside the bin crate. Required to meet the plan's `cargo clippy --all-targets -- -D warnings` success criterion."
  - "main.rs imports only `use rn_dash::{app, tui};` (modules actually referenced) rather than the literal 7-module import the plan text suggested. Unused imports would fail `cargo clippy -D warnings`. Behavior-equivalent to the plan's intent."

patterns-established:
  - "Pattern: any new domain/infra/app/ui public API must pass clippy -D warnings — previously-private items can surface dormant lints when promoted to pub"
  - "Pattern: Makefile.cov-baseline output path is committed into .planning/phases/12-coverage-gate/ so the baseline is diffable text alongside the phase docs (D-03)"

requirements-completed: []

# Metrics
duration: 4min
completed: 2026-04-23
---

# Phase 12 Plan 00: Coverage-Gate Scaffolding Summary

**rn-dash converted from bin-only to bin+lib so `tests/*.rs` can `use rn_dash::domain::...`; `cargo-llvm-cov` Makefile + `tests/common/mod.rs` helper landed for Wave 2 (12-01..12-03) to build on.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-04-23T18:01:22Z
- **Completed:** 2026-04-23T18:05:22Z
- **Tasks:** 2
- **Files modified:** 4
- **Files created:** 3

## Accomplishments

- Converted rn-dash to a dual-target crate (`[lib] name = "rn_dash"` + `[[bin]] name = "rn-dash"`) so integration tests in `tests/` can import `pub` items from the library. This was the foundational unlock for all of Phase 12 — without it, 12-01 and 12-02 cannot write `use rn_dash::domain::metro::MetroManager;` because integration-test binaries can only see public items of the **library** crate, not the **bin** crate.
- Added the coverage-tool Makefile surface (`cov`, `cov-html`, `cov-baseline`, `cov-check`) with the exact `cargo-llvm-cov` invocation that 12-04 will run to produce the committed baseline JSON.
- Added `tests/common/mod.rs` with `fake_metro_handle(pid, worktree)` — the shared helper 12-01 will call from two separate `#[tokio::test]` functions to construct a minimal `MetroHandle` without touching the real subprocess path.
- Fixed a pre-existing dormant clippy warning (`new_without_default` on `MetroManager::new`) that only surfaced once `domain::metro` became `pub mod`. Required for `-D warnings` to pass.

## Task Commits

1. **Task 00.1: Convert rn-dash to bin+lib crate** — `ee3dc6d` (feat)
   - Added `src/lib.rs` with `pub mod {action, app, domain, event, infra, tui, ui};`
   - Rewrote `src/main.rs` to import via `use rn_dash::{app, tui};` — only the two modules actually referenced in `main()`
   - Added `[lib]`, `[[bin]]`, `[dev-dependencies]` sections to `Cargo.toml`
   - Added `impl Default for MetroManager` (Rule-3 deviation to pass clippy)

2. **Task 00.2: Add Makefile + .gitignore + tests/common/mod.rs** — `bc62b2e` (chore)
   - `Makefile` with 4 targets; recipe lines use literal TAB
   - `.gitignore` gained `/target/llvm-cov*` (D-03)
   - `tests/common/mod.rs` exports `fake_metro_handle(pid: u32, worktree: &str) -> MetroHandle`

## Files Created/Modified

Created:
- `src/lib.rs` — library crate root; `pub mod` for each of the seven top-level modules
- `Makefile` — local-only coverage targets (D-02); cov-baseline writes to `.planning/phases/12-coverage-gate/BASELINE-COVERAGE.json`
- `tests/common/mod.rs` — shared `fake_metro_handle` factory for 12-01's integration tests

Modified:
- `src/main.rs` — dropped 7 private `mod` declarations; added `use rn_dash::{app, tui};` (only the two references used inside `main()`). Runtime behavior unchanged.
- `Cargo.toml` — new `[lib]` / `[[bin]]` / `[dev-dependencies]` sections. Runtime `[dependencies]` are untouched.
- `.gitignore` — added `/target/llvm-cov*` comment + pattern
- `src/domain/metro.rs` — added `impl Default for MetroManager { fn default() -> Self { Self::new() } }` (Rule-3 deviation)

## Decisions Made

- **Named the lib `rn_dash` (underscore), kept the bin `rn-dash` (hyphen).** Rust identifiers can't contain hyphens, so `use rn_dash::…` is mandatory from Rust code; `cargo install rn-dash` keeps the public-facing binary name unchanged.
- **Only imported modules used in `main()`.** The plan text suggested `use rn_dash::{action, app, domain, event, infra, tui, ui};` but `main()` only references `app::` and `tui::` — the other five would trigger `unused_imports` under `-D warnings`. Functionally identical (main.rs depends on the lib), without a clippy failure.
- **Re-declared `tokio` in `[dev-dependencies]` with a minimal feature set**, not `workspace = true`. rn-dash is a single crate, not a workspace, so workspace-inheritance syntax is unavailable (per research §Dev-Dependencies).
- **Kept `anyhow` in `[dev-dependencies]`** even though it's also a runtime dep. Dev-dep and runtime-dep scopes are separate; cargo feature-unifies the resolver, so this adds zero binary size but gives the integration-test crate explicit access.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added `impl Default for MetroManager`**
- **Found during:** Task 00.1 (verification `cargo clippy --all-targets -- -D warnings`)
- **Issue:** Clippy's `new_without_default` lint fires because `MetroManager::new()` is now reachable as public API via `rn_dash::domain::metro::MetroManager::new`. The lint was dormant previously because `mod domain` inside the bin-only crate kept it effectively private. Task 00.1's success criteria include `cargo clippy --all-targets -- -D warnings` passing — the lint would block this and every subsequent wave-2 plan's clippy check.
- **Fix:** Added a three-line `impl Default for MetroManager { fn default() -> Self { Self::new() } }` above the existing `impl MetroManager`. Preserves the existing `new()` call sites; adds idiomatic `Default::default()` access.
- **Files modified:** `src/domain/metro.rs`
- **Verification:** `cargo clippy --all-targets -- -D warnings` now exits 0; `cargo test --lib --quiet` reports 26/26 passing (no regression in existing inline tests).
- **Committed in:** `ee3dc6d` (part of Task 00.1 commit)

**2. [Rule 3 - Blocking] main.rs imports minimal set instead of literal 7-module import from plan**
- **Found during:** Task 00.1 (planning review of main.rs's actual references)
- **Issue:** Plan text says to replace the 7 `mod` lines with `use rn_dash::{action, app, domain, event, infra, tui, ui};`. But `main()` only references `app::run` and `tui::setup_logging` — the other five imports would fire `unused_imports` under `-D warnings` and fail clippy.
- **Fix:** Imported only `use rn_dash::{app, tui};`. Preserves the plan's essential intent: main.rs depends on lib.rs, integration tests can still `use rn_dash::...` because `src/lib.rs` still declares all seven modules as `pub`.
- **Files modified:** `src/main.rs`
- **Verification:** `cargo build --quiet` exits 0; `cargo clippy --all-targets -- -D warnings` exits 0; `grep '^mod ' src/main.rs` returns 0 lines; `grep 'crate::' src/main.rs` returns 0 lines.
- **Committed in:** `ee3dc6d` (part of Task 00.1 commit)

---

**Total deviations:** 2 auto-fixed (both Rule 3 - Blocking)
**Impact on plan:** Both deviations preserve the plan's exact functional intent (bin+lib conversion + clean clippy) while satisfying the `-D warnings` success criterion the plan's literal wording would have failed. No scope creep; no architecture change.

## Research Deviation Note

Per plan output spec: **one material deviation from `12-RESEARCH.md`.** The research assumed `rn_dash` was already a lib (it repeatedly wrote `use rn_dash::domain::metro::MetroManager` as if that already worked). Planning caught this via `grep -n "^pub mod\|^mod " src/main.rs` — main.rs used private `mod` declarations and there was no `src/lib.rs`. This plan (12-00) created the lib target, and every downstream plan's `use rn_dash::...` is now valid. No changes required to the research document — the test patterns it described are still correct; they just need this plan to run first.

## One-Time Dev Prerequisites

These are NOT installed by any target in this plan — they are per-developer-machine setup. Plan 12-04 will re-document them in `BASELINE-COVERAGE.md`:

```bash
cargo install cargo-llvm-cov --locked      # ~2 minutes
rustup component add llvm-tools-preview    # seconds
```

Without both, `make cov-baseline` will fail with "command `llvm-profdata` not found" or "unknown subcommand `llvm-cov`".

## Issues Encountered

- None during the two planned tasks. The Rule-3 clippy fix was a pre-existing latent issue that only surfaces when the type is promoted to public API; it was fixed in Task 00.1 rather than deferred because it would otherwise block every later plan's clippy check.

## User Setup Required

None for this plan specifically. The one-time `cargo install cargo-llvm-cov` + `rustup component add llvm-tools-preview` prereqs are developer-machine setup documented in the Makefile header and will be re-surfaced in 12-04's `BASELINE-COVERAGE.md`.

## Next Phase Readiness

Wave 2 (12-01, 12-02, 12-03) can now fan out in parallel worktrees:

- **12-01** `tests/metro_single_instance.rs` — `use rn_dash::domain::metro::{MetroManager, MetroHandle};` + `mod common; use common::fake_metro_handle;` compiles against this plan's lib+tests/common
- **12-02** `tests/process_group_kill.rs` — doesn't need `rn_dash` symbols (direct `tokio::process::Command`), but the tests/ dir and `[dev-dependencies] tokio` are live
- **12-03** `src/app/dispatch_tests.rs` — inline `#[cfg(test)]` under `app.rs`, doesn't need the integration-test crate; but `[dev-dependencies]` tokio's `macros` feature enables `#[tokio::test]` for the `command_queue` drain test

Main tree is clean, committed, and clippy-clean at `HEAD=bc62b2e`. Worktrees for wave 2 will branch cleanly from this point.

## Self-Check: PASSED

File existence:
- FOUND: src/lib.rs
- FOUND: Makefile
- FOUND: tests/common/mod.rs

Commit existence:
- FOUND: ee3dc6d (Task 00.1)
- FOUND: bc62b2e (Task 00.2)

Verification:
- `cargo build --quiet` — exit 0
- `cargo build --tests --quiet` — exit 0
- `cargo test --lib --quiet` — 26 passed; 0 failed
- `cargo clippy --all-targets -- -D warnings` — Finished dev profile (0 warnings)
- `make -n cov-baseline` — prints expected `cargo llvm-cov --workspace --json --summary-only --output-path .planning/phases/12-coverage-gate/BASELINE-COVERAGE.json`
- `grep -c '^pub mod' src/lib.rs` — 7
- `grep -c '^mod ' src/main.rs` — 0
- `grep -c 'crate::' src/main.rs` — 0
- `grep -c '^\[lib\]' Cargo.toml` — 1
- `grep -c '^\[\[bin\]\]' Cargo.toml` — 1
- `grep -c '^\[dev-dependencies\]' Cargo.toml` — 1
- `grep -q '/target/llvm-cov\*' .gitignore` — present
- Makefile recipe-line first byte — `\t` (tab) confirmed via od

---
*Phase: 12-coverage-gate*
*Completed: 2026-04-23*
