//! Per-worktree task + queue + log slice — Phase 14 / D-01, D-02.
//!
//! Pure domain. No I/O, no tokio. The `task` field holds an opaque
//! `Box<dyn TaskHandle>` (inside `TaskRecord`) so the domain layer never sees
//! `JoinHandle`. Mirrors `src/domain/refresh.rs` for inline-tests convention.
//!
//! This struct REPLACES four fields of `CommandRunnerState` in Phase 14:
//!   - `running_command: Option<CommandSpec>`     -> `task: Option<TaskRecord>`
//!   - `command_task: Option<JoinHandle<()>>`     -> inside `task.handle: Box<dyn TaskHandle>`
//!   - `command_queue: VecDeque<CommandSpec>`     -> `queue: VecDeque<CommandSpec>`
//!   - `post_drain_action: Option<Box<Action>>`   -> `post_drain: Option<Box<Action>>`
//!
//! Command logs are retained per slice so headless consumers can inspect task
//! history without coupling that history to a TUI panel.

#![allow(dead_code)]

use crate::domain::action::Action;
use crate::domain::command::{CommandSpec, RunVariant};
use crate::domain::metro::WorktreeMetro;
use crate::domain::native_cache::{
    AndroidCacheState, IosSimulatorCacheState, PendingCachedAndroidLaunch, PendingCachedIosLaunch,
};
use crate::domain::task::TaskRecord;
use crate::domain::worktree::WorktreeId;
use std::collections::VecDeque;

/// Maximum number of command log lines retained per worktree.
pub const MAX_LOG_LINES: usize = 1000;

/// Last fully selected UMP run config for one platform in one worktree.
/// Used by the "repeat last run" keybinding; cache use is decided at run time by
/// `dispatch_run`, so no cache flag is stored here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastRunConfig {
    pub device_id: String,
    pub variant: RunVariant,
}

/// Per-worktree state. One slice per `WorktreeId` lives in
/// `AppState.worktrees: HashMap<WorktreeId, WorktreeSlice>` (D-16).
#[derive(Debug, Default)]
pub struct WorktreeSlice {
    pub id: WorktreeId,
    pub metro: WorktreeMetro,
    pub task: Option<TaskRecord>,
    pub queue: VecDeque<CommandSpec>,
    pub logs: VecDeque<String>,
    pub post_drain: Option<Box<Action>>,
    pub last_android_run: Option<LastRunConfig>,
    pub last_ios_run: Option<LastRunConfig>,
    pub ios_simulator_cache: IosSimulatorCacheState,
    pub pending_cached_ios_launch: Option<PendingCachedIosLaunch>,
    pub android_cache: AndroidCacheState,
    pub pending_cached_android_launch: Option<PendingCachedAndroidLaunch>,
}

impl WorktreeSlice {
    pub fn append_log(&mut self, line: impl Into<String>) {
        self.logs.push_back(line.into());
        while self.logs.len() > MAX_LOG_LINES {
            self.logs.pop_front();
        }
    }

    pub fn log_tail(&self, tail: Option<usize>) -> Vec<String> {
        let total = self.logs.len();
        let count = tail.unwrap_or(total).min(total);
        self.logs.iter().skip(total - count).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_slice_has_no_task_and_empty_queue() {
        let s = WorktreeSlice::default();
        assert!(s.task.is_none());
        assert!(s.queue.is_empty());
        assert!(s.post_drain.is_none());
        assert!(s.last_android_run.is_none());
        assert!(s.last_ios_run.is_none());
        assert_eq!(s.ios_simulator_cache, IosSimulatorCacheState::Unknown);
        assert!(s.pending_cached_ios_launch.is_none());
        assert_eq!(s.android_cache, AndroidCacheState::Unknown);
        assert!(s.pending_cached_android_launch.is_none());
    }

    #[test]
    fn default_slice_has_empty_logs() {
        let s = WorktreeSlice::default();
        assert!(s.logs.is_empty());
    }

    #[test]
    fn command_logs_keep_only_the_newest_lines() {
        let mut s = WorktreeSlice::default();
        for index in 0..=MAX_LOG_LINES {
            s.append_log(format!("line-{index}"));
        }

        assert_eq!(s.logs.len(), MAX_LOG_LINES);
        assert_eq!(s.logs.front().map(String::as_str), Some("line-1"));
        assert_eq!(s.logs.back().map(String::as_str), Some("line-1000"));
    }

    #[test]
    fn command_log_tail_preserves_order_and_respects_the_requested_limit() {
        let mut s = WorktreeSlice::default();
        s.append_log("first");
        s.append_log("second");
        s.append_log("third");

        assert_eq!(s.log_tail(Some(2)), vec!["second", "third"]);
        assert_eq!(s.log_tail(None), vec!["first", "second", "third"]);
    }

    #[test]
    fn slice_with_explicit_id_preserves_id() {
        let s = WorktreeSlice {
            id: WorktreeId("wt-test".into()),
            ..Default::default()
        };
        assert_eq!(s.id, WorktreeId("wt-test".into()));
    }

    #[test]
    fn slice_owns_independent_metro_runtime_state() {
        let mut a = WorktreeSlice {
            id: WorktreeId("wt-a".into()),
            ..Default::default()
        };
        let mut b = WorktreeSlice {
            id: WorktreeId("wt-b".into()),
            ..Default::default()
        };

        a.metro.reserve_start(8081);
        b.metro.reserve_start(8082);

        assert_eq!(a.metro.running_port(), Some(8081));
        assert_eq!(b.metro.running_port(), Some(8082));
        assert!(!a.metro.is_running());
        assert!(!b.metro.is_running());
    }
}
