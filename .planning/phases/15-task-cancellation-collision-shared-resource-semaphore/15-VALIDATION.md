---
phase: 15
slug: task-cancellation-collision-shared-resource-semaphore
status: approved
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-18
approved: 2026-05-19
---

# Phase 15 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Derived from `15-RESEARCH.md` §Validation Architecture and the per-task
> `<verify><automated>` blocks in Plans 15-01..15-06.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (built-in) + `#[tokio::test]` via tokio `"macros"` |
| **Config file** | `Cargo.toml` `[dev-dependencies]`; no separate config |
| **Quick run command** | `cargo test --lib --quiet` |
| **Full suite command** | `cargo test --workspace` |
| **Shape guard command** | `make arch-lint` |
| **Estimated runtime** | ~20 seconds (lib: ~8s; integration: ~10s additional) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib --quiet` (~8s; well under the 15s Nyquist budget)
- **After every plan wave:** Run `cargo test --workspace && make arch-lint`
- **Before `/gsd:verify-work`:** Full suite green + `cargo clippy --all-targets -- -D warnings` clean + `make arch-lint` green
- **Max feedback latency:** ~20 seconds wall-clock

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 15-01-01 | 01 | 1 | TASK-04 | T-15-01-SC | tokio-util pinned as direct dep (supply-chain audit surface) | build + metadata | `cargo metadata --format-version 1 \| jq '.packages[] \| select(.name=="tokio-util")' && cargo build --quiet` | ✅ | ⬜ pending |
| 15-01-02 | 01 | 1 | TASK-04 | T-15-01-01 | `CommandEvent::ProcessStarted{task_id,pid}` variant constructs correctly | unit | `cargo test --lib command_runner_port::tests::process_started_variant_constructs` | ✅ | ⬜ pending |
| 15-01-03 | 01 | 1 | TASK-04 | T-15-01-01 | `.process_group(0)` set + `ProcessStarted` emitted before stdout/stderr | unit + integration | `cargo test --lib command_runner::tests::run_command_emits_process_started_first && cargo test --test process_group_kill` | ✅ | ⬜ pending |
| 15-02-01 | 02 | 2 | TASK-04 | T-15-02-01 | `TokioTaskHandle` constructs with 3 fields + dispatches through trait object | unit | `cargo test --lib task_handle::tests::construct_with_all_three_fields task_handle::tests::boxed_task_handle_dispatches_through_trait_object` | ✅ | ⬜ pending |
| 15-02-02 | 02 | 2 | TASK-04 | T-15-02-02 | SIGTERM → 200ms → SIGKILL escalation is no-op on placeholder/dead PIDs | unit | `cargo test --lib task_handle::tests::abort_with_placeholder_pid_zero_is_noop task_handle::tests::abort_with_placeholder_pid_one_is_noop task_handle::tests::abort_with_dead_pid_does_not_panic` | ✅ | ⬜ pending |
| 15-02-03 | 02 | 2 | TASK-04 | T-15-02-03 | Signal-aware `From<ExitStatus>` maps SIGKILL → Killed, SIGTERM → Cancelled | unit | `cargo test --lib task_handle::tests::from_sigkill_status_maps_to_killed task_handle::tests::from_sigterm_status_maps_to_cancelled` | ✅ | ⬜ pending |
| 15-04-01 | 04 | 2 | TASK-05 | T-15-04-01 | `CollisionPolicy` enum + `collision_policy()` covers every Command variant | unit | `cargo test --lib command::tests::collision_policy_idempotent_installs_block_new command::tests::collision_policy_builds_tests_runs_cancel_previous command::tests::collision_policy_git_variants_all_block_new command::tests::collision_policy_covers_every_variant` | ✅ | ⬜ pending |
| 15-03-01 | 03 | 3 | TASK-06 | T-15-03-02 | `Effect::SpawnTask` carries `repo_root` for semaphore keying (≥15 variants) | unit | `cargo test --lib effect::tests::spawn_task_carries_repo_root_for_semaphore_key effect::tests::effect_variants_compile effect::tests::effect_has_at_least_fifteen_variants` | ✅ | ⬜ pending |
| 15-03-02 | 03 | 3 | TASK-04 + TASK-06 | T-15-03-01, T-15-03-03 | `SpawnTask` arm extension compiles + lib suite stays green | build + unit | `cargo build --quiet && cargo test --lib --quiet` | ✅ | ⬜ pending |
| 15-05-01 | 05 | 4 | TASK-04 | T-15-05-01 | `CommandCancel` `is_cancellable()` guard prevents cancelling non-cancellable tasks | build + unit | `cargo build --quiet && cargo test --lib --quiet` | ✅ | ⬜ pending |
| 15-05-02 | 05 | 4 | TASK-05 | T-15-05-03 | `dispatch_command` collision gate enforces `CollisionPolicy` decisions | build + unit | `cargo build --quiet && cargo test --lib --quiet` | ✅ | ⬜ pending |
| 15-05-03 | 05 | 4 | TASK-04 + TASK-05 | T-15-05-01, T-15-05-03 | 6 inline `dispatch_tests::collision::*` + `dispatch_tests::cancellation_guard::*` assertions | unit | `cargo test --lib dispatch_tests::collision dispatch_tests::cancellation_guard` | ✅ | ⬜ pending |
| 15-06-01 | 06 | 5 | TASK-04 | T-15-06-01, T-15-06-02 | Process-group cancel propagates SIGTERM→SIGKILL to subprocess tree (integration probe) | integration | `cargo test --test process_group_cancel --quiet` | ✅ | ⬜ pending |
| 15-06-02 | 06 | 5 | TASK-06 | T-15-06-01 | Yarn semaphore serializes back-to-back yarn-install dispatches per repo_root | integration | `cargo test --test yarn_semaphore_serializes --quiet` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

All Wave 0 dependencies satisfied by the time Plan 15-07 runs (Plan 15-06 in the same wave produces both integration test files; earlier waves produced all inline tests):

- [x] `tests/process_group_cancel.rs` — produced by Plan 15-06 Task 1 (Wave 5)
- [x] `tests/yarn_semaphore_serializes.rs` — produced by Plan 15-06 Task 2 (Wave 5)
- [x] Inline `dispatch_tests::collision::collision_block_new_*` — Plan 15-05 Task 3 (Wave 4)
- [x] Inline `dispatch_tests::collision::collision_cancel_previous_*` — Plan 15-05 Task 3 (Wave 4)
- [x] Inline `dispatch_tests::cancellation_guard::is_cancellable_gate_*` — Plan 15-05 Task 3 (Wave 4)
- [x] Inline `command::tests::collision_policy_*` (4 tests) — Plan 15-04 Task 1 (Wave 2)

15-RESEARCH §Wave 0 Gaps enumerated exactly these six items; all are closed by the time `/gsd:verify-work` runs.

---

## Manual-Only Verifications

All phase behaviors have automated verification.

(Phase 15 produces zero new UI behaviors; spinner / elapsed indicators are owned by Phase 16. All cancellation, collision, and serialization semantics are observable via test assertions on state mutations + subprocess probe assertions — no manual UAT items are required for Phase 15.)

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies — every row has a command, none missing
- [x] Sampling continuity: no 3 consecutive tasks without automated verify — confirmed by scanning the Per-Task Verification Map (all 14 rows have commands)
- [x] Wave 0 covers all MISSING references — Plan 15-06 produces both integration test files; earlier waves produced all inline tests
- [x] No watch-mode flags — `--quiet`, `--workspace`, `--lib` only; no `--watch`
- [x] Feedback latency < 15s for quick run, < 25s for full suite — quick `cargo test --lib --quiet` ~8s; full `cargo test --workspace` ~20s
- [x] `nyquist_compliant: true` set in frontmatter — confirmed

**Approval:** approved 2026-05-19
