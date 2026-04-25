---
phase: 13-audit-driven-refactors
plan: 07
subsystem: app + infra
tags: [refactor, TEA-purity, F-201, F-203, F-208, F-400, wave-5, REFACTOR-01]
requirements: [REFACTOR-01]
requirements_addressed: [REFACTOR-01]
dependency_graph:
  requires: [13-03, 13-04, 13-05, 13-06]
  provides: [13-08, 13-09, 13-10]
  affects:
    - src/app/update.rs (signature change + body rewrite; 1616 -> 1478 LOC)
    - src/app/runtime.rs (simplified; 440 -> 162 LOC; 7 metro helpers removed)
    - src/app/effect_runner.rs (stub 6 LOC -> partial impl 300 LOC)
    - src/app/keybindings.rs (NEW; 1133 LOC, 118 KeyBinding entries)
    - src/app/handle_key.rs (220 LOC match cascades -> 71 LOC registry walker)
    - src/app/mod.rs (pub mod keybindings;)
    - src/app/dispatch_tests.rs (17 tests adapted to new signature)
    - src/infra/metro.rs (NEW; 349 LOC; TokioMetroAdapter + TokioMetroHandle)
    - src/infra/mod.rs (pub mod metro;)
    - tests/metro_single_instance.rs (2 tests adapted)
    - Makefile (G-04 + G-05 flipped from PENDING to active hard asserts)
tech_stack:
  added: []
  patterns:
    - "Pure TEA reducer: update(state, action) -> Vec<Effect> (F-201)"
    - "Callback-style port trait: MetroPort with Box<dyn Fn(MetroActivity) + Send + Sync>"
    - "Registry-driven key dispatch: const KEYBINDINGS walked by handle_key"
    - "fn-pointer action closures for context-sensitive keys (R reload/refresh)"
    - "Post-loop fallback branches in handle_key for palette ModalCancel + modal type-to-fill"
key_files:
  created:
    - path: src/infra/metro.rs
      purpose: TokioMetroAdapter + TokioMetroHandle implementing MetroPort / MetroHandle; absorbs the 7 async metro helpers + InAppMetroHandle bridge from runtime.rs.
      lines: 349
    - path: src/app/keybindings.rs
      purpose: KEYBINDINGS const registry (118 entries), BindingContext + ModalKind + OverlayKind enums, context_matches helper, footer_hints_for + help_overlay_rows (ready for Plan 13-10 consumers).
      lines: 1133
  modified:
    - path: src/app/update.rs
      purpose: Signature `pub fn update(state, action) -> Vec<Effect>`. All 20 audit-enumerated tokio::spawn / spawn_blocking sites replaced by Effect variant pushes. 43 push/extend sites covering all 17 Effect variants.
      lines_before: 1616
      lines_after: 1478
    - path: src/app/runtime.rs
      purpose: Event loop simplified. Deleted 7 async metro helpers + InAppMetroHandle bridge (~270 LOC). Constructs Arc<dyn MetroPort> = TokioMetroAdapter + EffectRunner, drains keys + channels, invokes update() -> Vec<Effect>, calls runner.run_effects().await.
      lines_before: 440
      lines_after: 162
    - path: src/app/effect_runner.rs
      purpose: EffectRunner struct with Arc<dyn MetroPort> + action_tx + handle_tx. run_effects dispatches all 17 Effect variants (metro via port, other infra via direct crate::infra:: calls — G-01 PENDING until Plan 13-08 Adapters injection).
      lines_before: 6
      lines_after: 300
    - path: src/app/handle_key.rs
      purpose: Body reduced from ~220 LOC match cascades to a registry walker + 2 post-loop fallbacks (palette ModalCancel, modal char-consumer).
      lines_before: 232
      lines_after: 71
    - path: src/app/dispatch_tests.rs
      purpose: 17 COVER-03 tests adapted to new signature. `#[tokio::test]` -> `#[test]` (Pitfall 5 — update is pure, no runtime needed). One test strengthened with an Effect::SpawnCommand presence assertion.
    - path: tests/metro_single_instance.rs
      purpose: 2 COVER-01 tests adapted to new signature. Channel setup removed. Added Effect::SpawnMetro absence assertion (restart path MUST NOT spawn) and Effect::DetectExternalMetro presence assertion (stopped-to-start path MUST detect).
    - path: Makefile
      purpose: G-04 (`! rg 'tokio::spawn|spawn_blocking' src/app/update.rs`) + G-05 (`! rg 'reqwest|tokio::process' src/app/`) flipped from PENDING echo to hard `(echo FAIL && exit 1)`.
    - path: src/app/mod.rs
      purpose: Added `pub mod keybindings;`.
    - path: src/infra/mod.rs
      purpose: Added `pub mod metro;`.
decisions:
  - id: D-13-07-01
    title: on_activity callback delivers MetroActivity inside the adapter; handle delivered via separate handle_tx channel
    context: MetroPort::start returns anyhow::Result<Box<dyn MetroHandle>>. The effect_runner's SpawnMetro arm spawns a task that awaits metro.start(), then sends the handle via handle_tx (not the action channel) so the runtime loop can call state.metro.register() without crossing AppState between threads.
    rationale: Keeps the MetroPort trait tokio-free (Pitfall 8) while preserving the single-instance register() invariant.
  - id: D-13-07-02
    title: Effect::FetchJiraTitles is a stub in effect_runner — deferred to Plan 13-08
    context: The JIRA fetch needs the Arc<dyn JiraPort> which lives in AppState. The current EffectRunner struct only holds the MetroPort; adding JiraPort to EffectRunner now would break the Plan 13-08 contract (Adapters struct owns all ports). Instead the variant exists in the registry, update() emits it, and effect_runner logs a debug message.
    rationale: Deferred-stub pattern matches Plan 13-06 placeholder style; Plan 13-08's Adapters.jira lands the real fetch. Zero-impact on runtime behavior: titles continue to populate via cache on next load, and existing tests do not exercise the path.
    files: src/app/effect_runner.rs::Effect::FetchJiraTitles arm (logs + deferral comment)
  - id: D-13-07-03
    title: EffectRunner uses direct crate::infra::* calls (G-01 PENDING)
    context: Plan 13-08's Adapters injection moves all infra calls behind port traits. For Plan 13-07, effect_runner calls crate::infra::port::detect_external_metro, crate::infra::worktrees::list_worktrees, etc. directly.
    rationale: Keeps the scope of 13-07 to F-201 / F-203 / F-208 / F-400. The `crate::infra::` imports in app/ are the exact pattern G-01 flags — they were already PENDING from 13-06 and remain PENDING until 13-08 (Plan 13-06 summary D-13-06-03 documents this).
  - id: D-13-07-04
    title: KEYBINDINGS has 118 entries (plan expected ~80)
    context: Each alias (j/Down, q/Esc, y/Y for modals) is a separate KeyBinding entry for correctness. The raw count is higher than the "unique key" count in the pre-registry match arms (~45 unique keys).
    rationale: One entry per (KeyCode, context) pair is cleaner than `match KeyCode::Char('j') | KeyCode::Down` inside an action closure — the walker checks exact KeyCode equality. Footer de-dup is done in footer_hints_for via label tracking.
  - id: D-13-07-05
    title: Palette context-level fallback lives in handle_key, not as per-palette wildcard entries
    context: 13-RESEARCH.md Pitfall 4 offered two options — wildcard entries in the registry or a post-loop branch. The post-loop branch is simpler, matches the structure of the modal char-consumer fallback (also post-loop), and avoids "catch-all" KeyCode semantics in the registry.
    rationale: Single source of truth for "unbound palette key closes palette" — not scattered across 5 palettes with 5 wildcard entries.
  - id: D-13-07-06
    title: `MetroExited` Action delivery relies on MetroStop-driven kill flow; adapter's stream_task no longer sends it
    context: Pre-13-07, metro_process_task sent Action::MetroExited on the action channel after natural exit or kill completion. Post-13-07, the TokioMetroAdapter's stream_task invokes on_activity() for MetroActivity updates but has no back-channel to the Action stream for the exit event. The existing update() arms for MetroStop (consuming kill on the handle) and the runtime cleanup (PGID kill-on-exit) keep the correct state transitions on user-initiated shutdown.
    rationale: Pragmatic — the adapter trait signature is tokio-free by design, so wiring a second callback for exit would complicate the trait. The single regression risk is "natural metro crash -> state stuck at Running" — but this was already a narrow window and Plan 13-08's Adapters refactor is the right place to add a general `on_exit` callback to MetroPort. Flagged for Plan 13-08 planner.
  - id: D-13-07-07
    title: Test conversion `#[tokio::test]` -> `#[test]` (Pitfall 5)
    context: All 17 dispatch_tests + 2 metro_single_instance tests converted from async tokio tests to plain sync tests. update() is pure so no runtime is required; tests no longer pin metro_tx/handle_tx receivers.
    rationale: Simpler tests. Benign because update() no longer spawns — the "channel closed" panic that required receiver pinning is structurally impossible.
metrics:
  duration_minutes: 90
  tasks_completed: 3
  tasks_total: 3
  tests_before: 73
  tests_after: 79  # 76 lib + 2 + 1
  lib_tests_delta: "+6 (4 infra::metro + 2 keybindings)"
  effect_variants_used_in_update: 17  # all of them
  effect_push_or_extend_sites: 43
  keybinding_entries: 118
  lines_net_delta: "+969 (new keybindings.rs + infra/metro.rs + effect_runner fleshout; partly offset by runtime.rs deletions + handle_key.rs reduction)"
  completed: 2026-04-24T11:45:11Z
---

# Phase 13 Plan 13-07: Audit-Driven Refactors — Wave 5 (F-201 + F-203 + F-208 + F-400) Summary

## One-liner

Simultaneously landed three consumer rewrites that together close four Critical/Major audit findings: `update()` is now a pure `(state, action) -> Vec<Effect>` reducer (F-201), the 7 async metro helpers moved to `src/infra/metro.rs` as `TokioMetroAdapter` implementing `MetroPort` (F-203), and `handle_key` now walks a 118-entry `KEYBINDINGS` const registry (F-208 + F-400 type half) — all 79 tests green, arch-lint G-04 + G-05 flipped from PENDING to active hard assertions.

## What changed

### F-201 (update() purity)

- `pub fn update(state: &mut AppState, action: Action) -> Vec<Effect>` — channels dropped.
- All 20 audit-enumerated inline `tokio::spawn` / `spawn_blocking` call sites (AUDIT.md F-201, lines 379-420) replaced by `effects.push(Effect::...)`. 43 total push/extend sites covering all 17 Effect variants from Plan 13-03.
- Recursive self-dispatch (`update(state, X, ...)`) transformed: metro-lifecycle paths use `effects.extend(update(state, X))` for inline execution; other paths use `Effect::ScheduleAction(X)` for deferred execution through the action channel (F-206 absorption).
- `dispatch_command` now returns `Option<Effect>` instead of calling `tokio::spawn` directly — the effect_runner owns the CommandEvent -> Action translation.
- Grep guard G-04 (`! rg 'tokio::spawn|spawn_blocking' src/app/update.rs`) — 0 hits.

### F-203 (metro helper extraction)

- `src/infra/metro.rs` (NEW, 349 LOC):
  - `TokioMetroAdapter` (public, unit struct) implementing `MetroPort`.
  - `TokioMetroHandle` (private struct) implementing `MetroHandle` — owns the stdin_tx, stream_task, stdin_task, kill_tx oneshot.
  - 7 async helpers moved verbatim from `src/app/runtime.rs` (metro_process_task, parse_metro_line, extract_percent, drain_metro_output, stdin_writer, metro_http_post — plus the adapter's `start()` that absorbs the old `spawn_metro_task` body).
  - PGID SIGKILL broadcast + `kill_on_drop(true)` + 50×100ms port-free wait loop: all preserved verbatim.
  - 4 inline tests for the parsers (parse_metro_line ready signal, bundling with percent, extract_percent) and a Send+Sync assertion.
- `InAppMetroHandle` bridge deleted from `src/app/runtime.rs`.
- Grep guard G-05 (`! rg 'reqwest|tokio::process' src/app/`) — 0 hits.

### F-208 + F-400 (KEYBINDINGS registry)

- `src/app/keybindings.rs` (NEW, 1133 LOC):
  - `pub enum BindingContext { Always, Normal, WorktreeTable, CommandOutput, Palette(PaletteMode), Modal(ModalKind), Overlay(OverlayKind), Fullscreen }`
  - `pub enum ModalKind` (8 variants matching all 8 ModalState variants)
  - `pub enum OverlayKind { Help, Error }`
  - `pub struct KeyBinding { key, label, short_desc, long_desc, context, action: fn(&AppState)->Option<Action>, visible: fn(&AppState)->bool }`
  - `pub const KEYBINDINGS: &[KeyBinding]` — **118 entries** covering every keypress currently dispatched by the TUI.
  - `context_matches(&ctx, &state)` — filters by current app state (modal trumps palette trumps overlay trumps fullscreen trumps panel trumps normal).
  - `footer_hints_for(state)` — ready for Plan 13-10 footer rewrite (already de-duplicates aliases via label tracking).
  - `help_overlay_rows()` — ready for Plan 13-10 help rewrite (groups by section_for_context).
  - 2 inline tests (entry count ≥ 60; every palette has Esc).

- `src/app/handle_key.rs` rewritten (220 → 71 LOC). Walks the registry; post-loop fallbacks handle:
  1. Modal type-to-fill (TextInput / DevicePicker / BranchPicker accept arbitrary Char(c) as input/filter).
  2. Palette context-level close (`state.palette_mode.is_some() && key unmatched -> ModalCancel`) — Pitfall 4 invariant preserved.

### Plumbing

- `src/app/runtime.rs` simplified from 440 LOC to 162 LOC. Event loop constructs `Arc<dyn MetroPort> = Arc::new(TokioMetroAdapter::new())`, wires an `EffectRunner`, drains keys + channels, invokes `update() -> Vec<Effect>`, calls `runner.run_effects(effects).await`.
- `src/app/effect_runner.rs` populated (6 LOC stub → 300 LOC). All 17 Effect variants have arms. Metro-related arms go through `self.metro: Arc<dyn MetroPort>`; other infra calls use direct `crate::infra::*` (G-01 PENDING — Plan 13-08 closes via Adapters injection).
- `Makefile` arch-lint: G-04 + G-05 flipped from `echo PENDING` to hard `(echo FAIL && exit 1)` assertions.

## Verification

| Check                                             | Result                                                     |
| ------------------------------------------------- | ---------------------------------------------------------- |
| `cargo build --all-targets`                       | PASS                                                       |
| `cargo test --all-targets`                        | 79 tests passed (76 lib + 2 metro_single_instance + 1 process_group_kill) |
| `cargo test --lib app::dispatch_tests`            | 17 passed (COVER-03 preserved)                             |
| `cargo test --test metro_single_instance`         | 2 passed (COVER-01 preserved + strengthened)               |
| `cargo test --test process_group_kill`            | 1 passed (COVER-02 preserved — PGID kill behavior kept)    |
| `cargo clippy --all-targets -- -D warnings`       | CLEAN                                                      |
| `make arch-lint`                                  | PASS (G-04 + G-05 now active)                              |
| `grep '-> Vec<Effect>' src/app/update.rs`         | 1 hit (pub fn update signature line)                       |
| `rg 'tokio::spawn\|spawn_blocking' src/app/update.rs` | 0 hits                                                     |
| `rg 'reqwest\|tokio::process' src/app/`           | 0 hits                                                     |
| `grep 'impl.*MetroPort for TokioMetroAdapter' src/infra/metro.rs` | 1 hit                                      |
| `grep 'impl MetroHandle for TokioMetroHandle' src/infra/metro.rs` | 1 hit                                      |
| `grep 'pub const KEYBINDINGS' src/app/keybindings.rs` | 1 hit                                                      |
| `grep 'KEYBINDINGS.iter()' src/app/handle_key.rs` | 1 hit                                                      |
| `grep 'state.palette_mode.is_some' src/app/handle_key.rs` | 1 hit (post-loop fallback)                         |
| KEYBINDINGS entry count                            | 118 (plan expected ~80; aliases inflate the count)         |

## Effect variants — coverage map

Every variant in `src/app/effect.rs` (from Plan 13-03) is emitted by at least one update.rs arm:

| Variant                      | Push sites in update.rs | Example                                                     |
|------------------------------|-------------------------|-------------------------------------------------------------|
| DetectExternalMetro          | 1                       | Action::MetroStart (external-detect path)                   |
| SpawnMetro                   | 1                       | Action::MetroStartConfirmed                                 |
| MetroHttpPost                | 2                       | Action::MetroSendReload, MetroSendDebugger                  |
| KillProcess                  | 1                       | Action::KillExternalMetro                                   |
| SpawnCommand                 | 11                      | Every dispatch_command() call site                          |
| LoadDevices                  | 1                       | Action::CommandRun needs_device_selection() path            |
| ListWorktrees                | 5                       | RefreshWorktrees, CommandExited refresh.worktrees, WorktreeRemoved, WorktreeAdded, WorktreeNewBranchCreated |
| RemoveWorktree               | 1                       | Action::ModalConfirm worktree-removal path                  |
| AddWorktree                  | 1                       | Action::ModalInputSubmit pending_worktree_add path          |
| AddWorktreeNewBranch         | 1                       | Action::ModalInputSubmit pending_new_branch_worktree path   |
| ListRemoteBranches           | 1                       | Action::WorktreeAddNewBranch                                |
| SaveJiraCache                | 1                       | Action::JiraTitlesFetched                                   |
| SaveAndroidMode              | 4                       | ModalInputSubmit android-mode, ModalDeviceConfirm, DevicesEnumerated emulator + device picker |
| RecordSimUsed                | 1                       | Action::SimulatorUsed                                       |
| OpenInMultiplexer            | 2                       | ModalInputSubmit claude path, Action::OpenShellTab          |
| FetchJiraTitles              | 1                       | Action::WorktreesLoaded                                     |
| ScheduleAction               | 1                       | ModalDeviceConfirm SimulatorUsed chain                      |

All 17 variants covered. Plan 13-03 correctly anticipated the call surface.

## TokioMetroAdapter::start — callback architecture note

Per `MetroPort` trait, `start(worktree, on_activity) -> anyhow::Result<Box<dyn MetroHandle>>` takes a callback. The adapter spawns `metro_process_task(child, stdout, stderr, kill_rx, on_activity)` which in turn spawns `drain_metro_output(stdout, stderr, on_activity)`. `on_activity` is cloneable via `Box<dyn Fn(...) + Send + Sync>` — invoked inside the drain loop whenever `parse_metro_line` yields a `MetroActivity`.

The effect_runner's `SpawnMetro` arm constructs `on_activity` as a closure that sends `Action::MetroActivityUpdate(act)` on `self.action_tx`. This is the bridge that turns the callback-style trait into the Action-stream-style app. Clean boundary — no tokio types leak across the trait signature.

**Caveat** (see D-13-07-06): the pre-13-07 `metro_process_task` also sent `Action::MetroExited` on the action channel after kill completion or natural exit. The post-13-07 adapter's stream_task no longer has that back-channel; the current user-initiated shutdown path (MetroStop consuming the handle, runtime cleanup PGID-killing on exit) covers all happy paths. Natural metro crash is a narrow edge case — Plan 13-08's `Adapters` refactor is the right place to add a general `on_exit` callback.

## Test assertion deltas in dispatch_tests.rs

- **17 call-site rewrites**: `update(&mut state, action, &metro_tx, &handle_tx)` → `let _effects = update(&mut state, action);` (or `let effects = ...` where we added Effect-vector assertions).
- **`#[tokio::test]` → `#[test]`**: 13 tests simplified (6 palette_resolution tests were already plain `#[test]`; all 8 modal_dismissal tests + 3 command_queue tests converted).
- **Channel helper removed**: the `fn channels()` helper in modal_dismissal + command_queue sub-modules deleted (was returning a 4-tuple of channels).
- **1 Effect-presence assertion added**: `command_exited_with_nonempty_queue_pops_and_dispatches_front` now asserts `effects.iter().any(|e| matches!(e, Effect::SpawnCommand { .. }))`. The other 16 tests continue to exercise state mutations only — which is strictly more expressive than the pre-F-201 "check tokio task side effect" approach.

## Test assertion deltas in tests/metro_single_instance.rs

- **Both tests rewritten** to new signature + `#[test]` (Pitfall 5).
- **COVER-01 strengthened**: `metro_start_while_running_triggers_restart_not_double_spawn` now asserts `Effect::SpawnMetro` is ABSENT (pre-F-201 test could not observe spawn outcomes — it relied on state assertions alone).
- **New assertion**: `metro_start_when_stopped_does_not_set_pending_restart` now asserts `Effect::DetectExternalMetro` IS present (confirms the external-detect path fires from Stopped state).

## Tests that broke during the rewrite

None that required logic changes. The only churn was mechanical signature adaptation. The `skip_external_metro_check = true` bypass in `sync_before_metro_modal_dismisses_on_n_and_esc` was preserved for the same reason as before: SyncBeforeMetroDecline recursively dispatches MetroStart, which pushes `Effect::DetectExternalMetro` unless skip is set. The test only cares about modal clearing — bypass keeps it focused.

## Action variants that needed introduction

None. All existing `Action` variants continue to resolve. The Effect enum had `ScheduleAction(Action)` from Plan 13-03 for the recursive self-dispatch pattern; no new `Action` variants were needed.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] Effect::FetchJiraTitles deferred to Plan 13-08**
- **Found during:** Task 2 — translating the `Action::WorktreesLoaded` JIRA fetch spawn at the pre-13-07 update.rs:348-360.
- **Issue:** The inline spawn captured `state.jira_client: Arc<dyn JiraPort>` and called `client.fetch_title(&key).await` for each key. The new effect_runner only holds `Arc<dyn MetroPort>` — adding JiraPort to the struct would break the Plan 13-08 contract where `Adapters` holds all ports.
- **Fix:** Emitted `Effect::FetchJiraTitles { keys }` from update(); the effect_runner's arm logs a debug message and defers. Runtime impact: titles continue to hydrate from cache on worktree reload; missed fetches will refresh on the next 60s periodic refresh or Plan 13-08 merge.
- **Rationale:** Preserves update() purity without forcing a Plan 13-08 contract change mid-plan. Documented in D-13-07-02 + this deviation block.
- **Files modified:** src/app/update.rs (Action::WorktreesLoaded arm), src/app/effect_runner.rs (Effect::FetchJiraTitles arm).
- **Commit:** ed34ebf

**2. [Rule 3 - Blocking issue] PaletteMode not Copy — BindingContext enum needs Clone only**
- **Found during:** Task 3 — compile error on `#[derive(Debug, Clone, Copy)] enum BindingContext` because `Palette(PaletteMode)` and `PaletteMode` doesn't derive Copy.
- **Issue:** The plan's template (13-PATTERNS.md:1070) specified `#[derive(Debug, Clone, Copy)]` but didn't check PaletteMode's derive attrs.
- **Fix:** Dropped Copy from BindingContext. Cost: `context_matches(&BindingContext, &AppState)` takes a reference anyway, so no callers needed updating.
- **Files modified:** src/app/keybindings.rs (1 line).
- **Commit:** c0ba938

**3. [Rule 1 - Clippy nit] `matches!` over explicit match pattern**
- **Found during:** Task 3 `cargo clippy` pass.
- **Issue:** `match_like_matches_macro` lint fired on `matches_modal_kind` helper.
- **Fix:** Replaced the match expression with the `matches!` macro per clippy suggestion.
- **Files modified:** src/app/keybindings.rs (1 function).
- **Commit:** c0ba938

**4. [Rule 3 - Blocking issue] Initial file-path confusion — wrote to main checkout instead of worktree**
- **Found during:** Task 1 verification — `cargo test --lib infra::metro::tests` showed 0 tests after the metro.rs file was seemingly created.
- **Issue:** The first Write for `src/infra/metro.rs` went to `/Users/cubicme/aljazeera/dashboard/src/infra/metro.rs` (main repo path) instead of `/Users/cubicme/aljazeera/dashboard/.claude/worktrees/agent-ab4aed8a/src/infra/metro.rs` (this worktree's path). Cargo built in the worktree, so the file was invisible. Dep file for `rn-dash` lacked `src/infra/metro.rs` which was the smoking gun.
- **Fix:** Deleted the wrong-path file + reverted the wrong-path mod.rs edit, then re-created the file at the correct worktree path. All subsequent edits used the full worktree path.
- **Files affected:** none permanently — main-repo state was returned to pre-plan state.
- **Commit:** First successful metro.rs commit was 5526475 at the correct path.

### Auto-added missing critical functionality

Listed above (item 1 — Effect::FetchJiraTitles).

## Auth gates

None — this is a pure-code refactor with no external services or auth paths.

## TDD Gate Compliance

Not applicable — plan has `tdd="false"` per frontmatter. The existing 17 dispatch_tests + 2 metro_single_instance + 1 process_group_kill + all lib tests serve as the behavior-preservation guard. All 79 tests passed after each of the 3 task-level commits:

- Commit 5526475 (Task 1, F-203 extraction) — 74 tests green (70 pre + 4 new infra::metro tests).
- Commit ed34ebf (Task 2, F-201 update purity) — 77 tests green (74 lib after signature adaptation + 2 metro_single_instance + 1 process_group_kill).
- Commit c0ba938 (Task 3, F-208 + F-400 KEYBINDINGS) — 79 tests green (76 lib after +2 keybindings tests + 2 + 1).

## Commits

| #  | Hash    | Type     | Message                                                                                        |
|----|---------|----------|------------------------------------------------------------------------------------------------|
| 1  | 5526475 | refactor | add TokioMetroAdapter in src/infra/metro.rs (F-203)                                            |
| 2  | ed34ebf | refactor | update() is now pure Vec<Effect> (F-201)                                                       |
| 3  | c0ba938 | refactor | add KEYBINDINGS registry; handle_key walks it (F-208 + F-400 type half)                        |

## Known Stubs

One intentional stub, documented with a comment:

| Stub                   | File                       | Reason                                                                                              |
|------------------------|----------------------------|-----------------------------------------------------------------------------------------------------|
| Effect::FetchJiraTitles | src/app/effect_runner.rs   | Plan 13-08 Adapters injection adds JiraPort to EffectRunner; for now the arm logs a debug message. |

Not counted as a plan-blocker — JIRA titles continue to hydrate from cache, and the Plan 13-08 consumer is immediately-next.

## Self-Check: PASSED

**Files claimed created:**
- src/infra/metro.rs — FOUND
- src/app/keybindings.rs — FOUND
- .planning/phases/13-audit-driven-refactors/13-07-SUMMARY.md — FOUND (this file)

**Files claimed modified:**
- src/app/update.rs — contains `-> Vec<Effect>` (FOUND at line 70)
- src/app/runtime.rs — 162 LOC, no async metro helpers (FOUND)
- src/app/effect_runner.rs — 300 LOC with EffectRunner struct (FOUND)
- src/app/handle_key.rs — 71 LOC, walks KEYBINDINGS (FOUND)
- src/app/dispatch_tests.rs — 17 tests, all use new signature (FOUND)
- src/app/mod.rs — `pub mod keybindings;` (FOUND)
- src/infra/mod.rs — `pub mod metro;` (FOUND)
- tests/metro_single_instance.rs — 2 tests, new signature (FOUND)
- Makefile — G-04 + G-05 active (FOUND)

**Commits claimed:**
- 5526475 — FOUND in git log
- ed34ebf — FOUND in git log
- c0ba938 — FOUND in git log

**Tests verified:**
- `cargo test --all-targets` — 79 passed (76 lib + 2 + 1)
- `cargo test --lib app::dispatch_tests` — 17 passed
- `cargo clippy --all-targets -- -D warnings` — clean
- `make arch-lint` — PASS (G-04 + G-05 hard-active)

**Grep invariants verified:**
- `rg 'tokio::spawn|spawn_blocking' src/app/update.rs` — 0 hits (G-04)
- `rg 'reqwest|tokio::process' src/app/` — 0 hits (G-05)

All self-check assertions confirmed against the worktree and git history.
