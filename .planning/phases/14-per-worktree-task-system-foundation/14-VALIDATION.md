---
phase: 14
slug: per-worktree-task-system-foundation
status: approved
nyquist_compliant: true
wave_0_complete: true
created: 2026-04-28
plans_validated: 2026-04-28
---

# Phase 14 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Derived from `14-RESEARCH.md` §Validation Architecture (lines 770-825).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (rustc 1.94.1) — `#[test]` + `#[tokio::test]` |
| **Config file** | `Cargo.toml` ([VERIFIED: read by researcher]) |
| **Quick run command** | `cargo test --lib` (76 lib tests, ~0s) |
| **Full suite command** | `cargo test --workspace` (76 lib + 2 metro_single_instance + 1 process_group_kill = 79 tests) |
| **Shape guard command** | `make arch-lint` (20 active G-01..G-20; G-21 added by this phase) |
| **Estimated runtime** | ~5 seconds full suite + arch-lint |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib` (≤ 1s; covers all inline + dispatch_tests)
- **After every plan wave:** Run `cargo test --workspace && make arch-lint` (≤ 5s; includes integration + 20 grep guards)
- **Before `/gsd-verify-work`:** `cargo test --workspace && cargo clippy --all-targets -- -D warnings && make arch-lint && make cov-check`
- **Max feedback latency:** 5 seconds

The 79-test count must be ≥ 79 (Phase 13 baseline) and is expected to rise to ≥ 84 once Phase 14's new parallelism + routing tests land (D-21).

---

## Per-Task Verification Map

| Req ID | Behavior | Test Type | Automated Command | File Exists |
|--------|----------|-----------|-------------------|-------------|
| TASK-01 | Per-worktree slice replaces 4 globals | unit (inline in `state.rs` + `worktree_slice.rs`) | `cargo test --lib worktree_slice` | ❌ W0 — new file |
| TASK-01 | All 17 existing dispatch_tests still pass with rewritten assertions | unit (inline) | `cargo test --lib dispatch_tests` | ✅ (existing 570 LOC; D-21 rewrite) |
| TASK-01 | G-21 forbids re-introduction of deleted field names | shape guard | `make arch-lint` (G-21) | ❌ W0 — Makefile addition (last plan in D-23) |
| TASK-01 | All 20 existing G-XX guards stay green | shape guard | `make arch-lint` | ✅ (existing) |
| TASK-02 | Parallel yarn-on-A + jest-on-B both have `task.is_some()` simultaneously | unit (inline in `dispatch_tests.rs`) | `cargo test --lib parallel_yarn_jest` | ❌ W0 — new test (D-21) |
| TASK-02 | Metro single-instance preserved (COVER-01 unchanged) | integration | `cargo test --test metro_single_instance` | ✅ (must pass unchanged per D-22) |
| TASK-02 | Process-group kill preserved (COVER-02 unchanged) | integration | `cargo test --test process_group_kill` | ✅ (must pass unchanged per D-22) |
| TASK-03 | `task_for_worktree(state, id)` returns the running TaskRecord | unit (inline) | `cargo test --lib task_for_worktree` | ❌ W0 — new helper test |
| TASK-03 | `CommandOutputLine` routes to correct slice regardless of `active_worktree_id` | unit (inline in `dispatch_tests.rs`) | `cargo test --lib output_line_routing` | ❌ W0 — new test (D-21) |
| TASK-03 | `CommandExited` drains slice-local queue, not other slice's queue | unit (inline in `dispatch_tests.rs`) | `cargo test --lib exit_drains_slice_local` | ❌ W0 — new test (D-21) |
| Cross-cut | TaskId monotonicity + injection helper | unit (inline in `task.rs`) | `cargo test --lib task::tests` | ❌ W0 — new file |
| Cross-cut | `merge_slices` keep-running-task semantics | unit (inline) | `cargo test --lib merge_slices` | ❌ W0 — new helper |
| Cross-cut | Late `CommandOutputLine` for cancelled task is silently dropped | unit (inline) | `cargo test --lib stale_output_drop` | ❌ W0 — new test (P-3 mitigation) |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

These artifacts must be created by the first wave that needs them (per D-23 migration sequencing the Wave-0 equivalents land in plan #1):

- [ ] `src/domain/worktree_slice.rs` — module + inline `#[cfg(test)] mod tests`
- [ ] `src/domain/task.rs` — module + inline tests for `TaskId::next_for_test`, `ExitStatus` mapping
- [ ] `src/domain/ports/task_handle.rs` — `trait TaskHandle: Send + Sync + Debug` (no inline test — pure trait def)
- [ ] `src/domain/ports/mod.rs` — `+pub mod task_handle;`
- [ ] `src/infra/task_handle.rs` (or extension to `infra/command_runner.rs`) — `impl TaskHandle for TokioTaskHandle`
- [ ] `Makefile` arch-lint target — `+G-21` echo line + grep (lands in the SAME plan that deletes the 4 global fields per CONTEXT.md §Specifics line 248-249)
- [ ] New parallelism + routing tests in `src/app/dispatch_tests.rs` (5+ per D-21):
  - parallel yarn-on-A + jest-on-B
  - metro-conflict on A while metro running on B
  - CommandOutputLine routing by task_id (not active_worktree_id)
  - CommandExited slice-local drain
  - stale-task line drop (P-3 race)

**Coverage threshold:** Per-file ratchet from Phase 12's BASELINE-COVERAGE.json — `floor(baseline, 5)` applies. New files start at whatever the wave-end coverage is; the threshold becomes `floor(initial, 5)` going forward. No regression on existing files.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| End-to-end parallel UX (yarn install on worktree A while jest runs on worktree B, both visible in TUI) | TASK-02 | Requires real subprocesses + interactive TUI; Phase 16 will cover live indicators (UI-01..03) | `cargo run`, open two worktrees, trigger `Y` on one + `J` on another, confirm both worktree rows show running state |

*All other phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 5s
- [ ] `nyquist_compliant: true` set in frontmatter (after planner verifies the per-task verify map matches PLAN.md task IDs)

**Approval:** pending
