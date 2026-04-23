# Phase 12: Coverage Gate - Context

**Gathered:** 2026-04-23
**Status:** Ready for planning
**Mode:** Auto-generated via `/gsd-discuss-phase 12 --auto` (YOLO mode per CLAUDE.md)

<domain>
## Phase Boundary

Lock in targeted characterization tests and a baseline coverage report **before** any audit-driven refactor (Phase 13) or task-system rewrite (Phase 14+) touches the modules the tests cover. The phase is a **hard gate**: no Phase 13/14/15/16 code change merges until COVER-01..COVER-04 are green.

**In scope (from REQUIREMENTS.md §COVER):**
- COVER-01 — characterization test for the metro single-instance invariant
- COVER-02 — characterization test for process-group kill behavior (full subprocess tree terminated)
- COVER-03 — coverage tests for command-dispatch paths (`CommandQueued`/`CommandExited` routing, modal dismissal flow, palette→action resolution for a/i/x/y/g/w)
- COVER-04 — committed baseline coverage report with documented per-module minimum thresholds

**Out of scope (explicit):**
- Broader unit/integration test expansion beyond these four items (deferred per §Future Requirements)
- Any refactor of audited modules (Phase 13 handles that)
- Any structural change to the task system (Phase 14+)
- CI integration of the coverage tool (post-milestone per §Future Requirements)

</domain>

<decisions>
## Implementation Decisions

### Coverage Tooling
- **D-01:** Coverage tool is `cargo-llvm-cov` (not `cargo-tarpaulin`). Rationale: LLVM source-based coverage is more accurate on Rust async/generics than tarpaulin's ptrace sampling; cargo-llvm-cov is faster, is the ecosystem default for 2025, and produces lcov/HTML/json output natively.
- **D-02:** Coverage is invoked locally only this phase — no CI wiring. A Make target (or `.cargo/config.toml` alias) is added so `cargo llvm-cov --workspace --html` reproduces the baseline. CI integration is explicitly deferred.
- **D-03:** Baseline report is committed as `.planning/phases/12-coverage-gate/BASELINE-COVERAGE.json` (raw llvm-cov JSON, for diffable text) plus a human-readable `BASELINE-COVERAGE.md` summary. The HTML output is gitignored.

### Threshold Policy
- **D-04:** Per-module minimum threshold = `floor(measured_baseline_pct, 5)` — i.e., round the phase-12 baseline down to the nearest 5 % for each module. Example: domain/refresh.rs measured 92 % → threshold 90 %. This documents a ratchet without inventing aspirational numbers, and gives Phase 13+ a concrete regression signal.
- **D-05:** Thresholds are recorded in `.planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md` with `module | baseline % | threshold %` columns. No enforcement script this phase — it is a human-checked gate. (Enforcement script is a post-milestone concern.)

### Test Organization
- **D-06:** Pure domain-logic tests stay as inline `#[cfg(test)] mod tests` at the bottom of the module they cover (matches existing convention — 26 such tests today, with `domain/refresh.rs` canonized in Phase 11 as the exemplary deep-module reference with 17 inline tests).
- **D-07:** Characterization tests for COVER-01 and COVER-02 that require real subprocess behavior live in a new `tests/` directory at the workspace root (Rust integration-test convention) because they need `std::process` / `tokio::process::Command` at the crate-external boundary and cannot run inside `#[cfg(test)]` of the owning module.
- **D-08:** Command-dispatch tests for COVER-03 (queue routing, modal dismissal, palette → action) stay inline in `src/app.rs` (or a new `src/app/dispatch_tests.rs` sub-module if `app.rs` already exceeds ~1500 lines post-audit) because they are TEA `update()` tests — pure input-action-state transitions with no subprocess.

### Characterization Test Strategy
- **D-09:** COVER-01 (metro single-instance) is tested at the `MetroManager` type boundary: a test constructs a `MetroManager`, calls `register()` with a stub `MetroHandle`, and asserts that a second `register()` without an intervening `take_handle()` panics (the existing `debug_assert!`). Plus a second test at the `update()` layer: dispatch `Action::MetroStart { worktree: A }`, then dispatch `Action::MetroStart { worktree: B }` while A is running — assert the B dispatch enters the `ExternalMetroConflict` flow or is rejected, **not** that a second `MetroHandle` is constructed.
- **D-10:** COVER-02 (process-group kill) is tested with a real shell-script fixture: a test spawns `bash -c 'trap "" SIGTERM; sleep 30 & wait'` via the existing `CommandRunner` (which sets `.process_group(0)` per `src/infra/process.rs:37`), records the PGID, sends the cancellation, and asserts via `kill(pgid, 0)` that both the parent shell AND its `sleep` child are reaped within 2 s. The test is `#[tokio::test]` and is `#[cfg_attr(not(target_os = "linux"), ignore)]` + `#[cfg_attr(not(target_os = "macos"), ignore)]` — it runs on both dev (macOS) and CI (Linux) but is skipped on other targets.
- **D-11:** COVER-03 uses table-driven tests. A single `#[test] fn palette_action_resolution()` iterates `[(Palette::Actions, expected_actions), (Palette::Info, ...), ...]` for all six palettes — concise, and a new palette automatically fails the test if not added to the table. Modal dismissal is covered by one test per modal type (SyncBeforeMetro, ExternalMetroConflict, DevicePicker, SimPicker, BaseBranchPicker, DeleteWorktreeConfirm, CheckoutBranchPrompt, CreateBranchPrompt, others surfaced during planning).

### Dev Dependencies
- **D-12:** Add to `[dev-dependencies]` in Cargo.toml: `cargo-llvm-cov` is a **cargo subcommand** (installed via `cargo install`, not declared in Cargo.toml). Only a `tokio` dev-feature flag (`test-util` + `macros` if not already enabled) is added to `[dev-dependencies]`. No new runtime dep. `tempfile` is added ONLY if a process-group test needs a scratch directory — decide at planning time.
- **D-13:** No `mockall`, no `rstest`, no `proptest`. Keep dev-deps minimal; inline table-driven tests are sufficient. Fixture scripts are small `bash` strings built inline, not committed shell files.

### Claude's Discretion
- Choice between `BASELINE-COVERAGE.md` vs. embedding the summary directly in the Phase 12 SUMMARY.md — planner picks whichever keeps the per-plan SUMMARY ≤ 200 lines.
- Exact list of modals enumerated for COVER-03 modal dismissal coverage — planner greps `Modal::` variants in `src/app.rs` and uses that as the authoritative set.
- Whether COVER-03 palette tests live in `src/app.rs` or a new `src/app/dispatch_tests.rs` — decided by size of `app.rs` after Phase 11 comments settle.

### Folded Todos
None — no pending todos matched Phase 12 scope.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone & Requirements
- `.planning/PROJECT.md` — Core value, tech stack (Rust + Ratatui), Ousterhout constraint
- `.planning/REQUIREMENTS.md` §COVER (lines 11-18) — COVER-01..COVER-04 acceptance criteria, ordering rule ("no refactor or task-system phase ships until the coverage gate is green")
- `.planning/ROADMAP.md` §Phase 12 — goal, depends on Phase 11, success criteria

### Audit Findings That Inform Coverage Priorities
- `.planning/phases/11-architecture-audit/AUDIT.md` — severity-tagged findings from Phase 11; Critical/Major items ≈ highest-risk paths that must be characterized before refactor
- `.planning/phases/11-architecture-audit/11-CONTEXT.md` — Phase 11 decisions on module scoring; `domain/refresh.rs` identified as exemplary deep-module with 17 inline tests (reference pattern)

### Code Reference Points (for characterization tests)
- `src/domain/metro.rs` — `MetroManager::register()` and `take_handle()` enforce the single-instance invariant at the type level; `debug_assert!` on double-register is the test hook for COVER-01
- `src/infra/process.rs:37` — `.process_group(0)` + `kill_on_drop(true)` on `tokio::process::Command` is the mechanism COVER-02 must characterize
- `src/infra/port.rs` — port 8081 lsof lookup + SIGKILL; relevant to COVER-01's external-metro-conflict path
- `src/app.rs` — `update()`, `Action::CommandQueued` (line ~873), `Action::CommandExited` (line ~978), `command_queue: VecDeque` (line ~89); COVER-03 test surface

### Project Conventions
- Inline `#[cfg(test)] mod tests` convention (26 current instances) — matches D-06
- `cargo clippy -D warnings` must pass per Phase 13 REFACTOR success criteria — tests must clippy-clean too
- CLAUDE.md: `check-types` always uses `--incremental`; metro logs only stream when a filter is applied — no impact on this phase but noted

### External / Tool Docs
- `cargo-llvm-cov` README — invocation, flag set, JSON output schema (consulted by researcher during plan-phase)
- Rust Reference §integration tests (`tests/` directory) — confirms D-07 placement rule

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `#[cfg(test)] mod tests` pattern in `src/jira.rs` and `src/domain/refresh.rs` — template for inline module tests (D-06)
- `CommandRunner` in `src/infra/command_runner.rs` — already spawns processes with `.process_group(0)`; a test can drive this type directly
- `MetroManager::register` / `take_handle` — already has `debug_assert!` at line 115 that a test can trigger

### Established Patterns
- TEA `update(state, action) -> state` — pure function in `src/app.rs`; ideal for table-driven action-dispatch tests
- Inline constant tables for spinner frames (mentioned in Phase 11 state) — same style works for `[(palette, expected_actions)]` tables (D-11)
- No existing integration-test directory — this phase creates `tests/` for the first time (D-07)
- No existing coverage tooling — this phase introduces `cargo-llvm-cov` via README+Makefile instructions (D-02)

### Integration Points
- Makefile (or `.cargo/config.toml` alias) — add `cargo cov` target that runs `cargo llvm-cov --workspace --html`
- `.gitignore` — add `target/llvm-cov*` so HTML output is not tracked
- `Cargo.toml` `[dev-dependencies]` — only minimal additions (D-12)
- `tests/` directory — new top-level directory; contains `metro_single_instance.rs` and `process_group_kill.rs`
- `.planning/phases/12-coverage-gate/` — holds `BASELINE-COVERAGE.json`, `BASELINE-COVERAGE.md`, `COVERAGE-THRESHOLDS.md`

</code_context>

<specifics>
## Specific Ideas

- The metro single-instance test MUST assert the **type-level invariant** (`MetroManager::handle: Option<MetroHandle>` can hold only one value), not just a runtime flag. This matches the Phase 11 finding that `MetroHandle` is kept in `domain/` deliberately as an infrastructure-bridging type whose sole purpose is to be held inside the `Option<MetroHandle>` wrapper.
- The process-group test must run on BOTH macOS (dev) and Linux (CI), gated with `cfg_attr(target_os = ...)` so it is skipped on Windows if that target is ever added.
- Threshold policy (D-04) intentionally does NOT invent numbers. The threshold is whatever the baseline measures, floored to the nearest 5 %. This creates a one-way ratchet.

</specifics>

<deferred>
## Deferred Ideas

- Property-based tests (`proptest`) for command-dispatch logic — tempting but out of scope per §Future Requirements "Broader unit/integration test expansion beyond the targeted Coverage Gate".
- `cargo-deny` / `cargo-modules` CI integration — explicitly post-milestone per REQUIREMENTS §Future Requirements.
- CI wiring for the coverage tool — deferred per D-02.
- An enforcement script that fails CI when a module falls below its threshold — post-milestone per D-05.
- Tests for arbitrary shell command cancellation (TASK-04) — belongs in Phase 15, not Phase 12.
- `.github/workflows/coverage.yml` — post-milestone.

</deferred>

---

*Phase: 12-coverage-gate*
*Context gathered: 2026-04-23*
