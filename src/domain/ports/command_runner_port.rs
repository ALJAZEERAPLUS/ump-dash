//! Command runner port — F-101 domain boundary for subprocess dispatch.
//!
//! The pre-Phase-13 adapter (`src/infra/command_runner.rs`) imported
//! `crate::domain::action::Action` and sent `Action::CommandOutputLine`
//! / `Action::CommandExited` directly on its output channel. That is the
//! F-101 Fowler violation: the infra (Data-Source) layer knew the
//! app-layer (Service) messaging grammar.
//!
//! This port closes the violation by introducing a typed domain event —
//! `CommandEvent` — as the adapter's output grammar. The adapter emits
//! `CommandEvent`, and the app-layer consumer (currently the inline
//! translation in `src/app.rs::dispatch_command`; eventually Plan 13-08's
//! `effect_runner`) translates `CommandEvent → Action` at the boundary.
//!
//! Naming note: `spawn` returns the receiver *synchronously*. The actual
//! `tokio::spawn` happens inside the fn body, so this trait has no async
//! methods and does not need `#[async_trait]`.

#![allow(dead_code)]

use crate::domain::command::CommandSpec;
use std::path::PathBuf;
use std::process::ExitStatus;

/// Typed lifecycle events emitted by a `CommandRunnerPort` implementation.
///
/// Stream shape: zero or more `OutputLine(..)` events (interleaved stdout+stderr
/// in arrival order), followed by exactly one `Exited(status)` event, after
/// which the receiver closes.
#[derive(Debug)]
pub enum CommandEvent {
    /// A single line of stdout or stderr from the running subprocess.
    OutputLine(String),
    /// The subprocess has terminated. Carries the OS-reported `ExitStatus`.
    Exited(ExitStatus),
}

/// Spawns a command and streams its lifecycle as typed domain events.
///
/// Implementations MUST:
/// - Build argv from `spec` (honoring any per-variant runtime substitutions
///   such as `GitResetHard` → `reset --hard origin/{branch}`),
/// - Spawn the subprocess with `cwd` as its working directory,
/// - Preserve `kill_on_drop` semantics (dropping the owning task or receiver
///   terminates the subprocess tree),
/// - Emit each stdout/stderr line as `CommandEvent::OutputLine`,
/// - Emit exactly one `CommandEvent::Exited(status)` after the process
///   terminates (or after a spawn failure), then close the receiver.
pub trait CommandRunnerPort: Send + Sync {
    /// Spawn the command described by `spec` in `cwd` with contextual `branch`
    /// (used by per-variant argv builders like `GitResetHard`).
    ///
    /// Returns the receiver *synchronously*; the subprocess runs on a
    /// background `tokio::spawn`ed task.
    fn spawn(
        &self,
        spec: CommandSpec,
        cwd: PathBuf,
        branch: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<CommandEvent>;
}
