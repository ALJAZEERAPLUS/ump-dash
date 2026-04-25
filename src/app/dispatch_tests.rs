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
    state.worktree_browser.worktrees.push(Worktree {
        id: WorktreeId("wt-1".into()),
        path: std::path::PathBuf::from("/tmp/wt-1"),
        branch: "main".into(),
        head_sha: "0000000".into(),
        metro_status: WorktreeMetroStatus::Stopped,
        jira_title: None,
        stale: false,
        stale_pods: false,
        jira_key: None,
    });
    state.worktree_browser.worktree_table_state.select(Some(0));
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
        assert!(state.command_runner.command_queue.is_empty());

        let _effects = update(
            &mut state,
            Action::CommandQueuePush(CommandSpec::YarnInstall),
        );
        let _effects = update(
            &mut state,
            Action::CommandQueuePush(CommandSpec::YarnPodInstall),
        );

        assert_eq!(state.command_runner.command_queue.len(), 2);
        assert_eq!(
            state.command_runner.command_queue.front(),
            Some(&CommandSpec::YarnInstall)
        );
        assert_eq!(
            state.command_runner.command_queue.back(),
            Some(&CommandSpec::YarnPodInstall)
        );
    }

    #[test]
    fn command_exited_with_empty_queue_clears_running_command() {
        let mut state = base_state();
        state.command_runner.running_command = Some(CommandSpec::GitFetch);
        assert!(state.command_runner.command_queue.is_empty());

        let _effects = update(&mut state, Action::CommandExited);

        assert!(
            state.command_runner.running_command.is_none(),
            "CommandExited must clear running_command"
        );
        assert!(state.command_runner.command_queue.is_empty(), "queue stays empty");
    }

    #[test]
    fn command_exited_with_nonempty_queue_pops_and_dispatches_front() {
        let mut state = base_state();
        // Seed one worktree so `dispatch_command` does not early-return.
        seed_one_worktree(&mut state);
        state.command_runner.running_command = Some(CommandSpec::GitFetch);
        // GitFetch has RefreshSet::none() — no tokio::spawn on the refresh
        // path. YarnInstall doesn't need metro, so drain routes through
        // `dispatch_command`, which sets running_command to the popped spec.
        state.command_runner.command_queue.push_back(CommandSpec::YarnInstall);
        state.command_runner.command_queue.push_back(CommandSpec::YarnPodInstall);

        let effects = update(&mut state, Action::CommandExited);

        assert_eq!(
            state.command_runner.running_command.as_ref(),
            Some(&CommandSpec::YarnInstall),
            "CommandExited must set running_command to the popped front of the queue"
        );
        assert_eq!(state.command_runner.command_queue.len(), 1);
        assert_eq!(
            state.command_runner.command_queue.front(),
            Some(&CommandSpec::YarnPodInstall)
        );
        // Post-F-201: dispatch_command returned Effect::SpawnCommand, which
        // should be present in the returned vec.
        assert!(
            effects.iter().any(|e| matches!(e, Effect::SpawnCommand { .. })),
            "CommandExited drain must emit Effect::SpawnCommand for the popped spec; got {effects:?}"
        );
    }
}
