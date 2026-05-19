//! COVER-03 — TEA dispatch-path characterization.
//!
//! This file covers three TEA surfaces that Phase 13 refactors are most
//! likely to touch:
//!
//! 1. Palette → Action resolution (`handle_key` palette branches): 5
//!    `PaletteMode` variants × 2-7 keys each + unrecognized-key fallback.
//! 2. Modal dismissal: 8 `ModalState` variants × dismiss keys. Post-condition:
//!    `state.modal_stack.modal == None` after `update()`.
//! 3. Command queue routing: `CommandQueuePush` appends; `CommandExited` drains.
//!
//! Post-F-201 (Plan 13-07): `update()` signature is now
//! `pub fn update(state: &mut AppState, action: Action) -> Vec<Effect>`.
//! Tests no longer need the `metro_tx` / `handle_tx` channels — they just
//! call `update()` and (optionally) assert on the returned `Vec<Effect>`.
//! Most tests in this file care about state mutations, not effects, so the
//! return value is typically bound to `_`.

use super::*;
use super::effect::Effect;
use crate::domain::action::Action;
use crate::domain::command::{CleanOptions, CommandSpec, ModalState};
use crate::domain::worktree::{Worktree, WorktreeId, WorktreeMetroStatus};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
};

/// Build a `KeyEvent` for a single character press — the 99% case for these tests.
fn key(c: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// Build a `KeyEvent` for a non-char keycode (Esc, Enter, etc.).
fn key_code(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// `AppState` focused on `WorktreeTable` — matches the pre-condition for
/// palette-key interpretation (`handle_key` pre-checks).
///
/// `AppState::default()` already sets `focused_panel` to `WorktreeTable`,
/// so no further reassignment is needed here.
fn base_state() -> AppState {
    AppState::default()
}

/// Seed one worktree so `dispatch_command` does not early-return on an empty
/// worktree list. Used by the `command_queue` drain test.
fn seed_one_worktree(state: &mut AppState) {
    seed_one_worktree_id(state, "wt-1");
}

/// Seed a single worktree with a given id, also initialising its slice.
fn seed_one_worktree_id(state: &mut AppState, id: &str) {
    state.worktree_browser.worktrees.push(Worktree {
        id: WorktreeId(id.into()),
        path: std::path::PathBuf::from(format!("/tmp/{id}")),
        branch: "main".into(),
        head_sha: "0000000".into(),
        metro_status: WorktreeMetroStatus::Stopped,
        jira_title: None,
        stale: false,
        stale_pods: false,
        jira_key: None,
    });
    let idx = state.worktree_browser.worktrees.len() - 1;
    state.worktree_browser.worktree_table_state.select(Some(idx));
    // Ensure a slice exists for this worktree.
    state.worktrees
        .entry(WorktreeId(id.into()))
        .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
            id: WorktreeId(id.into()),
            ..Default::default()
        });
}

/// Seed two worktrees (A then B) and their slices.
fn seed_two_worktrees(state: &mut AppState, id_a: &str, id_b: &str) {
    seed_one_worktree_id(state, id_a);
    seed_one_worktree_id(state, id_b);
    // Keep selection on A (index 0).
    state.worktree_browser.worktree_table_state.select(Some(0));
}

// =========================================================================
// Phase 14 / D-21 slice-side assertion helpers
// =========================================================================

/// Assert the named worktree's slice has a running task.
fn assert_running_in(state: &AppState, id: &str) {
    let wid = WorktreeId(id.into());
    assert!(
        state.worktrees.get(&wid).and_then(|s| s.task.as_ref()).is_some(),
        "expected worktree {id:?} to have a running task; slice = {:?}",
        state.worktrees.get(&wid).map(|s| (s.task.is_some(), s.queue.len())),
    );
}

/// Assert no slice has a running task.
fn assert_no_running_task_anywhere(state: &AppState) {
    let any = state.worktrees.values().any(|s| s.task.is_some());
    assert!(!any, "expected no slice to have a running task, but at least one does");
}

/// Queue length for the named worktree's slice.
fn slice_queue_len(state: &AppState, id: &str) -> usize {
    state.worktrees
        .get(&WorktreeId(id.into()))
        .map(|s| s.queue.len())
        .unwrap_or(0)
}

/// Snapshot of slice output lines for the named worktree.
fn slice_output(state: &AppState, id: &str) -> Vec<String> {
    state.worktrees
        .get(&WorktreeId(id.into()))
        .map(|s| s.output.iter().cloned().collect())
        .unwrap_or_default()
}

/// A test-only `TaskHandle` that does nothing — abort() is a no-op.
#[derive(Debug)]
struct NoopHandle;

impl crate::domain::ports::task_handle::TaskHandle for NoopHandle {
    fn abort(&self) {}
}

/// Build a synthetic `TaskRecord` for unit tests (no real runtime needed).
fn synthetic_task_record(
    id_value: u64,
    spec: crate::domain::command::CommandSpec,
) -> crate::domain::task::TaskRecord {
    crate::domain::task::TaskRecord {
        id: crate::domain::task::TaskId(id_value),
        spec,
        started_at: std::time::Instant::now(),
        handle: Box::new(NoopHandle),
    }
}

// =========================================================================
// Sub-module 1: Palette resolution (COVER-03 layer 1)
// =========================================================================

mod palette_resolution {
    use super::*;

    #[test]
    fn android_palette_resolves_every_key() {
        let mut state = base_state();
        state.modal_stack.palette_mode = Some(PaletteMode::Android);

        // 'd' composes a ShellCommand at call time — match the outer variant
        // (the command-string format is internal and not the invariant we lock).
        match handle_key(&state, key('d')) {
            Some(Action::CommandRun(CommandSpec::ShellCommand { .. })) => {}
            other => panic!("android 'd' must produce ShellCommand; got {other:?}"),
        }

        match handle_key(&state, key('e')) {
            Some(Action::CommandRun(CommandSpec::RnRunAndroid { .. })) => {}
            other => panic!("android 'e' must produce RnRunAndroid; got {other:?}"),
        }

        assert_eq!(
            handle_key(&state, key('r')),
            Some(Action::CommandRun(CommandSpec::RnReleaseBuild))
        );
        assert_eq!(
            handle_key(&state, key('m')),
            Some(Action::StartSetAndroidMode)
        );
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );

        // Fallback: unrecognized key must produce ModalCancel — not silently drop.
        // Regression-guard against future palette additions that forget this.
        assert_eq!(
            handle_key(&state, key('z')),
            Some(Action::ModalCancel),
            "android palette unrecognized-key fallback regression-guard"
        );
    }

    #[test]
    fn ios_palette_resolves_every_key() {
        let mut state = base_state();
        state.modal_stack.palette_mode = Some(PaletteMode::Ios);

        assert_eq!(
            handle_key(&state, key('d')),
            Some(Action::CommandRun(CommandSpec::RnRunIosDevice))
        );
        match handle_key(&state, key('e')) {
            Some(Action::CommandRun(CommandSpec::RnRunIos { device_id })) => {
                assert_eq!(device_id, "");
            }
            other => panic!(
                "ios 'e' must produce RnRunIos with empty device_id; got {other:?}"
            ),
        }
        assert_eq!(
            handle_key(&state, key('p')),
            Some(Action::CommandRun(CommandSpec::YarnPodInstall))
        );
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
        assert_eq!(
            handle_key(&state, key('z')),
            Some(Action::ModalCancel),
            "ios palette unrecognized-key fallback regression-guard"
        );
    }

    #[test]
    fn yarn_palette_resolves_every_key() {
        let mut state = base_state();
        state.modal_stack.palette_mode = Some(PaletteMode::Yarn);

        assert_eq!(
            handle_key(&state, key('i')),
            Some(Action::CommandRun(CommandSpec::YarnInstall))
        );
        assert_eq!(
            handle_key(&state, key('p')),
            Some(Action::CommandRun(CommandSpec::YarnPodInstall))
        );
        assert_eq!(
            handle_key(&state, key('u')),
            Some(Action::CommandRun(CommandSpec::YarnUnitTests))
        );
        assert_eq!(
            handle_key(&state, key('t')),
            Some(Action::CommandRun(CommandSpec::YarnCheckTypes))
        );
        match handle_key(&state, key('j')) {
            Some(Action::CommandRun(CommandSpec::YarnJest { filter })) => {
                assert_eq!(filter, "");
            }
            other => panic!(
                "yarn 'j' must produce YarnJest with empty filter; got {other:?}"
            ),
        }
        assert_eq!(
            handle_key(&state, key('l')),
            Some(Action::CommandRun(CommandSpec::YarnLint))
        );
        // 'c' opens the CleanToggle modal — this is the ENTRY half of the
        // "palette x" flow from the phase description (Research A2).
        assert_eq!(handle_key(&state, key('c')), Some(Action::OpenCleanMenu));

        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
        assert_eq!(
            handle_key(&state, key('z')),
            Some(Action::ModalCancel),
            "yarn palette unrecognized-key fallback regression-guard"
        );
    }

    #[test]
    fn git_palette_resolves_every_key() {
        let mut state = base_state();
        state.modal_stack.palette_mode = Some(PaletteMode::Git);

        assert_eq!(
            handle_key(&state, key('f')),
            Some(Action::CommandRun(CommandSpec::GitFetch))
        );
        assert_eq!(
            handle_key(&state, key('p')),
            Some(Action::CommandRun(CommandSpec::GitPull))
        );
        // Uppercase 'P' is distinct from lowercase 'p' — handle_key matches on
        // `Char('P')` directly; the KeyEvent char is the raw unicode scalar, so
        // passing `'P'` exercises the uppercase arm regardless of modifier state.
        assert_eq!(
            handle_key(&state, key('P')),
            Some(Action::CommandRun(CommandSpec::GitPush))
        );
        assert_eq!(
            handle_key(&state, key('X')),
            Some(Action::CommandRun(CommandSpec::GitResetHardFetch))
        );
        match handle_key(&state, key('b')) {
            Some(Action::CommandRun(CommandSpec::GitCheckout { branch })) => {
                assert_eq!(branch, "");
            }
            other => panic!(
                "git 'b' must produce GitCheckout with empty branch; got {other:?}"
            ),
        }
        match handle_key(&state, key('c')) {
            Some(Action::CommandRun(CommandSpec::GitCheckoutNew { branch })) => {
                assert_eq!(branch, "");
            }
            other => panic!(
                "git 'c' must produce GitCheckoutNew with empty branch; got {other:?}"
            ),
        }
        match handle_key(&state, key('r')) {
            Some(Action::CommandRun(CommandSpec::GitRebase { target })) => {
                assert_eq!(target, "");
            }
            other => panic!(
                "git 'r' must produce GitRebase with empty target; got {other:?}"
            ),
        }

        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
        assert_eq!(
            handle_key(&state, key('z')),
            Some(Action::ModalCancel),
            "git palette unrecognized-key fallback regression-guard"
        );
    }

    #[test]
    fn worktree_palette_resolves_every_key() {
        let mut state = base_state();
        state.modal_stack.palette_mode = Some(PaletteMode::Worktree);

        assert_eq!(handle_key(&state, key('w')), Some(Action::WorktreeAdd));
        assert_eq!(handle_key(&state, key('d')), Some(Action::WorktreeRemove));
        assert_eq!(
            handle_key(&state, key('b')),
            Some(Action::WorktreeAddNewBranch)
        );

        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
        assert_eq!(
            handle_key(&state, key('z')),
            Some(Action::ModalCancel),
            "worktree palette unrecognized-key fallback regression-guard"
        );
    }

    /// Covers the phase-description's "palette x" item (Research A2): Yarn
    /// palette 'c' enters the `CleanToggle` modal; 'x' inside `CleanToggle`
    /// confirms and produces `Action::CleanConfirm`.
    #[test]
    fn yarn_c_opens_clean_toggle_then_x_confirms() {
        // Step 1 (entry): from Yarn palette, 'c' produces OpenCleanMenu.
        let mut state = base_state();
        state.modal_stack.palette_mode = Some(PaletteMode::Yarn);
        assert_eq!(handle_key(&state, key('c')), Some(Action::OpenCleanMenu));

        // Step 2 (exit): with CleanToggle modal active, 'x' produces CleanConfirm.
        // This test targets the key→action mapping; modal construction via
        // update(OpenCleanMenu) is covered by its own integration surface.
        let mut state = base_state();
        state.modal_stack.modal = Some(ModalState::CleanToggle {
            options: CleanOptions::default(),
        });
        assert_eq!(
            handle_key(&state, key('x')),
            Some(Action::CleanConfirm)
        );

        // Step 3 (cancel): Esc from CleanToggle produces ModalCancel.
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
    }
}

// =========================================================================
// Sub-module 2: Modal dismissal (COVER-03 layer 2)
// =========================================================================
//
// For each of 8 `ModalState` variants, assert that the documented dismiss key
// produces the documented `Action` AND that `update()` clears `state.modal_stack.modal`
// to `None`. Post-F-201: tests are plain `#[test]` (no tokio runtime needed —
// update() is pure and effects are data, not spawns).

mod modal_dismissal {
    use super::*;

    #[test]
    fn confirm_modal_dismisses_on_n_and_esc() {
        let mut state = base_state();
        state.modal_stack.modal = Some(ModalState::Confirm {
            prompt: "Run?".into(),
            pending_command: CommandSpec::YarnInstall,
        });
        assert_eq!(handle_key(&state, key('n')), Some(Action::ModalCancel));
        assert_eq!(handle_key(&state, key('N')), Some(Action::ModalCancel));
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );

        let _effects = update(&mut state, Action::ModalCancel);
        assert!(state.modal_stack.modal.is_none(), "ModalCancel must clear state.modal_stack.modal");
    }

    #[test]
    fn text_input_modal_dismisses_on_esc() {
        let mut state = base_state();
        state.modal_stack.modal = Some(ModalState::TextInput {
            prompt: "Branch:".into(),
            buffer: String::new(),
            pending_template: Box::new(CommandSpec::GitCheckout {
                branch: String::new(),
            }),
        });
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
        let _effects = update(&mut state, Action::ModalCancel);
        assert!(state.modal_stack.modal.is_none());
    }

    #[test]
    fn device_picker_modal_dismisses_on_esc() {
        let mut state = base_state();
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: Vec::new(),
            selected: 0,
            pending_template: Box::new(CommandSpec::RnRunIos {
                device_id: String::new(),
            }),
            filter: String::new(),
        });
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
        let _effects = update(&mut state, Action::ModalCancel);
        assert!(state.modal_stack.modal.is_none());
    }

    #[test]
    fn clean_toggle_modal_dismisses_on_esc() {
        let mut state = base_state();
        state.modal_stack.modal = Some(ModalState::CleanToggle {
            options: CleanOptions::default(),
        });
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
        let _effects = update(&mut state, Action::ModalCancel);
        assert!(state.modal_stack.modal.is_none());
    }

    #[test]
    fn sync_before_run_modal_dismisses_on_n_and_esc() {
        let mut state = base_state();
        state.modal_stack.modal = Some(ModalState::SyncBeforeRun {
            run_command: Box::new(CommandSpec::YarnUnitTests),
            needs_yarn: true,
            needs_pods: false,
        });
        // n/N/Esc all emit SyncBeforeRunDecline (distinct from ModalCancel).
        assert_eq!(
            handle_key(&state, key('n')),
            Some(Action::SyncBeforeRunDecline)
        );
        assert_eq!(
            handle_key(&state, key('N')),
            Some(Action::SyncBeforeRunDecline)
        );
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::SyncBeforeRunDecline)
        );

        // Applying the decline takes state.modal_stack.modal via `.take()` → modal is None
        // BEFORE the conditional dispatch. YarnUnitTests does not need metro,
        // and with an empty worktrees vec `dispatch_command` early-returns
        // without pushing SpawnCommand.
        let _effects = update(&mut state, Action::SyncBeforeRunDecline);
        assert!(
            state.modal_stack.modal.is_none(),
            "SyncBeforeRunDecline must clear modal"
        );
    }

    #[test]
    fn sync_before_metro_modal_dismisses_on_n_and_esc() {
        let mut state = base_state();
        state.modal_stack.modal = Some(ModalState::SyncBeforeMetro {
            needs_yarn: true,
            needs_pods: false,
        });
        // Bypass the external-metro-detect effect path in the transitive
        // MetroStart dispatch — we only care that modal is cleared.
        state.metro_state.skip_external_metro_check = true;

        assert_eq!(
            handle_key(&state, key('n')),
            Some(Action::SyncBeforeMetroDecline)
        );
        assert_eq!(
            handle_key(&state, key('N')),
            Some(Action::SyncBeforeMetroDecline)
        );
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::SyncBeforeMetroDecline)
        );

        let _effects = update(&mut state, Action::SyncBeforeMetroDecline);
        assert!(
            state.modal_stack.modal.is_none(),
            "SyncBeforeMetroDecline must clear modal"
        );
    }

    #[test]
    fn external_metro_conflict_dismisses_on_n_and_esc() {
        let mut state = base_state();
        state.modal_stack.modal = Some(ModalState::ExternalMetroConflict {
            pid: 12345,
            working_dir: "/tmp".into(),
        });
        assert_eq!(handle_key(&state, key('n')), Some(Action::ModalCancel));
        assert_eq!(handle_key(&state, key('N')), Some(Action::ModalCancel));
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
        let _effects = update(&mut state, Action::ModalCancel);
        assert!(state.modal_stack.modal.is_none());
    }

    #[test]
    fn branch_picker_modal_dismisses_on_esc() {
        let mut state = base_state();
        state.modal_stack.modal = Some(ModalState::BranchPicker {
            branches: Vec::new(),
            selected: 0,
            filter: String::new(),
        });
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
        let _effects = update(&mut state, Action::ModalCancel);
        assert!(state.modal_stack.modal.is_none());
    }
}

// =========================================================================
// Sub-module 3: Command queue routing (COVER-03 layer 3)
// =========================================================================

mod command_queue {
    use super::*;

    #[test]
    fn command_queue_push_appends_to_back() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        // Phase 14 / D-21: assert against the slice (primary source of truth).
        assert_eq!(slice_queue_len(&state, "wt-1"), 0, "precondition: slice queue empty");

        let _effects = update(
            &mut state,
            Action::CommandQueuePush(CommandSpec::YarnInstall),
        );
        let _effects = update(
            &mut state,
            Action::CommandQueuePush(CommandSpec::YarnPodInstall),
        );

        // D-21: slice-side queue assertions.
        assert_eq!(slice_queue_len(&state, "wt-1"), 2, "slice queue must hold 2 items");
        assert_eq!(
            state.worktrees.get(&WorktreeId("wt-1".into())).and_then(|s| s.queue.front()),
            Some(&CommandSpec::YarnInstall),
            "slice queue front must be YarnInstall"
        );
        assert_eq!(
            state.worktrees.get(&WorktreeId("wt-1".into())).and_then(|s| s.queue.back()),
            Some(&CommandSpec::YarnPodInstall),
            "slice queue back must be YarnPodInstall"
        );
    }

    #[test]
    fn command_exited_with_empty_queue_clears_running_command() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        // Simulate a running task in the slice (D-21: task lives in slice).
        state.worktrees.get_mut(&WorktreeId("wt-1".into())).unwrap().task =
            Some(synthetic_task_record(1, CommandSpec::GitFetch));
        assert_eq!(slice_queue_len(&state, "wt-1"), 0, "precondition: slice queue empty");

        let _effects = update(&mut state, Action::CommandExited {
            task_id: crate::domain::task::TaskId(1),
            status: crate::domain::task::ExitStatus::Success,
        });

        // D-21: slice-side assertion — task cleared after CommandExited.
        assert_no_running_task_anywhere(&state);
        assert_eq!(slice_queue_len(&state, "wt-1"), 0, "queue stays empty");
    }

    #[test]
    fn command_exited_with_nonempty_queue_pops_and_dispatches_front() {
        let mut state = base_state();
        // Seed one worktree so `dispatch_command` does not early-return.
        seed_one_worktree(&mut state);

        // D-21: task + queue live in the slice.
        let wid = WorktreeId("wt-1".into());
        state.worktrees.get_mut(&wid).unwrap().task =
            Some(synthetic_task_record(2, CommandSpec::GitFetch));
        // GitFetch has RefreshSet::none() — no tokio::spawn on the refresh path.
        // YarnInstall doesn't need metro, so drain routes through dispatch_command,
        // which emits Effect::SpawnTask.
        state.worktrees.get_mut(&wid).unwrap().queue.push_back(CommandSpec::YarnInstall);
        state.worktrees.get_mut(&wid).unwrap().queue.push_back(CommandSpec::YarnPodInstall);
        let effects = update(&mut state, Action::CommandExited {
            task_id: crate::domain::task::TaskId(2),
            status: crate::domain::task::ExitStatus::Success,
        });

        // D-21: after draining the queue head, the slice task is cleared
        // (SpawnTask effect was emitted; the runtime will write back slice.task
        // when the effect resolves — not visible in a pure update() test).
        // The remaining YarnPodInstall stays in the slice queue.
        assert_eq!(slice_queue_len(&state, "wt-1"), 1,
            "one item (YarnPodInstall) must remain in the slice queue after drain");
        // Plan 14-06: dispatch_command now returns Effect::SpawnTask.
        assert!(
            effects.iter().any(|e| matches!(e, Effect::SpawnTask { .. })),
            "CommandExited drain must emit Effect::SpawnTask for the popped spec; got {effects:?}"
        );
    }
}

// =========================================================================
// Sub-module 4: WorktreesLoaded — slice map population (Plan 14-03)
// =========================================================================

mod worktrees_loaded {
    use super::*;

    fn make_worktree(id: &str, branch: &str) -> Worktree {
        Worktree {
            id: WorktreeId(id.into()),
            path: std::path::PathBuf::from(format!("/tmp/{id}")),
            branch: branch.into(),
            head_sha: "0000000".into(),
            metro_status: WorktreeMetroStatus::Stopped,
            jira_title: None,
            stale: false,
            stale_pods: false,
            jira_key: None,
        }
    }

    #[test]
    fn worktrees_loaded_populates_slice_map() {
        let mut state = AppState::default();
        let worktrees = vec![
            make_worktree("wt-A", "main"),
            make_worktree("wt-B", "feat"),
        ];
        let _ = update(&mut state, Action::WorktreesLoaded(worktrees));
        assert!(state.worktrees.contains_key(&WorktreeId("wt-A".into())));
        assert!(state.worktrees.contains_key(&WorktreeId("wt-B".into())));
    }
}

// =========================================================================
// Sub-module 5: Parallelism — TASK-02 contract (Plan 14-08)
// =========================================================================

mod parallelism {
    use super::*;

    /// TASK-02: two worktrees can each have a running task simultaneously.
    ///
    /// Pure data test — slice.task fields are populated directly (no runtime).
    /// The runtime would set slice.task when it processes Effect::SpawnTask;
    /// here we verify the data model supports concurrent task ownership.
    #[test]
    fn yarn_install_on_a_while_jest_on_b_both_have_tasks() {
        let mut state = base_state();
        seed_two_worktrees(&mut state, "wt-A", "wt-B");

        // Simulate concurrent tasks across two worktrees.
        state.worktrees.get_mut(&WorktreeId("wt-A".into())).unwrap().task =
            Some(synthetic_task_record(1, CommandSpec::YarnInstall));
        state.worktrees.get_mut(&WorktreeId("wt-B".into())).unwrap().task =
            Some(synthetic_task_record(2, CommandSpec::YarnJest { filter: String::new() }));

        assert_running_in(&state, "wt-A");
        assert_running_in(&state, "wt-B");

        let count_running = state.worktrees.values().filter(|s| s.task.is_some()).count();
        assert_eq!(count_running, 2, "TASK-02 contract: parallel tasks across worktrees");
    }

    /// COVER-01 / D-13 contract: MetroStart while already running triggers
    /// the restart path, not a double-spawn.
    ///
    /// This is a unit-level restatement of the integration test in
    /// `tests/metro_single_instance.rs` (COVER-01). If accessing
    /// `FakeMetroHandle` from within `src/` is impractical without exposing
    /// a public test-helper type, this test is marked `#[ignore]` and
    /// coverage deferred to COVER-01.
    #[test]
    fn metro_start_on_a_while_metro_running_on_b_keeps_single_instance() {
        let mut state = base_state();
        seed_two_worktrees(&mut state, "wt-A", "wt-B");

        // Register a fake MetroHandle to simulate metro running in wt-B.
        #[derive(Debug)]
        struct FakeMetroHandle { pid: u32, worktree_id: String }
        impl crate::domain::ports::metro_port::MetroHandle for FakeMetroHandle {
            fn pid(&self) -> u32 { self.pid }
            fn worktree_id(&self) -> &str { &self.worktree_id }
            fn send_stdin(&self, _bytes: Vec<u8>) -> anyhow::Result<()> { Ok(()) }
            fn kill(self: Box<Self>) -> anyhow::Result<()> { Ok(()) }
        }
        state.metro.register(Box::new(FakeMetroHandle { pid: 9001, worktree_id: "wt-B".into() }));
        assert!(state.metro.is_running(), "precondition: metro running in wt-B");

        // Set active worktree to A (index 0) and dispatch MetroStart.
        state.worktree_browser.worktree_table_state.select(Some(0));
        // Skip external detection so the test stays synchronous.
        state.metro_state.skip_external_metro_check = true;
        let _effects = update(&mut state, Action::MetroStart);

        // COVER-01 contract: still only one MetroHandle registered (restart path,
        // not double-spawn). The handler calls MetroStop first (pending_restart=true)
        // so metro may be Stopping or about to be restarted — either way is_running()
        // reflects the single-instance invariant.
        // Regardless of whether pending_restart or restart-in-progress, no second
        // SpawnMetro effect should have fired unconditionally.
        assert!(state.metro_state.pending_restart,
            "COVER-01: MetroStart-while-running must set pending_restart=true");
    }
}

// =========================================================================
// Sub-module 6: Routing — TASK-03 contract (Plan 14-08)
// =========================================================================

mod routing {
    use super::*;

    /// TASK-03 / D-08: CommandOutputLine routes to the slice that owns the
    /// task_id, regardless of which worktree is currently selected in the UI.
    #[test]
    fn command_output_line_routes_to_correct_slice_regardless_of_active_worktree() {
        let mut state = base_state();
        seed_two_worktrees(&mut state, "wt-A", "wt-B");

        // slice_A holds task with id=5; slice_B has no task.
        state.worktrees.get_mut(&WorktreeId("wt-A".into())).unwrap().task =
            Some(synthetic_task_record(5, CommandSpec::YarnInstall));

        // Active worktree = B (UI has selected index 1 = wt-B).
        // seed_two_worktrees puts wt-A at 0 and wt-B at 1; select(1) = B.
        state.worktree_browser.worktree_table_state.select(Some(1));

        let _ = update(&mut state, Action::CommandOutputLine {
            task_id: crate::domain::task::TaskId(5),
            line: "from-A".into(),
        });

        let a_out = slice_output(&state, "wt-A");
        let b_out = slice_output(&state, "wt-B");
        assert!(
            a_out.iter().any(|l| l == "from-A"),
            "D-08: line must land in slice_A (task owner); A={:?} B={:?}", a_out, b_out
        );
        assert!(
            !b_out.iter().any(|l| l == "from-A"),
            "D-08: line must NOT land in slice_B (not task owner); A={:?} B={:?}", a_out, b_out
        );
    }

    /// TASK-03 / D-11: CommandExited drains the slice-local queue of the
    /// exiting task, leaving other worktrees' queues untouched.
    #[test]
    fn command_exited_drains_slice_local_queue_not_other() {
        let mut state = base_state();
        seed_two_worktrees(&mut state, "wt-A", "wt-B");

        // Task on A; both slices have one queued item.
        state.worktrees.get_mut(&WorktreeId("wt-A".into())).unwrap().task =
            Some(synthetic_task_record(7, CommandSpec::GitFetch));
        state.worktrees.get_mut(&WorktreeId("wt-A".into())).unwrap()
            .queue.push_back(CommandSpec::YarnInstall);
        state.worktrees.get_mut(&WorktreeId("wt-B".into())).unwrap()
            .queue.push_back(CommandSpec::YarnPodInstall);

        let queue_b_before = slice_queue_len(&state, "wt-B");

        let _ = update(&mut state, Action::CommandExited {
            task_id: crate::domain::task::TaskId(7),
            status: crate::domain::task::ExitStatus::Success,
        });

        // A's queue was drained (YarnInstall dispatched via SpawnTask effect).
        // slice.task is None post-exit; SpawnTask effect will populate it in runtime.
        assert_eq!(slice_queue_len(&state, "wt-A"), 0,
            "D-11: A's queue must drain on CommandExited");
        // B's queue untouched.
        assert_eq!(slice_queue_len(&state, "wt-B"), queue_b_before,
            "D-11: B's queue must not change");
    }
}

// =========================================================================
// Sub-module 7: Stale-drop — RESEARCH §Pitfall P-3 (Plan 14-08)
// =========================================================================

mod stale_drop {
    use super::*;

    /// P-3 / D-08: a CommandOutputLine for a task that no slice owns is
    /// silently dropped — it must not contaminate any slice's output buffer.
    ///
    /// This guards the fast-cancel+respawn race: late stdout from the dead
    /// process arrives AFTER the slice cleared its task. Since no slice has
    /// `task.id == 99`, the line should not appear anywhere in `state.worktrees`.
    #[test]
    fn late_command_output_line_for_cancelled_task_is_silently_dropped() {
        let mut state = base_state();
        seed_one_worktree_id(&mut state, "wt-A");
        // No task on slice_A — task_id 99 belongs to nobody.

        let _ = update(&mut state, Action::CommandOutputLine {
            task_id: crate::domain::task::TaskId(99),
            line: "stale".into(),
        });

        // P-3 contract: the stale line must not land in any slice's output.
        let any_slice_has_it = state.worktrees.values()
            .any(|s| s.output.iter().any(|l| l == "stale"));
        assert!(!any_slice_has_it,
            "P-3: stale output line must not contaminate any slice; \
             slices = {:?}",
            state.worktrees.iter().map(|(k, v)| (k, v.output.len())).collect::<Vec<_>>()
        );
    }
}

// =========================================================================
// Sub-module 8: Collision gate — TASK-05 contract (Plan 15-05)
// =========================================================================

mod collision {
    use super::*;

    /// CollisionPolicy::BlockNew path — the existing YarnInstall task keeps
    /// running and the new dispatch is silently dropped. No SpawnTask Effect,
    /// no `$ argv` line, no `[cancelled by new dispatch]` line.
    #[test]
    fn collision_block_new_yarn_install_drops_new_dispatch() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let wid = WorktreeId("wt-1".into());

        // Seed a running YarnInstall task on the slice.
        state.worktrees.get_mut(&wid).unwrap().task =
            Some(synthetic_task_record(100, CommandSpec::YarnInstall));
        let output_before: Vec<String> = state.worktrees.get(&wid).unwrap()
            .output.iter().cloned().collect();

        let effects = update(
            &mut state,
            Action::CommandRun(CommandSpec::YarnInstall),
        );

        // (a) No SpawnTask effect emitted.
        let has_spawn = effects.iter().any(|e| matches!(e, Effect::SpawnTask { .. }));
        assert!(!has_spawn, "BlockNew must NOT emit Effect::SpawnTask; got {effects:?}");

        // (b) slice.task is STILL Some with the original task_id (100).
        let task = state.worktrees.get(&wid).unwrap().task.as_ref();
        assert!(task.is_some(), "BlockNew must leave existing task in slice");
        assert_eq!(task.unwrap().id, crate::domain::task::TaskId(100),
            "BlockNew must preserve the original task_id");

        // (c) slice.output is unchanged — no $ argv line, no [cancelled ...] line.
        let output_after: Vec<String> = state.worktrees.get(&wid).unwrap()
            .output.iter().cloned().collect();
        assert_eq!(output_before, output_after,
            "BlockNew must not write any output line; before={output_before:?} after={output_after:?}");
    }

    /// CollisionPolicy::CancelPrevious path — existing YarnJest task is aborted
    /// and replaced by the NEW dispatch. SpawnTask emitted with the NEW filter;
    /// `[cancelled by new dispatch]` appears in output.
    ///
    /// NOTE: `Action::CommandRun(YarnJest { .. })` routes through the TextInput
    /// modal before reaching `dispatch_command`. To exercise the gate directly
    /// without the modal dance, this test simulates the TextInput-submit path:
    /// it opens a TextInput modal with a YarnJest template and pre-fills the
    /// buffer ("second"), then submits via `Action::ModalInputSubmit`. The
    /// submit handler invokes `dispatch_command(state, YarnJest { filter:
    /// "second" })` — which is exactly where the collision gate lives.
    #[test]
    fn collision_cancel_previous_yarn_jest_replaces_task() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let wid = WorktreeId("wt-1".into());

        // Seed a running YarnJest("first") task on the slice.
        state.worktrees.get_mut(&wid).unwrap().task = Some(synthetic_task_record(
            200,
            CommandSpec::YarnJest { filter: "first".into() },
        ));

        // Stage a TextInput modal whose template is YarnJest, buffer = "second".
        // ModalInputSubmit will compose CommandSpec::YarnJest { filter: "second" }
        // and call dispatch_command — the collision gate's true entry point.
        state.modal_stack.modal = Some(ModalState::TextInput {
            prompt: "Jest filter:".into(),
            buffer: "second".into(),
            pending_template: Box::new(CommandSpec::YarnJest { filter: String::new() }),
        });

        let effects = update(&mut state, Action::ModalInputSubmit);

        // (a) Exactly one SpawnTask Effect with the NEW filter.
        let has_spawn_second = effects.iter().any(|e| matches!(
            e,
            Effect::SpawnTask { spec: CommandSpec::YarnJest { filter }, .. } if filter == "second"
        ));
        assert!(has_spawn_second,
            "CancelPrevious must emit Effect::SpawnTask for the NEW YarnJest dispatch; got {effects:?}");

        // (b) slice.task is None — old record was taken; new record arrives
        //     asynchronously via task_handle_tx (not visible to synchronous update()).
        assert!(state.worktrees.get(&wid).unwrap().task.is_none(),
            "CancelPrevious must take the existing record from slice.task");

        // (c) slice.output contains [cancelled by new dispatch].
        let output: Vec<String> = state.worktrees.get(&wid).unwrap()
            .output.iter().cloned().collect();
        assert!(output.iter().any(|l| l == "[cancelled by new dispatch]"),
            "CancelPrevious must push [cancelled by new dispatch]; output={output:?}");
    }

    /// Different discriminants do NOT collide — existing YarnInstall task is
    /// untouched when a YarnLint dispatch arrives. No [cancelled ...] line.
    ///
    /// YarnLint is chosen because it has no text-input or device-picker
    /// pre-processing, so `Action::CommandRun(YarnLint)` reaches
    /// `dispatch_command` directly — the gate's true site.
    #[test]
    fn collision_different_discriminants_dispatch_normally() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let wid = WorktreeId("wt-1".into());

        // Seed a running YarnInstall task.
        state.worktrees.get_mut(&wid).unwrap().task =
            Some(synthetic_task_record(300, CommandSpec::YarnInstall));

        let effects = update(
            &mut state,
            Action::CommandRun(CommandSpec::YarnLint),
        );

        // (a) Existing YarnInstall record is still in slice.task — different
        //     discriminant means no collision, no cancellation.
        let task = state.worktrees.get(&wid).unwrap().task.as_ref();
        assert!(task.is_some(), "existing YarnInstall task must remain — different discriminant");
        assert_eq!(task.unwrap().id, crate::domain::task::TaskId(300),
            "original YarnInstall task_id must be preserved");

        // (b) No [cancelled by new dispatch] line — gate did not fire.
        let output: Vec<String> = state.worktrees.get(&wid).unwrap()
            .output.iter().cloned().collect();
        assert!(!output.iter().any(|l| l == "[cancelled by new dispatch]"),
            "no [cancelled by new dispatch] line when discriminants differ; output={output:?}");

        // (c) SpawnTask Effect emitted for the new YarnLint (different discriminant
        //     means the dispatch flows through normally to allocate a new task).
        let has_spawn_lint = effects.iter().any(|e| matches!(
            e,
            Effect::SpawnTask { spec: CommandSpec::YarnLint, .. }
        ));
        assert!(has_spawn_lint,
            "different-discriminant dispatch must emit Effect::SpawnTask for the new spec; got {effects:?}");
    }

    /// Q-4 honor: git → BlockNew. Existing GitPull task keeps running; second
    /// GitPull dispatch is silently dropped. No SpawnTask, no output change.
    #[test]
    fn collision_git_pull_block_new() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let wid = WorktreeId("wt-1".into());

        // Seed a running GitPull task.
        state.worktrees.get_mut(&wid).unwrap().task =
            Some(synthetic_task_record(400, CommandSpec::GitPull));
        let output_before: Vec<String> = state.worktrees.get(&wid).unwrap()
            .output.iter().cloned().collect();

        let effects = update(&mut state, Action::CommandRun(CommandSpec::GitPull));

        // (a) No SpawnTask effect emitted.
        let has_spawn = effects.iter().any(|e| matches!(e, Effect::SpawnTask { .. }));
        assert!(!has_spawn, "git BlockNew must NOT emit Effect::SpawnTask; got {effects:?}");

        // (b) Existing GitPull task still in slice.
        let task = state.worktrees.get(&wid).unwrap().task.as_ref();
        assert!(task.is_some(), "git BlockNew must preserve existing task");
        assert_eq!(task.unwrap().id, crate::domain::task::TaskId(400),
            "original GitPull task_id must be preserved");

        // (c) Output unchanged.
        let output_after: Vec<String> = state.worktrees.get(&wid).unwrap()
            .output.iter().cloned().collect();
        assert_eq!(output_before, output_after,
            "git BlockNew must not write any output line");
    }
}

// =========================================================================
// Sub-module 9: Cancellation guard — TASK-04 contract (Plan 15-05)
// =========================================================================

mod cancellation_guard {
    use super::*;

    /// 15-RESEARCH §Pitfall 5: CommandCancel on a running git task is a NO-OP.
    /// The record is re-inserted into the slice; queue and output are unchanged.
    #[test]
    fn cancel_on_git_pull_is_noop_record_reinserted() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let wid = WorktreeId("wt-1".into());

        // Seed a running GitPull task; pre-load the queue with a follow-up
        // (which must NOT be cleared).
        state.worktrees.get_mut(&wid).unwrap().task =
            Some(synthetic_task_record(500, CommandSpec::GitPull));
        state.worktrees.get_mut(&wid).unwrap().queue.push_back(CommandSpec::YarnInstall);
        let output_before: Vec<String> = state.worktrees.get(&wid).unwrap()
            .output.iter().cloned().collect();

        let _effects = update(&mut state, Action::CommandCancel);

        // (a) slice.task still Some with the SAME task_id (re-inserted).
        let task = state.worktrees.get(&wid).unwrap().task.as_ref();
        assert!(task.is_some(), "non-cancellable: record must be re-inserted");
        assert_eq!(task.unwrap().id, crate::domain::task::TaskId(500),
            "non-cancellable: original task_id must be preserved");

        // (b) Queue is unchanged — NOT cleared.
        assert_eq!(slice_queue_len(&state, "wt-1"), 1,
            "non-cancellable: queue must NOT be cleared");

        // (c) Output does NOT contain [cancelled] line.
        let output_after: Vec<String> = state.worktrees.get(&wid).unwrap()
            .output.iter().cloned().collect();
        assert_eq!(output_before, output_after,
            "non-cancellable: no [cancelled] line written; before={output_before:?} after={output_after:?}");
        assert!(!output_after.iter().any(|l| l == "[cancelled]"),
            "non-cancellable: explicit no-[cancelled] check");
    }

    /// Cancellable variants preserve Phase 14 behavior: take, abort, clear queue,
    /// push [cancelled].
    #[test]
    fn cancel_on_yarn_install_aborts_and_clears() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let wid = WorktreeId("wt-1".into());

        // Seed a running YarnInstall task and pre-load a queued follow-up.
        state.worktrees.get_mut(&wid).unwrap().task =
            Some(synthetic_task_record(600, CommandSpec::YarnInstall));
        state.worktrees.get_mut(&wid).unwrap().queue.push_back(CommandSpec::YarnLint);

        let _effects = update(&mut state, Action::CommandCancel);

        // (a) slice.task is None — record was taken.
        assert!(state.worktrees.get(&wid).unwrap().task.is_none(),
            "cancellable: slice.task must be cleared");

        // (b) Queue is empty — cleared.
        assert_eq!(slice_queue_len(&state, "wt-1"), 0,
            "cancellable: queue must be cleared");

        // (c) Output contains [cancelled].
        let output: Vec<String> = state.worktrees.get(&wid).unwrap()
            .output.iter().cloned().collect();
        assert!(output.iter().any(|l| l == "[cancelled]"),
            "cancellable: output must contain [cancelled]; got {output:?}");
    }
}
