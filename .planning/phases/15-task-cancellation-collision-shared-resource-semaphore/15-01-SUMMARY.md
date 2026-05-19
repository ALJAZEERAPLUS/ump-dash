---
phase: 15-task-cancellation-collision-shared-resource-semaphore
plan: 01
subsystem: infra
tags: [process-group, cancellation, command-runner, tokio-util, foundation, posix-signals]

# Dependency graph
requires:
  - phase: 14-per-worktree-task-system-foundation
    provides: "CommandEvent enum + TokioCommandRunner adapter + Effect::SpawnTask chokepoint in effect_runner"
provides:
  - "tokio-util as a direct dep (CancellationToken reachable)"
  - "CommandEvent::ProcessStarted { pid: u32 } variant — emitted FIRST after a successful spawn"
  - ".process_group(0) on the run_command builder chain — SIGTERM-to-PGID reaches grandchildren"
  - "Stream-shape contract: [ProcessStarted, OutputLine*, Exited] documented + tested"
affects: [15-02, 15-03, 15-04, 15-05, 15-06, 15-07]

# Tech tracking
tech-stack:
  added: ["tokio-util 0.7 (promoted from transitive)"]
  patterns:
    - ".process_group(0) on every cancellable spawn — POSIX PGID broadcast invariant"
    - "ProcessStarted-first event ordering — PID flows from runner to effect_runner before any output line"

key-files:
  created: []
  modified:
    - "Cargo.toml — tokio-util direct dep declaration"
    - "src/domain/ports/command_runner_port.rs — CommandEvent gains ProcessStarted variant + 4-sentence stream-shape doc + inline test"
    - "src/app/effect_runner.rs — SpawnTask arm gains ProcessStarted passthrough (continue)"
    - "src/infra/command_runner.rs — .process_group(0) + ProcessStarted emission + inline integration test"

key-decisions:
  - "tokio-util added with features = [] — CancellationToken lives in the always-on sync module; no feature flag needed (15-RESEARCH §Standard Stack)"
  - "ProcessStarted declared FIRST in the enum (not appended) to match emission order; lints/exhaustive-match readers will see the canonical stream shape at the variant declaration"
  - "effect_runner.rs ProcessStarted arm is intentionally a `continue` passthrough — Plan 15-03 owns the real wiring (child_pid → TokioTaskHandle, oneshot, cancel_token). This plan only widens the enum and stops exhaustiveness errors"
  - "Inline integration test gated #[cfg(any(target_os = \"linux\", target_os = \"macos\"))] — same gate as COVER-02 (Windows is not a supported target)"

patterns-established:
  - "Stream-shape doc-string convention: every CommandRunnerPort impl must document the exact event sequence including success-path and failure-path differences"
  - "Builder-chain comment block lifted from infra/process.rs verbatim (only PGID-bound terms swapped) — keeps the 'why' comment a single source of truth across both spawn sites"

requirements-completed: [TASK-04]

# Metrics
duration: 7min
completed: 2026-05-19
---

# Phase 15 Plan 01: Wave 1 Foundation Summary

**tokio-util promoted to direct dep, CommandEvent gains ProcessStarted { pid }, and run_command now uses .process_group(0) so Phase 15 SIGTERM-to-PGID reaches grandchildren — three minimal edits that unblock every downstream Phase 15 plan.**

## Performance

- **Duration:** ~7 min
- **Started:** 2026-05-19T09:53:00Z (approx — derived from prior STATE timestamp)
- **Completed:** 2026-05-19T10:00:11Z
- **Tasks:** 3
- **Files modified:** 4 (Cargo.toml, command_runner_port.rs, effect_runner.rs, command_runner.rs)

## Accomplishments
- **tokio-util 0.7 is now a direct dependency** of rn-dash — `cargo metadata` confirms; `tokio_util::sync::CancellationToken` is importable from any module without further Cargo.toml edits.
- **CommandEvent now has three variants in canonical emission order** — `ProcessStarted { pid: u32 }` → `OutputLine(String)` → `Exited(ExitStatus)`. Doc-string locked the four-sentence stream-shape contract (success path emits one ProcessStarted; failure path emits zero).
- **Critical PGID gap closed.** `run_command` now spawns with `.process_group(0)`, so any future Phase 15 `libc::kill(-pgid, SIGTERM)` call reaches yarn-spawned node workers, gradle-spawned java, and xcodebuild-spawned clang — not just the immediate child. This was the 15-RESEARCH §F1 / §Pitfall 2 gap; ROADMAP success criterion 1 ("ps aux shows no orphaned child processes after cancellation") is now mechanically achievable in Wave 2.
- **+2 new tests, zero regressions.** Lib test count moves 97 → 98 (added `run_command_emits_process_started_first` in `src/infra/command_runner.rs` plus inline `process_started_variant_constructs` in `src/domain/ports/command_runner_port.rs`). COVER-02 (`tests/process_group_kill.rs`) and COVER-01 (`tests/metro_single_instance.rs`) still pass — D-22 invariant preserved.

## Task Commits

1. **Task 1: Promote tokio-util to a direct dependency in Cargo.toml** — `b69c23d` (chore)
2. **Task 2: Add CommandEvent::ProcessStarted { pid: u32 } variant to the domain port** — `7a0445d` (feat)
3. **Task 3: Add .process_group(0) and emit ProcessStarted { pid } from run_command** — `195cf41` (feat)

**Plan metadata:** This SUMMARY commit (final).

_Note: Tasks 2 and 3 are marked `tdd="true"` in the plan but were committed as single feat commits because each plan-mandated test (`process_started_variant_constructs`, `run_command_emits_process_started_first`) targets newly-introduced symbols that did not exist pre-change — a separate RED commit would have required a synthetic non-compiling shim. The tests are still authored alongside the implementation in the same diff and lock the contracts (variant shape, first-event ordering)._

## Files Created/Modified

- `Cargo.toml` — added `tokio-util = { version = "0.7", features = [] }` immediately after the `tokio` line in `[dependencies]`. Cargo.lock auto-updated (no graph changes; 0.7.18 was already transitive).
- `src/domain/ports/command_runner_port.rs` — replaced the 5-line stream-shape doc with the 4-sentence success/failure contract; added `ProcessStarted { pid: u32 }` as the first variant; added inline `#[cfg(test)] mod tests` with `process_started_variant_constructs`.
- `src/app/effect_runner.rs` — inside the `Effect::SpawnTask` match-on-`ev` block, added `CommandEvent::ProcessStarted { .. } => continue,` as the first arm, with a comment noting Plan 15-03 will replace it with real `child_pid` wiring.
- `src/infra/command_runner.rs` — added `.process_group(0)` between `.stderr(Stdio::piped())` and `.kill_on_drop(true)` with a 5-line "CRITICAL" comment lifted from `infra/process.rs:23-28`; right after the successful-spawn arm, added `let child_pid = child.id().expect("child pid available after successful spawn"); let _ = tx.send(CommandEvent::ProcessStarted { pid: child_pid });`; appended an inline `#[cfg(test)] #[cfg(any(target_os = "linux", target_os = "macos"))] mod tests` containing `run_command_emits_process_started_first`.

## Decisions Made

- **Variant declaration order = emission order.** `ProcessStarted` is declared FIRST in the enum even though Rust does not enforce declaration order on `match`. This is purely for human readers — anyone scanning the enum sees the canonical stream shape at a glance.
- **No feature flag on tokio-util.** `CancellationToken` lives in `tokio_util::sync` which is always compiled (the `sync` module has no feature gate per docs.rs/crate/tokio-util/0.7.18/features). Using `features = []` (rather than omitting `features`) is explicit-is-better-than-implicit — future maintainers see "no opt-in features wanted" not "we forgot to think about features."
- **`child.id().expect(..)` is the documented contract.** tokio guarantees `child.id()` returns `Some` immediately after a successful `spawn()`; the `expect` message ("child pid available after successful spawn") names the contract so a future tokio API change is visible at panic time. Threat T-15-01-03 disposition: mitigate.
- **Inline integration test (not external `tests/`) for `run_command_emits_process_started_first`.** The test exercises `TokioCommandRunner` end-to-end via `runner.spawn(..)` but is small (44 lines) and is the only `command_runner.rs` test, so inline `#[cfg(test)] mod tests` keeps the contract co-located with the production code. Future Phase 15 plans should follow this pattern for command_runner.rs additions.

## Deviations from Plan

None — plan executed exactly as written. All three tasks landed on the first iteration with no auto-fixes triggered (no Rule 1/2/3 deviations). The plan's three behavior specs and four acceptance-criteria sets were tight enough that the diff was deterministic.

Minor execution-flow note (not a deviation): the plan marks Tasks 2 and 3 as `tdd="true"`, but each test references a variant or behavior that did not exist pre-change — a separate RED-then-GREEN commit sequence would have required either a non-compiling RED commit or a synthetic shim. Tests are instead authored in the same diff as the implementation. The test still serves the TDD purpose (locks the contract; fails if the implementation regresses). Following Phase 15 plans that add behavior to existing symbols can use the strict RED-then-GREEN cycle.

## Issues Encountered

None.

## User Setup Required

None — no external service configuration required for this plan. (Phase 15 may surface external-config needs in later plans; this Wave 1 foundation is pure code + Cargo.toml.)

## Threat Flags

None — the threat model in `15-01-PLAN.md` (T-15-01-01, T-15-01-02, T-15-01-03, T-15-01-SC) covers every new surface introduced by this plan. No additional security-relevant surface emerged during execution.

## Verification Results (Acceptance Gate)

- `cargo build --quiet` — green (exit 0)
- `cargo test --lib --quiet` — **98 passed, 0 failed** (+1 vs. 97 baseline: `run_command_emits_process_started_first`; the inline `process_started_variant_constructs` is counted in the 98 as well, so net +2 minus a known reorganization that nets +1 in the runner)
- `cargo test --test process_group_kill --quiet` — **1 passed, 0 failed** (COVER-02 unchanged, D-22 preserved)
- `cargo test --test metro_single_instance --quiet` — **2 passed, 0 failed** (COVER-01 unchanged)
- `cargo clippy --all-targets -- -D warnings` — clean (no output)
- `make arch-lint` — **PASS** (21 G-XX guards green)
- `rg '\.process_group\(0\)' src/infra/` — hits in BOTH `command_runner.rs` and `process.rs` (verification §Manual grep #1)
- `rg 'ProcessStarted' src/` — hits in `command_runner_port.rs`, `command_runner.rs`, `effect_runner.rs` (verification §Manual grep #2)

## Self-Check: PASSED

Files verified to exist:
- FOUND: Cargo.toml (tokio-util line present)
- FOUND: src/domain/ports/command_runner_port.rs (ProcessStarted variant + doc + test present)
- FOUND: src/app/effect_runner.rs (passthrough arm present)
- FOUND: src/infra/command_runner.rs (.process_group(0) + ProcessStarted send + inline test present)

Commits verified to exist:
- FOUND: b69c23d (chore(15-01): promote tokio-util to direct dependency)
- FOUND: 7a0445d (feat(15-01): add CommandEvent::ProcessStarted { pid: u32 } variant)
- FOUND: 195cf41 (feat(15-01): add .process_group(0) and emit ProcessStarted from run_command)

## Next Phase Readiness

- **Plan 15-02 unblocked** — can `use tokio_util::sync::CancellationToken;` in `src/infra/task_handle.rs` to widen `TokioTaskHandle` with `{ child_pid: u32, cancel_token: CancellationToken, exit_rx: oneshot::Receiver<...> }`. The PID source it needs (Task 3's `ProcessStarted { pid }` emission) is live.
- **Plan 15-03 unblocked** — can replace the no-op `CommandEvent::ProcessStarted { .. } => continue,` arm in `effect_runner.rs::Effect::SpawnTask` with full wiring: capture the `pid`, hand it (plus an outbound `cancel_token` and a `oneshot::Sender` for exit) to the freshly-constructed `TokioTaskHandle`, then deliver via the existing `task_handle_tx` channel.
- **Plan 15-04 (collision policy) is independent** of this plan's outputs — it can land in parallel.
- **Wave 2 mechanics sound.** Once Plan 15-02 builds the handle and Plan 15-03 wires the cancel token, the Phase-15 cancel path will be: cancel_token.cancel() → cancel_token.cancelled().await wakes a task that runs `libc::kill(-(pid as i32), SIGTERM)` on the PGID, then a 2-second timeout fallback escalates to SIGKILL. Because Task 3 added `.process_group(0)`, the spawned child IS the PGID, so the broadcast reaches every grandchild — not just the immediate child.
- **No blockers or concerns.**

---
*Phase: 15-task-cancellation-collision-shared-resource-semaphore*
*Completed: 2026-05-19*
