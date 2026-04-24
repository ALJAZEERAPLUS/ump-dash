---
phase: 13-audit-driven-refactors
plan: 05
subsystem: domain-ports + infra-adapter
tags: [refactor, domain, trait-def, infra-rewrite, wave-b, f-101-closed]
requires: [13-01, 13-03]
provides:
  - "crate::domain::ports::command_runner_port::{CommandRunnerPort, CommandEvent}"
  - "crate::infra::command_runner::TokioCommandRunner"
affects:
  - "src/app.rs::dispatch_command (inline CommandEvent→Action translation bridge; Plan 13-08 will relocate to effect_runner)"
tech_stack:
  added: []
  patterns:
    - "Typed-event port boundary (CommandEvent replaces Action at infra/domain seam — F-101)"
    - "Receiver-returning sync fn on trait (no #[async_trait]; tokio::spawn happens inside the fn body)"
key_files:
  created:
    - src/domain/ports/command_runner_port.rs
  modified:
    - src/domain/ports/mod.rs
    - src/infra/command_runner.rs
    - src/infra/mod.rs
    - src/app.rs
decisions:
  - id: no-legacy-shim
    decision: "Deleted `pub async fn spawn_command_task` outright rather than keeping a legacy shim."
    why: "The shim would still reference `crate::domain::action::Action` (either as `use` or fully-qualified path), which either re-triggers G-03 strict or weakens the F-101 closure. Inlining the translation at the single app-layer call site is the same line-count and produces a stronger end state."
  - id: exit-status-on-failure-paths
    decision: "Synthesize a failure ExitStatus (`from_raw(1 << 8)`) for the empty-argv and spawn-error paths, and emit `CommandEvent::Exited(status)` even when spawn fails."
    why: "Consumers treat the Exited event as the stream terminator. Suppressing it on failure would cause the `while let Some(ev) = rx.recv().await` loop in `dispatch_command` to hang on a never-closed channel after the error message line."
  - id: inline-bridge-at-app-boundary
    decision: "Translate `CommandEvent → Action` inline in `src/app.rs::dispatch_command` (single call site)."
    why: "Plan 13-05's scope is the F-101 trait + adapter. Plan 13-08 introduces Adapters injection + `effect_runner`, at which point the translation moves to its canonical home. Keeping the bridge at one well-commented site now (15 lines) is cheaper than an intermediate abstraction that 13-08 would delete."
metrics:
  tasks_completed: 1
  duration: "~20 min (single-task plan; no iteration needed)"
  commits: 1
  commit_hashes:
    - "1a9b1a3 refactor(13-05): add CommandRunnerPort + CommandEvent; rewrite infra/command_runner.rs; close F-101"
---

# Phase 13 Plan 05: F-101 Close — CommandRunnerPort + CommandEvent Summary

**One-liner:** Infra `command_runner.rs` no longer imports `Action`; it implements `CommandRunnerPort` and emits typed `CommandEvent { OutputLine, Exited }` — the F-101 Fowler violation (Data-Source layer coupled to Service-layer messaging grammar) is closed.

---

## What landed

### New trait + event type (domain layer)

`src/domain/ports/command_runner_port.rs` (61 lines, NEW):

- `pub enum CommandEvent { OutputLine(String), Exited(std::process::ExitStatus) }` — typed lifecycle events.
- `pub trait CommandRunnerPort: Send + Sync` with a single method:
  ```rust
  fn spawn(
      &self,
      spec: CommandSpec,
      cwd: PathBuf,
      branch: String,
  ) -> tokio::sync::mpsc::UnboundedReceiver<CommandEvent>;
  ```
- No `#[async_trait]` — the method is a plain `fn` returning the receiver synchronously; `tokio::spawn` happens inside the fn body.
- Registered in `src/domain/ports/mod.rs` alongside the seven existing ports.

### Adapter rewrite (infra layer)

`src/infra/command_runner.rs` (130 lines pre-rewrite → 181 lines post-rewrite):

- `pub struct TokioCommandRunner;` (unit struct, zero-sized) implements `CommandRunnerPort`.
- **Removed:** `use crate::domain::action::Action;` — the F-101 violation line.
- **Removed:** the legacy `pub async fn spawn_command_task(...)` entry point (no shim; app-layer translates).
- **Preserved VERBATIM** (confirmed by diff against pre-rewrite file):
  - `build_argv(&spec, &current_branch)` including the `GitResetHard` → `reset --hard origin/{current_branch}` special case.
  - `kill_on_drop(true)` on the spawned `tokio::process::Command` (line-for-line equivalent; now in `run_command`).
  - Concurrent stdout+stderr streaming via the `tokio::select!` loop (now in `stream_command_output`, type changed from `UnboundedSender<Action>` to `UnboundedSender<CommandEvent>`).
- **New:** `synthetic_failure_status()` emits an Exited event on the empty-argv / spawn-error / wait-error paths so downstream consumers always see a terminator.

### App-layer bridge

`src/app.rs::dispatch_command` (15 lines changed): previously called `spawn_command_task(spec, cwd, branch, tx).await`; now calls `TokioCommandRunner.spawn(...)` and runs a `while let Some(ev) = rx.recv().await` loop that translates:

| CommandEvent            | Action                      |
| ----------------------- | --------------------------- |
| `OutputLine(String)`    | `Action::CommandOutputLine` |
| `Exited(ExitStatus)`    | `Action::CommandExited`     |

The `ExitStatus` payload is discarded (`_status`) — `Action::CommandExited` has no payload today. Plan 13-08 may preserve the status when it rewires to `effect_runner`.

---

## Pre-rewrite `.process_group(0)` status

Per the plan's explicit verification request: **`src/infra/command_runner.rs` did NOT call `.process_group(0)` before this plan, and it does NOT call it after this plan.** This matches the pre-rewrite body (which only set `.kill_on_drop(true)`). The `.process_group(0)` invariant tested by `tests/process_group_kill.rs` is exercised through `tokio::process::Command` directly in the test body — not through `command_runner`. Therefore no new behavior was introduced.

Status quo documented for Plan 13-07/13-08: if process-group isolation ever becomes needed for user-dispatched commands, add `.process_group(0)` to `run_command` alongside `.kill_on_drop(true)`. The Phase 12 research flagged this as a known gap for user commands but deliberately out of scope for Phase 13's refactor-only mandate.

---

## `build_argv` preserved verbatim

Diff of `build_argv` body against the pre-rewrite file:

```
-fn build_argv(spec: &CommandSpec, current_branch: &str) -> Vec<String> {
-    match spec {
-        CommandSpec::GitResetHard => {
-            vec![
-                "git".into(),
-                "reset".into(),
-                "--hard".into(),
-                format!("origin/{current_branch}"),
-            ]
-        }
-        other => other.to_argv(),
-    }
-}
+fn build_argv(spec: &CommandSpec, current_branch: &str) -> Vec<String> {
+    match spec {
+        CommandSpec::GitResetHard => {
+            vec![
+                "git".into(),
+                "reset".into(),
+                "--hard".into(),
+                format!("origin/{current_branch}"),
+            ]
+        }
+        other => other.to_argv(),
+    }
+}
```

Identical — `GitResetHard` special case preserved.

---

## Verification

### Shape guards (G-03)

| Form                                                           | Result              |
| -------------------------------------------------------------- | ------------------- |
| `! rg 'use crate::(domain::)?action' src/infra/`               | **0 hits** (PASS)   |
| `! rg 'crate::action::Action' src/infra/` (exact F-101 form)   | **0 hits** (PASS)   |
| `! rg 'crate::domain::action::Action' src/infra/` (code refs)  | **0 code hits** (PASS — remaining matches are comments only) |

`make arch-lint` exits 0 (`arch-lint: PASS`).

### Test suite

`cargo test --all-targets`: **73 tests pass** across 4 binaries:

| Binary                                         | Tests | Result |
| ---------------------------------------------- | ----- | ------ |
| lib (`rn_dash`)                                | 70    | ok     |
| tests/common                                   | 0     | ok     |
| tests/metro_single_instance                    | 2     | ok     |
| tests/process_group_kill **(COVER-02)**        | 1     | ok     |

`cargo test --test process_group_kill` isolated run:
```
running 1 test
test killing_pgid_reaps_child_tree ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
```

PGID-kill invariant preserved — `kill_on_drop` + process-group broadcast both work. (Note: this test does not exercise `command_runner.rs` directly; it spawns `tokio::process::Command` inline. But it proves the underlying tokio facilities we depend on still behave correctly.)

### Lints

`cargo clippy --all-targets -- -D warnings` exits 0. No new warnings introduced.

---

## Temporary coupling: `src/app.rs::dispatch_command` is the translation home until Plan 13-08

Plan 13-08 (Adapters injection + `effect_runner`) will relocate the `CommandEvent → Action` translation loop out of `dispatch_command` into the canonical `effect_runner` boundary. Until then, `src/app.rs::dispatch_command` hosts the bridge at a single well-commented call site (15 lines). The comment block above the bridge explicitly names Plan 13-08 as its removal target so a future reader can trace the coupling.

This is **not** a deviation — it is the plan's explicit FINAL DECISION (line 206 of `13-05-PLAN.md`): "src/app.rs dispatch_command is rewritten to call `TokioCommandRunner::spawn` and translate CommandEvent → Action inline."

---

## Line diff summary

| File                                         | Before | After | Δ       |
| -------------------------------------------- | ------ | ----- | ------- |
| src/domain/ports/command_runner_port.rs      | 0      | 61    | +61 NEW |
| src/domain/ports/mod.rs                      | 16     | 17    | +1      |
| src/infra/command_runner.rs                  | 130    | 181   | +51     |
| src/infra/mod.rs                             | 18     | 22    | +4 (doc) |
| src/app.rs (`dispatch_command` only)         | —      | —     | ~+5 net (replaced 4 lines with ~13) |

Net: +120 lines across 5 files; **1 critical violation closed**.

---

## Deviations from Plan

None. The plan-prescribed FINAL DECISION (no shim; inline bridge in `dispatch_command`) was followed exactly.

One minor clean-up taken: updated the `//!`-level doc comment in `src/infra/mod.rs` that still said *"command_runner.rs still imports `crate::domain::action::Action` — Plan 13-05 removes this via CommandEvent per AUDIT F-101"*. That statement is now false after this plan, so the comment was rewritten to describe the post-closure state. This is an in-scope doc-sync, not a behavior change.

---

## Known Stubs

None. All scaffolding is wired: the trait has an implementation, the implementation has a caller, the caller handles every `CommandEvent` variant.

---

## Threat Flags

None. `threat_model_disposition: accept_refactor_only` in the plan frontmatter — no subprocess-security-relevant properties changed. `kill_on_drop(true)` preserved verbatim, argv construction preserved verbatim, `cwd` handling preserved verbatim.

---

## Self-Check: PASSED

**Files created:**
- FOUND: `src/domain/ports/command_runner_port.rs`

**Commit present:**
- FOUND: `1a9b1a3` (refactor(13-05): add CommandRunnerPort + CommandEvent; rewrite infra/command_runner.rs; close F-101)

**Acceptance criteria (from plan):**
- [x] `pub trait CommandRunnerPort` in `src/domain/ports/command_runner_port.rs`
- [x] `pub enum CommandEvent` in same file
- [x] `pub struct TokioCommandRunner` in `src/infra/command_runner.rs`
- [x] `impl CommandRunnerPort for TokioCommandRunner` in same file
- [x] G-03 strict (`! rg 'use crate::(domain::)?action' src/infra/`): 0 hits
- [x] G-03 exact (`! rg 'crate::action::Action' src/infra/`): 0 hits
- [x] `cargo test --test process_group_kill` passes (COVER-02 invariant)
- [x] `cargo test --all-targets` exits 0 (73 tests green)
- [x] `cargo clippy --all-targets -- -D warnings` exits 0
- [x] `make arch-lint` exits 0
- [x] `build_argv` body preserved VERBATIM (diff above)
- [x] `kill_on_drop(true)` still called at Command construction site
- [x] `.process_group(0)` status: not present before, not added after — matches pre-rewrite
- [x] Legacy `pub async fn spawn_command_task` deleted (0 hits in `src/infra/command_runner.rs`)

F-101 — one of the two **Critical** findings from the 11-01 audit — is closed. Wave B (plans 13-04 + 13-05) now has all new ports + their production adapters in place, ready for Wave C (Plans 13-06 → 13-10) to rewire consumers.
