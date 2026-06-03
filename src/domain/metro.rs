// src/domain/metro.rs
//
// Metro domain types — one worktree's Metro process handle and status.
//
// Plan 13-03: `MetroHandle` is now a TRAIT defined in
// `src/domain/ports/metro_port.rs`. This module re-exports it for callers
// that already import `crate::domain::metro::MetroHandle`. The tokio leak
// flagged by audit F-004 is gone — the trait signature hides the channel
// types, and the only concrete impls live infra-side (Plan 13-07:
// `TokioMetroAdapter`) and temporarily inside `src/app.rs` (`InAppMetroHandle`
// bridge, removed by Plan 13-07).
//
// Architectural note: `WorktreeMetro` is scoped to one WorktreeSlice. Multiple
// worktrees can run Metro concurrently because each slice owns its own
// WorktreeMetro value.

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
#[derive(Debug, Clone, PartialEq, Default)]
pub enum MetroStatus {
    /// No metro instance is running.
    #[default]
    Stopped,
    /// Metro is running with the given OS pid, selected port, and source worktree.
    Running {
        pid: u32,
        worktree_id: String,
        port: u16,
    },
    /// Spawn is in flight — transient state between MetroStart and first log line.
    Starting,
    /// Kill + port-free wait is in flight — transient state between MetroStop and port free.
    Stopping,
}

/// Tracks one worktree's Metro process.
///
/// All metro state transitions go through WorktreeMetro methods. The update() function
/// in app.rs calls these methods — it never manipulates handles directly.
#[derive(Debug)]
pub struct WorktreeMetro {
    /// Private — callers cannot bypass the one-Metro-per-worktree ownership check.
    ///
    /// Owns a `Box<dyn MetroHandle>` trait object so the concrete type
    /// (infra adapter or app-side bridge) stays invisible to the domain.
    handle: Option<Box<dyn MetroHandle>>,
    /// Public read-only status for UI rendering.
    pub status: MetroStatus,
    /// Most recent activity parsed from this worktree's metro stdout.
    pub activity: Option<MetroActivity>,
    /// Port selected before the process handle arrives. Once running, the
    /// handle's port is authoritative.
    reserved_port: Option<u16>,
}

impl Default for WorktreeMetro {
    fn default() -> Self {
        Self::new()
    }
}

impl WorktreeMetro {
    /// Create a new per-worktree Metro runtime in the Stopped state.
    pub fn new() -> Self {
        Self {
            handle: None,
            status: MetroStatus::Stopped,
            activity: None,
            reserved_port: None,
        }
    }

    /// True if this worktree currently owns a Metro handle.
    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }

    /// Register a freshly spawned process handle.
    ///
    /// # Panics
    /// Panics if called while this worktree already owns a handle. Callers
    /// must take and kill the handle before registering a replacement.
    pub fn register(&mut self, handle: Box<dyn MetroHandle>) {
        assert!(
            self.handle.is_none(),
            "BUG: WorktreeMetro::register() called with an existing handle — kill first"
        );
        let pid = handle.pid();
        let worktree_id = handle.worktree_id().to_string();
        let port = handle.port();
        self.handle = Some(handle);
        self.reserved_port = Some(port);
        self.status = MetroStatus::Running {
            pid,
            worktree_id,
            port,
        };
    }

    /// TCP port selected for this worktree's Metro instance.
    pub fn running_port(&self) -> Option<u16> {
        self.handle
            .as_ref()
            .map(|handle| handle.port())
            .or(self.reserved_port)
    }

    /// TCP port from a registered Metro process handle.
    ///
    /// Unlike `running_port`, this excludes a merely reserved startup port.
    pub fn process_port(&self) -> Option<u16> {
        self.handle.as_ref().map(|handle| handle.port())
    }

    /// Clear the handle after the process has been killed and reaped.
    /// Transitions status to Stopped and clears activity state.
    pub fn clear(&mut self) {
        self.handle = None;
        self.status = MetroStatus::Stopped;
        self.activity = None;
        self.reserved_port = None;
    }

    /// Send a raw byte sequence to metro's stdin via the background stdin-writer task.
    ///
    /// No-op if metro is not running. Delegates to the handle's trait method — the
    /// concrete impl owns the tokio channel.
    #[allow(dead_code)]
    pub fn send_stdin(&self, bytes: Vec<u8>) -> anyhow::Result<()> {
        if let Some(handle) = self.handle.as_ref() {
            handle.send_stdin(bytes)?;
        }
        Ok(())
    }

    /// Reserve the selected port and transition to Starting.
    pub fn reserve_start(&mut self, port: u16) {
        self.status = MetroStatus::Starting;
        self.activity = Some(MetroActivity::Starting);
        self.reserved_port = Some(port);
    }

    /// Transition to Starting state (spawn is in flight).
    pub fn set_starting(&mut self) {
        self.status = MetroStatus::Starting;
        self.activity = Some(MetroActivity::Starting);
    }

    /// Transition to Stopping state (kill + port-free wait is in flight).
    pub fn set_stopping(&mut self) {
        if self.handle.is_none() {
            self.status = MetroStatus::Stopping;
        }
    }

    pub fn record_activity(&mut self, activity: MetroActivity) {
        self.activity = Some(activity);
    }

    pub fn activity(&self) -> Option<&MetroActivity> {
        self.activity.as_ref()
    }

    /// Take ownership of the handle for kill operations.
    ///
    /// Returns None if no metro is running.
    pub fn take_handle(&mut self) -> Option<Box<dyn MetroHandle>> {
        let handle = self.handle.take();
        if handle.is_some() {
            self.status = MetroStatus::Stopped;
            self.activity = None;
            self.reserved_port = None;
        }
        handle
    }
}

// ---------------------------------------------------------------------------
// Tests — COVER-01 characterization of the one-Metro-per-worktree invariant
// at the WorktreeMetro::register() type boundary (D-09 first layer).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal trait-object impl used only to exercise `WorktreeMetro::register /
    /// is_running / take_handle / clear`. The tokio channels that the production
    /// adapter owns are deliberately absent — `send_stdin` / `kill` are no-ops.
    #[derive(Debug)]
    struct DummyHandle {
        pid: u32,
        worktree_id: String,
        port: u16,
    }

    impl MetroHandle for DummyHandle {
        fn pid(&self) -> u32 {
            self.pid
        }
        fn worktree_id(&self) -> &str {
            &self.worktree_id
        }
        fn port(&self) -> u16 {
            self.port
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
            port: 8081,
        })
    }

    #[test]
    #[should_panic(expected = "BUG: WorktreeMetro::register() called with an existing handle")]
    fn register_twice_in_same_worktree_panics() {
        let mut metro = WorktreeMetro::new();
        metro.register(dummy_handle(1));
        metro.register(dummy_handle(2));
    }

    #[test]
    fn register_once_then_clear_allows_second_register() {
        // Positive-case safety net — the test above only asserts panic on
        // double-register; this one asserts the legitimate sequence works.
        let mut metro = WorktreeMetro::new();
        metro.register(dummy_handle(10));
        assert!(metro.is_running());
        metro.clear();
        assert!(!metro.is_running());
        metro.register(dummy_handle(11)); // must not panic
        assert!(metro.is_running());
    }

    #[test]
    fn new_manager_is_stopped_not_running() {
        // Smallest possible smoke test — no runtime, no handle construction.
        let metro = WorktreeMetro::new();
        assert!(!metro.is_running());
        assert!(matches!(metro.status, MetroStatus::Stopped));
        assert!(metro.activity.is_none());
    }
}
