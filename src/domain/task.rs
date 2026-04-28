//! Task identity types — Phase 14 / D-04, D-06, D-09.
//!
//! Pure domain. `TaskId(u64)` from a process-wide `AtomicU64` (started at 1
//! so `0` is a sentinel for "no task" in tests). `TaskRecord` is the live-task
//! bag — id + spec + started-at + opaque `Box<dyn TaskHandle>` cancellation
//! handle. `ExitStatus` is a domain enum (NOT `std::process::ExitStatus`) so
//! Phase 15 can emit `Cancelled` cleanly without an infra type leaking into
//! the Action payload (G-15).
//!
//! Mirrors `src/domain/refresh.rs` for the inline-tests convention.

#![allow(dead_code)]

use crate::domain::command::CommandSpec;
use crate::domain::ports::task_handle::TaskHandle;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Process-wide monotonic task identity. `0` is the sentinel "no task"; the
/// production `next()` counter starts at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

impl TaskId {
    /// Production allocator. Returns `TaskId(N)` where N starts at 1 and
    /// increments per call. `Ordering::Relaxed` is canonical for monotonic
    /// counters (see std::sync::atomic::AtomicU64 rustdoc).
    pub fn next() -> Self {
        TaskId(NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Test injection — fixture supplies its own counter so tests stay
    /// isolated from the static `NEXT_TASK_ID`.
    pub fn next_for_test(counter: &AtomicU64) -> Self {
        TaskId(counter.fetch_add(1, Ordering::Relaxed))
    }
}

/// A live task record. Owned by `WorktreeSlice.task: Option<TaskRecord>`.
/// The `handle` is the SOLE owner of the cancellation handle — there is no
/// separate registry (Q3 RESEARCH lock: single ownership in slice.task.handle).
#[derive(Debug)]
pub struct TaskRecord {
    pub id: TaskId,
    pub spec: CommandSpec,
    pub started_at: Instant,
    pub handle: Box<dyn TaskHandle>,
}

/// Domain ExitStatus. Phase 14 emits Success/Failure only; Phase 15 emits
/// Cancelled and Killed when SIGTERM/SIGKILL escalation lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitStatus {
    Success,
    Failure { code: Option<i32> },
    Cancelled,
    Killed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_for_test_is_monotonic() {
        let counter = AtomicU64::new(100);
        let a = TaskId::next_for_test(&counter);
        let b = TaskId::next_for_test(&counter);
        assert_eq!(a, TaskId(100));
        assert_eq!(b, TaskId(101));
    }

    #[test]
    fn task_id_zero_unused_by_default_counter() {
        // The production NEXT_TASK_ID starts at 1, so the first allocation
        // returns TaskId(>=1). We can't read NEXT_TASK_ID directly without
        // perturbing global state — instead assert the start invariant via a
        // fresh local counter at 1.
        let counter = AtomicU64::new(1);
        let first = TaskId::next_for_test(&counter);
        assert_ne!(first, TaskId(0));
        assert_eq!(first, TaskId(1));
    }

    #[test]
    fn exit_status_variants_are_constructible() {
        let _ok = ExitStatus::Success;
        let _fail_no_code = ExitStatus::Failure { code: None };
        let _fail_with_code = ExitStatus::Failure { code: Some(2) };
        let _cancelled = ExitStatus::Cancelled;
        let _killed = ExitStatus::Killed;
    }
}
