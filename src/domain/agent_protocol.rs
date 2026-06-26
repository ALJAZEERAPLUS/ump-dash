//! MCP agent protocol vocabulary — the request/response types an embedded MCP
//! server uses to ask the dashboard to act on a worktree.
//!
//! Pure domain. No tokio, no I/O. Every type is `Serialize + Deserialize` so the
//! infra MCP server (`src/infra/mcp_server.rs`) can carry them over the wire and
//! `update()` can build them without touching transport concerns.
//!
//! Design intent: an MCP tool call becomes an `Action::Agent { request_id, cwd,
//! request }` that flows through the SAME `update()` pipeline the keyboard drives,
//! so every existing lock/dependency decision (collision policy, per-repo yarn
//! semaphore, metro single-instance, dependency recipes) is reused. The reply is
//! emitted as `Effect::AgentReply { request_id, outcome }` and correlated back to
//! the waiting tool call by the infra registry.
//!
//! `AgentRequest` only contains the variants that `update()` handles. Device
//! enumeration (`list_devices`) and completion blocking (`wait_for_task`) are
//! handled entirely in infra (async device port / a per-task waiter) and never
//! become an `Action`.
//!
//! Mirrors `src/domain/task.rs` for the `AtomicU64::next()` convention and
//! `src/domain/native_cache.rs` for the serde-on-domain-types convention.

#![allow(dead_code)]

use crate::domain::command::RunVariant;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-wide monotonic correlation id for an in-flight agent request.
/// Starts at 1 (0 reserved as a sentinel in tests), like [`crate::domain::task::TaskId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentRequestId(pub u64);

static NEXT_AGENT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

impl AgentRequestId {
    /// Production allocator — `AgentRequestId(N)` where N starts at 1.
    pub fn next() -> Self {
        AgentRequestId(NEXT_AGENT_REQUEST_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Test injection — fixture supplies its own counter for isolation.
    pub fn next_for_test(counter: &AtomicU64) -> Self {
        AgentRequestId(counter.fetch_add(1, Ordering::Relaxed))
    }
}

/// A request from an agent, resolved against the agent's worktree by `update()`.
///
/// `update()`-handled only. Each variant maps onto the existing dispatch
/// machinery (see `src/app/update.rs` `Action::Agent`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRequest {
    /// Start metro for this worktree (front-loads `yarn install` if stale). The
    /// reply carries the bound port so rozenite can connect.
    StartMetro,
    /// Run the app on an iOS device/simulator. `device_id` is required (agents
    /// enumerate via the infra `list_devices` tool and pick one).
    RunIos {
        device_id: String,
        variant: Option<RunVariant>,
    },
    /// Run the app on an Android device/emulator.
    RunAndroid {
        device_id: String,
        variant: Option<RunVariant>,
    },
    /// Build the release artifact; `install` also installs it via adb when done.
    Build { install: bool },
    /// Sync JS deps (`yarn install`), optionally pods too.
    SyncDeps { include_pods: bool },
    /// Run an arbitrary shell command in the worktree. Requires `confirm`.
    Shell { command: String, confirm: bool },
    /// Clean native build artifacts. Destructive — requires `confirm`.
    Clean {
        node_modules: bool,
        pods: bool,
        android: bool,
        confirm: bool,
    },
    /// `git reset --hard`. Destructive — requires `confirm`.
    ResetHard { confirm: bool },
    /// `rm -rf node_modules`. Destructive — requires `confirm`.
    RmNodeModules { confirm: bool },
    /// Create a worktree. With `base_branch`, `branch` is created as a new
    /// branch from that base; without it, `branch` is checked out directly.
    /// Requires `confirm` because it mutates local git worktree state.
    CreateWorktree {
        branch: String,
        base_branch: Option<String>,
        confirm: bool,
    },
    /// Delete an explicitly named worktree path. Destructive — requires
    /// `confirm`, and `update()` refuses the main repo root.
    DeleteWorktree {
        target_worktree: String,
        confirm: bool,
    },
    /// Read-only: full pre-flight picture of the worktree.
    GetWorktreeStatus,
    /// Read-only: running task + queued specs.
    GetTaskStatus,
    /// Read-only: tail of the worktree's command output.
    GetLogs { tail: Option<usize> },
    /// Cancel the running task (honors the non-cancellable git guard).
    Cancel,
}

/// The immediate decision returned to a tool call. Every variant surfaces *why*
/// the request did or did not proceed — the "lock/block decision".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOutcome {
    /// Dispatched now. `task_id` correlates with `get_task_status` / `wait_for_task`.
    /// `expanded` lists the labels of every step that was scheduled (e.g. a stale
    /// run expands to `[yarn install, yarn pod-install, Run iOS (UMP)]`).
    Accepted { task_id: u64, expanded: Vec<String> },
    /// Enqueued behind work already running/queued on this worktree.
    Queued { position: usize, ahead: Vec<String> },
    /// Not started — see `reason`.
    Blocked { reason: BlockReason },
    /// Metro is starting (or yarn-installing first); `port` is known once reserved.
    MetroStarting { port: Option<u16> },
    /// Metro already running on `port`.
    MetroAlready { port: u16 },
    /// Read-only status report.
    Status(WorktreeStatusReport),
    /// Read-only task status.
    TaskStatus(TaskStatusReport),
    /// Read-only log tail.
    Logs { lines: Vec<String> },
    /// The running task was cancelled.
    Cancelled { spec_label: String },
    /// Nothing was running to cancel.
    NothingToCancel,
    /// A worktree create/delete request was accepted and dispatched as an async
    /// worktree effect. The final result surfaces through the dashboard's
    /// normal worktree refresh/error state.
    WorktreeOperationStarted { operation: String, target: String },
    /// The request could not be processed (e.g. unknown worktree).
    Error { message: String },
}

/// Why a request was blocked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockReason {
    /// An identical idempotent task is already running (collision policy `BlockNew`).
    CollisionBlockNew { spec_label: String },
    /// A non-cancellable task (git porcelain) is running and cannot be interrupted.
    NonCancellableRunning { spec_label: String },
    /// The agent's cwd did not resolve to a known worktree.
    UnknownWorktree { cwd: String },
    /// A destructive/shell request arrived without `confirm: true`.
    ConfirmationRequired { spec_label: String },
}

/// Metro state for a worktree, reported to agents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetroReport {
    Stopped,
    Starting { port: Option<u16> },
    Running { port: u16 },
}

/// Pre-flight snapshot of a worktree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorktreeStatusReport {
    pub worktree_id: String,
    pub branch: String,
    pub yarn_stale: bool,
    pub pods_stale: bool,
    pub metro: MetroReport,
    /// Label of the currently running task, if any.
    pub current_task: Option<String>,
    /// Labels of queued specs, in order.
    pub queue: Vec<String>,
}

/// Running task + queue, for polling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStatusReport {
    pub running: Option<String>,
    pub queue: Vec<String>,
}

/// A run target (simulator/emulator/device) returned by the infra `list_devices`
/// tool. Defined here so the protocol vocabulary is complete in one place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceTarget {
    pub id: String,
    pub name: String,
    /// True if the device/simulator is currently booted/connected.
    pub running: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_next_for_test_is_monotonic() {
        let counter = AtomicU64::new(50);
        assert_eq!(AgentRequestId::next_for_test(&counter), AgentRequestId(50));
        assert_eq!(AgentRequestId::next_for_test(&counter), AgentRequestId(51));
    }

    #[test]
    fn request_round_trips_through_json() {
        let reqs = vec![
            AgentRequest::StartMetro,
            AgentRequest::RunIos {
                device_id: "udid-1".into(),
                variant: Some(RunVariant::Local),
            },
            AgentRequest::Build { install: true },
            AgentRequest::SyncDeps { include_pods: false },
            AgentRequest::Shell {
                command: "ls".into(),
                confirm: true,
            },
            AgentRequest::CreateWorktree {
                branch: "feature/fresh".into(),
                base_branch: Some("main".into()),
                confirm: true,
            },
            AgentRequest::DeleteWorktree {
                target_worktree: "/tmp/wt-2".into(),
                confirm: true,
            },
            AgentRequest::GetLogs { tail: Some(20) },
            AgentRequest::Cancel,
        ];
        for r in reqs {
            let json = serde_json::to_string(&r).unwrap();
            let back: AgentRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(r, back, "round-trip mismatch for {json}");
        }
    }

    #[test]
    fn outcome_round_trips_through_json() {
        let outcomes = vec![
            AgentOutcome::Accepted {
                task_id: 7,
                expanded: vec!["yarn install".into(), "Run iOS (UMP)".into()],
            },
            AgentOutcome::Queued {
                position: 1,
                ahead: vec!["yarn install".into()],
            },
            AgentOutcome::Blocked {
                reason: BlockReason::CollisionBlockNew {
                    spec_label: "yarn install".into(),
                },
            },
            AgentOutcome::MetroStarting { port: Some(8081) },
            AgentOutcome::MetroAlready { port: 8082 },
            AgentOutcome::NothingToCancel,
            AgentOutcome::WorktreeOperationStarted {
                operation: "create_worktree".into(),
                target: "feature/fresh".into(),
            },
            AgentOutcome::Error {
                message: "unknown worktree".into(),
            },
        ];
        for o in outcomes {
            let json = serde_json::to_string(&o).unwrap();
            let back: AgentOutcome = serde_json::from_str(&json).unwrap();
            assert_eq!(o, back, "round-trip mismatch for {json}");
        }
    }

    #[test]
    fn status_report_round_trips() {
        let report = WorktreeStatusReport {
            worktree_id: "/tmp/wt-a".into(),
            branch: "feat/x".into(),
            yarn_stale: true,
            pods_stale: false,
            metro: MetroReport::Running { port: 8081 },
            current_task: Some("yarn install".into()),
            queue: vec!["Run iOS (UMP)".into()],
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: WorktreeStatusReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
    }
}
