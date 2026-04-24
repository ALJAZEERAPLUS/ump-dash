//! COVER-01 layer 2 (D-09 second layer): characterization of the update()-level
//! rejection of Action::MetroStart when metro is already running.
//!
//! This file complements the inline `#[cfg(test)] mod tests` block in
//! src/domain/metro.rs (layer 1 = MetroManager::register() panic). Here we
//! assert the TEA boundary: dispatching MetroStart with metro.is_running()
//! flips pending_restart and dispatches MetroStop — it does NOT construct a
//! second MetroHandle.
//!
//! Post-F-201 (Plan 13-07): `update()` signature is
//! `pub fn update(state: &mut AppState, action: Action) -> Vec<Effect>`.
//! The tests no longer need to hold receivers (no channels are passed in) —
//! they assert on state mutations and (optionally) on the returned
//! `Vec<Effect>`.

mod common;

use common::fake_metro_handle;
use rn_dash::app::effect::Effect;
use rn_dash::app::{update, AppState};
use rn_dash::domain::action::Action;
use rn_dash::domain::metro::MetroStatus;

#[test]
fn metro_start_while_running_triggers_restart_not_double_spawn() {
    let mut state = AppState::default();
    // Simulate "metro already running in worktree A"
    state.metro.register(fake_metro_handle(9999, "wt-a"));
    assert!(state.metro.is_running(), "precondition: metro must be running");
    assert!(!state.pending_restart, "precondition: pending_restart starts false");

    // Dispatch a second MetroStart — the characterization target.
    let effects = update(&mut state, Action::MetroStart);

    // Invariant 1: the state machine flagged a restart — it did NOT silently drop
    // or double-spawn.
    assert!(
        state.pending_restart,
        "COVER-01: second MetroStart while running MUST set pending_restart = true"
    );

    // Invariant 2: metro is in Stopping OR still Running (MetroStop is
    // processed synchronously via the recursive update() call; the resulting
    // effects list may be empty — MetroStop consumes the handle inline via
    // state.metro.take_handle() + handle.kill() and does not emit Effects).
    let still_running_or_stopping = matches!(
        state.metro.status,
        MetroStatus::Running { pid: 9999, .. } | MetroStatus::Stopping
    );
    assert!(
        still_running_or_stopping,
        "COVER-01: metro must still track the ORIGINAL handle (pid 9999) or be Stopping — got {:?}",
        state.metro.status
    );

    // Invariant 3: no Effect::SpawnMetro should be present — a second spawn
    // would have bypassed the restart coordination.
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::SpawnMetro { .. })),
        "COVER-01: MetroStart-while-running MUST NOT emit SpawnMetro — got {effects:?}"
    );
}

#[test]
fn metro_start_when_stopped_does_not_set_pending_restart() {
    // Negative control — MetroStart when metro is NOT running should NOT flip
    // pending_restart. This catches a failure mode where a refactor makes
    // pending_restart always true (regressing the guard to a no-op).
    let mut state = AppState::default();
    assert!(!state.metro.is_running(), "precondition: metro stopped");

    let effects = update(&mut state, Action::MetroStart);

    assert!(
        !state.pending_restart,
        "MetroStart from Stopped must NOT set pending_restart — that flag is only for the restart path"
    );
    // Post-F-201: MetroStart from Stopped emits Effect::DetectExternalMetro
    // (or ScheduleAction(MetroStartConfirmed) if skip_external_metro_check is
    // set). Verify the external-detect path fires.
    assert!(
        effects.iter().any(|e| matches!(e, Effect::DetectExternalMetro { .. })),
        "MetroStart from Stopped must emit DetectExternalMetro effect; got {effects:?}"
    );
}
