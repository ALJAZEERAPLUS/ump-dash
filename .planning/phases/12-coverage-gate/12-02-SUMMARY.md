---
phase: 12-coverage-gate
plan: 02
subsystem: testing
tags: [rust, tokio, libc, process-group, signals, integration-tests, characterization-test, pgid, sigterm]

# Dependency graph
requires:
  - phase: 12-coverage-gate
    provides: Plan 12-00 bin+lib scaffolding — src/lib.rs with pub mod domain/infra/app/ui; Cargo.toml [dev-dependencies] tokio with macros/rt-multi-thread/process/time features; anyhow dev-dep. Without these the integration test in tests/process_group_kill.rs could not compile.
provides:
  - tests/process_group_kill.rs — characterization test locking in that tokio::process::Command::process_group(0) + libc::kill(-pgid, SIGTERM) reaps a bash+sleep subprocess tree within 2 s on macOS and Linux
  - A live regression trip-wire for Phase 13 REFACTOR and Phase 15 TASK-04 — any future change that silently removes .process_group(0) or changes when setpgid fires will fail this test in <5 seconds with a load-bearing error message
  - A worked example of the `trap : TERM` no-op handler pattern for adversarial signal tests (vs the broken `trap "" SIG` SIG_IGN inheritance trap)
affects: [13-refactor, 15-task-system, 12-03-dispatch-tests, 12-04-baseline-coverage]

# Tech tracking
tech-stack:
  added: []  # All deps already present from 12-00 (tokio, libc, anyhow); no new deps
  patterns:
    - "Adversarial characterization test: fixture is designed so the test can only pass if the mechanism under test actually works — a PGID-targeted signal is the only thing that can reach the sleep child when bash actively handles SIGTERM with a no-op trap"
    - "cfg-gate at file level via `#![cfg(any(target_os = ...))]` (not `#[cfg_attr(..., ignore)]`) — test doesn't compile on Windows at all, avoiding tokio 1.41+ Windows regression (Pitfall 5)"
    - "POSIX signal-disposition inheritance rule documented inline: `trap \"\" SIG` sets SIG_IGN which survives fork+exec, but `trap : SIG` (or any handler) gets reset to SIG_DFL in children"
    - "libc::kill(-pgid, 0) existence-probe idiom for asserting a process group is empty (ESRCH) without side effects"

key-files:
  created:
    - "tests/process_group_kill.rs"
  modified: []

key-decisions:
  - "[Rule 1 - Bug] Changed fixture from `trap \"\" SIGTERM; sleep 30 & wait` (plan's literal) to `trap : TERM; sleep 30 & wait` because SIG_IGN is inherited by forked children on POSIX. The plan's fixture reached nobody with PGID SIGTERM on macOS — sleep inherited SIG_IGN from bash's `trap \"\"` and ignored the signal too, so the test timed out at 2 s. The `trap :` no-op handler avoids SIG_IGN inheritance (handlers are reset to SIG_DFL in forked children, per POSIX execve semantics), so sleep dies from the PGID broadcast as intended. The fixture is MORE adversarial, not less: bash actively catches SIGTERM (not just ignores it) and keeps running, so only the PGID broadcast can kill sleep."
  - "Kept every other load-bearing detail verbatim: `.process_group(0)`, `.kill_on_drop(true)`, `assert!(pgid > 1, ...)` (Pitfall 3 guard), `timeout(Duration::from_secs(2), child.wait())`, 500 ms ESRCH-probe poll loop, `libc::kill(-pgid, libc::SIGTERM)`, `libc::kill(-pgid, 0)`, `#[tokio::test(flavor = \"multi_thread\")]`, file-level cfg gate."
  - "infra/command_runner.rs gap NOT fixed: command_runner.rs does NOT set .process_group(0) — only infra/process.rs does (Pitfall 6 in 12-RESEARCH.md). This test therefore spawns tokio::process::Command directly in the test file rather than going through CommandRunner. The gap is flagged for Phase 13 REFACTOR / Phase 15 TASK-04 per A5 in 12-RESEARCH.md and NOT fixed here."

patterns-established:
  - "Characterization-test fixture design: when writing a fixture that must fail if a mechanism is removed, verify the fixture's adversarial property manually BEFORE committing the test — the plan's `trap \"\" SIGTERM` looked adversarial but was actually non-discriminating on macOS because SIG_IGN inheritance swallowed the sleep's signal too"
  - "Document the POSIX signal-disposition-inheritance rule inline in any test using `trap` — it is subtle and `man signal` / `man bash` don't warn about it directly"

requirements-completed: [COVER-02]

# Metrics
duration: 8min
completed: 2026-04-23
---

# Phase 12 Plan 02: Process-Group Kill Characterization Summary

**tests/process_group_kill.rs locks in `.process_group(0)` + `libc::kill(-pgid, SIGTERM)` tree-reaping behavior in 0.11 s on macOS; fixture redesigned to dodge the POSIX SIG_IGN-inheritance trap that broke the plan's original `trap "" SIGTERM` string.**

## Performance

- **Duration:** ~8 min (diagnose + redesign + verify + summarize)
- **Started:** 2026-04-23T18:10:00Z (approximate — this plan was resumed from a prior agent's partial work)
- **Completed:** 2026-04-23T18:18:00Z (approximate)
- **Tasks:** 1 (Task 02.1, inherited as a ~105-line draft; primarily fixed the fixture)
- **Files modified:** 1 created (tests/process_group_kill.rs)

## Accomplishments

- One passing `#[tokio::test(flavor = "multi_thread")]` function at `tests/process_group_kill.rs::killing_pgid_reaps_child_tree`, completing in **0.11 s** (vs. 5 s budget).
- The test's failure mode (if `.process_group(0)` is removed or setpgid semantics change) is a **loud, unambiguous** timeout message in < 2 s — exactly the trip-wire Phase 13/15 need.
- POSIX SIG_IGN-inheritance hazard documented inline in the module-level doc comment so a future reader re-writing the fixture will not repeat the mistake.
- `infra/command_runner.rs` missing `.process_group(0)` remains flagged (not fixed) — Phase 13 concern, per 12-RESEARCH.md Pitfall 6 + A5.

## Task Commits

1. **Task 02.1: COVER-02 pgid kill characterization test** — `adcc3e9` (test)

**Plan metadata commit:** [this SUMMARY commit — hash recorded after the commit lands]

## Files Created/Modified

- `tests/process_group_kill.rs` (125 lines) — characterization test + module-level doc explaining the SIG_IGN-inheritance deviation from the plan's literal fixture string

## Decisions Made

- **Fixture string changed from `trap "" SIGTERM; sleep 30 & wait` to `trap : TERM; sleep 30 & wait`** (see Deviations below). This is the one load-bearing deviation from the plan's must_have.truths statement.
- **Kept everything else verbatim per the plan.** Every other acceptance-criterion grep pattern still matches: the cfg gate, `#[tokio::test(flavor = "multi_thread")]`, the fn name, `.process_group(0)`, `.kill_on_drop(true)`, `libc::kill(-pgid, libc::SIGTERM)`, `libc::kill(-pgid, 0)`, `timeout(Duration::from_secs(2)...)`, `assert!(pgid > 1, ...)`.
- **Did NOT switch to SIGKILL or remove the trap entirely** — both would have been simpler but either would eliminate the adversarial-fixture property. A no-op `trap :` keeps the test discriminating (bash actively catches SIGTERM, so a naive kill-the-parent-only approach would fail) without the SIG_IGN inheritance bug.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixture redesigned: `trap "" SIGTERM` → `trap : TERM`**

- **Found during:** Task 02.1 — inherited test file failed with `COVER-02 FAILED: bash parent + sleep child did not exit within 2 s of SIGTERM to PGID ... — PGID-kill invariant is broken`.
- **Issue:** The plan's must_have.truths and acceptance_criteria specified the literal fixture string `trap "" SIGTERM; sleep 30 & wait`. This string is fundamentally broken on POSIX: `trap "" SIG` sets the signal disposition to **SIG_IGN**, which — unlike trap handlers — **is inherited by forked children** across `fork()` + `execve()` (POSIX.1-2017 §2.4.3, `execve(2)` rationale). When bash backgrounded `sleep 30`, sleep inherited SIG_IGN for SIGTERM. Delivering SIGTERM to the PGID therefore reached nobody — bash ignored it (trap) AND sleep ignored it (inherited SIG_IGN). The test timed out at 2 s.
- **Verified root cause via standalone Python reproducer:**
  ```
  python3 -c "import os,subprocess,signal; p=subprocess.Popen(['bash','-c','trap \"\" SIGTERM; sleep 10 & wait'], preexec_fn=os.setsid); ...; os.killpg(os.getpgid(p.pid), signal.SIGTERM); p.wait(timeout=5)"
  → TimeoutExpired after 5 s; group members still alive
  ```
  vs. with `trap : TERM; sleep 10 & wait`:
  ```
  → bash exited rc=143 in 0.00 s
  ```
- **Fix:** Changed the fixture to `trap : TERM; sleep 30 & wait`. `:` is bash's no-op builtin, so this installs an **explicit handler** (not SIG_IGN). Handlers are reset to SIG_DFL in children on `execve()`, so sleep receives SIGTERM with its default disposition and dies; bash runs its no-op handler, `wait` returns, bash exits. The test is MORE adversarial, not less: bash actively catches SIGTERM rather than just ignoring it, so only the PGID broadcast can kill sleep.
- **Files modified:** `tests/process_group_kill.rs` (fixture string + module-level doc comment explaining the deviation + inline comment on the spawn site).
- **Verification:**
  - `cargo test --test process_group_kill --quiet` → 1 passed in 0.11 s
  - `cargo test --quiet` (full suite) → 30/30 passed
  - `cargo clippy --all-targets -- -D warnings` → clean
  - Every other acceptance-criteria grep pattern still matches (cfg gate, `#[tokio::test(flavor = "multi_thread")]`, fn name, `.process_group(0)`, `.kill_on_drop(true)`, `libc::kill(-pgid, libc::SIGTERM)`, `libc::kill(-pgid, 0)`, `timeout(2s)`, `assert!(pgid > 1, ...)`).
- **Committed in:** `adcc3e9` (Task 02.1 commit, message explains the deviation)

### Plan truth-statement violation

The plan's `must_haves.truths[0]` says verbatim: *"Spawning `bash -c 'trap \"\" SIGTERM; sleep 30 & wait'` with `.process_group(0)` + `.kill_on_drop(true)` produces a process group ..."*. The delivered fixture uses `trap : TERM` instead. The load-bearing **behavior** asserted by truths[1-4] (PGID reap within 2 s; ESRCH after 500 ms; cross-platform Linux+macOS; libc::kill direct) is unchanged and verified. The string-literal deviation is necessary because the plan's literal fixture doesn't work on POSIX. **Recommended action for the verifier:** accept the deviation; update REQUIREMENTS/plan archive with a note that future plans invoking COVER-02 should reference this summary for the correct fixture.

### Acceptance-criteria grep deviation

The plan's acceptance_criteria contains `grep -q 'trap "" SIGTERM; sleep 30 & wait' tests/process_group_kill.rs` — this grep will **not match** in the delivered file. All other grep acceptance criteria match. The substantive criterion — *the test exists, is cfg-gated, is a `#[tokio::test(flavor = "multi_thread")]`, spawns with process_group(0) + kill_on_drop(true), calls libc::kill(-pgid, SIGTERM), probes with libc::kill(-pgid, 0), has a 2 s timeout, guards pgid > 1, passes on macOS in < 5 s* — are all satisfied.

---

**Total deviations:** 1 auto-fixed (Rule 1 — bug fix to plan-literal fixture)
**Impact on plan:** The functional / test-coverage outcome of the plan is achieved. The plan's literal fixture string was a research-level error (12-RESEARCH.md Pattern 3 specified it without verifying the SIG_IGN-inheritance behavior on macOS). Deviation is necessary for the plan's actual goal — a passing characterization test — to be met. No scope creep.

## Issues Encountered

- Partial work from a prior agent: `tests/process_group_kill.rs` existed as an uncommitted ~105-line draft using the plan's literal fixture. Diagnosis (reading the research, running the test, reproducing the failure with a minimal Python harness, verifying both `trap ""` fails and `trap :` works) took ~5 minutes. Fix (doc updates + single-line fixture change) took ~1 minute. Verification (test + full suite + clippy) took ~1 minute.

## User Setup Required

None.

## Next Phase Readiness

- **COVER-02 requirement done.** Phase 12 wave 2 has this plan + 12-01 (metro single-instance) + 12-03 (dispatch tests) running independently; none block each other.
- **For Phase 13 REFACTOR:** This test will fire regression alarms in < 5 s if the refactor silently removes `.process_group(0)` from `infra/process.rs` OR introduces a subprocess spawn path that should have `.process_group(0)` but doesn't.
- **For Phase 15 TASK-04:** Known gap — `infra/command_runner.rs` does not currently set `.process_group(0)`. TASK-04's per-task SIGTERM-to-PGID cancellation requires command_runner to set it. This gap is Phase 13's to close (REFACTOR of command_runner), not fixed here.
- **Flag for plan-archive scanner:** the plan's literal fixture string is incorrect on POSIX; future plans/verifiers reading the plan archive should consult this summary for the corrected fixture.

## Self-Check: PASSED

- File exists: `tests/process_group_kill.rs` → FOUND
- Commit exists: `adcc3e9` → FOUND (`git log --oneline -5 | grep adcc3e9`)
- Test passes on macOS in 0.11 s → FOUND
- Clippy clean on all targets → FOUND
- `cargo test --quiet` (full suite 30/30) → FOUND

---
*Phase: 12-coverage-gate*
*Completed: 2026-04-23*
