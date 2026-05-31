//! COVER-01 layer 2 (D-09 second layer): characterization of the update()-level
//! MetroStart guard.
//!
//! This file complements the inline `#[cfg(test)] mod tests` block in
//! src/domain/metro.rs (layer 1 = WorktreeMetro::register() panic). Here we
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
use ump_dash::domain::worktree::{Worktree, WorktreeId, WorktreeMetroStatus};
use ump_dash::domain::worktree_slice::WorktreeSlice;

fn seed_worktree(state: &mut AppState, id: &str) -> WorktreeId {
    let wt_id = WorktreeId(id.to_string());
    state.worktree_browser.worktrees.push(Worktree {
        id: wt_id.clone(),
        path: std::path::PathBuf::from(format!("/tmp/{id}")),
        branch: id.to_string(),
        head_sha: "0000000".to_string(),
        metro_status: WorktreeMetroStatus::Stopped,
        jira_title: None,
        stale: false,
        stale_pods: false,
        jira_key: None,
    });
    let idx = state.worktree_browser.worktrees.len() - 1;
    state.worktree_browser.worktree_table_state.select(Some(idx));
    state.metro_state.active_worktree_path = Some(state.worktree_browser.worktrees[idx].path.clone());
    state.worktrees.insert(
        wt_id.clone(),
        WorktreeSlice {
            id: wt_id.clone(),
            ..Default::default()
        },
    );
    wt_id
}

#[test]
fn metro_start_on_same_worktree_is_noop() {
    let mut state = AppState::default();
    // Simulate "metro already running in worktree A"
    let wt_a = seed_worktree(&mut state, "wt-a");
    state.worktrees.get_mut(&wt_a).unwrap().metro.register(fake_metro_handle(9999, "wt-a"));
    assert!(state.worktrees.get(&wt_a).unwrap().metro.is_running(), "precondition: metro must be running");
    assert!(!state.metro_state.pending_restart, "precondition: pending_restart starts false");

    // Dispatch a second MetroStart — the characterization target.
    let effects = update(&mut state, Action::MetroStart);

    assert!(
        !state.metro_state.pending_restart,
        "starting Metro on an already-running worktree should be a no-op"
    );
    assert!(state.worktrees.get(&wt_a).unwrap().metro.is_running(), "original Metro handle should remain registered");

    assert!(
        !effects.iter().any(|e| matches!(e, Effect::SpawnMetro { .. })),
        "same-worktree MetroStart MUST NOT emit SpawnMetro — got {effects:?}"
    );
}

#[test]
fn metro_start_on_different_worktree_spawns_without_stopping_existing_metro() {
    let mut state = AppState::default();
    let _wt_a = seed_worktree(&mut state, "wt-a");
    let wt_b = seed_worktree(&mut state, "wt-b");
    state.worktree_browser.worktree_table_state.select(Some(0));
    state.metro_state.active_worktree_path = Some(std::path::PathBuf::from("/tmp/wt-a"));
    state.worktrees.get_mut(&wt_b).unwrap().metro.register(fake_metro_handle(9999, "wt-b"));

    let effects = update(&mut state, Action::MetroStart);

    assert!(
        !state.metro_state.pending_restart,
        "starting Metro on a different worktree must not enter stop/restart coordination"
    );
    assert!(
        state.worktrees.get(&wt_b).unwrap().metro.is_running(),
        "existing Metro on wt-b must continue running"
    );
    assert!(
        effects.iter().any(|effect| matches!(effect, Effect::SpawnMetro { worktree, .. } if worktree.ends_with("wt-a"))),
        "starting Metro on wt-a should emit a new SpawnMetro effect; got {effects:?}"
    );
    assert!(
        effects.iter().any(|effect| matches!(effect, Effect::SpawnMetro { port: 8082, .. })),
        "starting a second dashboard-owned Metro should reserve the next port; got {effects:?}"
    );
}

#[test]
fn metro_start_when_stopped_spawns_without_external_port_lock() {
    // Negative control — MetroStart when metro is NOT running should NOT flip
    // pending_restart. This catches a failure mode where a refactor makes
    // pending_restart always true (regressing the guard to a no-op).
    let mut state = AppState::default();
    seed_worktree(&mut state, "wt-a");
    assert!(
        state.worktrees.values().all(|slice| !slice.metro.is_running()),
        "precondition: metro stopped"
    );

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
