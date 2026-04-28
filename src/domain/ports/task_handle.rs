//! Task lifecycle port — Phase 14 / D-03.
//!
//! `TaskHandle` is the opaque cancellation handle for a spawned subprocess
//! task. Domain holds `Box<dyn TaskHandle>` inside `TaskRecord`; the concrete
//! `tokio::task::JoinHandle<()>` wrapper lives infra-side at
//! `src/infra/task_handle.rs` (Plan 14-02).
//!
//! Mirrors the `MetroPort` / `MetroHandle` pattern from Plan 13-03 / F-004.
//!
//! Phase 14: `abort()` is `JoinHandle::abort()` — cooperative tokio cancel.
//! `kill_on_drop(true)` on the inner `Child` (configured in
//! `infra/command_runner.rs`) triggers process termination as a side effect
//! when the task body unwinds.
//!
//! Phase 15 (deferred per CONTEXT.md): widens this trait to add
//! SIGTERM/SIGKILL escalation.

#![allow(dead_code)]

/// Opaque handle to a live subprocess task. Implementations live infra-side.
///
/// The trait method is the only surface domain + app see; the concrete tokio
/// `JoinHandle<()>` is private to `src/infra/task_handle.rs`.
pub trait TaskHandle: Send + Sync + std::fmt::Debug {
    /// Cooperative cancel. Phase 14 contract: best-effort. Tests MUST NOT
    /// assert "child dead within X ms of abort()" — that's a Phase 15 widening.
    fn abort(&self);
}
