# Milestone v1.3 Requirements — Per-Worktree Tasks + Architecture Audit

**Milestone goal:** Audit the architecture against Ousterhout, Martin Fowler (4-layer and hexagonal), and modal/action-dispatch completeness. Establish a test-coverage safety net before any structural change lands. Fix Critical/Major deviations. Then rework the task system to be per-worktree with parallel execution, individual cancellation, uniqueness-based collision handling, and live UI indicators.

**Ordering rule:** no refactor or task-system phase ships until the coverage gate (COVER-01..COVER-04) is green. This milestone treats missing coverage as a blocker, not a recommendation.

---

## v1.3 Requirements

### Coverage Gate (COVER)

Test coverage that must land before any audit-driven refactor or task-system change is merged. This is a hard prerequisite — no structural change lands without these tests passing.

- [x] **COVER-01**: Characterization test locks in the metro single-instance invariant — starting metro in one worktree while metro is running in another must fail/resolve via the existing conflict flow, and `MetroManager` must hold exactly one live handle at any time.
- [x] **COVER-02**: Characterization test locks in process-group kill behavior — killing a running command must terminate the full subprocess tree (yarn → node, gradle → java, xcodebuild → clang, etc.); no orphaned PIDs.
- [x] **COVER-03**: Coverage tests for the existing command-dispatch paths most likely to be touched by refactors: queue dispatch (`CommandQueued` / `CommandExited` routing), modal dismissal flow, and palette → action resolution for each palette (a / i / x / y / g / w).
- [x] **COVER-04**: A baseline coverage report (e.g. via `cargo llvm-cov` or `cargo-tarpaulin`) is committed to the milestone so subsequent phases can detect regressions; a minimum per-module threshold is documented (target value decided at planning time based on the baseline).

### Architecture Audit (ARCH)

- [x] **ARCH-01**: Auditor produces an Ousterhout review of every module (`domain/`, `infra/`, `app/`, `ui/`) scoring deep-module/narrow-interface criteria with severity-tagged findings (Critical / Major / Minor).
- [x] **ARCH-02**: Auditor maps the codebase to Martin Fowler's 4-layer model (Presentation / Domain / Data Source / Service) and flags modules with mixed responsibilities.
- [x] **ARCH-03**: Auditor checks hexagonal ports-and-adapters discipline — infra modules must be adapters for domain-defined traits; direct coupling between infra and app/ui is flagged.
- [x] **ARCH-04**: Auditor enumerates every `_ => {}` catch-all in match arms (handle_key, update, modal dispatch) and lists explicit gaps.
- [x] **ARCH-05**: Auditor surfaces every place where command ordering / prerequisite logic lives outside the domain layer (e.g. metro-before-run, pod-install-before-build, sync-before-metro inlined in `update()` or `app.rs`).
- [x] **ARCH-06**: Audit findings are written to `.planning/phases/11-architecture-audit/AUDIT.md` with prioritized recommendations consumed by the refactor phase.

### Audit-Driven Refactors (REFACTOR)

- [ ] **REFACTOR-01**: All Critical and Major findings from ARCH-01..ARCH-05 are resolved (Minor findings may be deferred to backlog with rationale). **Precondition:** COVER-01..COVER-04 are green before any refactor touches modified code.
- [ ] **REFACTOR-02**: `CommandSpec::is_cancellable()` predicate is added; git-porcelain variants return `false`, all other variants return `true`. Cancellation surface becomes type-driven, not convention.
- [ ] **REFACTOR-03**: Command prerequisites / action ordering is represented abstractly in domain code. Pick one during planning, whichever fits existing CommandSpec shape better:
  - (a) Prerequisite graph on `CommandSpec` — each variant declares what must precede it
  - (b) Domain-level `Pipeline` / `Recipe` type composing `CommandSpec`s with ordering rules
  - Update dispatcher to read ordering from domain representation instead of inline `update()` logic.

### Per-Worktree Task System (TASK)

- [ ] **TASK-01**: Global `running_command` / `command_task` / `command_queue` in `AppState` are replaced with per-worktree task state keyed by `WorktreeId` (e.g. `HashMap<WorktreeId, WorktreeTaskRecord>`). **Precondition:** COVER-01..COVER-04 green.
- [ ] **TASK-02**: Commands execute in parallel across different worktrees. The metro single-instance invariant is preserved — metro remains a single global slot enforced separately.
- [ ] **TASK-03**: A running task's identity is `(CommandKind, WorktreeId)`. This identity is available to UI, cancellation, and collision logic.
- [ ] **TASK-04**: Individual tasks can be cancelled by the user (any task type — yarn, clean, pod-install, run-android, run-ios, tests, arbitrary shell commands). Cancellation uses `CancellationToken` + SIGTERM to the process group (`libc::kill(-pgid, SIGTERM)`) + SIGKILL grace fallback + `kill_on_drop(true)` safety net. Git-porcelain commands are not cancellable (enforced by REFACTOR-02).
- [ ] **TASK-05**: Collision handling — when the user triggers a task whose `(CommandKind, WorktreeId)` identity matches one already running, the system either cancels-the-previous or blocks-the-new, per a documented policy attached to each command category. Default: block-the-new for idempotent installs, cancel-the-previous for builds/tests.
- [ ] **TASK-06**: Shared-resource contention (yarn global cache, same repo-root writes) is prevented with a narrower-scoped serialization mechanism (e.g. `tokio::sync::Semaphore(1)` keyed by repo-root `PathBuf`) layered on top of per-task uniqueness. Concurrent yarn installs across worktrees with the same repo-root MUST NOT corrupt `node_modules`.

### Live UI Indicators (UI)

- [ ] **UI-01**: The worktree table renders `Y` and `P` as two independent cells/characters (not a merged "Y/P" string). Each can independently show its letter or a spinner.
- [ ] **UI-02**: When a yarn-family task is running for a worktree, its `Y` cell is replaced by a rotating 6-frame yellow spinner (frame index = `elapsed.as_millis() / 150 % 6`, or whatever convention the chosen frame set requires). Same rule for `P` cell when a pod-family task is running. Other cancellable task categories (run-android, run-ios, tests, shell) get an equivalent animated indicator in the appropriate row position.
- [ ] **UI-03**: The worktree row shows live MM:SS elapsed time for the currently-running task, computed in the render path from `started_at.elapsed()` with no mutable tick state stored in `AppState`.

---

## Future Requirements (Deferred)

- Task history persistence per worktree (not this milestone)
- Current task name (e.g. `pod-install`) displayed inline in the worktree table row — user opted for spinner + elapsed only
- `app.rs` decomposition / god-object split — only happens if the audit makes it a Critical finding
- TEA purity refactor (Effect-return pattern for `update()`) — only happens if the audit makes it a Critical finding
- `cargo-modules` / `cargo-depgraph` / `cargo-deny` CI integration — post-milestone concern
- Broader unit/integration test expansion beyond the targeted Coverage Gate (COVER-01..COVER-04) — deferred to later milestones

## Out of Scope

- Configurable keybinding overrides, theme/color customization, multi-project support (future milestones, not this one)
- Cancellation of git operations — git porcelain must remain non-cancellable (data-integrity risk on interrupted rebase/merge)
- `throbber-widgets-tui` crate dependency — inline 6-frame const array is sufficient and avoids MSRV bump to 1.88
- `arch_test_core` runtime fitness functions — AGPL license incompatible with MIT project
- Mobile app, web UI, building/modifying the UMP React Native app itself
- Real-time JIRA sync or multi-user support

---

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| ARCH-01 | Phase 11 | Complete |
| ARCH-02 | Phase 11 | Complete |
| ARCH-03 | Phase 11 | Complete |
| ARCH-04 | Phase 11 | Complete |
| ARCH-05 | Phase 11 | Complete |
| ARCH-06 | Phase 11 | Complete |
| COVER-01 | Phase 12 | Complete |
| COVER-02 | Phase 12 | Complete |
| COVER-03 | Phase 12 | Complete |
| COVER-04 | Phase 12 | Complete |
| REFACTOR-01 | Phase 13 | Pending |
| REFACTOR-02 | Phase 13 | Pending |
| REFACTOR-03 | Phase 13 | Pending |
| TASK-01 | Phase 14 | Pending |
| TASK-02 | Phase 14 | Pending |
| TASK-03 | Phase 14 | Pending |
| TASK-04 | Phase 15 | Pending |
| TASK-05 | Phase 15 | Pending |
| TASK-06 | Phase 15 | Pending |
| UI-01 | Phase 16 | Pending |
| UI-02 | Phase 16 | Pending |
| UI-03 | Phase 16 | Pending |
