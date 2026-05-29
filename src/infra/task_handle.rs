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
use std::os::unix::process::ExitStatusExt;

/// SIGTERM → SIGKILL grace window for cancellation (Plan 15-02 / 15-RESEARCH §Q-3).
/// Hardcoded — no config knob. 200ms is enough for yarn/node to flush stdout
/// buffers after SIGTERM but short enough that a hung child gets SIGKILL'd
/// before the user notices the cancel didn't "feel" immediate.
const CANCEL_GRACE_MS: u64 = 200;

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
        // Plan 15-02 / Pattern 1: SIGTERM → 200ms grace → SIGKILL ladder.
        // Modeled on infra/metro.rs:157-168 (the metro-side PGID kill).
        // abort() is the domain trait surface — synchronous, infallible, no panic.
        let pid = self.child_pid as i32;
        // Pitfall 3 (15-RESEARCH): libc::kill(-1, SIG*) broadcasts to every
        // process owned by this UID — that would include ump-dash itself.
        // The placeholder pid=0 (Plan 15-03 will wire the real one) and the
        // init pid=1 are both refused silently. abort() must be infallible per
        // the domain trait, so we return rather than panic.
        if pid <= 1 {
            return;
        }
        // SAFETY: sending to our own process group; pid validated > 1 above;
        // ESRCH is a no-op (safe per 15-RESEARCH §Pitfall 1). Return value
        // intentionally ignored — abort() is infallible.
        unsafe {
            let _ = libc::kill(-pid, libc::SIGTERM);
        }
        // Grace window: fire-and-forget tokio task that escalates to SIGKILL
        // after CANCEL_GRACE_MS. JoinHandle is dropped intentionally — if the
        // runtime shuts down before the sleep elapses, the task is dropped
        // and no kill is sent (T-15-02-03 disposition: accept).
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(CANCEL_GRACE_MS)).await;
            // SAFETY: same as above — own PGID, pid > 1 (captured by-copy
            // from the validated pid), ESRCH is a no-op.
            unsafe {
                let _ = libc::kill(-pid, libc::SIGKILL);
            }
        });
        // Signal the forwarding loop (Plan 15-03 wires .cancelled() into the
        // select! arm) so it can emit ExitStatus::Cancelled without waiting
        // for the child to wind down.
        self.cancel_token.cancel();
        // Belt-and-suspenders cooperative tokio abort of the forwarding task.
        // If the child dies cleanly to SIGTERM before grace, the forwarding
        // loop's child.wait() resolves first; if it doesn't, this aborts the
        // loop so we never block waiting on a wedged stream.
        self.join_handle.abort();
    }
}

/// Translate OS exit status into the domain enum (D-09).
///
/// Phase 15: signal-aware mapping. SIGKILL → Killed (the cancel grace expired
/// and we hard-killed); any other signal (typically SIGTERM from the cancel
/// path before grace, or external SIGINT/SIGHUP) → Cancelled. Clean exit with
/// non-zero code → Failure { code: Some(N) }. See 15-RESEARCH §F4.
impl From<std::process::ExitStatus> for ExitStatus {
    fn from(status: std::process::ExitStatus) -> Self {
        if status.success() {
            ExitStatus::Success
        } else if let Some(signal) = ExitStatusExt::signal(&status) {
            if signal == libc::SIGKILL {
                ExitStatus::Killed
            } else {
                ExitStatus::Cancelled
            }
        } else {
            // Clean exit with non-zero code (no signal).
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

    #[test]
    fn abort_with_placeholder_pid_zero_is_noop() {
        // Pitfall 3 guard: if child_pid is the placeholder 0, abort() must
        // refuse to send any kill AND return before doing anything else.
        // Post-condition: cancel_token is NOT cancelled (proves the early
        // return executed before step 5 of the ladder).
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let token = tokio_util::sync::CancellationToken::new();
            let handle = TokioTaskHandle {
                join_handle: tokio::spawn(async {}),
                child_pid: 0,
                cancel_token: token.clone(),
            };
            handle.abort();
            assert!(
                !token.is_cancelled(),
                "abort() on placeholder pid=0 must short-circuit before cancel_token.cancel()"
            );
        });
    }

    #[test]
    fn abort_with_placeholder_pid_one_is_noop() {
        // Pitfall 3 guard (T-15-02-02): pid=1 is init on POSIX. Never send
        // libc::kill(-1, ..) — that would target every process owned by this
        // UID, including ump-dash itself.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let token = tokio_util::sync::CancellationToken::new();
            let handle = TokioTaskHandle {
                join_handle: tokio::spawn(async {}),
                child_pid: 1,
                cancel_token: token.clone(),
            };
            handle.abort();
            assert!(
                !token.is_cancelled(),
                "abort() on pid=1 must short-circuit before cancel_token.cancel()"
            );
        });
    }

    #[test]
    fn abort_with_dead_pid_does_not_panic() {
        // pid 999_999 is clearly not a running process (and positive when
        // cast to i32 — Pitfall: 0xDEAD_BEEF as i32 is negative, which the
        // `pid <= 1` guard would refuse). The libc::kill returns ESRCH
        // (Pitfall 1 — safe no-op); steps 5 + 6 of the ladder
        // (cancel_token.cancel() + join_handle.abort()) still run. Sleep
        // longer than CANCEL_GRACE_MS to give the grace task time to fire
        // its no-op SIGKILL — assert no panic.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let token = tokio_util::sync::CancellationToken::new();
            let handle = TokioTaskHandle {
                join_handle: tokio::spawn(async {
                    // Long-lived no-op so join_handle.abort() actually has
                    // something to abort.
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }),
                child_pid: 999_999,
                cancel_token: token.clone(),
            };
            handle.abort();
            // Steps 5 + 6 ran — cancel_token is cancelled.
            assert!(
                token.is_cancelled(),
                "abort() with valid (if dead) pid must cancel the token (step 5)"
            );
            // Wait past the grace window so the spawned escalation task fires
            // its no-op SIGKILL. If it panicked, the runtime would surface it.
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        });
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn from_sigkill_status_maps_to_killed() {
        // Spawn a real `sleep 30` child in its own process group, broadcast
        // SIGKILL to the PGID, await the child's exit, and assert the mapped
        // domain ExitStatus is Killed. Hard-timeout the wait at 3s to keep CI
        // bounded.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut child = tokio::process::Command::new("sleep")
                .arg("30")
                .process_group(0)
                .spawn()
                .unwrap();
            let pid = child.id().unwrap() as i32;
            assert!(pid > 1);
            // SAFETY: our own PGID, pid validated > 1; SIGKILL terminates.
            unsafe {
                let _ = libc::kill(-pid, libc::SIGKILL);
            }
            let status = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                child.wait(),
            )
            .await
            .expect("child did not exit within 3s after SIGKILL")
            .unwrap();
            let mapped: ExitStatus = status.into();
            assert_eq!(mapped, ExitStatus::Killed);
        });
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn from_sigterm_status_maps_to_cancelled() {
        // Same fixture as the SIGKILL test but with SIGTERM. The shell's
        // default SIGTERM handler terminates the process; the resulting
        // ExitStatus exposes signal() == Some(SIGTERM), which maps to
        // ExitStatus::Cancelled.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut child = tokio::process::Command::new("sleep")
                .arg("30")
                .process_group(0)
                .spawn()
                .unwrap();
            let pid = child.id().unwrap() as i32;
            assert!(pid > 1);
            // SAFETY: our own PGID, pid validated > 1; SIGTERM terminates by
            // default for `sleep`.
            unsafe {
                let _ = libc::kill(-pid, libc::SIGTERM);
            }
            let status = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                child.wait(),
            )
            .await
            .expect("child did not exit within 3s after SIGTERM")
            .unwrap();
            let mapped: ExitStatus = status.into();
            assert_eq!(mapped, ExitStatus::Cancelled);
        });
    }
}
