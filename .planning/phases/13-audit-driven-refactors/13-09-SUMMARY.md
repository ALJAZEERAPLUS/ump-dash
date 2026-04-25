---
phase: 13-audit-driven-refactors
plan: 09
subsystem: app
tags: [refactor, recipe-consumer, F-204, F-205, REFACTOR-03, wave-7]
requirements: [REFACTOR-01, REFACTOR-03]
requirements_addressed: [REFACTOR-01, REFACTOR-03]
dependency_graph:
  requires: [13-03, 13-07, 13-08]
  provides: [13-10]
  affects:
    - src/app/update.rs (1481 LOC -> 1554 LOC; 11 inline prereq sites consume Recipe::expand; deferred-spec uses command_queue.push_front)
    - src/app/state.rs (278 LOC -> 282 LOC; 3 prereq flags deleted; post_drain_action added)
    - src/app/handle_key.rs (71 LOC -> 92 LOC; outer match pivots on ModalState; exhaustive arms)
    - src/domain/pipeline.rs (318 LOC -> 327 LOC; DependencyState::new convenience constructor)
    - Makefile (G-06 + G-18 flipped from PENDING-echo to active hard-fail)
tech_stack:
  added: []
  patterns:
    - "Recipe::expand(&DependencyState) — domain-defined prereq ordering invoked at every dispatch site"
    - "command_queue front-push as deferred-spec store — replaces pending_metro_run; drained on MetroActivityUpdate(Ready)"
    - "post_drain_action: Option<Box<Action>> — generalized post-queue-drain coordination; replaces pending_metro_after_sync"
    - "Synchronous active_worktree_path updates at switch sites — replaces pending_switch_path stash"
    - "Exhaustive match on ModalState — Rust's exhaustiveness checker guards future modal additions"
key_files:
  created: []
  modified:
    - path: src/app/update.rs
      purpose: 11 inline prereq sites now construct a domain Recipe and call Recipe::expand(&DependencyState). Deferred-spec for metro-not-ready is pushed onto command_queue.push_front instead of pending_metro_run; the queue drains on MetroActivityUpdate(Ready). post_drain_action replaces pending_metro_after_sync. WorktreeSwitch sets active_worktree_path immediately (no pending_switch_path stash). Exhaustive ModalState arms in ModalInputChar / ModalInputBackspace.
      lines_before: 1481
      lines_after: 1554
    - path: src/app/state.rs
      purpose: AppState fields pending_metro_run, pending_metro_after_sync, pending_switch_path DELETED. post_drain_action: Option<Box<crate::domain::action::Action>> NEW. Survivors pending_restart and skip_external_metro_check kept (Pitfall 3 — metro-lifecycle, not prereq). Default impl updated; comments scrubbed of deleted-flag names so G-06 grep is strict-zero.
      lines_before: 278
      lines_after: 282
    - path: src/app/handle_key.rs
      purpose: Modal char-fallthrough block pivoted from match on (ModalState, KeyCode) to outer match on ModalState. Outer match is now exhaustive — Rust's exhaustiveness checker triggers on future ModalState additions. Inner KeyCode if-let intentionally narrow (KeyCode has many variants we have no semantic relationship with).
      lines_before: 71
      lines_after: 92
    - path: src/domain/pipeline.rs
      purpose: Added DependencyState::new(stale_yarn, stale_pods, is_ios_target) convenience constructor for the call-site pattern in update.rs.
      lines_before: 318
      lines_after: 327
    - path: Makefile
      purpose: G-06 (! rg 'pending_metro_run|pending_metro_after_sync|pending_switch_path' src/app/state.rs) and G-18 (! rg '\b_ => \{\}' src/app/handle_key.rs) flipped from PENDING-echo to active hard-fail with explicit error messages.
decisions:
  - id: D-13-09-01
    title: command_queue.push_front replaces pending_metro_run
    context: pending_metro_run was a single-slot Option<CommandSpec> that stored a deferred run command awaiting metro-Ready. Plan §interfaces specified "absorbed by Recipe::SyncThenRun" but Recipe::SyncThenRun expands to a sync sequence ending in the run command — it does not encode "wait for metro Ready." The actual deferral semantics live on the metro-Ready edge, which fires after MetroStart eventually emits MetroActivityUpdate.
    rationale: command_queue is already a FIFO of CommandSpecs that drains on CommandExited (and now on MetroActivityUpdate(Ready) when running_command is None). Pushing the deferred spec to the FRONT preserves the head-of-queue invariant the prior single-slot field guaranteed. The dispatcher pops one CommandSpec at a time as before; flatten-at-enqueue semantics preserved. CommandQueue tests in dispatch_tests continue to pass without change.
    files: src/app/update.rs (CommandRun, CommandExited, SyncBeforeRunDecline, MetroActivityUpdate handlers)
  - id: D-13-09-02
    title: post_drain_action: Option<Box<Action>> replaces pending_metro_after_sync (renamed, generalized)
    context: pending_metro_after_sync was a bool consumed in CommandExited's empty-queue branch to fire MetroStart after a sync sequence drained. Plan §interfaces hinted at "Effect chain" absorbing this — but Effect dispatch is immediate; we need state that survives across multiple CommandExited dispatches.
    rationale: Renamed and generalized: the Option<Box<Action>> slot lets future post-drain coordination reuse the same mechanism without re-introducing a string of ad-hoc bools. The bool was ALSO conceptually a coordination flag like pending_metro_run; the rename satisfies the G-06 grep AND keeps the semantic crisp ("dispatch this Action when the queue empties"). post_drain_action is cleared in CommandCancel and MetroSpawnFailed (the same sites that cleared the old bool).
    files: src/app/state.rs (field added), src/app/update.rs (5 sites — set in WorktreeSwitch auto-sync + SyncBeforeMetroAccept; consumed in CommandExited; cleared in CommandCancel + MetroSpawnFailed)
  - id: D-13-09-03
    title: pending_switch_path deletion via synchronous active_worktree_path update
    context: pending_switch_path stashed the target path during a worktree switch with stale deps OR with running metro. It was consumed by SyncBeforeMetroAccept/Decline (after the modal flow) and by MetroExited's restart path (after metro stopped).
    rationale: Setting active_worktree_path AT THE SWITCH SITE (instead of stashing it for later) eliminates the stash entirely. The two consumer sites (modal accept/decline + MetroExited restart) read active_worktree_path directly. MetroStart at line 135 reads active_worktree_path to choose the spawn worktree — that read sees the synchronously-updated value. No behavior change; the stash was redundant state.
    files: src/app/update.rs (WorktreeSwitchToSelected — auto-sync + modal-trigger + fresh-deps paths; MetroExited cleans no-op; SyncBeforeMetroAccept/Decline read active_worktree_path directly)
  - id: D-13-09-04
    title: handle_key.rs outer match pivot — ModalState first, KeyCode inner if-let
    context: The prior fallthrough block matched `(ModalState, KeyCode)` 2-tuples with explicit arms for (TextInput, Char(c)), (DevicePicker, Char(c) if !is_ascii_control), (BranchPicker, Char(c)) and a `_ => {}` catch-all. G-18 mandates strict zero `_ => {}` in handle_key.rs.
    rationale: Pivot the outer match to ModalState only. Inside each arm, an if-let on KeyCode handles the Char(c) extraction. Exhaustive ModalState match (5 arms with explicit no-op variants) means Rust's exhaustiveness checker now guards future ModalState additions. The inner if-let on KeyCode is intentionally narrow — KeyCode has dozens of variants we have no semantic stake in.
    files: src/app/handle_key.rs (Fallthrough 1 block)
  - id: D-13-09-05
    title: Auto-sync deps reconstruction at SyncBeforeRunAccept call site
    context: The plan's flag-migration table called for SyncBeforeRunAccept to use Recipe::SyncThenRun(spec). The modal's needs_yarn/needs_pods bools encode the staleness decision but not the is_ios_target flag — Recipe::SyncThenRun consults `deps.is_ios_target` to decide whether to add YarnPodInstall.
    rationale: At modal-construction time, needs_pods is only set true when the spec was an iOS run (CommandRun stale check at update.rs line 341). So at modal-acceptance time we can derive is_ios_target = needs_pods. Recipe::SyncThenRun's expansion: pods only included when stale_pods AND is_ios_target — needs_pods=true reproduces this. needs_pods=false (Android) means no pod install regardless of is_ios_target value.
    files: src/app/update.rs (Action::SyncBeforeRunAccept handler)
metrics:
  duration_minutes: 45
  tasks_completed: 3
  tasks_total: 3
  tests_before: 79
  tests_after: 79  # 76 lib + 2 metro_single_instance + 1 process_group_kill
  recipe_invocations_in_update_rs: 21  # match count of "Recipe::"
  inline_prereq_sites_replaced: 11  # per AUDIT F-204 enumeration
  flags_deleted: 3  # pending_metro_run, pending_metro_after_sync, pending_switch_path
  flags_kept: 2  # pending_restart, skip_external_metro_check (Pitfall 3 survivors)
  field_added: 1  # post_drain_action: Option<Box<Action>>
  guards_activated: 2  # G-06 + G-18 flipped from PENDING-echo to active hard-fail
  catchalls_removed: 3  # 2 in update.rs (ModalInputChar/Backspace) + 1 in handle_key.rs
  modalstate_variants_enumerated: 8  # Confirm, TextInput, DevicePicker, CleanToggle, SyncBeforeRun, SyncBeforeMetro, ExternalMetroConflict, BranchPicker
  lines_net_delta: "+103 (+73 update.rs, +4 state.rs, +21 handle_key.rs, +9 pipeline.rs, ~ Makefile)"
  completed: 2026-04-24T14:30:00Z
threat_model_disposition: accept_refactor_only
---

# Phase 13 Plan 13-09: Recipe Consumers + Exhaustive Modal Arms (F-204 + F-205) Summary

## One-liner

Closed REFACTOR-03 + the F-204/F-205 audit findings: every one of the 11
inline prerequisite/ordering sites in `src/app/update.rs` now constructs
a domain `Recipe` and calls `Recipe::expand(&DependencyState)`. The
three boolean coordination flags `pending_metro_run`,
`pending_metro_after_sync`, and `pending_switch_path` are deleted from
`AppState` — absorbed into `command_queue.push_front` (deferred-spec
store), `post_drain_action: Option<Box<Action>>` (post-queue-drain
coordination), and synchronous `active_worktree_path` updates
respectively. The wildcard `_ => {}` arms in modal dispatch are replaced
with exhaustive `ModalState` enumeration so Rust's exhaustiveness
checker now guards future modal additions. `make arch-lint` is green
with G-06 + G-18 active hard-fail. All 79 existing tests pass without
modification.

## Site → Recipe variant mapping (F-204 enumeration)

| # | Pre-refactor location (post-split file:arm) | Recipe variant invoked | Notes |
|---|---|---|---|
| 1 | `update.rs::Action::CommandRun` auto-sync fast path (was 350-369) | `Recipe::SyncThenRun(spec)` | Replaces inline yarn/pod sequencing; deps from worktree.stale + check_stale_pods |
| 2 | `update.rs::Action::CommandRun` metro prereq (was 382-387) | n/a — `command_queue.push_front + MetroStart` | Deferred-spec replaces `pending_metro_run` |
| 3 | `update.rs::Action::CommandRun` RnReleaseBuild dispatch (was 429-434) | `Recipe::ReleaseBuildAndInstall` | Two-step: RnReleaseBuild → AdbInstallApk |
| 4 | `update.rs::Action::CommandRun` GitResetHardFetch dispatch (was 438-444) | `Recipe::GitFetchThenReset` | Two-step: GitFetch → GitResetHard |
| 5 | `update.rs::Action::CommandExited` drain metro check (was 492-497) | n/a — `command_queue.push_front + MetroStart` | Same deferred-spec pattern as site 2 |
| 6 | `update.rs::Action::CleanConfirm` (was 1075-1110) | `Recipe::Clean(opts)` | Replaces inline option-to-command sequencing |
| 7 | `update.rs::Action::SyncBeforeRunAccept` (was 1143-1166) | `Recipe::SyncThenRun(*run_command)` | needs_pods=true ⇒ is_ios_target=true (D-13-09-05) |
| 8 | `update.rs::Action::SyncBeforeRunDecline` metro check (was 1167-1181) | n/a — `command_queue.push_front + MetroStart` | Same deferred-spec pattern |
| 9 | `update.rs::Action::SyncBeforeMetroAccept` (was 1183-1216) | `Recipe::SyncThenStartMetro` | post_drain_action set to MetroStart for queue-empty trigger |
| 10a | `update.rs::Action::WorktreeSwitchToSelected` auto-sync (was 929-943) | `Recipe::SyncThenStartMetro` | active_worktree_path set synchronously |
| 10b | `update.rs::Action::WorktreeSwitchToSelected` modal trigger (was 945-948) | (modal construction) | active_worktree_path set synchronously; modal carries no path |
| 11 | `update.rs::Action::WorktreeSwitchToSelected` fresh deps (was 951-962) | n/a — direct dispatch | active_worktree_path set synchronously; no pending_switch_path |

`Recipe::` appears 21 times in `src/app/update.rs` (well above the ≥5
threshold from the plan's verify command).

## Flag deletion ledger

| Flag | Pre-refactor type | Post-refactor encoding |
|------|-------------------|------------------------|
| `pending_metro_run` | `Option<CommandSpec>` | `command_queue.push_front(spec)` at 3 dispatch sites; drained on `MetroActivityUpdate(Ready)` when `running_command.is_none()` |
| `pending_metro_after_sync` | `bool` | `post_drain_action: Option<Box<Action>>` slot; set to `Some(Box::new(Action::MetroStart))` in WorktreeSwitch auto-sync + SyncBeforeMetroAccept; consumed in `CommandExited` empty-queue branch |
| `pending_switch_path` | `Option<PathBuf>` | `state.active_worktree_path = Some(path)` set synchronously at 3 WorktreeSwitchToSelected branches; the post-stop MetroStart re-reads the already-updated value |

## Survivor flags (Pitfall 3)

| Flag | Why it stays |
|------|--------------|
| `pending_restart: bool` | Metro-lifecycle stop-then-start handoff. `MetroExited` reads it to decide whether to dispatch `MetroStart`. NOT prereq ordering. May migrate to a `MetroState` sub-struct in Plan 13-10. |
| `skip_external_metro_check: bool` | Metro-lifecycle: signals the next `MetroStart` to bypass the `DetectExternalMetro` effect because the port is still releasing from our just-killed process. NOT prereq ordering. May also migrate to `MetroState` in 13-10. |

## F-205 — exhaustive ModalState enumeration

Three wildcard `_ => {}` arms removed:

1. `update.rs::Action::ModalInputChar` — was `Some(ModalState::TextInput
   | DevicePicker)` + `_ => {}`. Now enumerates all 8 ModalState
   variants explicitly: `Confirm | CleanToggle | SyncBeforeRun |
   SyncBeforeMetro | ExternalMetroConflict | BranchPicker | None` as the
   no-op arm; `TextInput`, `DevicePicker` keep their existing handlers.
2. `update.rs::Action::ModalInputBackspace` — same pattern, same 8
   variants.
3. `handle_key.rs` modal char-fallthrough block — pivoted from `match
   (modal, key.code)` with `_ => {}` to `match modal` with exhaustive
   ModalState arms. Inner if-let on `KeyCode::Char(c)` is intentionally
   narrow.

The `_ => {}` arm in `runtime.rs` (Event match for crossterm) is
intentionally KEPT — Event has many variants we don't care about
(Resize-followed-by tail variants, Mouse, Paste, Focus); a catch-all
there is the correct expression of "we only handle Key + already-
matched Resize." G-18 only covers `src/app/handle_key.rs`.

## command_queue semantics — preserved

Per 13-PATTERNS.md anti-pattern 11 ("don't delete command_queue
wholesale"), the queue is retained:

- Still a `VecDeque<CommandSpec>` field on AppState
- Still drained one-CommandSpec-per-CommandExited via `pop_front`
- `Recipe::expand()` is **flatten-at-enqueue**: callers construct a
  Recipe, call expand, and `push_back` every yielded CommandSpec.
  Dispatcher stays simple. CommandQueue tests in `dispatch_tests`
  (3 tests covering push/pop/drain) continue to pass without
  modification because the assertion is on the FIFO contract, not on
  who built the contents.
- New role: **deferred-spec store** for metro-not-ready. The 3 dispatch
  sites that previously stored a single CommandSpec in
  `pending_metro_run` now `push_front` the spec onto command_queue. On
  `MetroActivityUpdate(Ready)`, when `running_command.is_none()`, the
  handler `pop_front()`s and re-enters `Action::CommandRun` (preserving
  the full pipeline: stale check, device picker, etc.).

## Verification

| Check | Result |
|-------|--------|
| `cargo build --all-targets` | PASS |
| `cargo test --all-targets` | 79 tests passed (76 lib + 2 metro_single_instance + 1 process_group_kill) |
| `cargo test --lib dispatch_tests` | 17 passed (COVER-03 preserved) |
| `cargo test --lib domain::pipeline` | 16 passed (Recipe + DependencyState contract intact) |
| `cargo test --test metro_single_instance` | 2 passed (COVER-01 preserved) |
| `cargo test --test process_group_kill` | 1 passed (COVER-02 preserved) |
| `cargo clippy --all-targets -- -D warnings` | CLEAN |
| `make arch-lint` | PASS (G-06 + G-18 active hard-fail) |
| `! rg 'pending_metro_run\|pending_metro_after_sync\|pending_switch_path' src/app/state.rs` | 0 hits (G-06 strict-zero) |
| `! rg '\b_ => \{\}' src/app/handle_key.rs` | 0 hits (G-18 strict-zero) |
| `grep -q 'pending_restart' src/app/state.rs` | found (survivor) |
| `grep -q 'skip_external_metro_check' src/app/state.rs` | found (survivor) |
| `grep -c 'Recipe::' src/app/update.rs` | 21 (≥5 threshold) |
| `grep -c 'Some(ModalState::' src/app/update.rs` | 48 (well above the ≥16 threshold the plan asked about) |
| `grep -q 'pub struct Adapters' src/app/adapters.rs` | found (G-13 still active) |

## Test assertion deltas

**Zero test files required modification.** All 17 dispatch_tests + 2
metro_single_instance + 1 process_group_kill pass against the new
implementation as-is. Notable cases:

- **`command_exited_with_nonempty_queue_pops_and_dispatches_front`**:
  asserts the popped spec becomes `running_command` and an
  `Effect::SpawnCommand` is emitted. The new implementation preserves
  both invariants (the front of the queue is YarnInstall which doesn't
  need metro, so it dispatches directly).
- **`metro_start_while_running_triggers_restart_not_double_spawn`**:
  asserts `state.pending_restart` becomes true and no
  `Effect::SpawnMetro` is emitted. The MetroStart handler still flips
  `pending_restart` (survivor flag) and dispatches MetroStop.
- **`sync_before_metro_modal_dismisses_on_n_and_esc`**: asserts
  `state.modal` becomes None after SyncBeforeMetroDecline. The
  refactored Decline handler no longer touches pending_switch_path
  (deleted) — it just drops the modal and dispatches MetroStart (or
  MetroStop+pending_restart if running). The test only asserts on
  modal clearance; passes.
- **`yarn_c_opens_clean_toggle_then_x_confirms`**: only tests the
  key→action mapping at `handle_key`; doesn't observe Recipe
  expansion. Passes.

This is the testability advantage promised by Plan 13-03's pure
`Recipe::expand`: behavior assertions on inputs/outputs are stable
across the consumer rewrite because the prereq ordering rules are
captured in the domain type, not in the inline conditionals.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking issue] post_drain_action field rename instead of pure deletion**
- **Found during:** Task 1 design — the plan's flag-migration table said
  `pending_metro_after_sync → DELETED → "Absorbed by Recipe::SyncThenStartMetro + Effect chain"`.
- **Issue:** `Recipe::SyncThenStartMetro` is a pure expansion that yields
  yarn/pod CommandSpecs — it does NOT model the "after the queue drains
  go fire MetroStart" coordination, which spans multiple
  CommandExited cycles. Effects are dispatched immediately; we need
  state that survives across dispatches. A pure deletion with no
  replacement would lose the "dispatch metro after sync drains"
  invariant covered by no test today but observably load-bearing for
  the worktree-switch flow.
- **Fix:** Renamed and generalized to `post_drain_action: Option<Box<Action>>`
  — a generalized post-queue-drain action slot. The bool's role becomes
  one specific instance of the new slot's general capability. Future
  post-drain coordination can reuse it without growing AppState. Field
  cleared in CommandCancel + MetroSpawnFailed (same sites that cleared
  the old bool).
- **Files modified:** src/app/state.rs (field added), src/app/update.rs (5 sites)
- **Commit:** 9a2aa12
- **Rationale:** D-13-09-02. Satisfies the spirit of the plan ("don't
  resurrect the bool") while keeping the coordination semantic crisp
  and not introducing a regression on the worktree-switch flow.

**2. [Rule 1 - Bug] Recipe::SyncThenStartMetro expansion may be empty**
- **Found during:** Task 1 — Recipe::SyncThenStartMetro returns `vec![]`
  when both stale_yarn and stale_pods are false. The classic call sites
  always set at least one to true (the modal flow only fires when
  yarn_stale, the auto-sync path passes (true, false, false)). But the
  refactored SyncBeforeMetroAccept handler builds the deps from
  needs_yarn + needs_pods which are CARRIED FROM THE MODAL — and the
  modal could in principle be constructed with both false (defensive
  programming).
- **Fix:** Added a guard: `if sequence.is_empty() { state.post_drain_action = None; effects.extend(update(state, Action::MetroStart)); return effects; }`. This handles the no-sync-needed case directly without setting up post_drain_action that would never fire.
- **Files modified:** src/app/update.rs (SyncBeforeMetroAccept + WorktreeSwitchToSelected auto-sync)
- **Commit:** 9a2aa12

**3. [Rule 1 - Bug] handle_key.rs `_ => {}` literal in doc comment trips G-18**
- **Found during:** Task 2 — initial G-18 grep `! rg '\b_ => \{\}' src/app/handle_key.rs` matched a doc comment that explained "the prior `_ => {}` catch-all is gone". The plan's acceptance criterion uses the strict grep with no comment exclusion.
- **Fix:** Rewrote the doc comment to use "wildcard catch-all" instead
  of the literal `_ => {}` pattern. Same explanation, same intent, no
  literal pattern hit.
- **Files modified:** src/app/handle_key.rs (Fallthrough 1 doc comment)
- **Commit:** 7b2623c
- **Note:** The same issue applied to state.rs comments mentioning the
  deleted flag names. Same resolution — paraphrased the comment without
  the literal field name.

**4. [Rule 3 - Blocking issue] clippy collapsible-if warning on nested if-let**
- **Found during:** Task 2 — the rewritten DevicePicker arm in
  handle_key.rs had `if let KeyCode::Char(c) = key.code { if !c.is_ascii_control() { ... } }`
  which clippy flagged as collapsible.
- **Fix:** Used the let-else-style chained `if let X && cond` form:
  `if let KeyCode::Char(c) = key.code && !c.is_ascii_control() { ... }`.
- **Files modified:** src/app/handle_key.rs
- **Commit:** 7b2623c

### Auto-added missing critical functionality

None beyond items 1+2 above (post_drain_action field and the
empty-Recipe guard).

## Auth gates

None — pure-code refactor, no external services or auth paths.

## TDD Gate Compliance

Not applicable — plan has `tdd="false"` per frontmatter. The 17
dispatch_tests + 2 metro_single_instance + 1 process_group_kill + all
76 lib tests serve as the behavior-preservation guard. All 79 tests
passed after each of the 3 task-level commits.

## Commits

| #  | Hash    | Type     | Message                                                                                                            |
|----|---------|----------|--------------------------------------------------------------------------------------------------------------------|
| 1  | 9a2aa12 | refactor | replace 11 inline prereq sites with Recipe::expand; delete pending_metro_run/after_sync/switch_path flags         |
| 2  | 7b2623c | refactor | replace _ => {} modal catch-alls with exhaustive ModalState arms                                                  |
| 3  | 3608f62 | build    | activate G-06 + G-18 arch-lint guards                                                                              |

## Threat Flags

None. Plan frontmatter declares `threat_model_disposition: accept_refactor_only`
— no behavior change, no new trust boundaries. Recipe::expand produces
the same linear sequence of CommandSpecs that the inline arms produced;
the deferred-spec front-push pattern preserves the head-of-queue
invariant the prior single-slot pending_metro_run guaranteed; the
synchronous active_worktree_path update at the switch site preserves
the "active path is the target before MetroStart spawns" invariant the
prior pending_switch_path stash provided.

## Known Stubs

None.

## Self-Check: PASSED

**Files claimed modified:**
- src/app/update.rs — FOUND (1554 LOC; 21 Recipe:: occurrences; ModalInputChar / ModalInputBackspace exhaustive)
- src/app/state.rs — FOUND (282 LOC; pending_metro_run / pending_metro_after_sync / pending_switch_path absent; post_drain_action present; pending_restart + skip_external_metro_check survive)
- src/app/handle_key.rs — FOUND (92 LOC; outer match exhaustive on ModalState; no `_ => {}` arms)
- src/domain/pipeline.rs — FOUND (DependencyState::new constructor present)
- Makefile — FOUND (G-06 + G-18 active hard-fail with explicit FAIL messages)

**Commits claimed:**
- 9a2aa12 — FOUND in git log
- 7b2623c — FOUND in git log
- 3608f62 — FOUND in git log

**Tests verified:**
- `cargo test --all-targets` — 79 passed
- `cargo test --lib dispatch_tests` — 17 passed (COVER-03)
- `cargo test --lib domain::pipeline` — 16 passed (Recipe contract)
- `cargo test --test metro_single_instance` — 2 passed (COVER-01)
- `cargo test --test process_group_kill` — 1 passed (COVER-02)
- `cargo clippy --all-targets -- -D warnings` — CLEAN
- `make arch-lint` — PASS (G-06 + G-18 active hard-fail; G-12 + G-20 still PENDING for 13-10)

**Grep invariants verified:**
- `rg 'pending_metro_run|pending_metro_after_sync|pending_switch_path' src/app/state.rs` — 0 hits (G-06 strict)
- `rg '\b_ => \{\}' src/app/handle_key.rs` — 0 hits (G-18 strict)
- `grep -c 'Recipe::' src/app/update.rs` — 21 (≥5 threshold)
- `grep -q 'pub struct Adapters' src/app/adapters.rs` — 1 (G-13 still active)
- `rg 'pending_restart' src/app/state.rs` — found (survivor)
- `rg 'skip_external_metro_check' src/app/state.rs` — found (survivor)

All self-check assertions confirmed against the worktree and git history.
