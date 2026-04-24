# Phase 13: Audit-Driven Refactors — Pattern Map

**Mapped:** 2026-04-24
**Files analyzed:** 33 (created + modified + deleted)
**Analogs found:** 31 / 33 (2 files have no perfect analog and use research sketches)

This document maps every file that Phase 13 creates, modifies, or deletes to its
closest existing analog in the `rn-dash` codebase. The planner consumes this to
write per-plan action lists that cite concrete line ranges to copy from.

**Reading order for planners:** (1) §File Classification — locate the file you are
planning. (2) §Pattern Assignments — read the analog's excerpts. (3) §Shared
Patterns — cross-cutting conventions applied to every plan. (4) §Anti-patterns
to Avoid — the drift-and-regression trip-wires.

## File Classification

Role legend: `trait-def` (pure trait definition, no impl), `adapter` (concrete
impl of a port), `domain-type` (pure-data enum/struct), `app-module` (post-F-200
split app/* file), `test-helper`, `ui-consumer`, `build`.

Data-flow legend: `relocation` (pure file-move + import rewrite), `type-def` (new
enum/struct with no I/O), `event-stream` (async task + mpsc channel), `pure-fn`
(no side effects), `registry` (static data-driven dispatch), `structural-split`
(no behavior change — lift & shift), `consumer-rewire` (call-site migration).

### Files created

| New File | Role | Data Flow | Closest Analog | Match Quality |
|----------|------|-----------|----------------|---------------|
| `src/domain/action.rs` | domain-type | relocation | `src/action.rs` (same content, new location) | exact — file move |
| `src/domain/jira.rs` | domain-type + pure-fn + tests | pure-fn | `src/infra/jira.rs` `extract_jira_key` + 6 tests (lines 103-175) | exact — function + tests move verbatim |
| `src/domain/ports/mod.rs` | module-index | n/a | `src/domain/mod.rs` | exact convention |
| `src/domain/ports/process_port.rs` | trait-def | relocation | `src/infra/process.rs` trait block (lines 16-26) | exact — trait moves verbatim |
| `src/domain/ports/jira_port.rs` | trait-def | relocation | `src/infra/jira.rs` trait block (lines 22-30) | exact — trait moves |
| `src/domain/ports/multiplexer_port.rs` | trait-def | relocation | `src/infra/multiplexer.rs` trait block (lines 10-17) | exact — trait moves |
| `src/domain/ports/command_runner_port.rs` | trait-def + event-enum | type-def | `src/infra/process.rs` (async_trait shape) + Action::CommandOutputLine/CommandExited (src/action.rs:47-48) for the event split | role-match — new event-stream port |
| `src/domain/ports/metro_port.rs` | trait-def + opaque MetroHandle | type-def | `src/infra/process.rs` (ProcessClient trait shape) + `src/domain/metro.rs` (MetroHandle struct → trait, lines 54-76) | role-match — trait extracted from existing struct |
| `src/domain/ports/port_probe_port.rs` | trait-def + ExternalProcessInfo | type-def | `src/infra/port.rs` (free fns at lines 12, 28, 60 → trait methods) | role-match — trait shell around existing fns |
| `src/domain/ports/worktree_port.rs` | trait-def | type-def | `src/infra/worktrees.rs` (free fns at lines 196, 240, 285, 309, 334 → trait methods) | role-match — trait shell |
| `src/domain/ports/device_port.rs` | trait-def | type-def | `src/infra/devices.rs` (async fns → trait methods) | role-match — trait shell |
| `src/domain/pipeline.rs` | domain-type + pure-fn + tests | pure-fn | `src/domain/refresh.rs` (full file — pure fn + inline `#[cfg(test)] mod tests`) | exact — same domain shape pattern |
| `src/app/effect.rs` | domain-type (app tier) | type-def | `src/action.rs` (flat data enum pattern) | role-match — parallel enum layout |
| `src/app/mod.rs` | module-index | n/a | `src/domain/mod.rs` + `src/infra/mod.rs` + module re-exports `app.rs:2427 #[cfg(test)] mod dispatch_tests;` | exact — module-index convention |
| `src/app/state.rs` | app-module | structural-split | `src/app.rs:1-256` (AppState + FocusedPanel + ErrorState + PaletteMode + 3 helpers) | exact — verbatim relocation |
| `src/app/update.rs` | app-module | structural-split → consumer-rewire | `src/app.rs:538-2061` (the `update()` fn body) | exact — verbatim relocation then F-201 rewrite |
| `src/app/effect_runner.rs` | app-module | event-stream | `src/app.rs:2209-2410` (7 async metro helpers as the model of "owns tokio::spawn and mpsc channels") + `src/infra/command_runner.rs:26-70` (how to translate CommandEvent → Action at the boundary) | role-match — new component, composite analog |
| `src/app/handle_key.rs` | app-module | registry | `src/app.rs:258-478` (existing `handle_key` fn — body relocates; F-208 consumer rewrites body to walk KEYBINDINGS) | exact — code moves |
| `src/app/runtime.rs` | app-module | structural-split | `src/app.rs:2063-2202` (`pub async fn run`) | exact — verbatim relocation |
| `src/app/adapters.rs` | app-module (DI struct) | type-def | `src/app.rs:120,130` (existing `jira_client: Option<Arc<dyn JiraClient>>` + `multiplexer: Option<Box<dyn Multiplexer>>` fields — the pattern promotes to a dedicated struct) | role-match — existing single-field pattern generalized |
| `src/app/keybindings.rs` | registry + dispatch helpers | registry | `src/ui/footer.rs:29-161` (`key_hints_for` context-matching) + `src/app.rs:268-477` (`handle_key` modal/palette/panel cascade) | role-match — three-site registry consolidation |
| `src/infra/metro.rs` | adapter | event-stream | `src/infra/command_runner.rs` (full file — TokioProcessClient shape + stdout/stderr streaming + kill_on_drop) + `src/app.rs:2209-2425` (7 metro helpers — the code that moves in) | exact — structural analog + code moves |
| `Makefile` entry `arch-lint` | build | n/a | `Makefile` `cov-check` target (lines 24-27 — same grep/verification pattern) | exact — parallel Make target |

### Files modified in place

| Modified File | Role | Data Flow | Closest Analog (for the change pattern) | Match Quality |
|---------------|------|-----------|-----------------------------------------|---------------|
| `src/domain/command.rs` | domain-type | pure-fn (add predicate) | `src/domain/command.rs:108-117` (existing `is_destructive` predicate — new `is_cancellable` follows identical `matches!` shape) | exact — mirror existing impl |
| `src/domain/metro.rs` | domain-type | struct→trait | existing `MetroHandle` struct (lines 54-76) — fields become trait methods | role-match — self-analog |
| `src/infra/process.rs` | adapter | relocation | file stays; just relocates `use` for the trait (1-line change) | trivial |
| `src/infra/jira.rs` | adapter | relocation (extract pure fn out) | file stays; `extract_jira_key` + tests removed (lines 103-175 delete); trait import path updates | exact — subtractive edit |
| `src/infra/multiplexer.rs` | adapter | relocation | same pattern as `src/infra/process.rs` | trivial |
| `src/infra/port.rs` | adapter | wrap free fns | `src/infra/multiplexer.rs` (struct + impl pattern — wrap existing free fns) | exact — same structural shape |
| `src/infra/worktrees.rs` | adapter | wrap free fns | `src/infra/multiplexer.rs` (same pattern) | exact |
| `src/infra/devices.rs` | adapter | wrap free fns | `src/infra/multiplexer.rs` (same pattern) | exact |
| `src/infra/command_runner.rs` | adapter | re-emit CommandEvent | existing file body (lines 26-70) — change output type from `Action` sends to `CommandEvent` sends | exact — self-analog |
| `src/ui/panels.rs` | ui-consumer | consumer-rewire | existing line 71 `crate::infra::jira::extract_jira_key` → `crate::domain::jira::extract_jira_key` | trivial 1-line rewrite |
| `src/ui/footer.rs` | ui-consumer | consumer-rewire | replace `key_hints_for` body (lines 29-161) with single call to `keybindings::footer_hints_for(state)` | role-match — function body rewrite |
| `src/ui/help_overlay.rs` | ui-consumer | consumer-rewire | replace hand-coded keybinding rows (lines 17-112) with `keybindings::help_overlay_rows()`; Icons section (lines 108-112) stays hand-coded | role-match — function body rewrite |
| `tests/common/mod.rs` | test-helper | adapter type shift | existing `fake_metro_handle` (lines 17-28) — returns `Box<dyn MetroHandle>` after F-004 | exact — self-analog |
| `tests/metro_single_instance.rs` | test | signature update | existing 2 tests — call sites change from `update(&mut s, a, &tx, &htx)` to `let effects = update(&mut s, a); assert!(matches!(effects, ...))` | exact — research §Pitfall 5 spells out the shape |

### Files deleted

| Deleted File | Reason | Pre-delete validation |
|--------------|--------|-----------------------|
| `src/action.rs` | moved to `src/domain/action.rs` (F-002) | all imports rewritten across 2 sites + `lib.rs` |
| `src/app.rs` | split into `src/app/*.rs` (F-200) | all 26 tests still pass via `src/app/mod.rs` re-exports |
| `src/infra/tmux.rs` | dead code (F-112) | only internal ref was `OpenClaudeCode` → now uses `multiplexer::TmuxAdapter` |

## Pattern Assignments

For each new/modified file, this section gives the concrete analog code to
read-and-copy from, with line numbers.

### `src/domain/action.rs` (domain-type, relocation — F-002)

**Analog:** `src/action.rs` — entire file (151 lines).

**Imports pattern** (src/action.rs has no imports beyond `crate::domain::...`
qualified paths inside variants — this is intentional):

```rust
// Top of file — no `use` statements. Variants refer to domain types via
// fully-qualified crate paths so action.rs has no import cycles.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Action {
    // ...
}
```

**Variant reference pattern** (src/action.rs:42,46,47,64,125):

```rust
WorktreesLoaded(Vec<crate::domain::worktree::Worktree>),
CommandRun(crate::domain::command::CommandSpec),
CommandOutputLine(String),
DevicesEnumerated(Vec<crate::domain::command::DeviceInfo>),
ExternalMetroDetected(crate::infra::port::ExternalMetroInfo),
```

**Move rule:** One existing ref at `ExternalMetroDetected(crate::infra::port::ExternalMetroInfo)`
becomes `ExternalMetroDetected(crate::domain::ports::port_probe_port::ExternalProcessInfo)`
after F-102. Every other variant survives verbatim.

**Import rewrites required** (grep-verifiable):

```bash
# Before:
rg 'use crate::action::Action' src/       # expect 2 hits (app.rs:2, command_runner.rs:12)
rg 'use rn_dash::action::Action' tests/   # expect 1 hit (metro_single_instance.rs:13)

# After:
rg 'use crate::domain::action::Action' src/     # expect 3+ hits (app/*, never infra/)
rg 'use rn_dash::domain::action::Action' tests/  # expect 1 hit
! rg 'use crate::action::Action' src/infra/     # F-101 grep guard — must be 0
```

### `src/domain/jira.rs` (domain-type + pure-fn + tests — F-107)

**Analog:** `src/infra/jira.rs:103-175` (function + 6 tests).

**Imports pattern** (none required — pure-fn has zero imports):

```rust
// src/domain/jira.rs — no imports. Pure string manipulation.
```

**Core pattern** (copy from src/infra/jira.rs:103-120):

```rust
/// Extracts a JIRA ticket key from a git branch name using the given project prefix.
///
/// Supports branch formats like:
///   - "feature/UMP-1234-some-description"  → Some("UMP-1234")
///   - "UMP-5678"                           → Some("UMP-5678")
///   - "main"                               → None
pub fn extract_jira_key(branch: &str, project_prefix: &str) -> Option<String> {
    for segment in branch.split('/') {
        let mut parts = segment.splitn(3, '-');
        let first = match parts.next() {
            Some(v) => v,
            None => continue,
        };
        let second = match parts.next() {
            Some(v) => v,
            None => continue,
        };

        if first == project_prefix && !second.is_empty() && second.chars().all(|c| c.is_ascii_digit()) {
            return Some(format!("{project_prefix}-{second}"));
        }
    }
    None
}
```

**Test pattern** (copy from src/infra/jira.rs:130-175 — 6 tests verbatim):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_key_from_feature_branch() {
        assert_eq!(
            extract_jira_key("feature/UMP-1234-login", "UMP"),
            Some("UMP-1234".to_string())
        );
    }
    // ... 5 more tests as-is
}
```

**Coverage implication:** `src/infra/jira.rs` drops from 70% (with these 6 tests)
after removal. `src/domain/jira.rs` becomes new row at 100%. Update
`COVERAGE-THRESHOLDS.md` in same commit per research Pitfall 7.

### `src/domain/ports/mod.rs` (module-index)

**Analog:** `src/domain/mod.rs` (7 lines).

```rust
// src/domain/mod.rs — current convention:
//! Domain layer — pure Rust. Zero dependencies on ratatui, crossterm, or infra.
pub mod command;
pub mod metro;
pub mod refresh;
pub mod worktree;
```

**Copy pattern for `src/domain/ports/mod.rs`:**

```rust
//! Hexagonal ports — trait definitions for every external-world capability.
//! Each port has one or more adapters in `src/infra/*.rs`.
pub mod command_runner_port;
pub mod device_port;
pub mod jira_port;
pub mod metro_port;
pub mod multiplexer_port;
pub mod port_probe_port;
pub mod process_port;
pub mod worktree_port;
```

**Update `src/domain/mod.rs`** to add `pub mod action;`, `pub mod jira;`,
`pub mod pipeline;`, `pub mod ports;` lines.

### `src/domain/ports/process_port.rs` (trait-def — F-103 relocation)

**Analog:** `src/infra/process.rs:16-26` — trait block moves verbatim.

**Copy pattern:**

```rust
// src/domain/ports/process_port.rs
#![allow(dead_code)]

use std::path::PathBuf;
use tokio::process::Child;

/// Trait boundary for metro process spawning.
///
/// The domain and app layers depend only on this trait. TokioProcessClient is the
/// production implementation; tests may supply a fake.
#[async_trait::async_trait]
pub trait ProcessPort: Send + Sync {
    /// Spawn a metro dev server in the given worktree directory.
    async fn spawn_metro(&self, worktree_path: PathBuf) -> anyhow::Result<Child>;
}
```

**Rename:** `ProcessClient` → `ProcessPort` (consistent with the rest of the
`*Port` naming convention used by F-101/F-102/F-104/F-105/F-203).

**Tokio type in the trait signature:** `tokio::process::Child` crosses the
domain boundary. This mirrors the existing pragmatic exception in `domain/metro.rs`
for MetroHandle — see that file's architectural comment (lines 5-13) for the
justification template.

**infra-side change** (src/infra/process.rs keeps `TokioProcessClient` impl):

```rust
// src/infra/process.rs — replace trait block (lines 16-26) with:
use crate::domain::ports::process_port::ProcessPort;

pub struct TokioProcessClient;

#[async_trait::async_trait]
impl ProcessPort for TokioProcessClient {
    async fn spawn_metro(&self, worktree_path: PathBuf) -> anyhow::Result<Child> {
        // ... existing body at lines 33-50 unchanged
    }
}
```

### `src/domain/ports/jira_port.rs` (trait-def — F-106 relocation)

**Analog:** `src/infra/jira.rs:22-30` — trait block moves verbatim.

**Copy pattern:**

```rust
// src/domain/ports/jira_port.rs
use async_trait::async_trait;

#[async_trait]
pub trait JiraPort: Send + Sync + std::fmt::Debug {
    async fn fetch_title(&self, ticket_key: &str) -> Option<String>;
}
```

**Rename:** `JiraClient` → `JiraPort`.

**Infra side:** `HttpJiraClient` in `src/infra/jira.rs:33-88` stays unchanged
except for the `impl JiraClient for HttpJiraClient` line, which becomes
`impl crate::domain::ports::jira_port::JiraPort for HttpJiraClient`.

### `src/domain/ports/multiplexer_port.rs` (trait-def — F-110 relocation)

**Analog:** `src/infra/multiplexer.rs:9-17` — trait block moves verbatim.

**Copy pattern:**

```rust
// src/domain/ports/multiplexer_port.rs
use std::path::Path;

#[allow(dead_code)]
pub trait MultiplexerPort: Send + Sync + std::fmt::Debug {
    fn new_window(&self, path: &Path, name: &str, command: &str) -> anyhow::Result<()>;
    fn is_available(&self) -> bool;
}
```

**Rename:** `Multiplexer` → `MultiplexerPort`.

**detect_multiplexer** (src/infra/multiplexer.rs:77-85) stays in `infra/multiplexer.rs`
with return type changed to `Option<Box<dyn MultiplexerPort>>`.

### `src/domain/ports/command_runner_port.rs` (trait + event enum — F-101 NEW)

**Analog:** `src/infra/process.rs:16-26` (trait structural shape) + `src/action.rs:47-48`
(which the new `CommandEvent` replaces at the infra boundary).

**Research sketch** (research §Code Examples C, lines 1093-1116):

```rust
// src/domain/ports/command_runner_port.rs
use crate::domain::command::CommandSpec;
use std::path::PathBuf;
use std::process::ExitStatus;

pub enum CommandEvent {
    OutputLine(String),
    Exited(ExitStatus),
}

#[async_trait::async_trait]
pub trait CommandRunnerPort: Send + Sync {
    /// Spawn the command. Returns a receiver that emits output lines then one Exited event.
    /// Receiver closes after Exited is sent.
    fn spawn(
        &self,
        spec: CommandSpec,
        cwd: PathBuf,
        branch: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<CommandEvent>;
}
```

**Why `UnboundedReceiver<CommandEvent>` and not `UnboundedSender<Action>`:** The
existing infra adapter (`src/infra/command_runner.rs:12`) imports
`crate::action::Action` — the exact Fowler violation F-101 diagnoses. The port
trait returns typed `CommandEvent`s; the app-layer `effect_runner` translates
them into `Action::CommandOutputLine` / `Action::CommandExited` at the boundary
(see research §Code Examples C effect_runner translation).

### `src/domain/ports/metro_port.rs` (trait + opaque MetroHandle — F-203 + F-004)

**Analog (for trait shape):** `src/infra/process.rs:16-26` (`#[async_trait]` +
`Send + Sync` + path arg). **Analog (for MetroHandle trait):** the existing
`src/domain/metro.rs:54-76` struct that becomes a trait.

**Copy pattern for the trait:**

```rust
// src/domain/ports/metro_port.rs
use std::path::PathBuf;
use crate::domain::metro::MetroActivity;

/// Opaque handle to a live metro process. Implementations live in the infra
/// adapter — domain callers never see the concrete type.
pub trait MetroHandle: Send + Sync + std::fmt::Debug {
    fn pid(&self) -> u32;
    fn worktree_id(&self) -> &str;
    fn send_stdin(&self, bytes: Vec<u8>) -> anyhow::Result<()>;
    /// Consuming kill — infra adapter is responsible for process-group SIGKILL
    /// and port-free wait. Returns once port 8081 is observed free (or timeout).
    fn kill(self: Box<Self>) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait MetroPort: Send + Sync {
    /// Spawn metro in the given worktree. Streams activity via `on_activity`.
    /// Returns an opaque handle; caller registers it via `MetroManager::register`.
    async fn start(
        &self,
        worktree: PathBuf,
        on_activity: Box<dyn Fn(MetroActivity) + Send + Sync>,
    ) -> anyhow::Result<Box<dyn MetroHandle>>;

    async fn http_post(&self, path: &str, body: &str) -> anyhow::Result<()>;

    async fn detect_external(&self, port: u16)
        -> Option<crate::domain::ports::port_probe_port::ExternalProcessInfo>;
}
```

**Open question (research Pitfall 8 / Q3):** whether `on_activity` should be a
callback (shown above, hexagonally pure) or an `UnboundedSender<MetroActivity>`
(tokio leak, pragmatic). The research defaults to the callback; planner may
tighten if `/gsd:discuss-phase 13` revisits.

**MetroHandle conversion** (from struct at `src/domain/metro.rs:54-76` — tokio
fields evaporate):

```rust
// BEFORE (src/domain/metro.rs:60-76 — current):
pub struct MetroHandle {
    pub pid: u32,
    pub worktree_id: String,
    pub stdin_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    pub stream_task: tokio::task::JoinHandle<()>,
    pub stdin_task: tokio::task::JoinHandle<()>,
    pub kill_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

// AFTER (trait in domain/ports/metro_port.rs; concrete in infra/metro.rs):
// Trait body shown above. Concrete `TokioMetroHandle` holds the tokio fields privately.
```

**MetroManager field update** (src/domain/metro.rs:85):

```rust
// BEFORE:
handle: Option<MetroHandle>,    // concrete struct

// AFTER:
handle: Option<Box<dyn MetroHandle>>,  // trait object, tokio types hidden
```

### `src/domain/ports/port_probe_port.rs` (trait + ExternalProcessInfo — F-102)

**Analog:** `src/infra/port.rs` (full file — struct + 3 free fns become a trait).

**Rename:** `ExternalMetroInfo` → `ExternalProcessInfo` (matches F-102 audit
recommendation — the struct is generic, not metro-specific).

**Copy pattern:**

```rust
// src/domain/ports/port_probe_port.rs

/// Information about an external (non-dashboard) process occupying a port.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalProcessInfo {
    pub pid: u32,
    pub working_dir: String,
}

#[async_trait::async_trait]
pub trait PortProbePort: Send + Sync {
    fn port_is_free(&self, port: u16) -> bool;
    async fn detect_external(&self, port: u16) -> Option<ExternalProcessInfo>;
    async fn kill_process(&self, pid: u32) -> anyhow::Result<()>;
}
```

**infra side** (src/infra/port.rs — wrap existing free fns):

```rust
// src/infra/port.rs — ADD at bottom:
pub struct LsofPortProbe;

#[async_trait::async_trait]
impl crate::domain::ports::port_probe_port::PortProbePort for LsofPortProbe {
    fn port_is_free(&self, port: u16) -> bool { port_is_free(port) }
    async fn detect_external(&self, port: u16)
        -> Option<crate::domain::ports::port_probe_port::ExternalProcessInfo>
    {
        detect_external_metro(port).await.map(|info|
            crate::domain::ports::port_probe_port::ExternalProcessInfo {
                pid: info.pid,
                working_dir: info.working_dir,
            }
        )
    }
    async fn kill_process(&self, pid: u32) -> anyhow::Result<()> { kill_process(pid).await }
}
```

**Also update:** `src/action.rs:125` `ExternalMetroDetected(crate::infra::port::ExternalMetroInfo)`
→ `ExternalMetroDetected(crate::domain::ports::port_probe_port::ExternalProcessInfo)`.

### `src/domain/ports/worktree_port.rs` (trait — F-104)

**Analog:** `src/infra/worktrees.rs` — 5 async free fns (lines 196, 240, 285,
309, 334) become trait methods.

**Copy pattern:**

```rust
// src/domain/ports/worktree_port.rs
use crate::domain::worktree::Worktree;
use std::path::{Path, PathBuf};

#[async_trait::async_trait]
pub trait WorktreePort: Send + Sync {
    async fn list(&self, repo_root: &Path) -> anyhow::Result<Vec<Worktree>>;
    async fn remove(&self, repo_root: &Path, worktree_path: &Path) -> anyhow::Result<()>;
    async fn add(&self, repo_root: &Path, branch_name: &str) -> anyhow::Result<PathBuf>;
    async fn add_new_branch(&self, repo_root: &Path, new_branch: &str, base_branch: &str)
        -> anyhow::Result<PathBuf>;
    async fn list_remote_branches(&self, repo_root: &Path) -> anyhow::Result<Vec<String>>;
}
```

**infra side** (ADD `GitWorktreeAdapter` struct at end of `src/infra/worktrees.rs`
that delegates to existing free fns — same pattern as `LsofPortProbe` above).

**Keep pure parser** (`parse_worktree_porcelain`, `check_stale`, `check_stale_pods`
at `src/infra/worktrees.rs:23-185`) as module-private. **Analog:** `src/infra/devices.rs`
uses the same "pure parsers + async I/O shells" pattern (lines 32-100 parsers,
async wrappers elsewhere).

### `src/domain/ports/device_port.rs` (trait — F-105)

**Analog:** `src/infra/devices.rs` — parallel to `WorktreePort` above.

**Copy pattern:**

```rust
// src/domain/ports/device_port.rs
use crate::domain::command::DeviceInfo;

#[derive(Debug, Clone, Copy)]
pub enum DeviceKind { Android, Ios }

#[async_trait::async_trait]
pub trait DevicePort: Send + Sync {
    async fn list(&self, kind: DeviceKind) -> anyhow::Result<Vec<DeviceInfo>>;
}
```

### `src/domain/pipeline.rs` (Prerequisite + Recipe + tests — F-204 + REFACTOR-03)

**Analog:** `src/domain/refresh.rs` (full file, 248 lines) — **exemplary domain
module** with pure fn + inline `#[cfg(test)] mod tests` + 17 tests. This is the
"reference standard" the AUDIT.md calls out.

**Structural pattern from `src/domain/refresh.rs:1-69`:**

```rust
//! Data dependency model: maps completed commands to required refreshes.
//!
//! `refresh_needed()` is a pure domain function — no I/O, no side effects.
//! The app layer calls it after a command exits to determine which
//! background refresh tasks to dispatch.

use super::command::CommandSpec;

/// Which background refreshes a completed command requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshSet {
    pub worktrees: bool,
    pub staleness: bool,
    pub jira_titles: bool,
}

impl RefreshSet {
    pub fn none() -> Self { /* ... */ }
    pub fn any(&self) -> bool { /* ... */ }
}

pub fn refresh_needed(cmd: &CommandSpec) -> RefreshSet {
    match cmd {
        // ... per-variant arms
        _ => RefreshSet::none(),
    }
}
```

**Test pattern from `src/domain/refresh.rs:71-248`** (copy the shape — helpers,
one test per variant family, 2-3 assertions each):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn full_refresh() -> RefreshSet { /* test helper ctor */ }
    fn staleness_only() -> RefreshSet { /* test helper ctor */ }

    #[test]
    fn git_reset_hard_triggers_full_refresh() {
        assert_eq!(refresh_needed(&CommandSpec::GitResetHard), full_refresh());
    }
    // ... one test per variant family
}
```

**Pipeline-specific content** (from research §Pattern 2, lines 347-430):

```rust
// src/domain/pipeline.rs
use crate::domain::command::{CleanOptions, CommandSpec};

pub enum Prerequisite {
    MetroRunning,
    DependenciesFresh { yarn: bool, pods: bool },
}

impl CommandSpec {
    pub fn prerequisites(&self) -> Vec<Prerequisite> {
        match self {
            CommandSpec::RnRunAndroid { .. }
            | CommandSpec::RnRunIos { .. }
            | CommandSpec::RnRunIosDevice
            | CommandSpec::RnReleaseBuild => vec![Prerequisite::MetroRunning],
            _ => vec![],
        }
    }
}

pub enum Recipe {
    Single(CommandSpec),
    Sequence(Vec<CommandSpec>),
    Clean(CleanOptions),
    SyncThenRun(CommandSpec),
    SyncThenStartMetro,
    ReleaseBuildAndInstall,
    GitFetchThenReset,
}

pub struct DependencyState {
    pub stale_yarn: bool,
    pub stale_pods: bool,
    pub is_ios_target: bool,
}

impl Recipe {
    pub fn expand(&self, deps: &DependencyState) -> Vec<CommandSpec> {
        // per-variant match — see research §Pattern 2 lines 400-428 for body
    }
}
```

**Test coverage goal:** match `src/domain/refresh.rs`'s 17-test density. Research
§Wave 0 Gaps lists 8-10 tests: Single, Sequence, Clean (each CleanOptions combo),
SyncThenRun stale/fresh, SyncThenStartMetro, ReleaseBuildAndInstall,
GitFetchThenReset.

### `src/app/effect.rs` (app-tier enum — F-201)

**Analog:** `src/action.rs` (full file, 151 lines) — same "flat enum with
qualified-path variants" shape. Effect is the dual of Action.

**Imports pattern** (minimal — same as Action):

```rust
// src/app/effect.rs
use crate::domain::command::{CommandSpec, DeviceInfo};
use std::collections::HashMap;
use std::path::PathBuf;
```

**Core enum pattern** (from research §Pattern 1 + audit F-201, plus mirroring
src/action.rs shape — see src/action.rs:29-33 for the metro control analog):

```rust
// src/app/effect.rs
#[derive(Debug)]
pub enum Effect {
    // Metro lifecycle
    DetectExternalMetro { port: u16 },
    SpawnMetro { worktree: PathBuf },
    MetroHttpPost { url: String, body: String },
    KillProcess { pid: u32 },

    // Commands
    SpawnCommand { spec: CommandSpec, cwd: PathBuf, branch: String },
    LoadDevices { kind: crate::domain::ports::device_port::DeviceKind },

    // Worktrees
    ListWorktrees,
    RemoveWorktree { path: PathBuf },
    AddWorktree { branch: String },
    AddWorktreeNewBranch { new: String, base: String },
    ListRemoteBranches,

    // Persistence (spawn_blocking sites)
    SaveJiraCache(HashMap<String, String>),
    SaveAndroidMode(String),
    RecordSimUsed(String),

    // External processes
    OpenInMultiplexer { worktree: PathBuf, name: String, command: String },

    // JIRA
    FetchJiraTitles { keys: Vec<String> },

    // Recursive self-dispatch (F-206 absorption)
    ScheduleAction(crate::domain::action::Action),
}
```

**No `Clone`, no closures** — research §Anti-Patterns: "Effect enum becomes an
impurity loophole. `Effect::RunArbitraryClosure(Box<dyn Fn>)` defeats the
purpose." Every variant is plain data.

### `src/app/mod.rs` (module-index — F-200 structural)

**Analog:** `src/domain/mod.rs` (structural pattern) + `src/app.rs:2427-2428`
(the existing `#[cfg(test)] mod dispatch_tests;` declaration that must survive).

**Copy pattern:**

```rust
//! App layer — TEA event loop, state mutation, effect interpretation.
pub mod adapters;
pub mod effect;
pub mod effect_runner;
pub mod handle_key;
pub mod keybindings;
pub mod runtime;
pub mod state;
pub mod update;

// Re-exports for tests and ui/ (KEEPS rn_dash::app::* paths stable)
pub use state::{
    active_output, active_output_scroll, active_worktree_id,
    AppState, ErrorState, FocusedPanel, PaletteMode,
};
pub use update::update;
pub use runtime::run;
pub use handle_key::handle_key;

#[cfg(test)]
mod dispatch_tests;
```

**CRITICAL (research Pitfall 1):** the `#[cfg(test)] mod dispatch_tests;` line is
load-bearing — omitting it silently drops 17 COVER-03 tests from the build.

### `src/app/state.rs` (structural lift — F-200 then F-209 sub-struct grouping)

**Analog:** `src/app.rs:1-256` — the entire existing state definition block.

**Phase A (F-200 structural lift — no shape change):** Copy `src/app.rs:1-256`
verbatim into `src/app/state.rs`. Update the `use crate::action::Action;`
import at line 2 to `use crate::domain::action::Action;` (after F-002 lands).

**Phase B (F-209 sub-struct grouping — plan 13-10):** Group 39 fields into 6-7
sub-structs per research §Recommended Target Plans 13-10 (lines 878-885):

```rust
pub struct MetroState {
    pub metro: crate::domain::metro::MetroManager,
    pub active_worktree_path: Option<std::path::PathBuf>,
    pub skip_external_metro_check: bool,
    pub pending_restart: bool,
    pub pending_switch_path: Option<std::path::PathBuf>,
    pub pending_metro_after_sync: bool,
}

pub struct WorktreeBrowserState {
    pub worktrees: Vec<crate::domain::worktree::Worktree>,
    pub worktree_table_state: ratatui::widgets::TableState,
    pub selected_worktree_id: Option<crate::domain::worktree::WorktreeId>,
    pub fullscreen_panel: Option<FocusedPanel>,
    pub worktree_op_in_flight: bool,
}

pub struct CommandRunnerState {
    pub command_queue: std::collections::VecDeque<crate::domain::command::CommandSpec>,
    pub command_output_by_worktree: std::collections::HashMap<
        crate::domain::worktree::WorktreeId,
        std::collections::VecDeque<String>
    >,
    pub command_output_scroll_by_worktree: std::collections::HashMap<
        crate::domain::worktree::WorktreeId, usize>,
    pub running_command: Option<crate::domain::command::CommandSpec>,
    pub command_task: Option<tokio::task::JoinHandle<()>>,
}

pub struct ModalStackState { /* modal + palette + 7 pending_* */ }
pub struct JiraState { pub title_cache: ..., pub project_prefix: String }
pub struct AppConfigState { pub config: ..., pub repo_root, pub claude_flags, pub android_mode }

pub struct AppState {
    // root-level: focused_panel, show_help, error_state, should_quit
    pub focused_panel: FocusedPanel,
    pub show_help: bool,
    pub error_state: Option<ErrorState>,
    pub should_quit: bool,
    pub metro_state: MetroState,
    pub worktree_browser: WorktreeBrowserState,
    pub command_runner: CommandRunnerState,
    pub modal_stack: ModalStackState,
    pub jira_state: JiraState,
    pub app_config: AppConfigState,
    // jira_client + multiplexer REMOVED — moved to Adapters per F-202
}
```

**Helpers at src/app.rs:231-256** (`active_worktree_id`, `active_output`,
`active_output_scroll`) move with state.rs; field paths update from
`state.worktrees` → `state.worktree_browser.worktrees`.

### `src/app/update.rs` (structural lift then F-201 consumer rewrite)

**Analog:** `src/app.rs:485-2061` (`dispatch_command` + `update` + 1520 LOC body).

**Phase A — structural lift (plan 13-06):** Copy verbatim. Signature unchanged.

**Phase B — F-201 consumer rewrite (plan 13-07):** signature becomes
`pub fn update(state: &mut AppState, action: Action) -> Vec<Effect>`.

**Analog transformation excerpt** — the existing `Action::MetroStart` arm at
`src/app.rs:586-609` contains the inline `tokio::spawn` (line 602) that F-201
replaces:

```rust
// BEFORE (src/app.rs:586-609):
Action::MetroStart => {
    state.palette_mode = None;
    if state.metro.is_running() {
        state.pending_restart = true;
        update(state, Action::MetroStop, metro_tx, handle_tx);  // recursive
        return;
    }
    if state.skip_external_metro_check {
        state.skip_external_metro_check = false;
        let _ = metro_tx.send(Action::MetroStartConfirmed);
        return;
    }
    let tx = metro_tx.clone();
    tokio::spawn(async move {  // <-- F-201 target: no tokio::spawn in update()
        if let Some(info) = crate::infra::port::detect_external_metro(8081).await {
            let _ = tx.send(Action::ExternalMetroDetected(info));
        } else {
            let _ = tx.send(Action::MetroStartConfirmed);
        }
    });
}

// AFTER (src/app/update.rs):
Action::MetroStart => {
    state.modal_stack.palette_mode = None;
    let mut effects = Vec::new();
    if state.metro_state.metro.is_running() {
        state.metro_state.pending_restart = true;
        effects.extend(update(state, Action::MetroStop));
        return effects;
    }
    if state.metro_state.skip_external_metro_check {
        state.metro_state.skip_external_metro_check = false;
        effects.push(Effect::ScheduleAction(Action::MetroStartConfirmed));
        return effects;
    }
    effects.push(Effect::DetectExternalMetro { port: 8081 });
    effects
}
```

**Grep guard:** `! rg 'tokio::spawn' src/app/update.rs` — MUST be 0 hits after
F-201.

**Recursive-self-dispatch absorption (F-206):** every `update(state, Action::X,
...)` recursive call becomes either `effects.extend(update(state, Action::X))`
(inline execution) OR `effects.push(Effect::ScheduleAction(Action::X))`
(deferred execution). See research §Common Pitfalls Pitfall 3 on which flags
survive.

### `src/app/effect_runner.rs` (NEW — F-201 + F-202 consumer)

**Analog (structural):** `src/app.rs:2209-2256` (`spawn_metro_task` — how to
own tokio::spawn + channel sends). **Analog (CommandRunnerPort translation):**
research §Code Examples C (lines 1138-1151).

**Copy pattern** (composite):

```rust
// src/app/effect_runner.rs
use crate::app::adapters::Adapters;
use crate::app::effect::Effect;
use crate::domain::action::Action;
use crate::domain::ports::command_runner_port::CommandEvent;
use tokio::sync::mpsc::UnboundedSender;

pub struct EffectRunner {
    pub adapters: Adapters,
    pub action_tx: UnboundedSender<Action>,
}

impl EffectRunner {
    pub fn new(adapters: Adapters, action_tx: UnboundedSender<Action>) -> Self {
        Self { adapters, action_tx }
    }

    pub async fn run_effects(&self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::SpawnCommand { spec, cwd, branch } => {
                    // Same shape as src/app.rs:518-528 (existing dispatch_command logic)
                    // but now going through the port:
                    let mut rx = self.adapters.command_runner.spawn(spec, cwd, branch);
                    let tx = self.action_tx.clone();
                    tokio::spawn(async move {
                        while let Some(ev) = rx.recv().await {
                            let action = match ev {
                                CommandEvent::OutputLine(l) => Action::CommandOutputLine(l),
                                CommandEvent::Exited(_) => Action::CommandExited,
                            };
                            let _ = tx.send(action);
                        }
                    });
                }
                Effect::DetectExternalMetro { port } => {
                    let probe = self.adapters.port_probe.clone();
                    let tx = self.action_tx.clone();
                    tokio::spawn(async move {
                        match probe.detect_external(port).await {
                            Some(info) => { let _ = tx.send(Action::ExternalMetroDetected(info)); }
                            None => { let _ = tx.send(Action::MetroStartConfirmed); }
                        }
                    });
                }
                Effect::SpawnMetro { worktree } => {
                    // wraps the existing spawn_metro_task body (src/app.rs:2209-2256)
                    // but goes through self.adapters.metro — see infra/metro.rs below
                }
                Effect::ScheduleAction(action) => {
                    let _ = self.action_tx.send(action);
                }
                // ... one arm per Effect variant
            }
        }
    }
}
```

**Single boundary rule:** `effect_runner.rs` is the ONLY post-F-202 app-layer
file that holds tokio::spawn calls. `update.rs` must be tokio-free.

### `src/app/handle_key.rs` (registry walker — F-208)

**Analog:** `src/app.rs:258-478` — existing `handle_key` body. Phase A lifts
verbatim; Phase B rewrites body per research §Pattern 3 (lines 500-535).

**Phase A** (plan 13-06): copy lines 258-478 verbatim into `src/app/handle_key.rs`.

**Phase B** (plan 13-07): replace body with KEYBINDINGS walk:

```rust
// src/app/handle_key.rs (post-F-208)
use crate::app::keybindings::{context_matches, KEYBINDINGS};
use crate::app::AppState;
use crate::domain::action::Action;
use ratatui::crossterm::event::{KeyEvent, KeyEventKind};

pub fn handle_key(state: &AppState, key: KeyEvent) -> Option<Action> {
    if key.kind != KeyEventKind::Press { return None; }
    for kb in KEYBINDINGS.iter() {
        if context_matches(&kb.context, state) && kb.key == key.code {
            return (kb.action)(state);
        }
    }
    // Context-level fallback: unbound palette keys close the palette
    // (preserves the `_ => Some(Action::ModalCancel)` context-fallback
    // at src/app.rs:344,351,362,373,380 — research Pitfall 4).
    if state.modal_stack.palette_mode.is_some() {
        return Some(Action::ModalCancel);
    }
    None
}
```

### `src/app/runtime.rs` (event loop — F-200 structural lift)

**Analog:** `src/app.rs:2063-2202` — existing `pub async fn run(terminal)`.

**Phase A (plan 13-06):** verbatim lift. `run()` keeps the pre-refactor body
including the 7 metro helpers (they move OUT in 13-07 / F-203).

**Phase B (plan 13-08):** replace inline adapter construction with Adapters
struct. Research §Code Examples B (lines 1040-1088) has the exact target shape.
Excerpt:

```rust
// src/app/runtime.rs (post-F-202)
pub async fn run(mut terminal: ratatui::DefaultTerminal) -> color_eyre::Result<()> {
    let config = crate::infra::config::load_config().ok();
    let jira_title_cache = crate::infra::jira_cache::load_jira_cache().unwrap_or_default();

    let adapters = Adapters {
        command_runner: Arc::new(crate::infra::command_runner::TokioCommandRunner),
        metro: Arc::new(crate::infra::metro::TokioMetroAdapter::new()),
        port_probe: Arc::new(crate::infra::port::LsofPortProbe),
        worktrees: Arc::new(crate::infra::worktrees::GitWorktreeAdapter),
        devices: Arc::new(crate::infra::devices::AdbXcrunDevices),
        jira: config.as_ref().and_then(|c| /* build_jira */),
        multiplexer: crate::infra::multiplexer::detect_multiplexer().map(Arc::from),
    };

    let mut state = AppState::default();
    state.app_config.config = config;
    state.jira_state.title_cache = jira_title_cache;

    let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
    let runner = EffectRunner::new(adapters, action_tx.clone());

    let mut event_stream = EventStream::new();
    loop {
        if state.should_quit { break; }
        terminal.draw(|f| crate::ui::view(f, &mut state))?;
        tokio::select! {
            Some(ev_res) = event_stream.next() => {
                if let Event::Key(k) = ev_res? {
                    if let Some(action) = handle_key(&state, k) {
                        let effects = update(&mut state, action);
                        runner.run_effects(effects).await;
                    }
                }
            }
            Some(action) = action_rx.recv() => {
                let effects = update(&mut state, action);
                runner.run_effects(effects).await;
            }
        }
    }
    Ok(())
}
```

**Cleanup block** (src/app.rs:2182-2199): preserve the metro-kill + process_group
kill-on-exit logic; it becomes `adapters.metro.shutdown()` or equivalent.

### `src/app/adapters.rs` (DI struct — F-202)

**Analog:** `src/app.rs:120,130` — existing `jira_client: Option<Arc<dyn JiraClient>>`
and `multiplexer: Option<Box<dyn Multiplexer>>` fields show the single-field
pattern that this new file generalizes to a struct.

**Copy pattern** (research §Pattern 4, lines 541-585):

```rust
// src/app/adapters.rs
use std::sync::Arc;
use crate::domain::ports::{
    command_runner_port::CommandRunnerPort,
    device_port::DevicePort,
    jira_port::JiraPort,
    metro_port::MetroPort,
    multiplexer_port::MultiplexerPort,
    port_probe_port::PortProbePort,
    worktree_port::WorktreePort,
};

#[derive(Clone)]
pub struct Adapters {
    pub command_runner: Arc<dyn CommandRunnerPort>,
    pub metro: Arc<dyn MetroPort>,
    pub port_probe: Arc<dyn PortProbePort>,
    pub worktrees: Arc<dyn WorktreePort>,
    pub devices: Arc<dyn DevicePort>,
    pub jira: Option<Arc<dyn JiraPort>>,
    pub multiplexer: Option<Arc<dyn MultiplexerPort>>,
}
```

**`Clone` derive** is load-bearing: `effect_runner::run_effects` clones port
refs into spawned tasks. Arc<dyn Port> makes this cheap.

### `src/app/keybindings.rs` (registry — F-400 + F-208 + F-302 + F-303)

**Analog 1 (context-filter dispatch):** `src/ui/footer.rs:29-161` (`key_hints_for`
function — existing three-tier cascade by show_help → error_state → palette_mode
→ modal → focused_panel).

**Analog 2 (per-context key→Action mapping):** `src/app.rs:268-477` (`handle_key`
body — existing modal/palette/panel cascade).

**Analog 3 (help rows by section):** `src/ui/help_overlay.rs:17-112` (existing
hand-coded Row data — becomes registry-derived).

**Copy pattern** (research §Pattern 3, lines 444-535):

```rust
// src/app/keybindings.rs
use crate::app::{AppState, FocusedPanel, PaletteMode};
use crate::domain::action::Action;
use crate::domain::command::ModalState;
use ratatui::crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy)]
pub enum BindingContext {
    Always, Normal, WorktreeTable, CommandOutput,
    Palette(PaletteMode),
    Modal(ModalKind),
    Overlay(OverlayKind),
}

#[derive(Debug, Clone, Copy)]
pub enum ModalKind {
    Confirm, TextInput, DevicePicker, CleanToggle,
    SyncBeforeRun, SyncBeforeMetro, ExternalMetroConflict, BranchPicker,
}

#[derive(Debug, Clone, Copy)]
pub enum OverlayKind { Help, Error }

pub struct KeyBinding {
    pub key: KeyCode,
    pub label: &'static str,
    pub short_desc: &'static str,
    pub long_desc: &'static str,
    pub context: BindingContext,
    pub action: fn(&AppState) -> Option<Action>,
    pub visible: fn(&AppState) -> bool,
}

pub const KEYBINDINGS: &[KeyBinding] = &[
    // Normal mode (extracted from src/app.rs:466-477):
    KeyBinding {
        key: KeyCode::Char('q'),
        label: "q", short_desc: "quit", long_desc: "Quit the application",
        context: BindingContext::Normal,
        action: |_| Some(Action::Quit),
        visible: |_| true,
    },
    // WorktreeTable R — conditional action (extracted from src/app.rs:421-427):
    KeyBinding {
        key: KeyCode::Char('R'),
        label: "R", short_desc: "reload",
        long_desc: "Reload metro (when running) / Refresh list",
        context: BindingContext::WorktreeTable,
        action: |s| if s.metro_state.metro.is_running() {
            Some(Action::MetroSendReload)
        } else {
            Some(Action::RefreshWorktrees)
        },
        visible: |_| true,
    },
    // ... ~80 entries total, one per existing key in handle_key + footer + help_overlay
];

pub fn footer_hints_for(state: &AppState) -> Vec<(&'static str, &'static str)> {
    KEYBINDINGS.iter()
        .filter(|kb| context_matches(&kb.context, state) && (kb.visible)(state))
        .map(|kb| (kb.label, kb.short_desc))
        .collect()
}

pub struct HelpRow { pub section: &'static str, pub key: &'static str, pub desc: &'static str }
pub fn help_overlay_rows() -> Vec<HelpRow> { /* group by section */ }

pub fn context_matches(ctx: &BindingContext, state: &AppState) -> bool {
    // Replicate the logic from src/app.rs:268-328 (modal), 331-383 (palette),
    // 386-405 (overlay), 408-463 (panel) — single source of truth.
}
```

**Entry-count estimate:** 80+ entries covering every key in current handle_key
body (~45 keybindings) + footer hints (~35 unique labels) + help overlay (~40
rows). Compile-time `const` with fn pointers — zero allocations.

**Palette fallback** (research Pitfall 4): context-level fallback lives in
`handle_key.rs` (post-loop `if state.modal_stack.palette_mode.is_some() { ... }`)
not as per-key wildcard entries.

### `src/infra/metro.rs` (NEW — TokioMetroAdapter, F-203 + F-004)

**Analog 1 (structural — adapter shape):** `src/infra/command_runner.rs:1-129`
— full file shows how to implement an infra adapter that (a) owns tokio-typed
state, (b) takes a port trait, (c) streams events via mpsc.

**Analog 2 (code that moves IN):** `src/app.rs:2209-2425` — 7 existing async
metro helpers (`spawn_metro_task`, `metro_process_task`, `drain_metro_output`,
`stdin_writer`, `metro_http_post`, `parse_metro_line`, `extract_percent`) relocate
into this new file.

**Copy pattern:**

```rust
// src/infra/metro.rs
use crate::domain::metro::MetroActivity;
use crate::domain::ports::metro_port::{MetroHandle, MetroPort};
use std::path::PathBuf;

pub struct TokioMetroAdapter;

impl TokioMetroAdapter {
    pub fn new() -> Self { Self }
}

struct TokioMetroHandle {
    pid: u32,
    worktree_id: String,
    stdin_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    stream_task: tokio::task::JoinHandle<()>,
    stdin_task: tokio::task::JoinHandle<()>,
    kill_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl std::fmt::Debug for TokioMetroHandle { /* same as current MetroHandle */ }

impl MetroHandle for TokioMetroHandle {
    fn pid(&self) -> u32 { self.pid }
    fn worktree_id(&self) -> &str { &self.worktree_id }
    fn send_stdin(&self, bytes: Vec<u8>) -> anyhow::Result<()> {
        self.stdin_tx.send(bytes).map_err(|e| anyhow::anyhow!("stdin: {e}"))?;
        Ok(())
    }
    fn kill(self: Box<Self>) -> anyhow::Result<()> {
        // Adapted from src/app.rs:2182-2195 cleanup + 2270-2288 metro_process_task kill arm
        if let Some(kill_tx) = self.kill_tx { let _ = kill_tx.send(()); }
        self.stream_task.abort();
        self.stdin_task.abort();
        Ok(())
    }
}

#[async_trait::async_trait]
impl MetroPort for TokioMetroAdapter {
    async fn start(
        &self,
        worktree: PathBuf,
        on_activity: Box<dyn Fn(MetroActivity) + Send + Sync>,
    ) -> anyhow::Result<Box<dyn MetroHandle>> {
        // Body = src/app.rs:2209-2256 `spawn_metro_task`, adapted to:
        //   - return Box<dyn MetroHandle> instead of sending via handle_tx
        //   - call on_activity(activity) where current code calls
        //     action_tx.send(Action::MetroActivityUpdate(activity))
        todo!("Move spawn_metro_task body here")
    }

    async fn http_post(&self, url: &str, body: &str) -> anyhow::Result<()> {
        // Body = src/app.rs:2411-2425 `metro_http_post` verbatim
        todo!("Move metro_http_post body here")
    }

    async fn detect_external(&self, port: u16)
        -> Option<crate::domain::ports::port_probe_port::ExternalProcessInfo>
    {
        // Delegate to port_probe adapter OR inline the logic from infra/port.rs
        // (audit F-102 — detect_external is really a PortProbePort concern)
        todo!()
    }
}

// Private module-scope helpers — `parse_metro_line` (src/app.rs:2300-2332) +
// `extract_percent` (src/app.rs:2336-2354) move here as pure private fns
// (same pattern as `src/infra/devices.rs` parsers at lines 32-100).
fn parse_metro_line(line: &str) -> Option<MetroActivity> { /* ... */ }
fn extract_percent(s: &str) -> Option<u8> { /* ... */ }
```

**Process-group kill pattern** (to preserve from src/app.rs:2272-2288):

```rust
// Inside TokioMetroHandle::kill OR metro_process_task after kill_rx:
if let Some(id) = self.pid.into() {
    unsafe { libc::kill(-(id as i32), libc::SIGKILL); }
}
// Reap to prevent zombie
let _ = child.wait().await;
// Port-free wait loop (50 × 100ms = 5s budget)
for _ in 0..50 {
    if crate::infra::port::port_is_free(8081) { break; }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}
```

**Preserve this behavior** — `tests/process_group_kill.rs` guards the PGID kill
semantics (COVER-02). Any drift here fails that test.

**`src/infra/mod.rs` update:** add `pub mod metro;`.

### `src/domain/command.rs` (modify — REFACTOR-02)

**Analog:** existing `src/domain/command.rs:108-117` (`is_destructive` predicate)
— new `is_cancellable` mirrors this exactly.

**Copy pattern** (place after `is_destructive` impl):

```rust
// Add to impl CommandSpec block (after is_destructive):
/// Returns false for git-porcelain commands (data-integrity risk on cancellation);
/// true for all other commands (yarn, rn, rm, adb, shell).
///
/// REFACTOR-02: Type-driven cancellability. Git variants are closed by construction —
/// adding a new Git* variant requires explicit opt-in here (compile-error would be
/// ideal; today this is a flat-enum predicate).
pub fn is_cancellable(&self) -> bool {
    !matches!(
        self,
        CommandSpec::GitResetHard
            | CommandSpec::GitResetHardFetch
            | CommandSpec::GitPull
            | CommandSpec::GitPush
            | CommandSpec::GitRebase { .. }
            | CommandSpec::GitCheckout { .. }
            | CommandSpec::GitCheckoutNew { .. }
            | CommandSpec::GitFetch
    )
}
```

**Variant count check (research A9):** exactly 8 git variants return false; the
other 15 return true (default branch).

**Test pattern** — add to existing `#[cfg(test)] mod tests` (or create one if
absent). Mirror shape from `src/domain/refresh.rs:91-223`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_cancellable_git_variants_all_false() {
        for spec in [
            CommandSpec::GitResetHard, CommandSpec::GitResetHardFetch,
            CommandSpec::GitPull, CommandSpec::GitPush,
            CommandSpec::GitRebase { target: "m".into() },
            CommandSpec::GitCheckout { branch: "m".into() },
            CommandSpec::GitCheckoutNew { branch: "m".into() },
            CommandSpec::GitFetch,
        ] {
            assert!(!spec.is_cancellable(), "{:?} must not be cancellable", spec.label());
        }
    }

    #[test]
    fn is_cancellable_yarn_variants_all_true() {
        for spec in [
            CommandSpec::YarnInstall, CommandSpec::YarnPodInstall,
            CommandSpec::YarnUnitTests, CommandSpec::YarnCheckTypes,
            CommandSpec::YarnJest { filter: "".into() }, CommandSpec::YarnLint,
        ] {
            assert!(spec.is_cancellable(), "{:?} must be cancellable", spec.label());
        }
    }

    // ... 5 more tests per family per research §Wave 0 Gaps
}
```

### `tests/common/mod.rs` (modify — F-004 follow-on)

**Analog:** existing `tests/common/mod.rs:17-28` (`fake_metro_handle`) — type
shift only, same structure.

**Current (lines 17-28):**

```rust
pub fn fake_metro_handle(pid: u32, worktree: &str) -> MetroHandle {
    let (stdin_tx, _stdin_rx) = tokio::sync::mpsc::unbounded_channel();
    let (kill_tx, _kill_rx) = tokio::sync::oneshot::channel();
    MetroHandle {
        pid, worktree_id: worktree.to_string(),
        stdin_tx, stream_task: tokio::spawn(async {}),
        stdin_task: tokio::spawn(async {}),
        kill_tx: Some(kill_tx),
    }
}
```

**After F-004** (struct → trait):

```rust
use rn_dash::domain::ports::metro_port::MetroHandle;

struct FakeMetroHandle { pid: u32, worktree_id: String }

impl std::fmt::Debug for FakeMetroHandle { /* Debug derive fine */ }

impl MetroHandle for FakeMetroHandle {
    fn pid(&self) -> u32 { self.pid }
    fn worktree_id(&self) -> &str { &self.worktree_id }
    fn send_stdin(&self, _bytes: Vec<u8>) -> anyhow::Result<()> { Ok(()) }
    fn kill(self: Box<Self>) -> anyhow::Result<()> { Ok(()) }
}

pub fn fake_metro_handle(pid: u32, worktree: &str) -> Box<dyn MetroHandle> {
    Box::new(FakeMetroHandle { pid, worktree_id: worktree.to_string() })
}
```

**Also update** `src/domain/metro.rs::tests::dummy_handle` (lines 182-193) with
the same pattern — either inline a `FakeMetroHandle` there or expose a
`pub(crate) mod testing` with a shared helper.

### `tests/metro_single_instance.rs` (modify — F-201 consumer)

**Analog:** existing file (72 lines). Two tests — signature updates only.

**Before** (lines 20-32):

```rust
let (metro_tx, _metro_rx) = tokio::sync::mpsc::unbounded_channel();
let (handle_tx, _handle_rx) = tokio::sync::mpsc::unbounded_channel();
let mut state = AppState::default();
state.metro.register(fake_metro_handle(9999, "wt-a"));
update(&mut state, Action::MetroStart, &metro_tx, &handle_tx);
assert!(state.pending_restart, "COVER-01: ...");
```

**After F-201** (plan 13-07):

```rust
// No channels — pure function.
let mut state = AppState::default();
state.metro_state.metro.register(fake_metro_handle(9999, "wt-a"));
let effects = update(&mut state, Action::MetroStart);
assert!(state.metro_state.pending_restart, "COVER-01: ...");
// effects.len() depends on impl — MetroStart-while-running fan-out:
// [Effect::..., Effect::...] — assert specific Effect variants if behaviorally
// meaningful, otherwise just the state assertions.
```

**Note:** field path changes from `state.metro` → `state.metro_state.metro`
AFTER F-209 (plan 13-10). Between F-201 (plan 13-07) and F-209 (plan 13-10),
tests use flat `state.metro` / `state.pending_restart`. Plan 13-10 rewrites
these.

### `Makefile` entry `arch-lint` (NEW — validation scaffolding)

**Analog:** `Makefile:24-27` (`cov-check` target — same shape).

**Copy pattern:**

```makefile
# Architecture grep guards — verifies Phase 13 hexagonal invariants. Run after
# every wave. A failure here indicates a regression (trait/impl placement drift).
arch-lint:
	@echo "=== F-101: infra never imports Action ==="
	@! rg 'use crate::(domain::)?action' src/infra/
	@echo "=== F-202: app never imports infra ==="
	@! rg 'crate::infra::' src/app/
	@echo "=== F-300: ui never imports infra ==="
	@! rg 'crate::infra::' src/ui/
	@echo "=== F-201: update() is pure ==="
	@! rg 'tokio::spawn' src/app/update.rs
	@echo "=== domain never imports app or ratatui ==="
	@! rg 'use crate::app' src/domain/
	@! rg 'use (ratatui|crossterm)' src/domain/
	@echo "=== Required files exist ==="
	@test -f src/domain/action.rs
	@test -f src/domain/jira.rs
	@test -f src/domain/pipeline.rs
	@test -f src/app/effect.rs
	@test -f src/app/keybindings.rs
	@test -f src/infra/metro.rs
	@test ! -f src/action.rs
	@test ! -f src/app.rs
	@test ! -f src/infra/tmux.rs
	@echo "=== Required traits exported ==="
	@grep -q 'pub trait CommandRunnerPort' src/domain/ports/command_runner_port.rs
	@grep -q 'pub trait MetroPort' src/domain/ports/metro_port.rs
	@grep -q 'pub enum Effect' src/app/effect.rs
	@grep -q 'pub enum Recipe' src/domain/pipeline.rs
	@grep -q 'pub fn is_cancellable' src/domain/command.rs
	@grep -q 'pub const KEYBINDINGS' src/app/keybindings.rs
	@echo "arch-lint: PASS"

.PHONY: arch-lint
```

## Shared Patterns

Cross-cutting conventions that apply across multiple plans.

### Error handling — `anyhow::Result<T>`

**Source:** `src/infra/jira.rs:47`, `src/infra/process.rs:25`, every other
infra `async fn` returns `anyhow::Result<T>`.

**Apply to:** every new port trait method and every new infra adapter method
that performs I/O.

```rust
async fn add(&self, repo_root: &Path, branch_name: &str) -> anyhow::Result<PathBuf>;
```

**Do not use** `thiserror` for new ports — the codebase uses `anyhow` for I/O
error propagation (research §Standard Stack). `thiserror` is available but
lightly used.

### Async trait — `#[async_trait::async_trait]`

**Source:** `src/infra/jira.rs:22,62`, `src/infra/process.rs:16,31`.

**Apply to:** every new port trait with at least one `async fn` method.

```rust
#[async_trait::async_trait]
pub trait ProcessPort: Send + Sync {
    async fn spawn_metro(&self, worktree_path: PathBuf) -> anyhow::Result<Child>;
}
```

**Do NOT** use native AFIT (async-fn-in-trait) — research §Standard Stack
Alternatives: codebase consistency wins.

### Trait objects — `Arc<dyn Port>` for cross-task sharing

**Source:** `src/app.rs:120` (`Option<Arc<dyn JiraClient>>`).

**Apply to:** every port held by `Adapters` struct that may be cloned into
tokio::spawn closures (CommandRunnerPort, MetroPort — spawning from
effect_runner clones these).

**`Box<dyn Port>` for singletons:** `multiplexer` at `src/app.rs:130` uses
`Box` — single-owner, never cloned. Match that pattern if a port is known
to be single-owner. When in doubt, `Arc`.

### Pure parsers stay module-private

**Source:** `src/infra/devices.rs:32-100` (pure parsers) + async wrappers in
same file. `src/infra/worktrees.rs:23-89` same.

**Apply to:** `infra/metro.rs::parse_metro_line` + `extract_percent` (both pure,
move from app.rs as `fn` not `pub fn`). Keep them next to the I/O that consumes
them; domain layer does NOT need them.

### Inline `#[cfg(test)] mod tests` with pure-domain tests

**Source:** `src/domain/refresh.rs:71-248` (17 tests), `src/domain/metro.rs:175-227`
(3 tests).

**Apply to:** `src/domain/pipeline.rs` (Recipe + Prerequisite), `src/domain/jira.rs`
(extract_jira_key — 6 tests moved from infra), `src/domain/command.rs` (REFACTOR-02
is_cancellable tests).

**Format:** one test fn per variant family; tests MUST be pure (no tokio, no
I/O); use `#[test]` not `#[tokio::test]` for pure-domain tests.

### Behavior preservation — `kill_on_drop(true)` + `process_group(0)`

**Source:** `src/infra/command_runner.rs:49` (`kill_on_drop`) + `src/infra/process.rs:41`
(`process_group(0)`). **Also:** `src/app.rs:2278` (`libc::kill(-pid, SIGKILL)`
PGID broadcast).

**Apply to:** every new adapter that spawns long-running processes. Specifically:
- `TokioCommandRunner` (must keep both from existing command_runner.rs:44-50)
- `TokioMetroAdapter` (must keep both — COVER-02 test guards PGID kill)

**Test guard:** `tests/process_group_kill.rs` fails if PGID semantics drift.

### Pragmatic tokio exception in domain

**Source:** `src/domain/metro.rs:5-13` — architectural comment justifying the
exception.

**Apply to:** any new domain type that needs tokio types (MetroHandle replacement,
CommandRunnerPort's `UnboundedReceiver<CommandEvent>`). Include a parallel
doc-comment explaining why. Research Pitfall 8 / Open Question Q3 notes this
is a live decision.

### Per-module threshold ratchets (coverage)

**Source:** `.planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md` +
`Makefile::cov-baseline` / `cov-check` targets.

**Apply to:** file moves that restructure coverage rows (F-002, F-107, F-200).
Every plan that moves or splits a file MUST update COVERAGE-THRESHOLDS.md in
the same commit — new rows for new files, obsolete rows deleted, changelog
entry appended. Research Pitfall 7 spells out the full procedure.

## No Analog Found

| File | Role | Why no analog | Fallback |
|------|------|---------------|----------|
| `src/domain/pipeline.rs` `Recipe::expand` | domain-pure | No existing "domain command orchestration" type — refresh.rs is the closest pattern (pure fn + `match`) but doesn't compose commands | Copy refresh.rs's inline-test shape; use research §Pattern 2 sketch (lines 355-430) for the implementation. The audit's 11-line enumeration of inline-prereq sites (AUDIT.md:731-739) drives which arms Recipe must replace. |
| `src/app/effect_runner.rs` | app-layer composite | No existing component owns both tokio::spawn AND a port-trait struct. Closest structural analog is `spawn_metro_task` at `src/app.rs:2209-2256` (owns spawn + channels) | Compose two existing patterns: spawn_metro_task's shape + infra/command_runner.rs:26-70's channel-streaming shape. See research §Code Examples C (lines 1138-1151) for the translation skeleton. |

## Anti-patterns to Avoid

From research §Anti-Patterns to Avoid + §Common Pitfalls — these are the
specific drift modes the planner must surface in task lists.

1. **Effect enum impurity loophole.** Never add `Effect::RunArbitraryClosure(Box<dyn Fn>)`
   or similar. Every variant is plain data — no closures, no task handles.
   Research line 664.

2. **Palette `_ => Some(Action::ModalCancel)` as per-key wildcard.** The
   fallback is CONTEXT-level, not KEY-level. See research Pitfall 4 + §Pattern 3
   Pitfall. Encode as post-loop context-fallback in `handle_key`.

3. **`#[cfg(test)] mod dispatch_tests;` forgotten in `src/app/mod.rs`.** Silent
   test loss — 17 COVER-03 tests disappear from build. Research Pitfall 1.

4. **`fake_metro_handle` not updated in same commit as F-004 trait change.**
   Integration tests break compilation. Research Pitfall 2.

5. **Deleting `pending_restart` thinking F-204 Recipe absorbs it.** Only
   `pending_metro_run`/`pending_metro_after_sync` die; `pending_restart` survives
   as metro-lifecycle state (moved into `MetroState` sub-struct). Research
   Pitfall 3.

6. **Reopening F-501 category-split mid-phase.** LOCKED to flat-enum
   `is_cancellable()` unless `/gsd:discuss-phase 13` explicitly overrides.
   Research Pitfall 9.

7. **F-209 sub-struct grouping before F-204 Recipe consumer.** Codifies flags
   F-204 deletes, then has to re-group. Enforce ordering: 13-09 before 13-10.
   Research Pitfall 10.

8. **`tokio::spawn` in `src/app/update.rs`.** Grep-guard `arch-lint` catches
   this; if it appears, F-201 is incomplete. Every inline spawn becomes an
   `Effect::` return.

9. **`crate::infra::*` imports in `src/app/*.rs`.** Grep-guard `arch-lint`
   catches this; if any survive after F-202 consumer, that plan is incomplete.
   Every reference goes through `self.adapters.<port>.<method>(...)`.

10. **`use crate::action::Action` in `src/infra/command_runner.rs`.** The F-101
    anchor violation — replace with `CommandEvent` (typed, domain-defined).
    Grep-guard catches it.

11. **Deleting `command_queue` entirely.** It's both a prereq-ordering queue
    AND a dispatch FIFO for CommandOutput-line bursts during spawn. F-204
    replaces the prereq role; the FIFO role may persist (or lift into Phase 14's
    per-worktree queue). Do not wholesale-delete. Research §Anti-Patterns line 670.

## Metadata

**Analog search scope:** `src/{domain,infra,app,ui,*}/*.rs`, `tests/*.rs`,
`.planning/phases/{11-architecture-audit,12-coverage-gate,13-audit-driven-refactors}/*.md`,
`Makefile`.

**Files scanned:** 34 source files (all of `src/` except `src/app/dispatch_tests.rs`
which is test-only and already in place) + 3 test files + 4 planning docs.

**Pattern extraction date:** 2026-04-24.

**Plan-to-file mapping summary** (how the planner distributes files across
the 10 plans from research §Recommended Target Plans):

| Plan | Primary files assigned | Analog summary |
|------|------------------------|----------------|
| 13-01 | `src/domain/action.rs`, `src/domain/jira.rs`, `src/domain/ports/{mod,process_port,jira_port,multiplexer_port}.rs`; modify `src/infra/{process,jira,multiplexer}.rs` + `src/ui/panels.rs` + `src/lib.rs` | File moves; trait copies from existing trait blocks |
| 13-02 | modify `src/domain/command.rs` (add `is_cancellable`) | Mirror `is_destructive` pattern (same file, lines 108-117) |
| 13-03 | `src/app/effect.rs`, `src/domain/pipeline.rs`, `src/domain/ports/metro_port.rs`; modify `src/domain/metro.rs` + `tests/common/mod.rs` | Effect mirrors Action shape; pipeline mirrors refresh.rs; MetroPort mirrors ProcessClient shape |
| 13-04 | `src/domain/ports/{worktree_port,device_port,port_probe_port}.rs`; modify `src/infra/{worktrees,devices,port}.rs` | Adapter-shell pattern: multiplexer.rs is the template |
| 13-06 | `src/app/{mod,state,update,effect_runner(stub),handle_key,runtime,adapters(stub)}.rs`; delete `src/app.rs` | Verbatim relocation of current `src/app.rs` contents |
| 13-07 | rewrite `src/app/update.rs` (F-201 consumer); rewrite `src/app/handle_key.rs` (F-208); add `src/app/keybindings.rs` (F-400); add `src/infra/metro.rs` (F-203 + F-004) with 7 helpers moved in; update `tests/metro_single_instance.rs` | Research §Pattern 1 + §Pattern 3 + command_runner.rs structural analog for infra/metro.rs |
| 13-08 | populate `src/app/adapters.rs` (F-202) + `src/app/effect_runner.rs` (F-201 runtime); rewrite `src/infra/command_runner.rs` (F-101 — `CommandEvent`); all `crate::infra::*` in `src/app/*` disappear | Research §Code Examples B + C |
| 13-09 | rewrite `update.rs` Recipe dispatch sites (F-204 consumer); exhaustive modal arms (F-205) | Use Recipe from 13-03 pipeline.rs; arms become explicit match |
| 13-10 | F-209 sub-struct grouping (modify `src/app/state.rs` + touch ~450 field access sites); rewrite `src/ui/footer.rs` + `src/ui/help_overlay.rs` (F-302/F-303); delete `src/infra/tmux.rs`; move `is_inside_tmux` out of `infra/jira.rs` (F-108) | Compiler-driven field-access churn; KEYBINDINGS from 13-07 drives footer/help_overlay bodies |

All analog references in §Pattern Assignments cite specific line ranges from
already-committed files the planner can read directly via the `Read` tool.
