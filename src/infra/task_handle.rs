//! Task lifecycle adapter — Phase 14 / D-03 consumer + D-09 ExitStatus
//! translation. Mirrors `TokioMetroHandle` in `src/infra/metro.rs` for the
//! opaque-handle pattern.
//!
//! Preserved invariants:
//! - `tokio::process::Command` continues to spawn with `kill_on_drop(true)`
//!   inside `infra/command_runner.rs:71-87` — abort() inherits this.
//! - The trait stays infra-free (G-05); the concrete `JoinHandle` is private
//!   to this module's adapter struct.
//!
//! Phase 15 widening (this file):
//! - `TokioTaskHandle` becomes a 3-field named struct holding the forwarding
//!   `JoinHandle`, the child's `child_pid: u32` (for SIGTERM/SIGKILL broadcast
//!   to the process group), and a `CancellationToken` shared with the
//!   forwarding loop so cancel can emit `ExitStatus::Cancelled`.
//! - `abort()` performs the SIGTERM → 200ms grace → SIGKILL escalation ladder
//!   modeled on `infra/metro.rs:157-168` (the metro-side PGID kill exemplar).
//! - `From<std::process::ExitStatus>` widens to distinguish `Killed` (SIGKILL)
//!   from `Cancelled` (other signals) from `Failure { code }`.

#![allow(dead_code)]

use crate::domain::ports::task_handle::TaskHandle;
use crate::domain::task::ExitStatus;

/// Concrete `TaskHandle` impl. Owns:
/// - `join_handle`: the forwarding `tokio::task::JoinHandle<()>` that drains
///   `CommandEvent`s from the runner and dispatches Actions back to the app.
/// - `child_pid`: the OS pid of the spawned child (also the PGID since
///   `command_runner` calls `.process_group(0)`). Used by `abort()` to send
///   `libc::kill(-pid, SIGTERM)` to the whole process group — see
///   `infra/metro.rs:157-168` for the same pattern on the metro side.
/// - `cancel_token`: shared with the forwarding loop (Plan 15-03) so cancel
///   can short-circuit and emit `ExitStatus::Cancelled` without waiting for
///   the child to wind down.
///
/// `abort()` is synchronous, non-async, returns immediately. The task body
/// unwinds at the next `await`; the inner `Child` (spawned with
/// `kill_on_drop(true)`) sends a kill signal as a side effect when dropped.
/// See docs.rs/tokio/1.49.0/tokio/task/struct.JoinHandle.html.
// Plan 15-03 will update `src/app/effect_runner.rs` to plumb the real
// `child_pid` from `CommandEvent::ProcessStarted` via the runner stream.
pub struct TokioTaskHandle {
    pub join_handle: tokio::task::JoinHandle<()>,
    pub child_pid: u32,
    pub cancel_token: tokio_util::sync::CancellationToken,
}

impl std::fmt::Debug for TokioTaskHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // JoinHandle<()> + CancellationToken do not have meaningful Debug
        // output for diagnostics — print only the pid (the only field a human
        // log reader can correlate against `ps`).
        f.debug_struct("TokioTaskHandle")
            .field("child_pid", &self.child_pid)
            .finish_non_exhaustive()
    }
}

impl TaskHandle for TokioTaskHandle {
    fn abort(&self) {
        // Task 2 will widen this to the full SIGTERM → 200ms → SIGKILL ladder.
        self.join_handle.abort();
    }
}

/// Translate OS exit status into the domain enum (D-09). Phase 14 only
/// distinguishes success vs. failure — Phase 15 will widen to inspect signals
/// via `std::os::unix::process::ExitStatusExt::signal()` and emit `Killed`.
impl From<std::process::ExitStatus> for ExitStatus {
    fn from(status: std::process::ExitStatus) -> Self {
        if status.success() {
            ExitStatus::Success
        } else {
            // Phase 14: signal-killed processes show up as Failure { code: None }
            // because std::process::ExitStatus.code() returns None on signal exit.
            // Phase 15 widens this to detect signals explicitly.
            ExitStatus::Failure { code: status.code() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_success_status_maps_to_success() {
        // tokio always exposes a no-op task we can await to get a real
        // ExitStatus. Cheaper: spawn `true` and await.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let status = rt.block_on(async {
            tokio::process::Command::new("true").status().await.unwrap()
        });
        assert!(status.success());
        let mapped: ExitStatus = status.into();
        assert_eq!(mapped, ExitStatus::Success);
    }

    #[test]
    fn from_failure_status_with_code_maps_to_failure_code_some() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let status = rt.block_on(async {
            tokio::process::Command::new("sh").arg("-c").arg("exit 7").status().await.unwrap()
        });
        assert!(!status.success());
        let mapped: ExitStatus = status.into();
        assert_eq!(mapped, ExitStatus::Failure { code: Some(7) });
    }

    #[test]
    fn boxed_task_handle_dispatches_through_trait_object() {
        // Smoke test: a TokioTaskHandle is callable through &dyn TaskHandle.
        // We don't actually want to abort anything, so we spawn a future that
        // is already complete by the time we abort. Use child_pid: 999 — a
        // clearly-not-running pid; the future Task 2 guard `if pid <= 1`
        // permits this value, but libc::kill(-999, ..) would only return
        // ESRCH (no such process group) which is safe per 15-RESEARCH §Pitfall 1.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let token = tokio_util::sync::CancellationToken::new();
            let boxed: Box<dyn TaskHandle> = Box::new(TokioTaskHandle {
                join_handle: tokio::spawn(async {}),
                child_pid: 999,
                cancel_token: token,
            });
            boxed.abort();  // smoke test only — ensures dispatch compiles & runs
        });
    }

    #[test]
    fn construct_with_all_three_fields() {
        // Smoke-test the struct layout: build with all three named fields,
        // read each one, drop without aborting.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let handle = TokioTaskHandle {
                join_handle: tokio::spawn(async {}),
                child_pid: 99,
                cancel_token: tokio_util::sync::CancellationToken::new(),
            };
            assert_eq!(handle.child_pid, 99);
            assert!(!handle.cancel_token.is_cancelled());
        });
    }
}
