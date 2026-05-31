//! COVER-01 layer 2 (D-09 second layer): characterization of the update()-level
//! MetroStart guard.
//!
//! This file complements the inline `#[cfg(test)] mod tests` block in
//! src/domain/metro.rs (layer 1 = MetroManager::register() panic). Here we
//! assert the TEA boundary: dispatching MetroStart for an already-running
//! worktree does not construct a second MetroHandle, while starting another
//! worktree is allowed.
//!
//! Post-F-201 (Plan 13-07): `update()` signature is
//! `pub fn update(state: &mut AppState, action: Action) -> Vec<Effect>`.
//! The tests no longer need to hold receivers (no channels are passed in) —
//! they assert on state mutations and (optionally) on the returned
//! `Vec<Effect>`.

mod common;

use common::fake_metro_handle;
use ump_dash::app::effect::Effect;
use ump_dash::app::{update, AppState};
use ump_dash::domain::action::Action;

#[test]
fn metro_start_on_same_worktree_is_noop() {
    let mut state = AppState::default();
    // Simulate "metro already running in worktree A"
    state.metro.register(fake_metro_handle(9999, "wt-a"));
    state.metro_state.active_worktree_path = Some(std::path::PathBuf::from("/tmp/wt-a"));
    assert!(state.metro.is_running_for("wt-a"), "precondition: metro must be running");
    assert!(!state.metro_state.pending_restart, "precondition: pending_restart starts false");

    // Dispatch a second MetroStart — the characterization target.
    let effects = update(&mut state, Action::MetroStart);

    assert!(
        !state.metro_state.pending_restart,
        "starting Metro on an already-running worktree should be a no-op"
    );
    assert!(state.metro.is_running_for("wt-a"), "original Metro handle should remain registered");

    assert!(
        !effects.iter().any(|e| matches!(e, Effect::SpawnMetro { .. })),
        "same-worktree MetroStart MUST NOT emit SpawnMetro — got {effects:?}"
    );
}

#[test]
fn metro_start_on_different_worktree_spawns_without_stopping_existing_metro() {
    let mut state = AppState::default();
    state.metro.register(fake_metro_handle(9999, "wt-b"));
    state.metro_state.active_worktree_path = Some(std::path::PathBuf::from("/tmp/wt-a"));

    let effects = update(&mut state, Action::MetroStart);

    assert!(
        !state.metro_state.pending_restart,
        "starting Metro on a different worktree must not enter stop/restart coordination"
    );
    assert!(
        state.metro.is_running_for("wt-b"),
        "existing Metro on wt-b must continue running"
    );
    assert!(
        effects.iter().any(|effect| matches!(effect, Effect::SpawnMetro { worktree } if worktree.ends_with("wt-a"))),
        "starting Metro on wt-a should emit a new SpawnMetro effect; got {effects:?}"
    );
}

#[test]
fn metro_start_when_stopped_spawns_without_external_port_lock() {
    // Negative control — MetroStart when metro is NOT running should NOT flip
    // pending_restart. This catches a failure mode where a refactor makes
    // pending_restart always true (regressing the guard to a no-op).
    let mut state = AppState::default();
    assert!(!state.metro.is_running(), "precondition: metro stopped");

    let effects = update(&mut state, Action::MetroStart);

    assert!(
        !state.metro_state.pending_restart,
        "MetroStart from Stopped must NOT set pending_restart — that flag is only for the restart path"
    );
    assert!(
        effects.iter().any(|e| matches!(e, Effect::SpawnMetro { .. })),
        "MetroStart from Stopped must spawn directly instead of locking on external port detection; got {effects:?}"
    );
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::DetectExternalMetro { .. })),
        "MetroStart from Stopped must not emit the external Metro lock probe; got {effects:?}"
    );
}
