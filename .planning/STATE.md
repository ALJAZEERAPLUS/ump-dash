---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: Per-Worktree Tasks + Architecture Audit
status: in-progress
stopped_at: Phase 14 EXECUTED + VERIFIED (3/4 PASS, 1 manual-only) — all 9 plans across 8 waves landed. Wave 1 (14-01) domain types: WorktreeSlice + TaskId/TaskRecord/ExitStatus + TaskHandle 9th port. Wave 2 (14-02) TokioTaskHandle adapter. Wave 3 (14-03 + 14-04 parallel) AppState.worktrees root field + merge_slices + Effect::SpawnTask{task_id,worktree_id,spec,cwd,branch}. Wave 4 (14-05) widened Action::CommandOutputLine{task_id,line} + CommandExited{task_id,status}. Wave 5 (14-06) SpawnTask runner arm + dedicated task_handle_tx channel (Q2 lock) + dispatch_command flip. Wave 6 (14-07) all 10 Recipe::expand sites + slice-local CommandExited drain + per-slice post_drain + slice CommandCancel + MetroActivityUpdate(Ready) slice walk. Wave 7 (14-08) 17 dispatch_tests rewritten to read state.worktrees + 5 new parallelism/routing/stale-drop tests. Wave 8 (14-09) atomic delete-and-guard — CommandRunnerState struct + 4 globals + Effect::SpawnCommand deleted, G-21 grep guard active, helpers flipped to slice. 99 tests pass; all 21 G-XX arch guards green; clippy clean; TASK-01/TASK-02/TASK-03 closed. 14-HUMAN-UAT.md persists 1 manual TUI test (concurrent output panel display) per VALIDATION.md §Manual-Only Verifications. Phase 15 (Task Cancellation + Collision + Semaphore) unblocked.
last_updated: "2026-04-28T06:24:56Z"
last_activity: 2026-04-28
progress:
  total_phases: 6
  completed_phases: 4
  total_plans: 31
  completed_plans: 31
  percent: 67
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-13 after v1.1 milestone completion)

**Core value:** One place to see and control everything about your React Native worktrees — which one is running, what branch each is on, and execute any command without context-switching.
**Current focus:** Phase 14 COMPLETE 2026-04-28 (9/9 plans, 3/4 verified PASS, 1 manual-only TUI check pending in HUMAN-UAT). Phase 15 (Task Cancellation + Collision + Semaphore) unblocked.

## Current Position

Phase: 14 (per-worktree-task-system-foundation) — COMPLETE 2026-04-28 (9/9 plans executed, verified 3/4 PASS + 1 manual-only)
Resume file: None
Status: All 8 waves landed cleanly. Domain types (WorktreeSlice + TaskId/TaskRecord/ExitStatus + TaskHandle 9th port) live in src/domain/. Infra adapter TokioTaskHandle wraps tokio::JoinHandle and impls TaskHandle. AppState gained `worktrees: HashMap<WorktreeId, WorktreeSlice>` root field with task_for_worktree + merge_slices helpers; Effect::SpawnTask{task_id,worktree_id,spec,cwd,branch} replaced SpawnCommand at the chokepoint; Action::CommandOutputLine and CommandExited carry task_id; runtime owns the dedicated task_handle_tx channel (Q2 lock — Box<dyn TaskHandle> not Clone+PartialEq). All 11 Recipe::expand sites push to slice.queue; CommandExited drains slice-locally; CommandCancel calls handle.abort() via port; MetroActivityUpdate(Ready) walks slices. 17 dispatch tests rewritten + 5 new parallelism/routing/stale-drop tests. Atomic delete in 14-09: CommandRunnerState struct + 4 globals + Effect::SpawnCommand all gone; helpers flipped to slice; G-21 grep guard prevents regression. 99 tests pass (96 lib + 2 COVER-01 + 1 COVER-02); all 21 G-XX arch guards green; clippy clean. TASK-01/TASK-02/TASK-03 closed.

Next phase: Phase 15 (Task Cancellation + Collision + Semaphore) — `/gsd-discuss-phase 15` or `/gsd-plan-phase 15`.

Progress: [######    ] 67% (v1.3 — Phase 11 7/7; Phase 12 5/5; Phase 13 10/10; Phase 14 9/9)

## Performance Metrics

**Velocity:**

- Total plans completed: 0 (v1.3)
- Average duration: — min
- Total execution time: — hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

**Recent Trend:**

- Last 5 plans: —
- Trend: —

*Updated after each plan completion*
| Phase 11 P00 | 4 | 2 tasks | 2 files |
| Phase 11 P01 | 15min | 2 tasks | 1 files |
| Phase 11-architecture-audit P03 | 20min | 2 tasks tasks | 1 files files |
| Phase 11-architecture-audit P04 | 15min | 2 tasks tasks | 1 file files |
| Phase 11-architecture-audit P05 | 25min | 2 tasks | 1 files |
| Phase 11 P06 | 1 min | 1 tasks | 2 files |
| Phase 12-coverage-gate P00 | 4min | 2 tasks | 7 files |
| Phase 12-coverage-gate P02 | 8min | 1 task | 1 file |
| Phase 12-coverage-gate P01 | 5min | 2 tasks | 2 files |
| Phase 12-coverage-gate P03 | 3min | 1 task | 2 files |
| Phase 12-coverage-gate P04 | 3min 9s | 4 tasks (2 auto, 2 checkpoints) | 3 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Roadmap v1.3]: Phase 11 (ARCH) runs before Phase 12 (COVER) — audit is read-only and findings inform which paths are highest-risk
- [Roadmap v1.3]: COVER is a hard gate — no refactor or task-system phase ships until COVER-01..COVER-04 are green
- [Roadmap v1.3]: `throbber-widgets-tui` excluded (MSRV bump to 1.88, Out of Scope per REQUIREMENTS.md) — spinner uses inline `const SPINNER_FRAMES: [&str; 6]` with elapsed-time formula
- [Roadmap v1.3]: TEA purity: spinner frame computed from `started_at.elapsed().as_millis() / 150 % 6` in render path — no mutable tick counter in AppState (per Pitfall 10)
- [Roadmap v1.3]: `arch_test_core` excluded — AGPL-3.0 license incompatible with MIT project (per REQUIREMENTS.md Out of Scope)
- [Phase 11]: F-NNN ID ranges allocated per plan: 11-01 F-001..F-099, 11-02 F-100..F-199, 11-03 F-200..F-299, 11-04 F-300..F-399, 11-05 F-400..F-499; uniqueness enforced by validator
- [Phase 11]: Self-test mode skips coverage/catch-all/prereq/D-14 checks so empty Wave-0 skeleton passes; full mode only greens at end of phase
- [Phase 11]: [Phase 11]: metro.rs tokio-types compromise graded Major (F-004) — trait MetroHandle + move to infra/metro.rs adapter; not Critical because MetroManager itself is deep and refactor cost <1 day
- [Phase 11]: [Phase 11]: action.rs placement graded Major (F-002) — move src/action.rs to src/domain/action.rs; coordinate with Plan 11-02 so infra/command_runner.rs Action import dies rather than being rewritten
- [Phase 11]: [Phase 11]: domain/refresh.rs canonized as exemplary deep-module reference standard for rest of Phase 11 — 4-item interface, 17 inline tests, pure function
- [Phase 11-architecture-audit]: [Phase 11]: app.rs graded Shallow/God-object — 4 Criticals (F-200..F-203) covering god-object split, TEA impurity, hexagonal dependency inversion, and metro helper colocation — each with concrete D-04 target shape sketch
- [Phase 11-architecture-audit]: [Phase 11]: F-201 Effect enum concretely sketched with 15+ variants (SpawnCommand, StartMetro, MetroHttpPost, LoadDevices, ListWorktrees, SaveAndroidMode, ...) — Phase 13 can implement without re-deciding variant set
- [Phase 11-architecture-audit]: [Phase 11]: F-208 D-14 keybinding finding anchored here via handle_key; Plan 11-04 captures footer.rs + help_overlay.rs definition sites; Plan 11-05 finalizes the unified registry recommendation
- [Phase 11-architecture-audit]: [Phase 11]: ui layer scored clean (0 Criticals, 4 Majors) — F-300 panels.rs:71 UI->infra leak pairs with Plan 11-02 F-107 as symmetric two-line fix; F-301 mod.rs doc-claim contradiction kept separate so Phase 13 adds rg grep guard; F-302 footer.rs + F-303 help_overlay.rs finalize D-14 three-site evidence map
- [Phase 11-architecture-audit]: [Phase 11]: Shallow-by-duplication pattern established — a module with 1-item public interface is still Shallow if its implementation duplicates data owned elsewhere (footer.rs + help_overlay.rs vs handle_key). Hiding nothing original is shallow regardless of impl LOC
- [Phase 11-architecture-audit]: [Phase 11]: symmetric two-side fix pattern — when a misplaced helper sits across a layer boundary (extract_jira_key in infra, called from ui), file a finding on each side with cross-references (F-107 + F-300); Phase 13 resolves both in a single file move
- [Phase 11-architecture-audit]: [Phase 11]: Refactor Sequence uses 4 dependency-waved groups (A foundational / B infra adapters / C app rewiring / D UI rewiring) rather than a flat 1..N list — surfaces parallelism explicitly for Phase 13 planners
- [Phase 11-architecture-audit]: [Phase 11]: only 1 F-4NN finding introduced (F-400 unified D-14) — every catch-all RISK folds into F-205; every prerequisite site folds into F-204; every hexagonal port cites an existing per-module F-NNN. No-duplicate-finding pattern avoids inflating the Refactor Sequence
- [Phase 11-architecture-audit]: [Phase 11]: hexagonal cross-module table surfaces canonical port inventory — 8 domain ports needed (ProcessPort, MultiplexerPort, JiraPort, MetroPort, WorktreePort, DevicePort, PortProbePort, PersistencePort). Phase 13 uses this as the domain/ports/ module contents, not a re-derivation target
- [Phase 11]: D-11 path correction: extended to also fix NN-arch-audit template placeholder in REQUIREMENTS.md §ARCH-06 to satisfy the plan's artifacts.contains contract (not just the grep-based truth); full-mode 11-validate.sh now exits 0
- [Phase 12-00]: rn-dash converted from bin-only to bin+lib — `[lib] name = "rn_dash"` (underscore) + `[[bin]] name = "rn-dash"` (hyphen) coexist; src/lib.rs declares `pub mod {action,app,domain,event,infra,tui,ui};` so tests/*.rs can `use rn_dash::domain::metro::MetroManager`. Main.rs reduced to `use rn_dash::{app, tui};` (only modules actually referenced in main()).
- [Phase 12-00]: Added `impl Default for MetroManager` as a Rule-3 deviation — clippy's new_without_default was dormant while `mod domain` was private in the bin-only crate; firing once promoted to pub. Plan's `cargo clippy -D warnings` success criterion required the fix. Pre-existing latent lint, not caused by the conversion.
- [Phase 12-00]: Makefile recipe lines use literal TABs (verified via od); cov-baseline writes to `.planning/phases/12-coverage-gate/BASELINE-COVERAGE.json` per D-03; cov-check uses `jq '.data[0].files[] | .summary.lines.percent'` for the human-check gate per D-05.
- [Phase 12-00]: `tests/common/mod.rs` uses the Rust-book submodule pattern — Rust treats `tests/*.rs` as separate binaries but `tests/common/mod.rs` as a shared submodule, so the helper doesn't become its own test binary. `fake_metro_handle(pid, worktree)` builds a MetroHandle with dummy tokio channels for the register()/is_running()/take_handle() invariant tests 12-01 will write.
- [Phase 12-02]: COVER-02 fixture redesigned from plan-literal `trap "" SIGTERM; sleep 30 & wait` to `trap : TERM; sleep 30 & wait` — SIG_IGN (from `trap ""`) is inherited by forked children on POSIX, so the plan's fixture never reached sleep with PGID broadcast. A no-op handler (`trap :`) is reset to SIG_DFL in children, so sleep dies from the PGID broadcast as intended. This is MORE adversarial: bash actively catches SIGTERM rather than ignoring it. Test now passes in 0.11 s on macOS.
- [Phase 12-02]: infra/command_runner.rs gap (no `.process_group(0)` set) NOT fixed in this plan per 12-RESEARCH.md Pitfall 6 + A5 — flagged as Phase 13 REFACTOR concern / Phase 15 TASK-04 dependency. Test spawns tokio::process::Command directly rather than going through CommandRunner.
- [Phase 12-01]: COVER-01 locked at TWO layers per D-09: type-level (3 inline `#[cfg(test)]` tests in src/domain/metro.rs targeting MetroManager::register panic) + TEA-level (2 integration tests in tests/metro_single_instance.rs targeting update(_, Action::MetroStart, ..) double-dispatch guard). Both layers catch independent refactor failure modes — dropping the `assert!` in register OR dropping the `pending_restart = true; update(_, MetroStop, ..)` branch will each fail at least one test in < 1 s.
- [Phase 12-01]: `#[should_panic(expected = ...)]` uses substring prefix `"BUG: MetroManager::register() called with an existing handle"` (omits the em-dash suffix `— kill first`) so punctuation tweaks do not destabilize the characterization.
- [Phase 12-01]: TEA integration tests hold receivers in `_metro_rx` / `_handle_rx` bindings for the test body (not discarded with `_`) — Action::MetroStart spawns a follow-up tokio task that writes to metro_tx; dropping the receiver first causes `channel closed` panics (12-RESEARCH.md Pitfall 10).
- [Phase 12-01]: Status assertion accepts `Running { pid: 9999, .. } | Stopping` — the handler's recursive `update(_, MetroStop, ..)` transitions status synchronously, so either pre-stop OR Stopping is acceptable; the test's real target is "not a fresh second Running{pid: ≠9999}".
- [Phase 12-03]: COVER-03 placed under src/app/dispatch_tests.rs (new sub-module) per D-08 + Claude's Discretion: src/app.rs is 2425 lines, adding ~600 test lines inline would push past 3000 — well beyond the 2000-line Ousterhout threshold. Sub-module split is the research's explicit recommendation.
- [Phase 12-03]: Split 17 tests into 3 sub-modules (palette_resolution: 6 `#[test]`, modal_dismissal: 8 `#[tokio::test]`, command_queue: 3 `#[tokio::test]`). `#[tokio::test]` used wherever update() may transitively call tokio::spawn (MetroStart external-detect path, dispatch_command).
- [Phase 12-03]: Regression-guard via `key('z') → Some(Action::ModalCancel)` in every palette test catches the future-addition-drops-a-key class of bug. Re-declaring the palette table verbatim from src/app.rs:333-381 IS the characterization — Phase 13's F-208 keybinding-registry refactor or F-201 Effect-enum refactor will fail CI if behavior shifts.
- [Phase 12-03]: "Palette x" from phase description interpreted as CleanToggle modal confirm per Research A2. There are only 5 PaletteMode variants (a/i/y/g/w). The yarn_c_opens_clean_toggle_then_x_confirms test covers BOTH Yarn 'c' → Action::OpenCleanMenu entry AND CleanToggle 'x' → Action::CleanConfirm exit.
- [Phase 12-03]: CommandExited drain test seeds one worktree into state.worktrees so dispatch_command does not early-return at src/app.rs:497. Without the seed, the test would pop_front but never set running_command, silently masking the invariant. Similarly, sync_before_metro dismissal test sets state.skip_external_metro_check = true to route MetroStart through the synchronous channel-send path and avoid requiring cleanup of a spawned detect_external_metro task.
- [Phase 12-03]: [Rule 1 - Bug] Simplified base_state() helper from `let mut s = AppState::default(); s.focused_panel = FocusedPanel::WorktreeTable; s` to just `AppState::default()`. Clippy's -D clippy::field-reassign-with-default flagged the reassignment because FocusedPanel::WorktreeTable is already the #[default] variant (src/app.rs:13-18). Behavior unchanged; cargo clippy --all-targets -- -D warnings now clean.
- [Phase 12-04]: Post-wave-2 baseline locked at rustc 1.94.1 + cargo-llvm-cov 0.8.5. Workspace line coverage 12.84% (445/3465); function 20.82% (56/269); region 9.89% (589/5958). Per-file ratchet in COVERAGE-THRESHOLDS.md applies floor(baseline, 5) to every src/ file — 27 rows, all threshold ≤ baseline verified.
- [Phase 12-04]: 20 of 27 src/ files at 0% coverage (accepted by floor-to-5 policy) — ui/*, most of infra/*, tui.rs, event.rs, domain/worktree.rs, main.rs. Their 0% threshold is vacuously satisfied; adding unit tests is a Phase-13+ concern, not a COVER-04 blocker. Highest coverage: domain/refresh.rs (100%), domain/metro.rs (70%), infra/jira.rs (70%).
- [Phase 12-04]: cargo-llvm-cov writes absolute paths — Markdown tables strip the prefix via jq `sub("^/Users/cubicme/aljazeera/dashboard/"; "")` for repo-relative display. Phase 13+ contributors on other machines need to update this prefix or use a relative-path alternative.
- [Phase 12-04]: [Rule 3 - Blocking] `git add -f` required for new .planning/ files because .gitignore:5 is `.planning/`. Existing tracked files (all previous 12-NN-PLAN.md/SUMMARY.md) continue tracked; only new file addition needs the force flag. Convention, not a bug.
- [Phase 12-04]: Phase 12 COMPLETE — all four COVER-NN requirements (COVER-01 metro, COVER-02 process-group, COVER-03 dispatch, COVER-04 baseline) green. Phase 13 (audit-driven refactors) unblocked.

### Pending Todos

None.

### Blockers/Concerns

None.

## Session Continuity

Last session: 2026-04-23T19:02:42Z
Stopped at: Completed 12-04-PLAN.md (COVER-04 post-wave-2 baseline — 5cc75ae commits BASELINE-COVERAGE.json + BASELINE-COVERAGE.md + COVERAGE-THRESHOLDS.md; 27 src/ files, 12.84% line / 20.82% function / 9.89% region total; floor-to-5 ratchet locked; 46 lib tests + 3 integration tests pass under coverage; clippy all-targets -D warnings clean). Phase 12 COMPLETE — all 4 COVER-NN green. Phase 13 (audit-driven refactors) unblocked.
Resume file: None
