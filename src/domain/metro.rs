// src/domain/metro.rs
//
// Metro domain types — per-worktree process handles and status tracking.
//
// Plan 13-03: `MetroHandle` is now a TRAIT defined in
// `src/domain/ports/metro_port.rs`. This module re-exports it for callers
// that already import `crate::domain::metro::MetroHandle`. The tokio leak
// flagged by audit F-004 is gone — the trait signature hides the channel
// types, and the only concrete impls live infra-side (Plan 13-07:
// `TokioMetroAdapter`) and temporarily inside `src/app.rs` (`InAppMetroHandle`
// bridge, removed by Plan 13-07).
//
// Architectural note: `MetroManager.handles` is keyed by worktree id. Multiple
// worktrees can run Metro concurrently as long as each process owns a distinct
// port.

use std::collections::HashMap;

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

/// Tracks Metro processes by worktree.
///
/// All metro state transitions go through MetroManager methods. The update() function
/// in app.rs calls these methods — it never manipulates handles directly.
#[derive(Debug)]
pub struct MetroManager {
    /// Private — callers cannot bypass the per-worktree ownership check.
    ///
    /// Owns `Box<dyn MetroHandle>` trait objects so the concrete type
    /// (infra adapter or app-side bridge) stays invisible to the domain.
    handles: HashMap<String, Box<dyn MetroHandle>>,
    /// Public read-only summary status for legacy call sites and tests.
    pub status: MetroStatus,
    /// Most recent activity parsed from any metro stdout. None when no metro is running.
    pub activity: Option<MetroActivity>,
    activities: HashMap<String, MetroActivity>,
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
            handles: HashMap::new(),
            status: MetroStatus::Stopped,
            activity: None,
            activities: HashMap::new(),
        }
    }

    /// True if any metro handle is currently registered.
    pub fn is_running(&self) -> bool {
        !self.handles.is_empty()
    }

    /// True if a metro handle is currently registered for this worktree.
    pub fn is_running_for(&self, worktree_id: &str) -> bool {
        self.handles.contains_key(worktree_id)
    }

    /// Register a freshly spawned process handle.
    ///
    /// # Panics
    /// Panics if called while the same worktree already owns a handle. Callers
    /// must take and kill that worktree's handle before registering a replacement.
    pub fn register(&mut self, handle: Box<dyn MetroHandle>) {
        let worktree_id = handle.worktree_id().to_string();
        assert!(
            !self.handles.contains_key(&worktree_id),
            "BUG: MetroManager::register() called with an existing handle for this worktree — kill first"
        );
        let pid = handle.pid();
        let port = handle.port();
        self.handles.insert(worktree_id.clone(), handle);
        self.status = MetroStatus::Running {
            pid,
            worktree_id,
            port,
        };
    }

    /// TCP port for one registered Metro instance.
    pub fn running_port(&self) -> Option<u16> {
        self.handles.values().next().map(|handle| handle.port())
    }

    /// TCP port for the Metro instance registered to this worktree.
    pub fn running_port_for(&self, worktree_id: &str) -> Option<u16> {
        self.handles.get(worktree_id).map(|handle| handle.port())
    }

    pub fn running_worktree_ids(&self) -> impl Iterator<Item = &str> {
        self.handles.keys().map(String::as_str)
    }

    fn refresh_summary_status(&mut self) {
        if let Some((worktree_id, handle)) = self.handles.iter().next() {
            self.status = MetroStatus::Running {
                pid: handle.pid(),
                worktree_id: worktree_id.clone(),
                port: handle.port(),
            };
            self.activity = self.activities.get(worktree_id).cloned();
        } else {
            self.status = MetroStatus::Stopped;
            self.activity = None;
        }
    }

    /// Clear all handles after processes have been killed and reaped.
    /// Transitions summary status to Stopped and clears activity state.
    pub fn clear(&mut self) {
        self.handles.clear();
        self.status = MetroStatus::Stopped;
        self.activity = None;
        self.activities.clear();
    }

    /// Clear a single worktree handle after its process exits.
    pub fn clear_worktree(&mut self, worktree_id: &str) {
        self.handles.remove(worktree_id);
        self.activities.remove(worktree_id);
        self.refresh_summary_status();
    }

    /// Send a raw byte sequence to metro's stdin via the background stdin-writer task.
    ///
    /// No-op if metro is not running. Delegates to the handle's trait method — the
    /// concrete impl owns the tokio channel.
    #[allow(dead_code)]
    pub fn send_stdin(&self, bytes: Vec<u8>) -> anyhow::Result<()> {
        for handle in self.handles.values() {
            handle.send_stdin(bytes.clone())?;
        }
        Ok(())
    }

    pub fn send_stdin_to(&self, worktree_id: &str, bytes: Vec<u8>) -> anyhow::Result<()> {
        if let Some(handle) = self.handles.get(worktree_id) {
            handle.send_stdin(bytes)?;
        }
        Ok(())
    }

    /// Transition to Starting state (spawn is in flight).
    pub fn set_starting(&mut self) {
        self.status = MetroStatus::Starting;
        self.activity = Some(MetroActivity::Starting);
    }

    pub fn set_starting_for(&mut self, worktree_id: String) {
        self.status = MetroStatus::Starting;
        self.activity = Some(MetroActivity::Starting);
        self.activities.insert(worktree_id, MetroActivity::Starting);
    }

    /// Transition to Stopping state (kill + port-free wait is in flight).
    pub fn set_stopping(&mut self) {
        if self.handles.is_empty() {
            self.status = MetroStatus::Stopping;
        }
    }

    pub fn record_activity(&mut self, worktree_id: String, activity: MetroActivity) {
        self.activity = Some(activity.clone());
        self.activities.insert(worktree_id, activity);
    }

    pub fn activity_for(&self, worktree_id: &str) -> Option<&MetroActivity> {
        self.activities.get(worktree_id)
    }

    /// Take ownership of the handle for kill operations.
    ///
    /// Returns None if no metro is running.
    pub fn take_handle(&mut self) -> Option<Box<dyn MetroHandle>> {
        let worktree_id = self.handles.keys().next().cloned()?;
        self.take_handle_for(&worktree_id)
    }

    pub fn take_handle_for(&mut self, worktree_id: &str) -> Option<Box<dyn MetroHandle>> {
        let handle = self.handles.remove(worktree_id);
        self.activities.remove(worktree_id);
        self.refresh_summary_status();
        handle
    }

    pub fn take_all_handles(&mut self) -> Vec<Box<dyn MetroHandle>> {
        let handles = self.handles.drain().map(|(_, handle)| handle).collect();
        self.status = MetroStatus::Stopped;
        self.activity = None;
        self.activities.clear();
        handles
    }
}

// ---------------------------------------------------------------------------
// Tests — COVER-01 characterization of the per-worktree registration invariant
// at the MetroManager::register() type boundary (D-09 first layer).
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
    fn register_allows_multiple_worktrees() {
        let mut mgr = MetroManager::new();
        mgr.register(dummy_handle(1));
        mgr.register(dummy_handle(2));

        assert!(mgr.is_running_for("wt-1"));
        assert!(mgr.is_running_for("wt-2"));
        assert_eq!(mgr.running_port_for("wt-1"), Some(8081));
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
