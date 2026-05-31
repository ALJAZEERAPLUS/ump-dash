//! Per-worktree task + queue + output slice — Phase 14 / D-01, D-02.
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
//! Plus the two output map entries (`command_output_by_worktree`,
//! `command_output_scroll_by_worktree`) move to per-slice `output` + `output_scroll`.

#![allow(dead_code)]

use crate::domain::action::Action;
use crate::domain::command::{CommandSpec, RunVariant};
use crate::domain::metro::WorktreeMetro;
use crate::domain::task::TaskRecord;
use crate::domain::worktree::WorktreeId;
use std::collections::VecDeque;

/// Last fully selected UMP run config for one platform in one worktree.
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
    pub output: VecDeque<String>,
    pub output_scroll: usize,
    pub post_drain: Option<Box<Action>>,
    pub last_android_run: Option<LastRunConfig>,
    pub last_ios_run: Option<LastRunConfig>,
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
    }

    #[test]
    fn default_slice_has_empty_output_and_zero_scroll() {
        let s = WorktreeSlice::default();
        assert!(s.output.is_empty());
        assert_eq!(s.output_scroll, 0);
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
