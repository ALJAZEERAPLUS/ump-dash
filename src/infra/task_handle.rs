//! Task lifecycle adapter — Phase 14 / D-03 consumer + D-09 ExitStatus
//! translation. Mirrors `TokioMetroHandle` in `src/infra/metro.rs` for the
//! opaque-handle pattern.
//!
//! Preserved invariants:
//! - `tokio::process::Command` continues to spawn with `kill_on_drop(true)`
//!   inside `infra/command_runner.rs:71-87` — abort() inherits this.
//! - The trait stays infra-free (G-05); the concrete `JoinHandle` is private
//!   to this module's adapter struct.

#![allow(dead_code)]

use crate::domain::ports::task_handle::TaskHandle;
use crate::domain::task::ExitStatus;

/// Concrete `TaskHandle` impl. Owns one `tokio::task::JoinHandle<()>`.
///
/// `abort()` is `JoinHandle::abort()` — synchronous, non-async, returns
/// immediately. The task body unwinds at the next `await`; the inner `Child`
/// (spawned with `kill_on_drop(true)`) sends a kill signal as a side effect
/// when dropped. See docs.rs/tokio/1.49.0/tokio/task/struct.JoinHandle.html.
#[derive(Debug)]
pub struct TokioTaskHandle(pub tokio::task::JoinHandle<()>);

impl TaskHandle for TokioTaskHandle {
    fn abort(&self) {
        self.0.abort();
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
        // is already complete by the time we abort.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let jh: tokio::task::JoinHandle<()> = tokio::spawn(async {});
            // Drain the spawn before abort to avoid panic-on-drop semantics.
            let _ = jh.abort_handle();  // we keep the JH itself for the wrapper
            let boxed: Box<dyn TaskHandle> = Box::new(TokioTaskHandle(tokio::spawn(async {})));
            boxed.abort();  // smoke test only — ensures dispatch compiles & runs
        });
    }
}
