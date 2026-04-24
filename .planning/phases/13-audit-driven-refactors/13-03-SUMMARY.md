---
phase: 13-audit-driven-refactors
plan: 03
subsystem: domain-types + app-effects + metro-port
tags: [refactor, domain, types, trait-def, tdd, wave-a]
requirements: [REFACTOR-01, REFACTOR-03]
dependency_graph:
  requires:
    - "13-01 (Action + trait relocation to domain)"
    - "src/domain/command::CommandSpec + CleanOptions (pre-existing)"
    - "src/domain/metro::MetroActivity (pre-existing)"
  provides:
    - "crate::app::effect::Effect — 17-variant effect grammar (F-201 type half)"
    - "crate::app::effect::DeviceKind — stub enum (Plan 13-04 relocates)"
    - "crate::domain::pipeline::{Recipe, Prerequisite, DependencyState}"
    - "crate::domain::pipeline::Recipe::expand(&DependencyState) -> Vec<CommandSpec>"
    - "CommandSpec::prerequisites() -> Vec<Prerequisite>"
    - "crate::domain::ports::metro_port::{MetroHandle, MetroPort} traits"
    - "crate::domain::metro::MetroHandle (re-export of the trait)"
    - "InAppMetroHandle bridge impl in src/app.rs (temporary — removed by 13-07)"
  affects:
    - "Plan 13-07 can rewrite update() to return Vec<Effect>"
    - "Plan 13-09 can replace 11 inline prereq sites with Recipe::expand()"
    - "Plan 13-07 can drop InAppMetroHandle and implement TokioMetroAdapter"
tech_stack:
  added: []
  patterns:
    - "pub use re-export (src/domain/metro::MetroHandle → ports::metro_port::MetroHandle)"
    - "Opaque trait object in Option<Box<dyn Trait>> for single-instance invariant"
    - "Consuming trait method (fn kill(self: Box<Self>)) for object-safe shutdown"
    - "Callback-driven port signature (Box<dyn Fn(MetroActivity) + Send + Sync>) — no tokio leak"
    - "pub mod submodule alongside flat src/app.rs (bin+lib crate compatibility)"
key_files:
  created:
    - path: "src/app/effect.rs"
      purpose: "F-201 type half — Effect enum, 17 variants, every tokio::spawn call site mapped"
    - path: "src/domain/pipeline.rs"
      purpose: "F-204 + REFACTOR-03 type half — Recipe/Prerequisite/DependencyState + expand()"
    - path: "src/domain/ports/metro_port.rs"
      purpose: "F-203 trait half + F-004 opaque handle — MetroPort + MetroHandle traits"
    - path: ".planning/phases/13-audit-driven-refactors/13-03-SUMMARY.md"
      purpose: "this file"
  modified:
    - path: "src/app.rs"
      reason: "pub mod effect; + InAppMetroHandle bridge + handle_tx type + kill() call sites + update() signature"
    - path: "src/app/dispatch_tests.rs"
      reason: "channel type updated to Box<dyn MetroHandle>; #[allow(clippy::type_complexity)] on channels() helpers"
    - path: "src/domain/metro.rs"
      reason: "delete concrete MetroHandle struct; re-export trait; MetroManager.handle → Option<Box<dyn MetroHandle>>; DummyHandle trait impl in tests"
    - path: "src/domain/mod.rs"
      reason: "pub mod pipeline;"
    - path: "src/domain/ports/mod.rs"
      reason: "pub mod metro_port;"
    - path: "tests/common/mod.rs"
      reason: "fake_metro_handle returns Box<dyn MetroHandle> via FakeMetroHandle impl"
decisions:
  - "Keep src/app.rs as a flat file with `pub mod effect;` declaration; src/app/effect.rs is a sibling file. The pattern already works for `mod dispatch_tests;` in the same file."
  - "InAppMetroHandle bridge lives in src/app.rs (not infra/) for this plan — Plan 13-07 moves it to src/infra/metro.rs as TokioMetroAdapter when the full Effect consumer lands."
  - "MetroPort::detect_external intentionally omitted — belongs to PortProbePort (Plan 13-04) per AUDIT F-102."
  - "on_activity is a Box<dyn Fn(MetroActivity) + Send + Sync> callback, not an UnboundedSender — keeps the trait signature tokio-free (13-RESEARCH.md Pitfall 8 / Open Question Q3 resolution)."
  - "register_twice panic message preserved verbatim: `BUG: MetroManager::register() called with an existing handle — kill first`. Characterization test still triggers."
  - "Effect includes ScheduleAction(Action) variant — absorbs F-206 recursive self-dispatch pattern per plan interface spec."
metrics:
  duration_minutes: 9
  tasks_completed: 2
  tests_added: 18   # 2 effect + 16 pipeline
  tests_total_before: 49   # 46 lib + 2 integration + 1 doc
  tests_total_after: 67    # 64 lib + 2 integration + 1 doc
  files_created: 4
  files_modified: 6
  completed: 2026-04-24T10:38:23Z
---

# Phase 13 Plan 03: Domain Types for Effect + Recipe + MetroPort Summary

Defined the three type clusters that Wave C consumers (Plans 13-07, 13-09)
will wire into `update()` and `effect_runner.rs` — the `Effect` effect-grammar
enum (17 variants covering every current `tokio::spawn` call site), the
`Recipe` + `Prerequisite` + `DependencyState` pipeline trio with a pure
`Recipe::expand` orchestrator, and the `MetroPort` + opaque `MetroHandle`
traits that close audit finding F-004 (no more tokio types leaking through
domain `pub` fields). No consumers are rewired yet — this is a types-only
plan — and all 49 pre-13-03 tests continue to pass. A temporary
`InAppMetroHandle` bridge lives in `src/app.rs` so `spawn_metro_task`
compiles until Plan 13-07's `TokioMetroAdapter` replaces it.

## Plan-Level TDD Gate Compliance

Plan type is `execute` (not `tdd`), but the two tasks were TDD-driven and the
git log shows the conventional gate sequence:

1. `test(13-03): add failing tests for Effect + Recipe types` — RED
   (commit `d60a6a8`). 16 pipeline tests + 2 effect tests added; pipeline
   compile errors confirm RED (Recipe/Prerequisite/DependencyState + 
   `CommandSpec::prerequisites()` undefined).
2. `feat(13-03): add Effect enum + Recipe/Prerequisite/DependencyState …` —
   GREEN (commit `ffb115b`). Types + impl filled in; all 18 new tests pass;
   49 pre-plan tests unchanged.
3. `refactor(13-03): convert MetroHandle to trait object; add MetroPort trait` —
   REFACTOR (commit `57b1f6c`). The type migration is structurally a refactor:
   no new behavior, existing characterization tests (register_twice panic,
   metro_single_instance) still pass. A dedicated RED phase was not added
   because the test IS "existing tests must stay green under the trait swap"
   — the migration itself is the verification.

## Effect Variants + Provenance (F-201)

17 variants populate `src/app/effect.rs`. Each maps to one or more current
`tokio::spawn` / `tokio::spawn_blocking` call sites in src/app.rs:

| Variant                    | Fields                                                | Replaces spawn at src/app.rs line(s)     |
|----------------------------|-------------------------------------------------------|-------------------------------------------|
| DetectExternalMetro        | `port: u16`                                           | 602                                       |
| SpawnMetro                 | `worktree: PathBuf`                                   | 619 (wrapped spawn_metro_task)            |
| MetroHttpPost              | `url: String, body: String`                           | 636, 649                                  |
| KillProcess                | `pid: u32`                                            | 709                                       |
| SpawnCommand               | `spec: CommandSpec, cwd: PathBuf, branch: String`     | 524 (dispatch_command helper)             |
| LoadDevices                | `kind: DeviceKind`                                    | 929                                       |
| ListWorktrees              | (unit)                                                | 817, 993, 1863, 1903, 2042, 2107          |
| RemoveWorktree             | `path: PathBuf`                                       | 1101                                      |
| AddWorktree                | `branch: String`                                      | 1205                                      |
| AddWorktreeNewBranch       | `new: String, base: String`                           | 1186                                      |
| ListRemoteBranches         | (unit)                                                | 1928                                      |
| SaveJiraCache              | `HashMap<String, String>`                             | 1564                                      |
| SaveAndroidMode            | `String`                                              | 1170, 1339, 1362, 1392, 1413              |
| RecordSimUsed              | `String`                                              | 1678                                      |
| OpenInMultiplexer          | `worktree: PathBuf, name: String, command: String`    | 1236, 1548                                |
| FetchJiraTitles            | `keys: Vec<String>`                                   | 708, 794                                  |
| ScheduleAction             | `crate::domain::action::Action`                       | 7+ recursive update(state, next_action, …) sites (absorbs F-206) |

Invariants:
- Every variant is plain data — no closures, no `Box<dyn Fn>`, no tokio handles.
  (Verified by `grep -v '//' src/app/effect.rs | grep -c 'Box<dyn Fn'` → 0.)
- `DeviceKind` is defined as a local stub in `effect.rs` with a delete-me TODO;
  Plan 13-04 introduces `crate::domain::ports::device_port::DeviceKind` and the
  local stub is replaced by an import.
- Effect does NOT include `detect_external` — that call site is covered by
  `DetectExternalMetro { port }` which the PortProbePort adapter will handle
  in Plan 13-07/13-04.

## Recipe Tests (F-204 + REFACTOR-03)

16 inline tests in `src/domain/pipeline.rs::tests`. Coverage per Recipe variant:

| Test                                                    | Variant covered                  | Scenario                           |
|---------------------------------------------------------|----------------------------------|------------------------------------|
| `test_single_expands_to_one_spec`                       | `Single`                         | pass-through                       |
| `test_sequence_preserves_order`                         | `Sequence`                       | order-preservation                 |
| `test_clean_all_options_expands_to_four_plus_sync`      | `Clean`                          | all toggles on                     |
| `test_clean_none_expands_to_empty`                      | `Clean`                          | all toggles off (empty)            |
| `test_sync_then_run_stale_ios_adds_yarn_and_pods`       | `SyncThenRun`                    | iOS target, both stale             |
| `test_sync_then_run_stale_android_only_yarn`            | `SyncThenRun`                    | Android target skips pods          |
| `test_sync_then_run_fresh_passes_through`               | `SyncThenRun`                    | fresh deps — no sync prefix        |
| `test_sync_then_start_metro_stale_adds_both`            | `SyncThenStartMetro`             | stale — both yarn + pods           |
| `test_sync_then_start_metro_fresh_is_empty`             | `SyncThenStartMetro`             | fresh — empty (dispatcher follows) |
| `test_release_build_and_install_expands_to_two`         | `ReleaseBuildAndInstall`         | 2-step sequence                    |
| `test_git_fetch_then_reset_expands_to_two`              | `GitFetchThenReset`              | 2-step sequence                    |
| `test_prerequisites_rn_run_android_needs_metro`         | `CommandSpec::prerequisites`     | Android run → MetroRunning         |
| `test_prerequisites_rn_run_ios_needs_metro`             | `CommandSpec::prerequisites`     | iOS run → MetroRunning             |
| `test_prerequisites_rn_release_build_needs_metro`       | `CommandSpec::prerequisites`     | Release build → MetroRunning       |
| `test_prerequisites_yarn_install_no_prereq`             | `CommandSpec::prerequisites`     | yarn install → empty               |
| `test_prerequisites_git_fetch_no_prereq`                | `CommandSpec::prerequisites`     | git fetch → empty                  |

All 16 pass. `Recipe::expand` is a pure fn — no tokio, no I/O — so these
run as plain `#[test]` (no runtime).

## MetroHandle struct → trait migration (F-203 + F-004)

Before (`src/domain/metro.rs:54-76`):
```rust
pub struct MetroHandle {
    pub pid: u32,
    pub worktree_id: String,
    pub stdin_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    pub stream_task: tokio::task::JoinHandle<()>,
    pub stdin_task: tokio::task::JoinHandle<()>,
    pub kill_tx: Option<tokio::sync::oneshot::Sender<()>>,
}
```

After (`src/domain/ports/metro_port.rs`):
```rust
pub trait MetroHandle: Send + Sync + std::fmt::Debug {
    fn pid(&self) -> u32;
    fn worktree_id(&self) -> &str;
    fn send_stdin(&self, bytes: Vec<u8>) -> anyhow::Result<()>;
    fn kill(self: Box<Self>) -> anyhow::Result<()>;
}
```

`src/domain/metro.rs` re-exports the trait via
`pub use crate::domain::ports::metro_port::MetroHandle;`, so every existing
`use crate::domain::metro::MetroHandle;` keeps resolving. `MetroManager.handle`
is now `Option<Box<dyn MetroHandle>>`; `register/take_handle` signatures
updated to take/return the trait object.

### InAppMetroHandle bridge (src/app.rs)

Required because `spawn_metro_task` (unchanged by plan scope) still owns the
tokio channels. A local `#[derive(Debug)] struct InAppMetroHandle` holds the
same 4 tokio fields and implements `MetroHandle`. The trait's consuming
`kill(self: Box<Self>)` method performs `kill_tx.send(())` + `stream_task.abort()`
+ `stdin_task.abort()` — the exact sequence that previously lived inline at
`src/app.rs:633-636` (MetroStop) and `:2198-2201` (shutdown cleanup).

Plan 13-07 will:
1. Delete `InAppMetroHandle` from `src/app.rs`.
2. Create `src/infra/metro.rs::TokioMetroAdapter` + `TokioMetroHandle`.
3. Move `spawn_metro_task`, `metro_process_task`, `stdin_writer`,
   `parse_metro_line` into `src/infra/metro.rs`.

### Call-site updates

| Site                                    | Before                                             | After                                                |
|-----------------------------------------|----------------------------------------------------|------------------------------------------------------|
| `src/app.rs:11` (use)                   | `use crate::domain::metro::MetroHandle;`           | unchanged (re-export)                                |
| `src/app.rs:549` (update sig)           | `&UnboundedSender<MetroHandle>`                    | `&UnboundedSender<Box<dyn MetroHandle>>`             |
| `src/app.rs:629-637` (MetroStop)        | `handle.kill_tx.take() + stdin_task.abort()`       | `handle.kill()`                                      |
| `src/app.rs:2083` (channel decl)        | `unbounded_channel::<MetroHandle>()`               | `unbounded_channel::<Box<dyn MetroHandle>>()`        |
| `src/app.rs:~2229` (spawn_metro_task)   | `UnboundedSender<MetroHandle>`                     | `UnboundedSender<Box<dyn MetroHandle>>`              |
| `src/app.rs:~2261` (struct literal)     | `MetroHandle { ... }`                              | `Box::new(InAppMetroHandle { ... }) as Box<dyn MetroHandle>` |
| `src/app.rs:~2192-2203` (shutdown)      | direct field access                                | `handle.pid()` + `handle.kill()` + outer PGID kill   |
| `src/app/dispatch_tests.rs:327-328, 520-521` | `UnboundedSender<MetroHandle>`                 | `UnboundedSender<Box<dyn MetroHandle>>`              |
| `tests/common/mod.rs:17-28`             | struct literal construction                        | `Box::new(FakeMetroHandle { .. })`                   |
| `src/domain/metro.rs:182-193` (tests)   | struct literal dummy_handle                        | `Box<dyn MetroHandle>` via `DummyHandle` trait impl  |

### register_twice panic message — preserved verbatim

Assertion unchanged: `"BUG: MetroManager::register() called with an existing handle — kill first"`.
Characterization test `register_twice_panics` still triggers `#[should_panic]`.

Note: the inline tests in `src/domain/metro.rs::tests` lost their `#[tokio::test]`
annotations — since `DummyHandle` has no tokio channels, plain `#[test]` suffices.
The `#[tokio::test]` → `#[test]` change is benign: tokio runtime is no longer required
for these tests to run. The `should_panic` semantics are preserved (panic on
double-register still fires in the synchronous test context).

## Module-resolution compilation note

Per plan Step 1 of Task 1, the bin+lib crate layout permits `src/app.rs`
(flat file) and `src/app/` (directory with submodule files) to coexist.
Prior evidence: `mod dispatch_tests;` at the bottom of `src/app.rs` + the
existing `src/app/dispatch_tests.rs` file already worked before this plan.
Adding `pub mod effect;` near the top of `src/app.rs` + `src/app/effect.rs`
compiled first try — no fallback inlining needed.

Plan 13-06 performs the F-200 split that finally moves `src/app.rs` content
into `src/app/mod.rs` plus peer modules (`state.rs`, `update.rs`, `handle_key.rs`,
`runtime.rs`, `effect_runner.rs`, `adapters.rs`). This plan intentionally did
NOT do that — types-only scope.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — Bug] Worktree branch base mismatch at startup**
- **Found during:** pre-task-1 worktree_branch_check
- **Issue:** Orchestrator prompt provided expected base commit
  `5a8645ace2f02727c45ae7057a1a29edac685dce`, which does not exist in
  the repository. A different commit with an identical 7-character prefix —
  `5a8645a9c44317e3c00f8994b4f6ece916c41783` — exists and matches the
  expected description ("docs(phase-13): update tracking after wave 1
  (13-01 merged)"). The prompt hash appears to be a transcription error
  or transient pre-merge hash.
- **Fix:** Identified the correct commit via `git log --oneline`, then
  `git reset --hard 5a8645a9c44317e3c00f8994b4f6ece916c41783`. This matched
  the intended wave-1-merged baseline.
- **Files modified:** none (worktree state only)
- **Commit:** n/a (pre-commit operation)

**2. [Rule 2 — Missing critical lint allow] clippy::type_complexity on test channel helpers**
- **Found during:** Task 2 post-implementation clippy pass
- **Issue:** `fn channels()` in two test sub-modules returns a 4-tuple with
  two `UnboundedSender<Box<dyn crate::domain::metro::MetroHandle>>` and two
  receivers. Clippy flags this as `type_complexity` under `-D warnings`.
- **Fix:** Added `#[allow(clippy::type_complexity)]` to both helpers (plan
  did not mention it because pre-plan the tuple was simpler).
- **Files modified:** src/app/dispatch_tests.rs (2 lines)
- **Commit:** 57b1f6c

**3. [Rule 1 — Bug] tokio::test attribute on pure-sync tests**
- **Found during:** Task 2 metro.rs::tests migration
- **Issue:** The two `register_*` tests were `#[tokio::test]` because the
  original `dummy_handle` helper constructed tokio channels. Post-trait-swap
  DummyHandle is synchronous — no runtime required — so keeping
  `#[tokio::test]` would be dead overhead.
- **Fix:** Replaced `#[tokio::test]` with `#[test]`. Panic semantics of
  `should_panic` preserved. Test behavior identical.
- **Files modified:** src/domain/metro.rs (tests module)
- **Commit:** 57b1f6c

### make arch-lint — not yet wired

Plan's `<verify>` block references `make arch-lint`. No such Makefile target
exists yet (Makefile only has coverage targets). Other plans (13-04, 13-05,
13-06, 13-07) reference it as prospective. This plan therefore cannot invoke
it. The manual guard greps that `arch-lint` would run were executed inline
and all passed:

- G-08: `grep 'pub enum Recipe' src/domain/pipeline.rs` → present
- G-08: `grep 'pub enum Prerequisite' src/domain/pipeline.rs` → present
- G-09: `grep 'pub enum Effect' src/app/effect.rs` → present (17 variants)
- G-16: `grep 'stdin_tx: tokio::sync' src/domain/metro.rs` → 0 hits
- G-17: `grep 'pub trait MetroPort' src/domain/ports/metro_port.rs` → present

Future plan-level work: add the `arch-lint` Makefile target with these greps
codified (likely in Plan 13-06 or 13-07 per the reference pattern).

## Authentication Gates

None — this is a pure-code refactor with no external services or auth paths.

## Commits

| # | Hash    | Type     | Message                                                                                        |
|---|---------|----------|------------------------------------------------------------------------------------------------|
| 1 | d60a6a8 | test     | add failing tests for Effect + Recipe types                                                    |
| 2 | ffb115b | feat     | add Effect enum + Recipe/Prerequisite/DependencyState per F-201/F-204/REFACTOR-03              |
| 3 | 57b1f6c | refactor | convert MetroHandle to trait object; add MetroPort trait per F-203 + F-004                     |

## Verification Evidence

```
cargo test --all-targets --quiet → 64 + 0 + 2 + 1 = 67 passed; 0 failed
cargo clippy --all-targets -- -D warnings → Finished (clean)
cargo test --lib domain::metro::tests → 3 passed (includes register_twice should_panic)
cargo test --test metro_single_instance → 2 passed
cargo test --test process_group_kill → 1 passed
grep 'pub trait MetroHandle' src/domain/ports/metro_port.rs → 1 hit
grep 'pub trait MetroPort' src/domain/ports/metro_port.rs → 1 hit
grep 'stdin_tx: tokio::sync' src/domain/metro.rs → 0 hits (G-16 passes)
grep 'pub struct MetroHandle' src/domain/metro.rs → 0 hits
grep 'pub enum Effect' src/app/effect.rs → 1 hit (G-09 passes)
grep 'pub enum Recipe' src/domain/pipeline.rs → 1 hit (G-08 passes)
grep 'pub enum Prerequisite' src/domain/pipeline.rs → 1 hit (G-08 passes)
grep 'Box::new(FakeMetroHandle' tests/common/mod.rs → 1 hit
```

## Known Stubs

One intentional stub, documented with a delete-me comment:

| Stub        | File              | Reason                                                                                       |
|-------------|-------------------|----------------------------------------------------------------------------------------------|
| DeviceKind  | src/app/effect.rs | Plan 13-04 introduces `crate::domain::ports::device_port::DeviceKind`; local stub will be replaced by an import at that time. |

Not counted as a plan-blocker — consumers of `Effect::LoadDevices` don't
exist until Plan 13-07; the stub has exactly one use site (the `Effect` enum
variant) and its replacement is scheduled in the immediate-next plan.

## Self-Check: PASSED

- Files claimed created:
  - `src/app/effect.rs` → FOUND
  - `src/domain/pipeline.rs` → FOUND
  - `src/domain/ports/metro_port.rs` → FOUND
  - `.planning/phases/13-audit-driven-refactors/13-03-SUMMARY.md` → FOUND (this file)

- Commits claimed:
  - `d60a6a8` → FOUND (`git log --oneline` shows it)
  - `ffb115b` → FOUND
  - `57b1f6c` → FOUND

- Tests claimed green: `cargo test --all-targets --quiet` shows 64 + 2 + 1 = 67 passes, 0 failures.
- Clippy claimed clean: `cargo clippy --all-targets -- -D warnings` exits 0.
