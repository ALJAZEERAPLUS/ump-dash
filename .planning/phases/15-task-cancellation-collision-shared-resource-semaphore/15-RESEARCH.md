# Phase 15: Task Cancellation + Collision + Shared-Resource Semaphore - Research

**Researched:** 2026-05-18
**Domain:** Tokio async process management, POSIX signal delivery, collision policy, shared-resource serialization
**Confidence:** HIGH

## Summary

Phase 15 builds directly on the per-worktree task map and `TaskHandle` port from Phase 14.
Three capabilities must land: (1) real OS-level cancellation with SIGTERM→200ms→SIGKILL escalation
to the FULL process group (not just the immediate child), (2) a per-`(CommandSpec discriminant,
WorktreeId)` collision policy that blocks-new for idempotent installs and cancels-previous for
builds/tests, and (3) a `tokio::sync::Semaphore(1)` keyed by repo-root `PathBuf` that serializes
concurrent yarn installs sharing the same global cache.

The biggest structural gap found during research: `infra/command_runner.rs` does NOT currently
call `.process_group(0)` on spawned processes (confirmed by COVER-02 test comment and code
inspection). Adding `.process_group(0)` is a PREREQUISITE for SIGTERM PGID broadcast to work.
Additionally, `TokioTaskHandle` currently wraps only a `JoinHandle<()>` with no access to the
child PID. Phase 15 must extend the handle with a child PID so the SIGTERM PGID call can be made.

The child PID communication path uses a new `CommandEvent::ProcessStarted { pid: u32 }` event
emitted as the FIRST event from `run_command()`. `effect_runner`'s `SpawnTask` arm reads that
first event, constructs a fully-armed `TokioTaskHandle { join_handle, child_pid }`, and sends it
via `task_handle_tx`. No domain port signature changes required.

`tokio::sync::Semaphore` is already available (tokio "full" feature is active). `tokio-util` is
already a transitive dep in `Cargo.lock` at version 0.7.18 and needs only to be promoted to a
direct dep for `tokio_util::sync::CancellationToken`. No new crates need to be introduced.

**Primary recommendation:** Add `CommandEvent::ProcessStarted { pid: u32 }`, extend
`TokioTaskHandle` with `child_pid: u32`, wire SIGTERM+grace+SIGKILL escalation inside
`TaskHandle::abort()`, add collision policy as a domain predicate on `CommandSpec`, and host
the yarn semaphore map in `EffectRunner`.

---

## Project Constraints (from CLAUDE.md)

- Architecture: Rust + Ratatui, domain/infra/app/ui separation, Ousterhout deep-modules philosophy
- `check-types` always uses `--incremental` flag
- YOLO mode at workflow gates — no confirmation needed
- Branch labels are per-branch, not per-worktree

---

<user_constraints>
## User Constraints (from upstream phases)

### Locked Decisions (carried forward from Phase 14 CONTEXT.md)

- **D-01**: `WorktreeSlice` has 6 fields: `id, task, queue, output, output_scroll, post_drain`. Phase 15 adds `cancel_token: Option<CancellationToken>` per CONTEXT.md D-01 note — but given the research finding that the cancel signal is better embedded in `TokioTaskHandle` (infra layer), this field addition is an OPEN DECISION (see Open Questions Q-1).
- **D-03**: `TaskHandle` is a domain port trait (`fn abort(&self)`). Phase 15 WIDENS this trait to include OS-level kill. The widening must remain infra-free in the domain port definition.
- **D-05**: `CommandKind` = `std::mem::discriminant(&spec)` — two `YarnInstall` invocations collide; `Jest { filter: A }` and `Jest { filter: B }` collide. Collision identity: `(Discriminant<CommandSpec>, WorktreeId)`.
- **G-04/G-05**: `update()` purity preserved — zero tokio/infra imports in `src/app/update.rs`.
- **G-21**: Guard banning re-introduction of deleted Phase-14 fields stays active.
- **COVER-02** (`tests/process_group_kill.rs`) is READ-ONLY — Phase 15 writes NEW tests, does NOT modify this file.
- No `mockall` / `rstest` / `proptest` — no new test-only dev-deps.

### Deferred (Phase 14 CONTEXT.md §Deferred, still out of scope for Phase 15)
- Live UI indicators (spinner, elapsed) — Phase 16
- Task history persistence — future milestone
- Cross-worktree Recipe targeting — not needed
- F-111 PersistencePort — deferred to backlog
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| TASK-04 | Individual task cancellation: `CancellationToken` + SIGTERM to PGID + SIGKILL grace + `kill_on_drop(true)`. Git-porcelain commands non-cancellable via `is_cancellable()`. | §Key Findings F1–F4: process_group(0) gap found, CommandEvent::ProcessStarted approach, CancellationToken wiring, SIGTERM→200ms→SIGKILL ladder |
| TASK-05 | Collision policy: `(CommandKind, WorktreeId)` match blocks-new for idempotent installs, cancels-previous for builds/tests. | §Key Findings F5–F6: discriminant-based identity, per-category policy enum, dispatch_command integration point |
| TASK-06 | Shared-resource semaphore: `tokio::sync::Semaphore(1)` keyed by repo-root `PathBuf`; concurrent yarn installs must not corrupt `node_modules`. | §Key Findings F7–F8: Semaphore in tokio "full", EffectRunner ownership, canonicalization, commands in scope |
</phase_requirements>

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| SIGTERM/SIGKILL escalation | Infra (`infra/task_handle.rs`) | — | POSIX syscall via `libc`; domain stays signal-free |
| CancellationToken signaling | Infra (`infra/task_handle.rs`, `infra/command_runner.rs`) | App (effect_runner creates token) | tokio-util type; domain port exposes only `abort()` |
| `is_cancellable()` gate | Domain (`domain/command.rs`) | App (update.rs CommandCancel handler) | Type-driven predicate, already exists from Phase 13 |
| Collision policy predicate | Domain (`domain/command.rs`) | App (dispatch_command gate) | Pure enum logic, no I/O |
| Collision enforcement | App (`app/update.rs` dispatch_command) | — | Reads state, emits Effect; stays in TEA reducer |
| Yarn semaphore map | App/Infra boundary (`app/effect_runner.rs`) | — | Owns async permit acquisition; EffectRunner is the spawn chokepoint |
| Child PID extraction | Infra (`infra/command_runner.rs` via CommandEvent) | App (effect_runner reads first event) | PID is an OS concern; communicated via existing event channel |
| Repo-root semaphore key | App (SpawnTask payload carries `repo_root`) | Infra (effect_runner canonicalizes it) | Derived from `state.app_config.repo_root` at dispatch time |

---

## Standard Stack

### Core (no new crates needed for TASK-04/TASK-05)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tokio` | 1.49.0 (already direct dep, "full") | `tokio::sync::Semaphore`, `tokio::time::sleep`, `tokio::select!` | Already in use; "full" feature includes sync |
| `libc` | 0.2.182 (already direct dep) | `libc::kill(-pgid, SIGTERM)`, `libc::kill(-pgid, SIGKILL)`, `libc::SIGTERM`, `libc::SIGKILL` | Already used in `infra/metro.rs:164` for PGID kill |
| `tokio-util` | 0.7.18 (currently TRANSITIVE; promote to direct) | `tokio_util::sync::CancellationToken` | Already in Cargo.lock; promotion avoids version conflict |

[VERIFIED: Cargo.lock] `tokio 1.49.0`, `libc 0.2.182`, `tokio-util 0.7.18` all present.
[VERIFIED: infra/metro.rs:164] `libc::kill(-(id as i32), libc::SIGKILL)` pattern exists and works.
[VERIFIED: tests/process_group_kill.rs] SIGTERM→PGID→2s timeout test passes; proves the mechanism.
[ASSUMED] `tokio::process::Command::process_group()` stable since tokio 1.34.0 per Phase 12 research.

### TASK-06 Semaphore (no new crates)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tokio::sync::Semaphore` | part of tokio 1.49 | Serialize yarn installs by repo-root | In tokio "sync" feature (included via "full") |
| `std::sync::Arc` | std | Share semaphore across async tasks | Standard Rust |
| `std::collections::HashMap` | std | Map repo-root PathBuf → Arc<Semaphore> | No concurrent mutation needed (EffectRunner is !Send across spawn) |

**Installation (only change to Cargo.toml):**
```toml
tokio-util = { version = "0.7", features = [] }  # promotes transitive dep; CancellationToken is available without feature flags
```

`tokio-util`'s `sync` module containing `CancellationToken` has no required feature flags in 0.7.18 — confirmed via docs.rs/crate/tokio-util/0.7.18/features (0 default features; no "sync" entry).
[VERIFIED: docs.rs/crate/tokio-util/0.7.18/features] 14 feature flags, none required for `tokio_util::sync::CancellationToken`.

---

## Package Legitimacy Audit

> `slopcheck` was not available at research time. All packages are well-established crates
> from the tokio-rs GitHub organization (https://github.com/tokio-rs/tokio), with 561M+
> downloads. No new crates are introduced — only promotion of an existing transitive dep.

| Package | Registry | Age | Downloads | Source Repo | slopcheck | Disposition |
|---------|----------|-----|-----------|-------------|-----------|-------------|
| `tokio-util` | crates.io | ~6 yrs | 561M+ | github.com/tokio-rs/tokio | N/A (unavailable) | [VERIFIED: crates.io API] Approved — official tokio-rs crate, same repo as tokio |

**Packages removed due to slopcheck [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

*slopcheck was unavailable at research time. Both `tokio-util` origin (`tokio-rs/tokio`) and
download count (561M+) confirm legitimacy independently. No checkpoint required.*

---

## Architecture Patterns

### System Architecture Diagram

```
User presses cancel key
        │
        ▼
handle_key → Action::CommandCancel
        │
        ▼
update() CommandCancel handler ─── is_cancellable()=false ──→ no-op (git variants)
        │ is_cancellable()=true
        ▼
slice.task.take() → TaskRecord { handle: Box<dyn TaskHandle>, .. }
        │
        ▼
handle.abort()   [domain trait call]
        │
        ▼ [infra/task_handle.rs TokioTaskHandle::abort()]
        ├── libc::kill(-child_pid, SIGTERM)        ← PGID broadcast to process tree
        ├── tokio::spawn async { sleep(200ms); libc::kill(-child_pid, SIGKILL) }
        ├── cancel_token.cancel()                  ← signals forwarding task to stop
        └── join_handle.abort()                    ← stops stdout/stderr reader (belt+suspenders)
                │
                ▼
        child.wait() returns → CommandExited { task_id, status: Cancelled } sent to action_tx


User triggers second YarnInstall on same WorktreeId (collision path)
        │
        ▼
dispatch_command() ──── collision_policy(spec) = BlockNew ──→ enqueue or silently drop
                    └── collision_policy(spec) = CancelPrevious ──→ cancel previous + dispatch


SpawnTask effect (yarn install, semaphore path)
        │
        ▼
effect_runner SpawnTask arm
        ├── For YarnInstall|YarnPodInstall|RmNodeModules:
        │       get_or_create_semaphore(repo_root) → Arc<Semaphore>
        │       semaphore.acquire_owned().await    ← blocks if concurrent install running
        │       spawn task { run_command; drop permit on exit }
        └── For all others: spawn without semaphore
```

### Recommended Project Structure (changes from Phase 14 baseline)

```
src/
├── domain/
│   ├── command.rs         # ADD: collision_policy() predicate + CollisionPolicy enum
│   └── ports/
│       └── task_handle.rs # WIDEN: abort() documented as SIGTERM+SIGKILL (no new methods)
├── infra/
│   ├── command_runner.rs  # ADD: .process_group(0) + CommandEvent::ProcessStarted { pid }
│   └── task_handle.rs     # EXTEND: TokioTaskHandle adds child_pid: u32 + full abort() impl
└── app/
    ├── effect.rs          # ADD: repo_root: PathBuf to SpawnTask payload
    ├── effect_runner.rs   # ADD: yarn_semaphores field + semaphore acquire in SpawnTask arm
    └── update.rs          # MODIFY: dispatch_command adds collision gate; CommandCancel consults is_cancellable()
```

### Pattern 1: SIGTERM → Grace Period → SIGKILL (mirrors metro kill)

**What:** Two-phase OS-level process group kill. SIGTERM gives the process tree a chance to
flush buffers and exit cleanly. SIGKILL is the hard backstop after 200ms.

**When to use:** Any time a process tree must be fully reaped (not just the immediate child).
Required because `yarn` spawns `node` children that hold the workspace lock.

**Example:**
```rust
// Source: infra/metro.rs:159-164 (existing PGID kill pattern, adapted)
// In TokioTaskHandle::abort():
let pid = self.child_pid as i32;
// SAFETY: child_pid > 1 (validated at construction); sending to our own process group.
unsafe { libc::kill(-pid, libc::SIGTERM); }
// Grace: spawn a cleanup task that sends SIGKILL after 200ms
tokio::spawn(async move {
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    unsafe { libc::kill(-pid, libc::SIGKILL); }
});
// Stop the stdout/stderr forwarding task
self.cancel_token.cancel();
self.join_handle.abort();
```

### Pattern 2: CancellationToken Wiring in SpawnTask Forwarding Loop

**What:** The `effect_runner`'s per-task spawned async block races event forwarding against
the `CancellationToken`. When cancelled, the forwarding loop exits cleanly; `kill_on_drop`
backstops if the forwarding loop exits before the child.

**When to use:** Every cancellable task spawned via `Effect::SpawnTask`.

**Example:**
```rust
// Source: tokio_util::sync::CancellationToken docs (verified via docs.rs)
// In effect_runner.rs SpawnTask arm:
let token = tokio_util::sync::CancellationToken::new();
let token_clone = token.clone();  // for TokioTaskHandle storage
// ...
let join_handle = tokio::spawn(async move {
    // First event must be ProcessStarted { pid }
    let child_pid = match rx.recv().await {
        Some(CommandEvent::ProcessStarted { pid }) => pid,
        _ => return,  // spawn failed, no pid available
    };
    loop {
        tokio::select! {
            maybe_ev = rx.recv() => {
                match maybe_ev {
                    Some(CommandEvent::OutputLine(line)) => {
                        let _ = tx.send(Action::CommandOutputLine { task_id, line });
                    }
                    Some(CommandEvent::Exited(status)) => {
                        let _ = tx.send(Action::CommandExited {
                            task_id,
                            status: ExitStatus::from(status),
                        });
                        break;
                    }
                    Some(CommandEvent::ProcessStarted { .. }) => {}  // ignore (already read)
                    None => break,
                }
            }
            _ = token_clone.cancelled() => {
                // SIGTERM+grace+SIGKILL already sent by TokioTaskHandle::abort()
                let _ = tx.send(Action::CommandExited {
                    task_id,
                    status: ExitStatus::Cancelled,
                });
                break;
            }
        }
    }
    // child_pid used to construct TokioTaskHandle — send via oneshot or
    // deliver via a separate channel alongside the record
});
```

**Note on child_pid delivery:** The forwarding task reads `child_pid` from the first
`CommandEvent::ProcessStarted { pid }`. This pid must be available to construct
`TokioTaskHandle` BEFORE sending via `task_handle_tx`. Two implementation options:

- **Option A (simpler):** Read the first event SYNCHRONOUSLY before `tokio::spawn` by
  making the first `rx.recv()` blocking (use `try_recv()` in a spin or a brief
  `block_on`). Not recommended — blocks the effect runner.
- **Option B (recommended):** Spawn the forwarding task, have it send `child_pid` via an
  `oneshot::Sender<u32>`, wait on the `oneshot::Receiver` in a separate tiny task, then
  assemble `TokioTaskHandle` once the pid arrives. `task_handle_tx` send is deferred
  until pid is known.

[ASSUMED] The oneshot-pid approach adds ~1 async hop but avoids blocking the event loop.
Planner should choose at planning time.

### Pattern 3: Collision Policy as Domain Predicate

**What:** `CommandSpec::collision_policy() -> CollisionPolicy` — a pure domain predicate
that returns the behavior when a new task collides with an existing one on the same
`(discriminant, WorktreeId)`.

**When to use:** `dispatch_command()` checks this before pushing `Effect::SpawnTask`.

**Example:**
```rust
// Source: domain/command.rs (new, following is_cancellable() pattern from 13-02-SUMMARY.md)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionPolicy {
    /// Block the new dispatch — the existing task keeps running.
    /// Used for idempotent installs (running a second yarn install while one is
    /// in progress produces the same result; no point in double-running).
    BlockNew,
    /// Cancel the existing task, then dispatch the new one.
    /// Used for builds/tests where the user intent is "run THIS version NOW".
    CancelPrevious,
}

impl CommandSpec {
    pub fn collision_policy(&self) -> CollisionPolicy {
        match self {
            // Idempotent installs: block new if already running
            CommandSpec::YarnInstall
            | CommandSpec::YarnPodInstall => CollisionPolicy::BlockNew,
            // Builds/tests/runs: cancel previous, run new
            CommandSpec::YarnUnitTests
            | CommandSpec::YarnJest { .. }
            | CommandSpec::YarnLint
            | CommandSpec::YarnCheckTypes
            | CommandSpec::RnRunAndroid { .. }
            | CommandSpec::RnRunIos { .. }
            | CommandSpec::RnRunIosDevice
            | CommandSpec::RnReleaseBuild
            | CommandSpec::AdbInstallApk
            | CommandSpec::ShellCommand { .. } => CollisionPolicy::CancelPrevious,
            // Clean operations: cancel previous (user wants fresh clean state)
            CommandSpec::RnCleanAndroid
            | CommandSpec::RnCleanCocoapods
            | CommandSpec::RmNodeModules => CollisionPolicy::CancelPrevious,
            // Git variants: non-cancellable + block new (data integrity)
            _ => CollisionPolicy::BlockNew,
        }
    }
}
```

### Pattern 4: Per-Repo-Root Yarn Semaphore in EffectRunner

**What:** `Arc<Mutex<HashMap<PathBuf, Arc<Semaphore>>>>` owned by `EffectRunner`. For
commands that write `node_modules` or the yarn global cache, acquire a permit keyed by the
canonicalized `repo_root` before spawning the process.

**When to use:** `YarnInstall`, `YarnPodInstall`, `RmNodeModules` commands in `SpawnTask`.

**Example:**
```rust
// Source: tokio docs + std HashMap pattern (training knowledge, [ASSUMED] for exact API calls)
// EffectRunner field:
pub yarn_semaphores: Arc<std::sync::Mutex<HashMap<PathBuf, Arc<tokio::sync::Semaphore>>>>,

// In SpawnTask arm, for yarn-family commands:
let semaphore = {
    let mut map = self.yarn_semaphores.lock().unwrap();
    map.entry(repo_root.clone())
       .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(1)))
       .clone()
};
// semaphore is an Arc<Semaphore> — clone it into the spawned async block
let join_handle = tokio::spawn(async move {
    let _permit = semaphore.acquire_owned().await.expect("semaphore not closed");
    // run the command; _permit drops when block exits
    // ...
});
```

**Canonicalization:** Call `repo_root.canonicalize()` before using as map key to resolve
symlinks and trailing-slash differences. Fall back to the raw path if `canonicalize()` fails
(NFS or missing-dir edge case).

### Anti-Patterns to Avoid

- **Sending SIGKILL immediately without SIGTERM:** Prevents clean shutdown of yarn/node children
  (yarn may leave lock files). Always SIGTERM first with 200ms grace.
- **Calling `libc::kill(-1, SIGTERM)`:** Would broadcast to ALL processes of this UID. Guard
  with `assert!(pgid > 1)` as in COVER-02.
- **Putting CancellationToken in WorktreeSlice (domain layer):** `tokio_util` types are infra.
  Keep `cancel_token` inside `TokioTaskHandle` in `infra/task_handle.rs`.
- **Holding Mutex<HashMap> lock across await points:** The yarn semaphore lock must be released
  before the `acquire_owned().await`. Use a scope block to drop the MutexGuard, then await.
- **Using discriminant comparison for collision in update():** `std::mem::discriminant` is
  correct for this use case (two `Jest { filter: A }` vs `Jest { filter: B }` should still
  collide — D-05 intent). Do NOT compare full spec equality.
- **Skipping `is_cancellable()` check in CommandCancel handler:** The current Phase 14
  `CommandCancel` handler does NOT check `is_cancellable()` — it calls `abort()` on whatever
  task is running. Phase 15 must add the `is_cancellable()` guard: if the running task's spec
  has `is_cancellable() = false`, `CommandCancel` is a no-op.
- **Forgetting to add `.process_group(0)` to command_runner.rs:** This is THE critical missing
  piece. Without it, `libc::kill(-pgid, SIGTERM)` hits a non-existent group and the child
  survives. COVER-02 spawns directly — it does NOT test through command_runner.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Process group kill | Custom `waitpid` loop | `libc::kill(-pgid, SIGTERM/SIGKILL)` | Already proven in COVER-02; one syscall |
| Cancellation signaling | Custom channel type | `tokio_util::sync::CancellationToken` | Clone semantics, `child_token()` for composition, already in Cargo.lock |
| Concurrent-access serialization | `Arc<Mutex<Option<running: bool>>>` | `tokio::sync::Semaphore(1)` | Permits are RAII, auto-released on drop/panic; Mutex<bool> has no timeout semantics |
| Orphan detection | polling `/proc/{pid}/status` | `libc::kill(-pgid, 0)` (probe signal) | Returns `ESRCH` when group empty; used in COVER-02 |
| 200ms grace timer | `std::thread::sleep` | `tokio::time::sleep(Duration::from_millis(200))` | Non-blocking; project is all-tokio |

**Key insight:** The SIGTERM→SIGKILL ladder is well-trodden POSIX territory. The only project-specific concern is correctness of PGID targeting after `.process_group(0)`.

---

## Key Findings

### F1: command_runner.rs Missing `.process_group(0)` — Critical Gap

[VERIFIED: src/infra/command_runner.rs:71-87] `TokioCommandRunner::run_command()` spawns with
`.kill_on_drop(true)` but WITHOUT `.process_group(0)`. This means `kill_on_drop` sends SIGKILL
to the direct child only, leaving Node.js grandchildren orphaned.

**Fix required:** Add `.process_group(0)` before `.spawn()` in `command_runner.rs`. This is the
same flag used in `infra/process.rs:27` (metro spawn) and verified by COVER-02 to work on
macOS and Linux via `tokio::process::Command::process_group()`.

`tokio::process::Command::process_group()` is stable since tokio 1.34. We use 1.49.
[ASSUMED: tokio 1.34 stabilization date — per Phase 12 research doc which cited docs.rs/tokio/1.49.0]

### F2: Child PID Must Flow via CommandEvent::ProcessStarted

[VERIFIED: src/infra/command_runner.rs:71-100, src/app/effect_runner.rs:337-372]
`TokioTaskHandle` currently holds only `JoinHandle<()>`. The child PID (needed for
`libc::kill(-pgid, ...)`) is buried inside `run_command()`'s local scope.

**Recommended approach:** Add `CommandEvent::ProcessStarted { pid: u32 }` emitted as the
FIRST event (after successful `cmd.spawn()`, before any `OutputLine`). `effect_runner`'s
`SpawnTask` forwarding task reads this first event, stores the pid, then constructs
`TokioTaskHandle { join_handle, child_pid: pid, cancel_token }` before sending via
`task_handle_tx`.

No domain port changes needed. `CommandRunnerPort::spawn()` signature unchanged.

### F3: TokioTaskHandle Widens to {join_handle, child_pid, cancel_token}

[VERIFIED: src/infra/task_handle.rs] Currently: `TokioTaskHandle(pub tokio::task::JoinHandle<()>)`.

Phase 15 shape:
```rust
pub struct TokioTaskHandle {
    pub join_handle: tokio::task::JoinHandle<()>,
    pub child_pid: u32,          // for libc::kill(-child_pid, SIGTERM/SIGKILL)
    pub cancel_token: tokio_util::sync::CancellationToken,  // signals forwarding loop
}
```

`TaskHandle::abort()` implementation widens from `self.join_handle.abort()` to:
1. `libc::kill(-child_pid, SIGTERM)` — PGID broadcast
2. `tokio::spawn` — cleanup task: `sleep(200ms)` then `libc::kill(-child_pid, SIGKILL)`
3. `self.cancel_token.cancel()` — signals forwarding loop to emit `ExitStatus::Cancelled`
4. `self.join_handle.abort()` — belt-and-suspenders

The domain `TaskHandle` trait does NOT change its surface (still just `fn abort(&self)`).
[VERIFIED: src/domain/ports/task_handle.rs] Trait has single `fn abort(&self)` method.

### F4: ExitStatus::Killed vs ExitStatus::Cancelled

[VERIFIED: src/infra/task_handle.rs:32-41] Phase 14's `From<std::process::ExitStatus>` already
notes "Phase 15 widens to detect signals via `ExitStatusExt::signal()`".

When SIGKILL fires, the process exits with `std::process::ExitStatus.code() = None` and
`ExitStatusExt::signal() = Some(9)`. The `From` impl should be widened:
```rust
// std::os::unix::process::ExitStatusExt  (already imported in command_runner.rs:167)
if status.success() { ExitStatus::Success }
else if let Some(signal) = status.signal() {
    if signal == libc::SIGKILL as i32 { ExitStatus::Killed }
    else { ExitStatus::Cancelled }  // SIGTERM exits here when process handles it
} else { ExitStatus::Failure { code: status.code() } }
```

When the cancel path fires before the process exits (forwarding loop cancelled first),
emit `ExitStatus::Cancelled` directly without waiting for the OS exit.

### F5: Collision Identity = `(std::mem::discriminant(&spec), worktree_id)` — Already Decided

[VERIFIED: .planning/phases/14-per-worktree-task-system-foundation/14-CONTEXT.md:D-05]
"The Phase 15 collision identity will be `(std::mem::discriminant(&spec), worktree_id)`."

The check lives in `dispatch_command()` in `update.rs`. Before emitting `Effect::SpawnTask`:
```rust
let existing = task_for_worktree(state, &wt_id);
if let Some(existing_task) = existing {
    if std::mem::discriminant(&existing_task.spec) == std::mem::discriminant(&spec) {
        match spec.collision_policy() {
            CollisionPolicy::BlockNew => return None,  // drop silently
            CollisionPolicy::CancelPrevious => {
                // cancel existing — mirrors CommandCancel logic
                let slice = state.worktrees.get_mut(&wt_id).unwrap();
                if let Some(record) = slice.task.take() {
                    record.handle.abort();
                }
                slice.queue.clear();
                slice.output.push_back("[cancelled by new dispatch]".into());
            }
        }
    }
}
```
[ASSUMED] Exact placement and signature for the collision gate in dispatch_command — planner
confirms against current dispatch_command shape.

### F6: `collision_policy()` is a New Domain Predicate on CommandSpec

[VERIFIED: src/domain/command.rs] `is_cancellable()` at line 125 is the direct template.
`collision_policy()` follows identical placement: `impl CommandSpec`, after `is_cancellable()`.
[VERIFIED: 13-02-SUMMARY.md] `is_cancellable()` was added with 6 inline tests + TDD (RED→GREEN).
Same discipline applies to `collision_policy()`.

Policy table (planner may adjust but this is the documented default):
- `BlockNew`: `YarnInstall`, `YarnPodInstall`, all git variants
- `CancelPrevious`: yarn quality (`YarnUnitTests`, `YarnJest`, `YarnLint`, `YarnCheckTypes`),
  RN runs (`RnRunAndroid`, `RnRunIos`, `RnRunIosDevice`), `RnReleaseBuild`, `AdbInstallApk`,
  `ShellCommand`, clean ops (`RnCleanAndroid`, `RnCleanCocoapods`, `RmNodeModules`)

### F7: tokio::sync::Semaphore Already Available (No New Dep)

[VERIFIED: Cargo.toml] `tokio = { version = "1.49", features = ["full"] }`. The "full" feature
includes "sync" which includes `tokio::sync::Semaphore`. No new dependency needed for TASK-06.

`Semaphore::acquire_owned()` returns `OwnedSemaphorePermit` which is `Send` — safe to move
into `tokio::spawn(async move { ... })` closures. [ASSUMED: acquire_owned() is the correct
method for use inside async move closures — standard tokio Semaphore pattern.]

### F8: Semaphore Map Ownership and Repo-Root Key

[VERIFIED: src/app/effect_runner.rs:53-71, src/app/runtime.rs:49]
`EffectRunner` is constructed once in `runtime.rs:49` and held by the event loop. The semaphore
map can be a field on `EffectRunner` without `Arc<>` wrapping at the top level:
```rust
// EffectRunner gains:
pub yarn_semaphores: std::sync::Mutex<HashMap<PathBuf, Arc<tokio::sync::Semaphore>>>,
```
Inside `run_one()` (sync fn), lock the mutex briefly, clone or create the `Arc<Semaphore>`,
drop the lock, then pass the `Arc` into the `tokio::spawn` closure.

**Repo-root key:** `Effect::SpawnTask` payload gains `repo_root: PathBuf` (same pattern as
`ListWorktrees { repo_root }`, `RemoveWorktree { repo_root }`, etc.). `dispatch_command()` sets
`repo_root: state.app_config.repo_root.clone()`. [VERIFIED: src/app/effect.rs:46-52] This
pattern already exists for worktree-management effects.

**Commands gated by semaphore:** `YarnInstall`, `YarnPodInstall`, `RmNodeModules`. All three
write to (or delete) `node_modules` in the same worktree directory. Serializing these prevents
yarn integrity hash corruption.

---

## Common Pitfalls

### Pitfall 1: SIGTERM to Non-Existent Process Group (ESRCH)
**What goes wrong:** `libc::kill(-pgid, SIGTERM)` returns -1 with errno `ESRCH` if the child
already exited between the cancel call and the kill. The kill is a no-op — this is safe.
**Why it happens:** Process can exit naturally between the cancel decision and the syscall.
**How to avoid:** Ignore the return value of the kill syscall. The grace-period SIGKILL is also
safe to send to a dead group (same ESRCH, no-op).
**Warning signs:** Test hangs if ESRCH is treated as an error.

### Pitfall 2: kill_on_drop Without process_group(0) Only Kills Immediate Child
**What goes wrong:** JoinHandle abort triggers `kill_on_drop` → SIGKILL to child PID. But the
child's children (Node workers spawned by yarn) belong to the SAME original process group as
the parent, not a new one. They survive.
**Why it happens:** Without `.process_group(0)` the child inherits the parent's PGID. SIGKILL
to PID alone kills only that one process.
**How to avoid:** MUST add `.process_group(0)` to `command_runner.rs` spawn. Verify with a
new integration test that forks a grandchild (like COVER-02 but via full stack).
**Warning signs:** `ps aux | grep sleep` shows leftover `sleep 60` processes after cancel.

### Pitfall 3: libc::kill(-1, SIGTERM) = Kill All Processes of This UID
**What goes wrong:** If `child_pid` is somehow 0 or 1, `kill(-pgid)` with pgid=0 means
"kill my own group" and pgid=1 means "kill init".
**Why it happens:** Race between process exit and PID reuse; or bug in ProcessStarted { pid }.
**How to avoid:** Validate `child_pid > 1` before every `libc::kill(-child_pid, ...)` call.
Assert in tests. [VERIFIED: tests/process_group_kill.rs:56] COVER-02 already does this.

### Pitfall 4: Mutex Held Across Await Point (Compile Error)
**What goes wrong:** Holding `std::sync::MutexGuard` across an `.await` point is a compile
error (MutexGuard is !Send, and the closure must be Send for tokio::spawn).
**Why it happens:** Code written as `let permit = map.lock().unwrap().get(key).acquire().await`.
**How to avoid:** Separate the lock scope from the await:
```rust
let semaphore = { let map = self.yarn_semaphores.lock().unwrap(); map.get(key).clone() };
let _permit = semaphore.acquire_owned().await;
```

### Pitfall 5: CommandCancel Without is_cancellable() Gate
**What goes wrong:** Phase 14's `CommandCancel` handler calls `record.handle.abort()` regardless
of the task's spec. After Phase 15 widens `abort()` to SIGTERM+SIGKILL, this would send SIGTERM
to a running `git rebase` or `git push`, potentially corrupting the repository.
**Why it happens:** Phase 14 left the gate as a TODO (Phase 15 concern).
**How to avoid:** Add `if record.spec.is_cancellable()` guard in `CommandCancel` handler before
calling `handle.abort()`. Non-cancellable tasks: clear `slice.output` cancel message, re-insert
the task record.
**Warning signs:** `git rebase` interrupted mid-operation; dangling MERGE_HEAD files.

### Pitfall 6: Stale CommandExited After Cancellation (Late PID Race)
**What goes wrong:** After SIGTERM, the child sends a few more stdout lines, then the process
exits, then `CommandEvent::Exited(status)` arrives. The forwarding loop may have already sent
`ExitStatus::Cancelled` to action_tx. Now a second `CommandExited` action arrives.
**Why it happens:** The cancel path and the natural exit path both send `CommandExited`.
**How to avoid:** The Phase 14 D-08 stale-task drop already handles this: `CommandExited` handler
looks up the slice by `task_id`; if `slice.task` is already `None` (taken by the cancel), the
second `CommandExited` is silently dropped. No new code needed — verify in test.

### Pitfall 7: Semaphore Never Released on Cancel
**What goes wrong:** If the spawned task is aborted (JoinHandle::abort()) while holding a
semaphore permit, the permit is dropped (Rust Drop runs on JoinHandle abort if the task panics
or is aborted). This is CORRECT behavior — `OwnedSemaphorePermit` implements `Drop` and releases
the permit even on task cancellation.
**Why it happens:** Not a bug if `OwnedSemaphorePermit` is used (Send + Drop-safe).
**How to avoid:** Use `acquire_owned()` not `acquire()`. The owned permit can be held across
await points and is safely dropped when the async block unwinds.

---

## Code Examples

### Verified Patterns from Existing Code

#### Existing PGID Kill (metro)
```rust
// Source: src/infra/metro.rs:159-164
// Kill the entire process group. process_group(0) in spawn makes
// the child the group leader (PID == PGID). Sending SIGKILL to -PGID
// terminates every member of the group.
unsafe { libc::kill(-(id as i32), libc::SIGKILL); }
```

#### Existing process_group(0) Spawn (metro, NOT command_runner yet)
```rust
// Source: src/infra/process.rs:27-30
.process_group(0)    // CRITICAL: sets PGID = child PID
.kill_on_drop(true)  // safety net
```

#### Existing is_cancellable() Predicate Pattern (template for collision_policy)
```rust
// Source: src/domain/command.rs:125-137
pub fn is_cancellable(&self) -> bool {
    !matches!(
        self,
        CommandSpec::GitResetHard | CommandSpec::GitResetHardFetch | ...
    )
}
```

#### Existing COVER-02 PGID Probe (validate kill worked)
```rust
// Source: tests/process_group_kill.rs
let probe = unsafe { libc::kill(-pgid, 0) };
if probe == -1 { break; }  // ESRCH — group empty
```

#### Existing ExitStatusExt Import (ready for Phase 15 widening)
```rust
// Source: src/infra/command_runner.rs:167
use std::os::unix::process::ExitStatusExt;
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `JoinHandle::abort()` only (Phase 14 TaskHandle) | SIGTERM+200ms+SIGKILL to PGID (Phase 15) | Phase 15 | Full subprocess tree reaped, not just tokio task |
| `kill_on_drop` only in command_runner | `process_group(0)` + `kill_on_drop` | Phase 15 (fix the gap) | Grandchildren die on cancel |
| No collision detection (Phase 14) | Per-spec CollisionPolicy gate in dispatch_command | Phase 15 | Prevents double-yarn-install corruption |
| No yarn serialization (Phase 14) | tokio::sync::Semaphore(1) per repo-root | Phase 15 | Prevents .yarn-integrity corruption |

**Deprecated/outdated after Phase 15:**
- Phase 14's `CommandCancel` handler calling `abort()` without `is_cancellable()` check — replace with guarded version.
- Phase 14's `From<std::process::ExitStatus>` only mapping success/failure — widen to detect SIGKILL → `ExitStatus::Killed`.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `tokio::process::Command::process_group()` stable since tokio 1.34 | F1, Standard Stack | Low risk — Phase 12 research cited docs.rs for this, project uses 1.49 |
| A2 | `tokio_util::sync::CancellationToken` has no required feature flags in 0.7.18 | Standard Stack | Low risk — verified via docs.rs feature page (0 default features, no "sync" entry required) |
| A3 | `Semaphore::acquire_owned()` is correct for async move closures (vs acquire()) | Pattern 4 | Low risk — standard tokio pattern; `OwnedSemaphorePermit` is `Send` |
| A4 | `oneshot::Sender<u32>` approach for pid delivery adds only ~1 frame of latency | F2 | Low risk — alternative (ProcessStarted event) is equally viable; planner decides |
| A5 | CollisionPolicy categories (BlockNew vs CancelPrevious) match product intent | F6 | Medium risk — user-observable behavior; planner should confirm with user if unsure |
| A6 | `RmNodeModules` should be gated by the yarn semaphore | F8 | Medium risk — `rm -rf node_modules` runs concurrently with `yarn install` could be safe OR could corrupt yarn cache state; semaphore is conservative |

**If this table is empty:** N/A — A1–A6 above require user/planner confirmation for A5–A6.

---

## Open Questions

1. **Q-1: `cancel_token` in WorktreeSlice vs. in TokioTaskHandle only**
   - What we know: Phase 14 CONTEXT.md D-01 says "Phase 15 adds `cancel_token: Option<CancellationToken>` to WorktreeSlice"
   - What's unclear: Keeping it in the slice requires `tokio_util` in the domain layer (violates G-05 spirit). Keeping it ONLY in `TokioTaskHandle` (infra) is architecturally cleaner.
   - Recommendation: Keep `cancel_token` ONLY in `TokioTaskHandle` (infra). No domain/slice change needed — the cancel signal flows through `TaskHandle::abort()`. Update D-01 note in CONTEXT/PATTERNS.

2. **Q-2: Should collision policy be enforced in the queue too?**
   - What we know: `dispatch_command()` enforces on the RUNNING task. But what if spec X is queued (not yet running) and user dispatches another X?
   - What's unclear: Should the queue check apply too?
   - Recommendation: Phase 15 scope is "task whose identity matches one already RUNNING." Queue deduplication is out of scope (not in TASK-05). Keep it simple.

3. **Q-3: Grace period duration — 200ms hardcoded or configurable?**
   - What we know: ROADMAP success criterion says "within 2 seconds." 200ms grace is mentioned in REQUIREMENTS.
   - What's unclear: Should the duration be a const, configurable, or per-command?
   - Recommendation: `const CANCEL_GRACE_MS: u64 = 200` in `infra/task_handle.rs`. No config needed.

4. **Q-4: Should `git` variants get CollisionPolicy::BlockNew or are they irrelevant?**
   - What we know: Git variants are non-cancellable (REFACTOR-02). The cancel path is a no-op for them.
   - What's unclear: If a second `GitPull` fires while one is running, block-new is still the right policy (don't double-pull).
   - Recommendation: Git variants → `CollisionPolicy::BlockNew`. Non-cancellable variants can never be "cancel previous," so BlockNew is the only valid policy for them.

---

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `libc` crate | SIGTERM/SIGKILL syscalls | ✓ | 0.2.182 | — |
| `tokio` "full" | Semaphore, sleep, select! | ✓ | 1.49.0 | — |
| `tokio-util` | CancellationToken | ✓ (transitive → promote) | 0.7.18 | oneshot::channel<()> as simpler alternative |
| `cargo test` | Validation | ✓ | 99 tests passing | — |
| `make arch-lint` | G-22/G-23 guards | ✓ | 21 guards active | — |
| macOS + Linux | POSIX SIGTERM/SIGKILL | ✓ | macOS 25.3.0 (Darwin) | — |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** `tokio-util` has fallback (`oneshot` channel) if promotion fails.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in + `#[tokio::test]` via tokio dev-dep |
| Config file | none (cargo-native) |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| TASK-04 | Cancelling a running task terminates full process group within 2s | integration | `cargo test --test process_group_cancel` | ❌ Wave 0 |
| TASK-04 | Git-porcelain CommandCancel is a no-op | unit | `cargo test --lib is_cancellable_gate` | ❌ Wave 0 |
| TASK-04 | ExitStatus::Cancelled emitted on cancel path | unit | `cargo test --lib cancelled_status_emitted` | ❌ Wave 0 |
| TASK-04 | ExitStatus::Killed emitted when SIGKILL exits | unit | `cargo test --lib killed_status_from_sigkill` | ❌ Wave 0 |
| TASK-05 | BlockNew: second YarnInstall on same worktree → no-op | unit (dispatch_tests.rs) | `cargo test --lib collision_block_new` | ❌ Wave 0 |
| TASK-05 | CancelPrevious: second Jest on same worktree → cancels first | unit (dispatch_tests.rs) | `cargo test --lib collision_cancel_previous` | ❌ Wave 0 |
| TASK-06 | Concurrent YarnInstall on two worktrees with same repo-root serializes | integration | `cargo test --test yarn_semaphore_serializes` | ❌ Wave 0 |

**Critical test notes:**

- TASK-04 integration test (`tests/process_group_cancel.rs`) MUST NOT modify `tests/process_group_kill.rs` (D-22). New file only.
- The orphan-detection pattern from COVER-02 applies: spawn bash with `sleep 60 & echo $!; wait`, parse the sleep PID from the first OutputLine, cancel, then probe with `libc::kill(-pgid, 0)` → ESRCH.
- TASK-06 yarn semaphore integration test does NOT actually run `yarn install` (too slow). Instead, substitute a slow fixture command (`bash -c 'sleep 0.5; echo done'`) and assert the SECOND task starts AFTER the first completes. Timing assertion: second task's started_at >= first task's started_at + 450ms.

### Sampling Rate
- **Per task commit:** `cargo test --lib --quiet`
- **Per wave merge:** `cargo test --workspace && make arch-lint`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `tests/process_group_cancel.rs` — covers TASK-04 end-to-end orphan kill
- [ ] `tests/yarn_semaphore_serializes.rs` — covers TASK-06 serialization
- [ ] Inline test: `fn collision_block_new` in `src/app/dispatch_tests.rs`
- [ ] Inline test: `fn collision_cancel_previous` in `src/app/dispatch_tests.rs`
- [ ] Inline test: `fn is_cancellable_gate_in_cancel_handler` in `src/app/dispatch_tests.rs`
- [ ] Unit tests for `CommandSpec::collision_policy()` in `src/domain/command.rs` (TDD pattern)

---

## Security Domain

> Phase adds process kill syscalls. ASVS V5 (input validation of PIDs) and general
> process control safety apply.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes | Validate `child_pid > 1` before `libc::kill` |
| V6 Cryptography | no | — |

### Known Threat Patterns

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| `kill(-1, SIGTERM)` broadcast | Tampering (DoS of unrelated processes) | Validate `pid > 1` before every `libc::kill(-pid, ...)` call; assert in tests |
| Semaphore never released (deadlock) | Denial of service | Use `OwnedSemaphorePermit` (`Send` + `Drop`); released on task abort/panic |
| PID reuse between ProcessStarted and kill | Elevation | Acceptable TOCTOU in this context (process trees are short-lived, pid reuse window tiny); no mitigation needed |

---

## Sources

### Primary (HIGH confidence)
- `src/infra/metro.rs:159-164` — PGID SIGKILL pattern in production code
- `src/infra/process.rs:23-30` — `process_group(0)` + `kill_on_drop` in production code
- `src/domain/command.rs:125-137` — `is_cancellable()` predicate pattern
- `src/app/effect_runner.rs:337-372` — `SpawnTask` arm (current baseline to extend)
- `tests/process_group_kill.rs` — COVER-02 full PGID kill integration test
- `src/infra/command_runner.rs:71-87` — confirmed ABSENCE of `process_group(0)` (the gap)
- `Cargo.toml` + `Cargo.lock` — confirmed versions and transitive deps
- `.planning/phases/14-per-worktree-task-system-foundation/14-CONTEXT.md` D-01..D-23 — locked Phase 14 decisions

### Secondary (MEDIUM confidence)
- `docs.rs/tokio-util/0.7.18/tokio_util/sync/struct.CancellationToken` — CancellationToken API verified via WebFetch
- `docs.rs/crate/tokio-util/0.7.18/features` — confirmed no required feature flags via WebFetch
- `crates.io/api/v1/crates/tokio-util` — 561M downloads, github.com/tokio-rs/tokio, verified via curl

### Tertiary (LOW confidence — [ASSUMED])
- tokio 1.34 stabilization date for `process_group()` (per Phase 12 research doc)
- `acquire_owned()` vs `acquire()` choice for semaphore (standard tokio pattern)

---

## Metadata

**Confidence breakdown:**
- Standard Stack: HIGH — all crates verified in Cargo.lock; APIs verified in production code
- Architecture: HIGH — design follows existing metro kill pattern + Phase 14 decisions
- Pitfalls: HIGH — Pitfall 2 (missing process_group) verified by code inspection; others from prior art
- Collision policy categories: MEDIUM — documented in REQUIREMENTS + D-05, but exact BlockNew/CancelPrevious per-spec assignment is [ASSUMED] for some variants

**Research date:** 2026-05-18
**Valid until:** 2026-06-18 (stable ecosystem — tokio 1.49, libc 0.2 are not fast-moving)
