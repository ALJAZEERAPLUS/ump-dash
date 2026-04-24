//! COVER-01 layer 2 (D-09 second layer): characterization of the update()-level
//! rejection of Action::MetroStart when metro is already running.
//!
//! This file complements the inline `#[cfg(test)] mod tests` block in
//! src/domain/metro.rs (layer 1 = MetroManager::register() panic). Here we
//! assert the TEA boundary: dispatching MetroStart with metro.is_running()
//! flips pending_restart and dispatches MetroStop — it does NOT construct a
//! second MetroHandle.

mod common;

use common::fake_metro_handle;
use rn_dash::domain::action::Action;
use rn_dash::app::{update, AppState};
use rn_dash::domain::metro::MetroStatus;

#[tokio::test]
async fn metro_start_while_running_triggers_restart_not_double_spawn() {
    // Hold receivers for the whole test so tokio::spawn'd followup tasks
    // (kill_metro, etc.) do not panic with "channel closed" — see Pitfall 10
    // in 12-RESEARCH.md.
    let (metro_tx, _metro_rx) = tokio::sync::mpsc::unbounded_channel();
    let (handle_tx, _handle_rx) = tokio::sync::mpsc::unbounded_channel();

    let mut state = AppState::default();
    // Simulate "metro already running in worktree A"
    state.metro.register(fake_metro_handle(9999, "wt-a"));
    assert!(state.metro.is_running(), "precondition: metro must be running");
    assert!(!state.pending_restart, "precondition: pending_restart starts false");

    // Dispatch a second MetroStart — the characterization target.
    update(&mut state, Action::MetroStart, &metro_tx, &handle_tx);

    // Invariant 1: the state machine flagged a restart — it did NOT silently drop
    // or double-spawn.
    assert!(
        state.pending_restart,
        "COVER-01: second MetroStart while running MUST set pending_restart = true"
    );

    // Invariant 2: metro is in Stopping OR still Running (MetroStop may be
    // processed synchronously via the recursive update() call; either end-state
    // is acceptable as long as it is NOT a fresh Running{pid: different, ...}).
    let still_running_or_stopping = matches!(
        state.metro.status,
        MetroStatus::Running { pid: 9999, .. } | MetroStatus::Stopping
    );
    assert!(
        still_running_or_stopping,
        "COVER-01: metro must still track the ORIGINAL handle (pid 9999) or be Stopping — got {:?}",
        state.metro.status
    );
}

#[tokio::test]
async fn metro_start_when_stopped_does_not_set_pending_restart() {
    // Negative control — MetroStart when metro is NOT running should NOT flip
    // pending_restart. This catches a failure mode where a refactor makes
    // pending_restart always true (regressing the guard to a no-op).
    let (metro_tx, _metro_rx) = tokio::sync::mpsc::unbounded_channel();
    let (handle_tx, _handle_rx) = tokio::sync::mpsc::unbounded_channel();

    let mut state = AppState::default();
    assert!(!state.metro.is_running(), "precondition: metro stopped");

    update(&mut state, Action::MetroStart, &metro_tx, &handle_tx);

    assert!(
        !state.pending_restart,
        "MetroStart from Stopped must NOT set pending_restart — that flag is only for the restart path"
    );
}
