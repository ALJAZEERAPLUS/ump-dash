# Roadmap: RN Dash

## Milestones

- ✅ **v1.0 MVP** — Phases 01-06 (shipped 2026-04-05)
- ✅ **v1.1 Public Release** — Phases 07-10 (shipped 2026-04-13)
- 🚧 **v1.3 Per-Worktree Tasks + Architecture Audit** — Phases 11-16 (in progress)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 01-06) — SHIPPED 2026-04-05</summary>

- [x] Phase 01: Scaffold and TUI Shell (3/3 plans) — completed 2026-03-02
- [x] Phase 02: Metro Process Control (3/3 plans) — completed 2026-03-02
- [x] Phase 03: Worktree Browser, Git, and RN Commands (5/5 plans) — completed 2026-03-02
- [x] Phase 04: Config and JIRA Integration (3/3 plans) — completed 2026-03-02
- [x] Phase 05: Worktree Switching and Claude Code (2/2 plans) — completed 2026-03-03
- [x] Phase 05.1: Milestone Feedback — UX overhaul (8/8 plans)
- [x] Phase 05.2: Milestone Feedbacks — Bug fixes and polish (10/10 plans)
- [x] Phase 06: Final UX Polish (3/3 plans)

See: `.planning/milestones/v1.0-ROADMAP.md` for full details.

</details>

<details>
<summary>✅ v1.1 Public Release (Phases 07-10) — SHIPPED 2026-04-13</summary>

- [x] Phase 07: Labels Removal (1/1 plans) — completed 2026-04-05
- [x] Phase 08: Palette and Keybinding Rework (5/5 plans) — completed 2026-04-07
- [x] Phase 09: Generalization and GitHub Prep (2/2 plans) — completed 2026-04-05
- [x] Phase 10: CI and Release (1/1 plan) — completed 2026-04-05

See: `.planning/milestones/v1.1-ROADMAP.md` for full details.

</details>

### 🚧 v1.3 Per-Worktree Tasks + Architecture Audit (In Progress)

**Milestone Goal:** Audit and refactor architecture against Ousterhout/4-layer/hexagonal principles, then rework the task/command system to be per-worktree with parallel execution, individual cancellation, collision handling, and live UI indicators.

**Ordering constraint:** COVER (Phase 12) is a hard gate — no refactor or task-system phase ships until COVER-01..COVER-04 are green. ARCH (Phase 11) is read-only and runs before COVER. REFACTOR (Phase 13) requires both ARCH findings and the COVER gate.

- [x] **Phase 11: Architecture Audit** — Read-only audit of all modules; produces AUDIT.md with severity-tagged findings (completed 2026-04-17)
- [x] **Phase 12: Coverage Gate** — Characterization tests and baseline coverage report locking in critical invariants before any structural change (completed 2026-04-23)
- [x] **Phase 13: Audit-Driven Refactors** — Resolve all Critical and Major findings from the audit; type-driven cancellability; domain-level prerequisite ordering (completed 2026-04-25)
- [ ] **Phase 14: Per-Worktree Task System Foundation** — Replace global task state with per-worktree task map; parallel execution across worktrees; correct output routing
- [ ] **Phase 15: Task Cancellation + Collision + Semaphore** — Individual task cancellation with SIGTERM/SIGKILL; uniqueness-based collision policy; shared-resource semaphore for yarn installs
- [ ] **Phase 16: Live UI Indicators** — Split Y/P cells; 6-frame inline spinner replacing Y/P during active tasks; live MM:SS elapsed time computed in render path

## Phase Details

### Phase 11: Architecture Audit
**Goal**: Produce a complete, prioritized audit of the codebase against Ousterhout deep-module criteria, 4-layer model, hexagonal ports-and-adapters discipline, and explicit coverage of catch-all match arms and misplaced prerequisite logic
**Depends on**: Phase 10 (v1.1 shipped baseline)
**Requirements**: ARCH-01, ARCH-02, ARCH-03, ARCH-04, ARCH-05, ARCH-06
**Success Criteria** (what must be TRUE):
  1. AUDIT.md exists at `.planning/phases/11-architecture-audit/AUDIT.md` with every finding tagged Critical, Major, or Minor and includes file and line range
  2. Every module in `domain/`, `infra/`, `app/`, `ui/` has been scored for deep-module / narrow-interface criteria
  3. Every `_ => {}` catch-all in `handle_key`, `update`, and modal dispatch is enumerated with explicit gaps listed
  4. Every place where command prerequisite / ordering logic lives outside the domain layer is identified with location
  5. Findings are prioritized with a recommended action for each Critical and Major item consumed by Phase 13
**Plans**: TBD

### Phase 12: Coverage Gate
**Goal**: Lock in characterization tests for the metro single-instance invariant and process-group kill behavior, plus targeted dispatch-path coverage tests and a committed baseline coverage report, before any structural change lands
**Depends on**: Phase 11 (audit findings inform which paths are highest-risk; gate must be green before any Phase 13+ work touches those paths)
**Requirements**: COVER-01, COVER-02, COVER-03, COVER-04
**Success Criteria** (what must be TRUE):
  1. A test asserts that starting metro in worktree A while metro is already running in worktree B fails/resolves through the conflict flow — `MetroManager` holds exactly one live handle at any time
  2. A test asserts that killing a running command terminates the full subprocess tree (yarn, gradle, xcodebuild, etc.) with no orphaned PIDs surviving the kill
  3. Tests cover `CommandQueued` / `CommandExited` routing, modal dismissal flow, and palette-to-action resolution for all six palettes (a / i / x / y / g / w)
  4. A baseline coverage report is committed and a per-module minimum threshold is documented so subsequent phases can detect regressions
**Plans**: 5 plans
  - [x] 12-00-PLAN.md — Scaffolding: bin→bin+lib conversion, Makefile cov targets, .gitignore, tests/common/mod.rs
  - [x] 12-01-PLAN.md — COVER-01: metro single-instance characterization (type-level + update()-level, D-09)
  - [x] 12-02-PLAN.md — COVER-02: process-group kill characterization (adversarial bash fixture, PGID SIGTERM)
  - [x] 12-03-PLAN.md — COVER-03: command-dispatch tests (5 palettes + CleanToggle flow + 8 modals + queue routing)
  - [x] 12-04-PLAN.md — COVER-04: baseline coverage report + floor(baseline, 5) thresholds (completed 2026-04-23; workspace total 12.84% line / 20.82% function / 9.89% region)

### Phase 13: Audit-Driven Refactors
**Goal**: Resolve all Critical and Major findings from the audit with the coverage gate green as a safety net; add type-driven cancellability to `CommandSpec`; represent command prerequisites abstractly in the domain layer
**Depends on**: Phase 11 (findings), Phase 12 (coverage gate green — hard precondition before any modified code is touched)
**Requirements**: REFACTOR-01, REFACTOR-02, REFACTOR-03
**Success Criteria** (what must be TRUE):
  1. All Critical and Major findings from AUDIT.md are resolved or explicitly deferred to backlog with written rationale; no new Critical/Major regressions introduced
  2. `CommandSpec::is_cancellable()` exists and returns `false` for all git-porcelain variants and `true` for all others; cancellation surface is type-driven, not convention
  3. Command prerequisites and action ordering are represented in domain code (either as a `Prerequisite` field on `CommandSpec` or a domain-level `Pipeline` / `Recipe` type); the dispatcher reads ordering from domain, not from inline `update()` logic
  4. All existing tests pass (`cargo test`, `cargo clippy -D warnings`) after refactors complete
**Plans**: 10 plans
  - [x] 13-01-PLAN.md — action.rs→domain; 3 trait relocations (ProcessPort/JiraPort/MultiplexerPort); extract_jira_key→domain (F-002+F-103+F-106+F-107+F-110+F-300+F-301) — 2026-04-24
  - [x] 13-02-PLAN.md — CommandSpec::is_cancellable() flat-enum predicate + make arch-lint target (REFACTOR-02) — 2026-04-24
  - [x] 13-03-PLAN.md — Effect enum + Recipe/Prerequisite/DependencyState + MetroPort/MetroHandle trait (F-201 type + F-204 type + F-203 trait + F-004) — 2026-04-24
  - [x] 13-04-PLAN.md — PortProbePort + WorktreePort + DevicePort + adapter shells (F-102+F-104+F-105) — 2026-04-24
  - [x] 13-05-PLAN.md — CommandRunnerPort + CommandEvent; remove Action import from infra (F-101) — 2026-04-24
  - [x] 13-06-PLAN.md — F-200 structural split of src/app.rs into src/app/{mod,state,update,handle_key,runtime,effect_runner,adapters}.rs — 2026-04-24
  - [x] 13-07-PLAN.md — update() purity (F-201 consumer) + metro helpers→infra/metro.rs (F-203 consumer) + KEYBINDINGS registry + handle_key walker (F-208+F-400) — 2026-04-25
  - [x] 13-08-PLAN.md — Adapters injection; effect_runner full impl; app/ infra-free (F-202 consumer + F-101 consumer) — 2026-04-25
  - [x] 13-09-PLAN.md — Recipe consumer replaces 11 inline prereq sites; 3 flags deleted; exhaustive modal arms (F-204 consumer + F-205) — 2026-04-25
  - [x] 13-10-PLAN.md — AppState sub-struct regroup (F-209) + footer/help_overlay consume KEYBINDINGS (F-302+F-303) + Minor cleanup (F-108+F-112+verify F-003/F-005/F-006/F-100) — 2026-04-25

### Phase 14: Per-Worktree Task System Foundation
**Goal**: Replace the three global task fields (`running_command`, `command_task`, `command_queue`) in `AppState` with a per-worktree task map, add `TaskId` and `TaskRecord` domain types, update `Action` routing so output lines and exit events carry `WorktreeId`, and enable parallel command execution across worktrees
**Depends on**: Phase 12 (coverage gate green — hard precondition per ordering rule), Phase 13 (audit-driven refactors complete so the structural baseline is stable before adding the new task system)
**Requirements**: TASK-01, TASK-02, TASK-03
**Success Criteria** (what must be TRUE):
  1. Dispatching `yarn install` in worktree A while worktree B is running a test causes both to execute concurrently — each worktree's output appears in its own output panel with no cross-contamination
  2. The metro single-instance invariant is preserved — starting metro in any worktree while metro is already running goes through the existing conflict flow unchanged
  3. `CommandOutputLine` and `CommandExited` actions carry `WorktreeId`/`TaskId` and are routed to the correct worktree's output buffer regardless of which worktree is currently selected in the UI
  4. A running task's identity `(CommandKind, WorktreeId)` is accessible to UI, cancellation, and collision logic via `task_for_worktree(state, id)`
**Plans**: 9 plans across 8 waves

  **Wave 1** *(no dependencies — foundation layer)*
  - [ ] 14-01-PLAN.md — Domain types: WorktreeSlice + TaskId/TaskRecord/ExitStatus + TaskHandle port (9th port; pure domain)

  **Wave 2** *(blocked on Wave 1 completion)*
  - [ ] 14-02-PLAN.md — Infra adapter: TokioTaskHandle + From<std::process::ExitStatus>

  **Wave 3** *(blocked on Wave 2; 14-03 + 14-04 run in parallel — disjoint files)*
  - [ ] 14-03-PLAN.md — AppState root field worktrees + task_for_worktree + merge_slices + WorktreesLoaded integration (D-16/D-17, Q4)
  - [ ] 14-04-PLAN.md — Effect::SpawnTask variant added (Q1: cwd+branch in payload)

  **Wave 4** *(blocked on Wave 3 completion)*
  - [ ] 14-05-PLAN.md — Action payload widening: CommandOutputLine{task_id,line} + CommandExited{task_id,status} (Q2 lock: dedicated channel, no TaskSpawned action)

  **Wave 5** *(blocked on Wave 4 completion)*
  - [ ] 14-06-PLAN.md — SpawnTask runner arm + task_handle_tx channel + dispatch_command emits SpawnTask (D-10/Q1/Q2/Q3)

  **Wave 6** *(blocked on Wave 5 completion)*
  - [ ] 14-07-PLAN.md — Drain migration: 10 Recipe::expand sites + CommandExited slice-local + CommandCancel + MetroActivityUpdate Ready (D-11/D-12/D-13/D-14)

  **Wave 7** *(blocked on Wave 6 completion)*
  - [ ] 14-08-PLAN.md — Test rewrite: D-21 17 dispatch tests + 5 new parallelism / routing / stale-drop tests

  **Wave 8** *(blocked on Wave 7 — atomic delete-and-guard, MUST land in a single plan per CONTEXT.md §specifics)*
  - [ ] 14-09-PLAN.md — Atomic deletion: CommandRunnerState + 4 fields + SpawnCommand + helper flips + G-21 grep guard

**Cross-cutting constraints** *(must hold across all 9 plans)*:
  - All 79+ existing tests stay green throughout the migration (Plans 14-03..14-08 keep legacy globals alive in parallel; 14-09 is the only plan that deletes)
  - All 20 existing G-XX shape guards stay green; new G-21 lands ONLY in Plan 14-09 alongside the deletion of `running_command` / `command_task` / `command_queue` / `post_drain_action`
  - `tests/metro_single_instance.rs` (COVER-01) and `tests/process_group_kill.rs` (COVER-02) are READ-ONLY — no plan modifies them (per D-22)
  - Metro single-instance invariant preserved (D-13: `state.metro` stays at AppState root, `MetroManager::register()` panic intact)
  - TEA `update()` purity preserved (G-04, G-05): zero `tokio::spawn` / `reqwest` / `tokio::process` in `src/app/update.rs`

### Phase 15: Task Cancellation + Collision + Shared-Resource Semaphore
**Goal**: Enable individual task cancellation via `CancellationToken` + SIGTERM to the process group with SIGKILL grace fallback; define and enforce a documented collision policy per command category; prevent yarn global cache / `node_modules` corruption from concurrent installs via a per-repo-root semaphore
**Depends on**: Phase 14 (per-worktree task map and `TaskId` must exist before per-task cancellation and collision detection are possible)
**Requirements**: TASK-04, TASK-05, TASK-06
**Success Criteria** (what must be TRUE):
  1. Cancelling a running yarn, clean, pod-install, run-android, run-ios, or test task terminates the full process group (SIGTERM → 200ms grace → SIGKILL) within 2 seconds; `ps aux` shows no orphaned child processes after cancellation
  2. Git-porcelain commands (`GitResetHard`, `GitPull`, `GitPush`, `GitRebase`, `GitCheckout`, `GitFetch`) cannot be cancelled — the cancel action is a no-op for these variants, enforced by `is_cancellable()` from Phase 13
  3. Triggering a task whose `(CommandKind, WorktreeId)` matches one already running either blocks the new dispatch or cancels the previous one per the documented per-category collision policy (idempotent installs block-new; builds/tests cancel-previous)
  4. Concurrent yarn installs across worktrees sharing the same repo root are serialized via a `tokio::sync::Semaphore(1)` keyed by repo-root `PathBuf`; both installs complete with valid `node_modules` and non-corrupt `.yarn-integrity`
**Plans**: TBD

### Phase 16: Live UI Indicators
**Goal**: Split the merged `Y/P` cell into two independent cells; replace each with a 6-frame rotating yellow spinner while its respective task category is running; show live MM:SS elapsed time in the worktree row computed directly from `started_at.elapsed()` in the render path with no mutable tick state stored in `AppState`
**Depends on**: Phase 14 (per-worktree task state and `task_for_worktree()` helper must exist), Phase 15 (cancellation and task lifecycle must be stable so spinner correctly appears/disappears)
**Requirements**: UI-01, UI-02, UI-03
**Success Criteria** (what must be TRUE):
  1. The worktree table renders `Y` and `P` as two independent cells — each can independently show its letter or a spinner without affecting the other
  2. When a yarn-family task is running for a worktree, the `Y` cell shows a rotating 6-frame yellow spinner (inline `const SPINNER_FRAMES: [&str; 6]`, frame = `elapsed.as_millis() / 150 % 6`); same for `P` during pod-family tasks; run/test tasks show an equivalent animated indicator in the appropriate position; all indicators return to static letters when idle
  3. Each active worktree row shows a live MM:SS elapsed counter that updates on every 250ms tick, computed as `started_at.elapsed()` in the render function with no mutable frame-counter field in `AppState`
**Plans**: TBD
**UI hint**: yes

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 01-06 | v1.0 | 37/37 | Complete | 2026-04-05 |
| 07-10 | v1.1 | 9/9 | Complete | 2026-04-13 |
| 11. Architecture Audit | v1.3 | 7/7 | Complete   | 2026-04-17 |
| 12. Coverage Gate | v1.3 | 5/5 | Complete | 2026-04-23 |
| 13. Audit-Driven Refactors | v1.3 | 0/TBD | Not started | - |
| 14. Per-Worktree Task System Foundation | v1.3 | 0/9 | Planned | - |
| 15. Task Cancellation + Collision + Semaphore | v1.3 | 0/TBD | Not started | - |
| 16. Live UI Indicators | v1.3 | 0/TBD | Not started | - |
