// src/domain/metro.rs
//
// Metro domain types — single-instance invariant and status tracking.
//
// Plan 13-03: `MetroHandle` is now a TRAIT defined in
// `src/domain/ports/metro_port.rs`. This module re-exports it for callers
// that already import `crate::domain::metro::MetroHandle`. The tokio leak
// flagged by audit F-004 is gone — the trait signature hides the channel
// types, and the only concrete impls live infra-side (Plan 13-07:
// `TokioMetroAdapter`) and temporarily inside `src/app.rs` (`InAppMetroHandle`
// bridge, removed by Plan 13-07).
//
// Architectural note: `MetroManager.handle` is `Option<Box<dyn MetroHandle>>`.
// The single-instance invariant still holds at the type level — you cannot
// register a second handle without first taking the existing one.

/// Re-export the `MetroHandle` trait from the ports module for convenience.
/// Callers using `crate::domain::metro::MetroHandle` continue to resolve.
pub use crate::domain::ports::metro_port::MetroHandle;

/// Real-time activity state parsed from metro bundler stdout.
#[derive(Debug, Clone, PartialEq)]
pub enum MetroActivity {
    Starting,
    Ready,
    Bundling { percent: Option<u8> },
    DeviceConnected,
    Error(String),
    Exited,
}

impl std::fmt::Display for MetroActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Starting => write!(f, "Starting..."),
            Self::Ready => write!(f, "Ready"),
            Self::Bundling { percent: Some(p) } => write!(f, "Bundling {p}%"),
            Self::Bundling { percent: None } => write!(f, "Bundling..."),
            Self::DeviceConnected => write!(f, "Device connected"),
            Self::Error(msg) => write!(f, "Error: {msg}"),
            Self::Exited => write!(f, "Exited"),
        }
    }
}

/// Current observable state of the metro process as seen by the domain layer.
#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
pub enum MetroStatus {
    /// No metro instance is running.
    #[default]
    Stopped,
    /// Metro is running with the given OS pid and the worktree it was started from.
    Running { pid: u32, worktree_id: String },
    /// Spawn is in flight — transient state between MetroStart and first log line.
    Starting,
    /// Kill + port-free wait is in flight — transient state between MetroStop and port free.
    Stopping,
}

/// Enforces the single-instance invariant: at most one metro process may run at a time.
///
/// All metro state transitions go through MetroManager methods. The update() function
/// in app.rs calls these methods — it never manipulates handles directly.
#[derive(Debug)]
pub struct MetroManager {
    /// Private — callers cannot bypass the single-instance check.
    ///
    /// Owns a `Box<dyn MetroHandle>` trait object so the concrete type
    /// (infra adapter or app-side bridge) stays invisible to the domain.
    handle: Option<Box<dyn MetroHandle>>,
    /// Public read-only status for UI rendering.
    pub status: MetroStatus,
    /// Most recent activity parsed from metro stdout. None when metro is not running.
    pub activity: Option<MetroActivity>,
}

impl Default for MetroManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MetroManager {
    /// Create a new manager in the Stopped state.
    pub fn new() -> Self {
        Self {
            handle: None,
            status: MetroStatus::Stopped,
            activity: None,
        }
    }

    /// True if a metro handle is currently registered (process is running or finishing).
    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }

    /// Register a freshly spawned process handle.
    ///
    /// # Panics
    /// Panics if called while a handle already exists. Callers MUST call `take_handle()`
    /// and kill the process before registering a new one.
    pub fn register(&mut self, handle: Box<dyn MetroHandle>) {
        assert!(
            self.handle.is_none(),
            "BUG: MetroManager::register() called with an existing handle — kill first"
        );
        let pid = handle.pid();
        let worktree_id = handle.worktree_id().to_string();
        self.handle = Some(handle);
        self.status = MetroStatus::Running { pid, worktree_id };
    }

    /// Clear the handle after the process has been killed and reaped.
    /// Transitions status to Stopped and clears activity state.
    pub fn clear(&mut self) {
        self.handle = None;
        self.status = MetroStatus::Stopped;
        self.activity = None;
    }

    /// Send a raw byte sequence to metro's stdin via the background stdin-writer task.
    ///
    /// No-op if metro is not running. Delegates to the handle's trait method — the
    /// concrete impl owns the tokio channel.
    #[allow(dead_code)]
    pub fn send_stdin(&self, bytes: Vec<u8>) -> anyhow::Result<()> {
        if let Some(ref h) = self.handle {
            h.send_stdin(bytes)?;
        }
        Ok(())
    }

    /// Transition to Starting state (spawn is in flight).
    pub fn set_starting(&mut self) {
        self.status = MetroStatus::Starting;
        self.activity = Some(MetroActivity::Starting);
    }

    /// Transition to Stopping state (kill + port-free wait is in flight).
    pub fn set_stopping(&mut self) {
        self.status = MetroStatus::Stopping;
    }

    /// Take ownership of the handle for kill operations.
    ///
    /// Returns None if metro is not running. After this call is_running() returns false,
    /// so register() can be called again once the kill completes. Callers typically
    /// follow up with `handle.kill()` on the returned `Box<dyn MetroHandle>`.
    pub fn take_handle(&mut self) -> Option<Box<dyn MetroHandle>> {
        self.handle.take()
    }
}

// ---------------------------------------------------------------------------
// Tests — COVER-01 characterization of the single-instance invariant at the
// MetroManager::register() type boundary (D-09 first layer).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal trait-object impl used only to exercise `MetroManager::register /
    /// is_running / take_handle / clear`. The tokio channels that the production
    /// adapter owns are deliberately absent — `send_stdin` / `kill` are no-ops.
    #[derive(Debug)]
    struct DummyHandle {
        pid: u32,
        worktree_id: String,
    }

    impl MetroHandle for DummyHandle {
        fn pid(&self) -> u32 {
            self.pid
        }
        fn worktree_id(&self) -> &str {
            &self.worktree_id
        }
        fn send_stdin(&self, _bytes: Vec<u8>) -> anyhow::Result<()> {
            Ok(())
        }
        fn kill(self: Box<Self>) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn dummy_handle(pid: u32) -> Box<dyn MetroHandle> {
        Box::new(DummyHandle {
            pid,
            worktree_id: format!("wt-{pid}"),
        })
    }

    #[test]
    #[should_panic(expected = "BUG: MetroManager::register() called with an existing handle")]
    fn register_twice_panics() {
        // COVER-01 — D-09 (a): the debug-assert on double-register is load-bearing.
        // Phase 13+ refactors that introduce a second MetroHandle construction path
        // MUST fail here. (No tokio runtime required post-13-03 — DummyHandle is
        // synchronous, so the test is now a plain `#[test]`.)
        let mut mgr = MetroManager::new();
        mgr.register(dummy_handle(1));
        mgr.register(dummy_handle(2)); // must panic
    }

    #[test]
    fn register_once_then_clear_allows_second_register() {
        // Positive-case safety net — the test above only asserts panic on
        // double-register; this one asserts the legitimate sequence works.
        let mut mgr = MetroManager::new();
        mgr.register(dummy_handle(10));
        assert!(mgr.is_running());
        mgr.clear();
        assert!(!mgr.is_running());
        mgr.register(dummy_handle(11)); // must not panic
        assert!(mgr.is_running());
    }

    #[test]
    fn new_manager_is_stopped_not_running() {
        // Smallest possible smoke test — no runtime, no handle construction.
        let mgr = MetroManager::new();
        assert!(!mgr.is_running());
        assert!(matches!(mgr.status, MetroStatus::Stopped));
        assert!(mgr.activity.is_none());
    }
}
