//! COVER-03 — TEA dispatch-path characterization.
//!
//! This file covers three TEA surfaces that Phase 13 refactors are most
//! likely to touch:
//!
//! 1. Palette → Action resolution (`handle_key` palette branches): 5
//!    `PaletteMode` variants × 2-7 keys each + unrecognized-key fallback.
//! 2. Modal dismissal: 9 `ModalState` variants × dismiss keys. Post-condition:
//!    `state.modal_stack.modal == None` after `update()`.
//! 3. Command queue routing: `CommandQueuePush` appends; `CommandExited` drains.
//!
//! Post-F-201 (Plan 13-07): `update()` signature is now
//! `pub fn update(state: &mut AppState, action: Action) -> Vec<Effect>`.
//! Tests no longer need the `metro_tx` / `handle_tx` channels — they just
//! call `update()` and (optionally) assert on the returned `Vec<Effect>`.
//! Most tests in this file care about state mutations, not effects, so the
//! return value is typically bound to `_`.

use super::effect::Effect;
use super::keybindings::{footer_hints_for, help_overlay_rows};
use super::*;
use crate::domain::action::Action;
use crate::domain::command::{CleanOptions, CommandSpec, ModalState, RunVariant};
use crate::domain::native_cache::{
    AndroidCacheHit, AndroidCacheLookup, AndroidCacheMetadata, AndroidCacheState,
    CachedAndroidLaunchResult, CachedIosLaunchResult, IosSimulatorCacheHit,
    IosSimulatorCacheMetadata, IosSimulatorCacheState,
};
use crate::domain::worktree::{Worktree, WorktreeId, WorktreeMetroStatus};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

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
    state
        .worktree_browser
        .worktree_table_state
        .select(Some(idx));
    // Ensure a slice exists for this worktree.
    state
        .worktrees
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

fn cached_ios_hit_fixture() -> IosSimulatorCacheHit {
    IosSimulatorCacheHit {
        metadata: IosSimulatorCacheMetadata {
            platform: "ios-simulator".into(),
            fingerprint: "fingerprint-a".into(),
            bundle_id: "com.aljazeera.test".into(),
            variant: "Debug".into(),
            created_at: "2026-06-01T00:00:00Z".into(),
            source_worktree: "wt-1".into(),
            artifact_kind: "app-bundle".into(),
            storage_mode: "copy".into(),
            source_artifact_path: std::path::PathBuf::from("/tmp/wt-1/app.app"),
            artifact_digest_algorithm: "sha256".into(),
            artifact_digest: "digest".into(),
        },
        artifact_path: std::path::PathBuf::from("/tmp/cached.app"),
    }
}

fn cached_android_hit_fixture() -> AndroidCacheHit {
    AndroidCacheHit {
        metadata: AndroidCacheMetadata {
            platform: "android".into(),
            fingerprint: "fingerprint-a".into(),
            application_id: "com.aljazeera.test".into(),
            variant: "localDebugOptimized".into(),
            created_at: "2026-06-04T00:00:00Z".into(),
            source_worktree: "wt-1".into(),
            artifact_kind: "apk".into(),
            storage_mode: "copy".into(),
            source_artifact_path: std::path::PathBuf::from("/tmp/wt-1/app.apk"),
            artifact_digest_algorithm: "sha256".into(),
            artifact_digest: "digest".into(),
        },
        artifact_path: std::path::PathBuf::from("/tmp/cached.apk"),
    }
}

fn pending_cached_ios_run_fixture(
    worktree_id: &str,
    cache_hit: IosSimulatorCacheHit,
) -> super::state::PendingCachedIosRun {
    super::state::PendingCachedIosRun {
        worktree_id: WorktreeId(worktree_id.into()),
        worktree_path: std::path::PathBuf::from(format!("/tmp/{worktree_id}")),
        cache_hit,
        device_request_id: 1,
    }
}

fn pending_cached_android_run_fixture(
    worktree_id: &str,
    cache_hit: AndroidCacheHit,
) -> super::state::PendingCachedAndroidRun {
    super::state::PendingCachedAndroidRun {
        worktree_id: WorktreeId(worktree_id.into()),
        worktree_path: std::path::PathBuf::from(format!("/tmp/{worktree_id}")),
        cache_hit,
        device_request_id: 1,
    }
}

// =========================================================================
// Phase 14 / D-21 slice-side assertion helpers
// =========================================================================

/// Assert the named worktree's slice has a running task.
fn assert_running_in(state: &AppState, id: &str) {
    let wid = WorktreeId(id.into());
    assert!(
        state
            .worktrees
            .get(&wid)
            .and_then(|s| s.task.as_ref())
            .is_some(),
        "expected worktree {id:?} to have a running task; slice = {:?}",
        state
            .worktrees
            .get(&wid)
            .map(|s| (s.task.is_some(), s.queue.len())),
    );
}

/// Assert no slice has a running task.
fn assert_no_running_task_anywhere(state: &AppState) {
    let any = state.worktrees.values().any(|s| s.task.is_some());
    assert!(
        !any,
        "expected no slice to have a running task, but at least one does"
    );
}

/// Queue length for the named worktree's slice.
fn slice_queue_len(state: &AppState, id: &str) -> usize {
    state
        .worktrees
        .get(&WorktreeId(id.into()))
        .map(|s| s.queue.len())
        .unwrap_or(0)
}

/// Snapshot of slice output lines for the named worktree.
fn slice_output(state: &AppState, id: &str) -> Vec<String> {
    state
        .worktrees
        .get(&WorktreeId(id.into()))
        .map(|s| s.output.iter().cloned().collect())
        .unwrap_or_default()
}

#[test]
fn cached_ios_launch_result_appends_to_selected_slice_output() {
    let mut state = base_state();
    seed_one_worktree(&mut state);

    let effects = update(
        &mut state,
        Action::CachedIosLaunchFinished {
            worktree_id: WorktreeId("wt-1".into()),
            result: CachedIosLaunchResult::Success(vec![
                "Metro port 8093".into(),
                "Booted iPhone 15 Pro".into(),
            ]),
        },
    );

    assert!(effects.is_empty());
    let output = slice_output(&state, "wt-1");
    assert!(
        output
            .iter()
            .any(|line| line.contains("installed and launched")),
        "expected success summary in output; got {output:?}"
    );
    assert!(
        output.iter().any(|line| line.contains("Metro port 8093")),
        "expected metro port line in output; got {output:?}"
    );
}

#[test]
fn cached_ios_launch_failure_appends_error_and_resets_scroll() {
    let mut state = base_state();
    seed_one_worktree(&mut state);
    state
        .worktrees
        .get_mut(&WorktreeId("wt-1".into()))
        .expect("slice should exist")
        .output_scroll = 12;

    let effects = update(
        &mut state,
        Action::CachedIosLaunchFinished {
            worktree_id: WorktreeId("wt-1".into()),
            result: CachedIosLaunchResult::Failure("install failed".into()),
        },
    );

    assert!(effects.is_empty());
    let slice = state
        .worktrees
        .get(&WorktreeId("wt-1".into()))
        .expect("slice should exist");
    assert!(
        slice
            .output
            .iter()
            .any(|line| line == "[cached-ios error] install failed"),
        "expected cached iOS error in output; got {:?}",
        slice.output
    );
    assert_eq!(slice.output_scroll, 0);
}

#[test]
fn invalid_cached_ios_artifact_falls_back_to_normal_run_for_origin_worktree() {
    let mut state = base_state();
    seed_one_worktree(&mut state);
    state
        .worktrees
        .get_mut(&WorktreeId("wt-1".into()))
        .expect("slice should exist")
        .ios_simulator_cache = IosSimulatorCacheState::Hit(Box::new(cached_ios_hit_fixture()));

    let effects = update(
        &mut state,
        Action::CachedIosLaunchFinished {
            worktree_id: WorktreeId("wt-1".into()),
            result: CachedIosLaunchResult::InvalidArtifact {
                message: "cached .app digest mismatch".into(),
                device_id: "SIM-1".into(),
                variant: RunVariant::Dev,
            },
        },
    );

    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::SpawnTask {
                worktree_id,
                spec: CommandSpec::UmpRunIos {
                    device_id,
                    variant: Some(RunVariant::Dev),
                },
                ..
            } if worktree_id == &WorktreeId("wt-1".into()) && device_id == "SIM-1"
        )),
        "invalid cached iOS artifact should fall back to the normal iOS run; got {effects:?}"
    );
    let output = slice_output(&state, "wt-1");
    assert!(
        output
            .iter()
            .any(|line| line.contains("cached .app digest mismatch")),
        "expected invalid artifact message in output; got {output:?}"
    );
    assert!(matches!(
        state
            .worktrees
            .get(&WorktreeId("wt-1".into()))
            .expect("slice should exist")
            .ios_simulator_cache,
        IosSimulatorCacheState::Error(ref message)
            if message.contains("cached .app digest mismatch")
    ));
}

#[test]
fn invalid_cached_android_artifact_falls_back_to_normal_run_for_origin_worktree() {
    let mut state = base_state();
    seed_one_worktree(&mut state);
    state
        .worktrees
        .get_mut(&WorktreeId("wt-1".into()))
        .expect("slice should exist")
        .android_cache = AndroidCacheState::Hit(Box::new(cached_android_hit_fixture()));

    let effects = update(
        &mut state,
        Action::CachedAndroidLaunchFinished {
            worktree_id: WorktreeId("wt-1".into()),
            result: CachedAndroidLaunchResult::InvalidArtifact {
                message: "cached APK digest mismatch".into(),
                device_id: "emulator-5554".into(),
                variant: RunVariant::Prod,
            },
        },
    );

    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::SpawnTask {
                worktree_id,
                spec: CommandSpec::UmpRunAndroid {
                    device_id,
                    variant: Some(RunVariant::Prod),
                },
                ..
            } if worktree_id == &WorktreeId("wt-1".into()) && device_id == "emulator-5554"
        )),
        "invalid cached Android artifact should fall back to the normal Android run; got {effects:?}"
    );
    let output = slice_output(&state, "wt-1");
    assert!(
        output
            .iter()
            .any(|line| line.contains("cached APK digest mismatch")),
        "expected invalid artifact message in output; got {output:?}"
    );
    assert!(matches!(
        state
            .worktrees
            .get(&WorktreeId("wt-1".into()))
            .expect("slice should exist")
            .android_cache,
        AndroidCacheState::Error(ref message) if message.contains("cached APK digest mismatch")
    ));
}

/// A test-only `TaskHandle` that does nothing — abort() is a no-op.
#[derive(Debug)]
struct NoopHandle;

impl crate::domain::ports::task_handle::TaskHandle for NoopHandle {
    fn abort(&self) {}
}

#[derive(Debug)]
struct FakeMetroHandle {
    pid: u32,
    worktree_id: String,
    port: u16,
}

impl crate::domain::ports::metro_port::MetroHandle for FakeMetroHandle {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn worktree_id(&self) -> &str {
        &self.worktree_id
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn send_stdin(&self, _bytes: Vec<u8>) -> anyhow::Result<()> {
        Ok(())
    }

    fn kill(self: Box<Self>) -> anyhow::Result<()> {
        Ok(())
    }
}

fn register_ready_metro(state: &mut AppState, id: &str, port: u16) {
    state
        .worktrees
        .get_mut(&WorktreeId(id.into()))
        .expect("slice should exist")
        .metro
        .register(Box::new(FakeMetroHandle {
            pid: 9001,
            worktree_id: id.into(),
            port,
        }));
    state
        .worktrees
        .get_mut(&WorktreeId(id.into()))
        .expect("slice should exist")
        .metro
        .record_activity(crate::domain::metro::MetroActivity::Ready);
}

fn register_metro_without_activity(state: &mut AppState, id: &str, port: u16) {
    state
        .worktrees
        .get_mut(&WorktreeId(id.into()))
        .expect("slice should exist")
        .metro
        .register(Box::new(FakeMetroHandle {
            pid: 9002,
            worktree_id: id.into(),
            port,
        }));
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

        assert_eq!(handle_key(&state, key('d')), Some(Action::ModalCancel));
        assert_eq!(handle_key(&state, key('e')), Some(Action::ModalCancel));

        assert_eq!(
            handle_key(&state, key('r')),
            Some(Action::CommandRun(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: None,
            }))
        );
        assert_eq!(handle_key(&state, key('m')), Some(Action::ModalCancel));
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

        assert_eq!(handle_key(&state, key('d')), Some(Action::ModalCancel));
        assert_eq!(handle_key(&state, key('e')), Some(Action::ModalCancel));
        assert_eq!(
            handle_key(&state, key('p')),
            Some(Action::CommandRun(CommandSpec::YarnPodInstall))
        );
        assert_eq!(
            handle_key(&state, key('r')),
            Some(Action::CommandRun(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: None,
            }))
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
    fn ios_palette_cached_key_is_not_exposed_when_cache_hit_exists() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        state.modal_stack.palette_mode = Some(PaletteMode::Ios);

        assert_eq!(handle_key(&state, key('c')), Some(Action::ModalCancel));

        let hit = cached_ios_hit_fixture();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("active slice should exist")
            .ios_simulator_cache = IosSimulatorCacheState::Hit(Box::new(hit.clone()));

        assert_eq!(handle_key(&state, key('c')), Some(Action::ModalCancel));
    }

    #[test]
    fn ios_palette_cached_key_is_not_exposed_for_matching_hit_from_another_worktree() {
        let mut state = base_state();
        seed_one_worktree_id(&mut state, "wt-hit");
        seed_one_worktree_id(&mut state, "wt-miss");
        state.modal_stack.palette_mode = Some(PaletteMode::Ios);

        let hit = cached_ios_hit_fixture();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-hit".into()))
            .expect("source slice should exist")
            .ios_simulator_cache = IosSimulatorCacheState::Hit(Box::new(hit.clone()));
        state
            .worktrees
            .get_mut(&WorktreeId("wt-miss".into()))
            .expect("selected slice should exist")
            .ios_simulator_cache = IosSimulatorCacheState::Miss {
            fingerprint: hit.metadata.fingerprint.clone(),
        };

        assert_eq!(handle_key(&state, key('c')), Some(Action::ModalCancel));
    }

    #[test]
    fn android_palette_cached_key_is_not_exposed_when_cache_hit_exists() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        state.modal_stack.palette_mode = Some(PaletteMode::Android);

        assert_eq!(handle_key(&state, key('c')), Some(Action::ModalCancel));

        let hit = cached_android_hit_fixture();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("active slice should exist")
            .android_cache = AndroidCacheState::Hit(Box::new(hit.clone()));

        assert_eq!(handle_key(&state, key('c')), Some(Action::ModalCancel));
    }

    #[test]
    fn android_palette_cached_key_is_not_exposed_for_matching_hit_from_another_worktree() {
        let mut state = base_state();
        seed_one_worktree_id(&mut state, "wt-hit");
        seed_one_worktree_id(&mut state, "wt-miss");
        state.modal_stack.palette_mode = Some(PaletteMode::Android);

        let hit = cached_android_hit_fixture();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-hit".into()))
            .expect("source slice should exist")
            .android_cache = AndroidCacheState::Hit(Box::new(hit.clone()));
        state
            .worktrees
            .get_mut(&WorktreeId("wt-miss".into()))
            .expect("selected slice should exist")
            .android_cache = AndroidCacheState::Miss {
            fingerprint: hit.metadata.fingerprint.clone(),
        };

        assert_eq!(handle_key(&state, key('c')), Some(Action::ModalCancel));
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
            other => panic!("yarn 'j' must produce YarnJest with empty filter; got {other:?}"),
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
            other => panic!("git 'b' must produce GitCheckout with empty branch; got {other:?}"),
        }
        match handle_key(&state, key('c')) {
            Some(Action::CommandRun(CommandSpec::GitCheckoutNew { branch })) => {
                assert_eq!(branch, "");
            }
            other => panic!("git 'c' must produce GitCheckoutNew with empty branch; got {other:?}"),
        }
        match handle_key(&state, key('r')) {
            Some(Action::CommandRun(CommandSpec::GitRebase { target })) => {
                assert_eq!(target, "");
            }
            other => panic!("git 'r' must produce GitRebase with empty target; got {other:?}"),
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

        assert_eq!(handle_key(&state, key('c')), Some(Action::WorktreeAdd));
        assert_eq!(
            handle_key(&state, key('n')),
            Some(Action::WorktreeAddNewBranch)
        );
        assert_eq!(handle_key(&state, key('d')), Some(Action::ModalCancel));
        assert_eq!(handle_key(&state, key('w')), Some(Action::ModalCancel));
        assert_eq!(handle_key(&state, key('b')), Some(Action::ModalCancel));

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

    #[test]
    fn worktree_table_footer_uses_open_menu_and_metro_labels() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        register_ready_metro(&mut state, "wt-1", 8081);

        let hints = footer_hints_for(&state);

        assert!(hints.contains(&("+", "add")));
        assert!(hints.contains(&("-", "remove")));
        assert!(hints.contains(&("o", "open")));
        assert!(hints.contains(&("Enter", "metro")));
        assert!(!hints.contains(&("w", "worktree")));
        assert!(!hints.contains(&("!", "shell")));
        assert!(!hints.contains(&("C", "claude")));
        assert!(!hints.contains(&("T", "shell tab")));
        assert!(!hints.contains(&("J", "debugger")));
        assert_eq!(
            handle_key(&state, key('+')),
            Some(Action::EnterWorktreePalette)
        );
        assert_eq!(handle_key(&state, key('-')), Some(Action::WorktreeRemove));
        assert_eq!(handle_key(&state, key('w')), None);
        assert_eq!(handle_key(&state, key('!')), None);
        assert_eq!(handle_key(&state, key('C')), None);
        assert_eq!(handle_key(&state, key('T')), None);
        assert_eq!(handle_key(&state, key('J')), None);
        assert_eq!(handle_key(&state, key('o')), Some(Action::EnterOpenPalette));
    }

    #[test]
    fn open_palette_resolves_lowercase_keys() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        register_ready_metro(&mut state, "wt-1", 8081);
        state.modal_stack.palette_mode = Some(PaletteMode::Open);

        assert_eq!(handle_key(&state, key('c')), Some(Action::OpenClaudeCode));
        assert_eq!(handle_key(&state, key('e')), Some(Action::OpenEditor));
        assert_eq!(handle_key(&state, key('t')), Some(Action::OpenShellTab));
        assert_eq!(
            handle_key(&state, key('j')),
            Some(Action::MetroSendDebugger)
        );
        assert_eq!(handle_key(&state, key('C')), Some(Action::ModalCancel));
        assert_eq!(handle_key(&state, key('E')), Some(Action::ModalCancel));
        assert_eq!(handle_key(&state, key('T')), Some(Action::ModalCancel));
        assert_eq!(handle_key(&state, key('J')), Some(Action::ModalCancel));
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
    }

    #[test]
    fn open_palette_footer_shows_debugger_key_without_running_metro() {
        let mut state = base_state();
        state.modal_stack.palette_mode = Some(PaletteMode::Open);

        let hints = footer_hints_for(&state);

        assert!(hints.contains(&("c", "claude")));
        assert!(hints.contains(&("e", "editor")));
        assert!(hints.contains(&("t", "shell tab")));
        assert!(hints.contains(&("j", "debugger")));
    }

    #[test]
    fn help_rows_reflect_open_and_worktree_shortcuts() {
        let rows = help_overlay_rows();

        assert!(rows.iter().any(|row| {
            row.section == "Worktree Table" && row.label == "o" && row.desc == "Open submenu"
        }));
        assert!(rows.iter().any(|row| {
            row.section == "Worktree Table"
                && row.label == "Enter"
                && row.desc == "Use selected worktree for Metro"
        }));
        assert!(rows.iter().any(|row| {
            row.section == "Worktree  (+>)" && row.label == "c" && row.desc == "Checkout worktree"
        }));
        assert!(rows.iter().any(|row| {
            row.section == "Worktree  (+>)"
                && row.label == "n"
                && row.desc == "New branch + worktree"
        }));
        assert!(!rows.iter().any(|row| {
            row.section == "Worktree Table" && matches!(row.label, "!" | "C" | "T" | "J" | "w")
        }));
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
        assert_eq!(handle_key(&state, key('x')), Some(Action::CleanConfirm));

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
        assert!(
            state.modal_stack.modal.is_none(),
            "ModalCancel must clear state.modal_stack.modal"
        );
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
            pending_template: Box::new(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: None,
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

    #[test]
    fn run_variant_picker_modal_dismisses_on_esc() {
        let mut state = base_state();
        state.modal_stack.modal = Some(ModalState::RunVariantPicker {
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunAndroid {
                device_id: "emulator-5554".into(),
                variant: None,
            }),
            boot_android_emulator: false,
            cache_launch_supported: false,
            cached_variants: [false; 3],
        });
        assert_eq!(
            handle_key(&state, key_code(KeyCode::Esc)),
            Some(Action::ModalCancel)
        );
        let _effects = update(&mut state, Action::ModalCancel);
        assert!(state.modal_stack.modal.is_none());
    }
}

mod ump_run_dialog {
    use super::*;
    use crate::domain::ports::device_port::DeviceKind;

    #[test]
    fn entering_ios_palette_starts_cache_lookup_for_selected_worktree() {
        let mut state = base_state();
        seed_one_worktree(&mut state);

        let effects = update(&mut state, Action::EnterIosPalette);

        assert_eq!(state.modal_stack.palette_mode, Some(PaletteMode::Ios));
        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .expect("selected slice should exist")
                .ios_simulator_cache,
            IosSimulatorCacheState::Checking
        );
        assert!(
            matches!(
                effects.as_slice(),
                [Effect::LookupIosSimulatorCache { worktree_id, worktree_path }]
                    if *worktree_id == WorktreeId("wt-1".into())
                        && worktree_path == std::path::Path::new("/tmp/wt-1")
            ),
            "expected one iOS cache lookup effect for wt-1; got {effects:?}"
        );
    }

    #[test]
    fn entering_android_palette_starts_cache_lookup_for_selected_worktree() {
        let mut state = base_state();
        seed_one_worktree(&mut state);

        let effects = update(&mut state, Action::EnterAndroidPalette);

        assert_eq!(state.modal_stack.palette_mode, Some(PaletteMode::Android));
        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .expect("selected slice should exist")
                .android_cache,
            AndroidCacheState::Checking
        );
        assert!(
            matches!(
                effects.as_slice(),
                [Effect::LookupAndroidCache { worktree_id, worktree_path }]
                    if *worktree_id == WorktreeId("wt-1".into())
                        && worktree_path == std::path::Path::new("/tmp/wt-1")
            ),
            "expected one Android cache lookup effect for wt-1; got {effects:?}"
        );
    }

    #[test]
    fn ios_cache_lookup_finished_maps_result_to_slice_state() {
        let worktree_id = WorktreeId("wt-1".into());
        let hit = cached_ios_hit_fixture();
        let mut state = base_state();
        seed_one_worktree(&mut state);

        let effects = update(
            &mut state,
            Action::IosSimulatorCacheLookupFinished {
                worktree_id: worktree_id.clone(),
                result: Ok(crate::domain::native_cache::IosSimulatorCacheLookup::Hit(
                    Box::new(hit.clone()),
                )),
            },
        );
        assert!(effects.is_empty());
        assert_eq!(
            state
                .worktrees
                .get(&worktree_id)
                .expect("slice should exist")
                .ios_simulator_cache,
            IosSimulatorCacheState::Hit(Box::new(hit))
        );

        let effects = update(
            &mut state,
            Action::IosSimulatorCacheLookupFinished {
                worktree_id: worktree_id.clone(),
                result: Ok(crate::domain::native_cache::IosSimulatorCacheLookup::Miss {
                    fingerprint: "0123456789abcdef".into(),
                }),
            },
        );
        assert!(effects.is_empty());
        assert_eq!(
            state
                .worktrees
                .get(&worktree_id)
                .expect("slice should exist")
                .ios_simulator_cache,
            IosSimulatorCacheState::Miss {
                fingerprint: "0123456789abcdef".into(),
            }
        );

        let effects = update(
            &mut state,
            Action::IosSimulatorCacheLookupFinished {
                worktree_id: worktree_id.clone(),
                result: Err("lookup failed".into()),
            },
        );
        assert!(effects.is_empty());
        assert_eq!(
            state
                .worktrees
                .get(&worktree_id)
                .expect("slice should exist")
                .ios_simulator_cache,
            IosSimulatorCacheState::Error("lookup failed".into())
        );
    }

    #[test]
    fn android_cache_lookup_finished_maps_result_to_slice_state() {
        let worktree_id = WorktreeId("wt-1".into());
        let hit = cached_android_hit_fixture();
        let mut state = base_state();
        seed_one_worktree(&mut state);

        let effects = update(
            &mut state,
            Action::AndroidCacheLookupFinished {
                worktree_id: worktree_id.clone(),
                result: Ok(AndroidCacheLookup::Hit(Box::new(hit.clone()))),
            },
        );
        assert!(effects.is_empty());
        assert_eq!(
            state
                .worktrees
                .get(&worktree_id)
                .expect("slice should exist")
                .android_cache,
            AndroidCacheState::Hit(Box::new(hit))
        );

        let effects = update(
            &mut state,
            Action::AndroidCacheLookupFinished {
                worktree_id: worktree_id.clone(),
                result: Ok(AndroidCacheLookup::Miss {
                    fingerprint: "0123456789abcdef".into(),
                }),
            },
        );
        assert!(effects.is_empty());
        assert_eq!(
            state
                .worktrees
                .get(&worktree_id)
                .expect("slice should exist")
                .android_cache,
            AndroidCacheState::Miss {
                fingerprint: "0123456789abcdef".into(),
            }
        );

        let effects = update(
            &mut state,
            Action::AndroidCacheLookupFinished {
                worktree_id: worktree_id.clone(),
                result: Err("lookup failed".into()),
            },
        );
        assert!(effects.is_empty());
        assert_eq!(
            state
                .worktrees
                .get(&worktree_id)
                .expect("slice should exist")
                .android_cache,
            AndroidCacheState::Error("lookup failed".into())
        );
    }

    #[test]
    fn android_cache_lookup_failure_appends_error_output() {
        let worktree_id = WorktreeId("wt-1".into());
        let mut state = base_state();
        seed_one_worktree(&mut state);

        let effects = update(
            &mut state,
            Action::AndroidCacheLookupFinished {
                worktree_id: worktree_id.clone(),
                result: Err("missing application id".into()),
            },
        );

        assert!(effects.is_empty());
        let output = state
            .worktrees
            .get(&worktree_id)
            .expect("slice should exist")
            .output
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            output
                .iter()
                .any(|line| line == "[cached-android error] missing application id"),
            "expected visible Android cache error in output; got {output:?}"
        );
    }

    #[test]
    fn ios_cache_hit_marks_matching_miss_slices_as_hits() {
        let mut state = base_state();
        seed_one_worktree_id(&mut state, "wt-hit");
        seed_one_worktree_id(&mut state, "wt-miss");
        let hit = cached_ios_hit_fixture();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-miss".into()))
            .expect("matching slice should exist")
            .ios_simulator_cache = IosSimulatorCacheState::Miss {
            fingerprint: hit.metadata.fingerprint.clone(),
        };

        let effects = update(
            &mut state,
            Action::IosSimulatorCacheLookupFinished {
                worktree_id: WorktreeId("wt-hit".into()),
                result: Ok(crate::domain::native_cache::IosSimulatorCacheLookup::Hit(
                    Box::new(hit.clone()),
                )),
            },
        );

        assert!(effects.is_empty());
        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-hit".into()))
                .expect("hit slice should exist")
                .ios_simulator_cache,
            IosSimulatorCacheState::Hit(Box::new(hit.clone()))
        );
        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-miss".into()))
                .expect("matching miss slice should exist")
                .ios_simulator_cache,
            IosSimulatorCacheState::Hit(Box::new(hit))
        );
    }

    #[test]
    fn ump_android_run_loads_targets_before_variant_or_metro() {
        let mut state = base_state();
        seed_one_worktree(&mut state);

        let effects = update(
            &mut state,
            Action::CommandRun(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: None,
            }),
        );

        assert!(
            matches!(
                effects.as_slice(),
                [Effect::LoadDevices {
                    kind: DeviceKind::Android,
                    request_id: None,
                }]
            ),
            "expected Android target load before run-type or metro; got {effects:?}"
        );
        assert!(matches!(
            state.modal_stack.pending_device_command,
            Some(CommandSpec::UmpRunAndroid { ref device_id, variant: None }) if device_id.is_empty()
        ));
    }

    #[test]
    fn ios_simulator_run_uses_matching_variant_cache_after_variant_confirm() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        register_ready_metro(&mut state, "wt-1", 19001);
        let mut hit = cached_ios_hit_fixture();
        hit.metadata.variant = RunVariant::Local.label().into();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("active slice should exist")
            .ios_simulator_cache = IosSimulatorCacheState::Hit(Box::new(hit.clone()));

        let effects = update(
            &mut state,
            Action::CommandRun(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: None,
            }),
        );
        assert!(matches!(
            effects.as_slice(),
            [Effect::LoadDevices {
                kind: DeviceKind::Ios,
                request_id: None,
            }]
        ));

        let effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Ios,
                request_id: None,
                devices: vec![crate::domain::command::DeviceInfo {
                    id: "SIM-1".into(),
                    name: "iPhone 15 (Shutdown)".into(),
                }],
            },
        );
        assert!(effects.is_empty());
        assert!(matches!(
            state.modal_stack.modal,
            Some(ModalState::RunVariantPicker {
                cached_variants: [true, false, false],
                ..
            })
        ));

        let effects = update(&mut state, Action::ModalRunVariantConfirm);

        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::InstallAndLaunchCachedIosSimulator { worktree_id, request }
                    if worktree_id == &WorktreeId("wt-1".into())
                        && request.simulator_udid == "SIM-1"
                        && request.bundle_id == hit.metadata.bundle_id
                        && request.app_path == hit.artifact_path
                        && request.metro_port == 19001
            )),
            "matching iOS simulator cache should launch cached artifact; got {effects:?}"
        );
        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .expect("slice should exist")
                .task
                .is_none(),
            "cached simulator launch must not spawn a normal run task"
        );
    }

    #[test]
    fn entering_ios_palette_preserves_existing_hit_while_refreshing() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_ios_hit_fixture();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("active slice should exist")
            .ios_simulator_cache = IosSimulatorCacheState::Hit(Box::new(hit.clone()));

        let effects = update(&mut state, Action::EnterIosPalette);

        assert!(
            effects.iter().any(|effect| {
                matches!(
                    effect,
                    Effect::LookupIosSimulatorCache { worktree_id, .. }
                        if worktree_id == &WorktreeId("wt-1".into())
                )
            }),
            "entering the iOS palette should still refresh cache metadata; got {effects:?}"
        );
        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .expect("active slice should exist")
                .ios_simulator_cache
                .hit(),
            Some(&hit),
            "cache hit should remain usable while refresh is pending"
        );
    }

    #[test]
    fn entering_android_palette_preserves_existing_hit_while_refreshing() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_android_hit_fixture();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("active slice should exist")
            .android_cache = AndroidCacheState::Hit(Box::new(hit.clone()));

        let effects = update(&mut state, Action::EnterAndroidPalette);

        assert!(
            effects.iter().any(|effect| {
                matches!(
                    effect,
                    Effect::LookupAndroidCache { worktree_id, .. }
                        if worktree_id == &WorktreeId("wt-1".into())
                )
            }),
            "entering the Android palette should still refresh cache metadata; got {effects:?}"
        );
        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .expect("active slice should exist")
                .android_cache
                .hit(),
            Some(&hit),
            "cache hit should remain usable while refresh is pending"
        );
    }

    #[test]
    fn ios_run_variant_picker_preselects_cached_variant() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let mut hit = cached_ios_hit_fixture();
        hit.metadata.variant = RunVariant::Dev.label().into();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("active slice should exist")
            .ios_simulator_cache = IosSimulatorCacheState::Hit(Box::new(hit));

        let _ = update(
            &mut state,
            Action::CommandRun(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: None,
            }),
        );
        let effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Ios,
                request_id: None,
                devices: vec![crate::domain::command::DeviceInfo {
                    id: "SIM-1".into(),
                    name: "iPhone 15 (Shutdown)".into(),
                }],
            },
        );

        assert!(effects.is_empty());
        assert!(matches!(
            state.modal_stack.modal,
            Some(ModalState::RunVariantPicker {
                selected: 1,
                cached_variants: [false, true, false],
                ..
            })
        ));
    }

    #[test]
    fn android_run_variant_picker_preselects_cached_variant() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let mut hit = cached_android_hit_fixture();
        hit.metadata.variant = RunVariant::Prod.label().into();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("active slice should exist")
            .android_cache = AndroidCacheState::Hit(Box::new(hit));

        let _ = update(
            &mut state,
            Action::CommandRun(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: None,
            }),
        );
        let effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Android,
                request_id: None,
                devices: vec![crate::domain::command::DeviceInfo {
                    id: "emulator-5554".into(),
                    name: "Pixel 9".into(),
                }],
            },
        );

        assert!(effects.is_empty());
        assert!(matches!(
            state.modal_stack.modal,
            Some(ModalState::RunVariantPicker {
                selected: 2,
                cached_variants: [false, false, true],
                ..
            })
        ));
    }

    #[test]
    fn ios_physical_device_run_ignores_simulator_cache() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        register_ready_metro(&mut state, "wt-1", 19001);
        let mut hit = cached_ios_hit_fixture();
        hit.metadata.variant = RunVariant::Local.label().into();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("active slice should exist")
            .ios_simulator_cache = IosSimulatorCacheState::Hit(Box::new(hit));

        let _ = update(
            &mut state,
            Action::CommandRun(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: None,
            }),
        );
        let effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Ios,
                request_id: None,
                devices: vec![crate::domain::command::DeviceInfo {
                    id: "00008150-000121040E02401C".into(),
                    name: "Dafone (26.5)".into(),
                }],
            },
        );
        assert!(effects.is_empty());
        assert!(matches!(
            state.modal_stack.modal,
            Some(ModalState::RunVariantPicker {
                cached_variants: [false, false, false],
                ..
            })
        ));

        let effects = update(&mut state, Action::ModalRunVariantConfirm);

        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::InstallAndLaunchCachedIosSimulator { .. })),
            "physical iOS devices must not use simulator cache; got {effects:?}"
        );
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::SpawnTask {
                    spec: CommandSpec::UmpRunIos {
                        device_id,
                        variant: Some(RunVariant::Local),
                    },
                    ..
                } if device_id == "00008150-000121040E02401C"
            )),
            "physical iOS run should stay on normal run path; got {effects:?}"
        );
    }

    #[test]
    fn android_run_uses_matching_variant_cache_after_variant_confirm() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        register_ready_metro(&mut state, "wt-1", 19001);
        let hit = cached_android_hit_fixture();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("active slice should exist")
            .android_cache = AndroidCacheState::Hit(Box::new(hit.clone()));

        let effects = update(
            &mut state,
            Action::CommandRun(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: None,
            }),
        );
        assert!(matches!(
            effects.as_slice(),
            [Effect::LoadDevices {
                kind: DeviceKind::Android,
                request_id: None,
            }]
        ));

        let effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Android,
                request_id: None,
                devices: vec![crate::domain::command::DeviceInfo {
                    id: "emulator-5554".into(),
                    name: "Pixel 8".into(),
                }],
            },
        );
        assert!(effects.is_empty());
        assert!(matches!(
            state.modal_stack.modal,
            Some(ModalState::RunVariantPicker {
                cached_variants: [true, false, false],
                ..
            })
        ));

        let effects = update(&mut state, Action::ModalRunVariantConfirm);

        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::InstallAndLaunchCachedAndroid { worktree_id, request }
                    if worktree_id == &WorktreeId("wt-1".into())
                        && request.device_id == "emulator-5554"
                        && request.application_id == hit.metadata.application_id
                        && request.apk_path == hit.artifact_path
                        && request.metro_port == 19001
            )),
            "matching Android cache should launch cached artifact; got {effects:?}"
        );
        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .expect("slice should exist")
                .task
                .is_none(),
            "cached Android launch must not spawn a normal run task"
        );
    }

    #[test]
    fn android_run_with_mismatched_variant_cache_falls_back_to_normal_run() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        register_ready_metro(&mut state, "wt-1", 19001);
        let mut hit = cached_android_hit_fixture();
        hit.metadata.variant = RunVariant::Dev.label().into();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("active slice should exist")
            .android_cache = AndroidCacheState::Hit(Box::new(hit));

        let _ = update(
            &mut state,
            Action::CommandRun(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: None,
            }),
        );
        let effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Android,
                request_id: None,
                devices: vec![crate::domain::command::DeviceInfo {
                    id: "emulator-5554".into(),
                    name: "Pixel 8".into(),
                }],
            },
        );
        assert!(effects.is_empty());
        assert!(matches!(
            state.modal_stack.modal,
            Some(ModalState::RunVariantPicker {
                cached_variants: [false, true, false],
                ..
            })
        ));
        if let Some(ModalState::RunVariantPicker { selected, .. }) = &mut state.modal_stack.modal {
            *selected = 0;
        }

        let effects = update(&mut state, Action::ModalRunVariantConfirm);

        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::InstallAndLaunchCachedAndroid { .. })),
            "mismatched Android cache variant must not be used; got {effects:?}"
        );
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::SpawnTask {
                    spec: CommandSpec::UmpRunAndroid {
                        device_id,
                        variant: Some(RunVariant::Local),
                    },
                    ..
                } if device_id == "emulator-5554"
            )),
            "mismatched cache should fall back to normal Android run; got {effects:?}"
        );
    }

    #[test]
    fn cached_ios_run_loads_ios_devices() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        state.modal_stack.palette_mode = Some(PaletteMode::Ios);
        let hit = cached_ios_hit_fixture();

        let effects = update(&mut state, Action::CachedIosRun(hit.clone()));

        assert!(state.modal_stack.modal.is_none());
        assert!(state.modal_stack.palette_mode.is_none());
        let pending_run = state
            .modal_stack
            .pending_cached_ios_run
            .as_ref()
            .expect("cached run should be pending");
        assert_eq!(pending_run.worktree_id, WorktreeId("wt-1".into()));
        assert_eq!(
            pending_run.worktree_path,
            std::path::PathBuf::from("/tmp/wt-1")
        );
        assert_eq!(pending_run.cache_hit, hit);
        assert_eq!(pending_run.device_request_id, 1);
        assert!(matches!(
            state.modal_stack.pending_device_command,
            Some(CommandSpec::UmpRunIos {
                ref device_id,
                variant: Some(RunVariant::Local),
            }) if device_id.is_empty()
        ));
        assert!(
            matches!(
                effects.as_slice(),
                [Effect::LoadDevices {
                    kind: DeviceKind::Ios,
                    request_id: Some(request_id),
                }]
                    if *request_id == pending_run.device_request_id
            ),
            "expected cached iOS run to load iOS devices; got {effects:?}"
        );
    }

    #[test]
    fn cached_android_run_loads_android_devices() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        state.modal_stack.palette_mode = Some(PaletteMode::Android);
        let hit = cached_android_hit_fixture();

        let effects = update(&mut state, Action::CachedAndroidRun(hit.clone()));

        assert!(state.modal_stack.modal.is_none());
        assert!(state.modal_stack.palette_mode.is_none());
        let pending_run = state
            .modal_stack
            .pending_cached_android_run
            .as_ref()
            .expect("cached Android run should be pending");
        assert_eq!(pending_run.worktree_id, WorktreeId("wt-1".into()));
        assert_eq!(
            pending_run.worktree_path,
            std::path::PathBuf::from("/tmp/wt-1")
        );
        assert_eq!(pending_run.cache_hit, hit);
        assert_eq!(pending_run.device_request_id, 1);
        assert!(matches!(
            state.modal_stack.pending_device_command,
            Some(CommandSpec::UmpRunAndroid {
                ref device_id,
                variant: Some(RunVariant::Local),
            }) if device_id.is_empty()
        ));
        assert!(
            matches!(
                effects.as_slice(),
                [Effect::LoadDevices {
                    kind: DeviceKind::Android,
                    request_id: Some(request_id),
                }]
                    if *request_id == pending_run.device_request_id
            ),
            "expected cached Android run to load Android devices; got {effects:?}"
        );
    }

    #[test]
    fn cached_ios_run_binds_device_enumeration_to_origin_worktree() {
        let mut state = base_state();
        seed_two_worktrees(&mut state, "wt-A", "wt-B");
        let hit = cached_ios_hit_fixture();

        let effects = update(&mut state, Action::CachedIosRun(hit.clone()));
        assert!(
            matches!(
                effects.as_slice(),
                [Effect::LoadDevices {
                    kind: DeviceKind::Ios,
                    request_id: Some(_),
                }]
            ),
            "cached run should begin by loading iOS simulators; got {effects:?}"
        );

        state.worktree_browser.worktree_table_state.select(Some(1));

        let effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Ios,
                request_id: Some(1),
                devices: vec![crate::domain::command::DeviceInfo {
                    id: "SIM-origin".into(),
                    name: "iPhone 15".into(),
                }],
            },
        );

        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-A".into()))
                .and_then(|slice| slice.pending_cached_ios_launch.as_ref())
                .map(|pending| (&pending.device_id, &pending.cache_hit)),
            Some((&"SIM-origin".to_string(), &hit))
        );
        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-B".into()))
                .and_then(|slice| slice.pending_cached_ios_launch.as_ref())
                .is_none(),
            "selection changes must not move cached launch state to wt-B"
        );
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::SpawnMetro { worktree, .. } if worktree.ends_with("wt-A")
            )),
            "cached launch should start Metro for origin wt-A; got {effects:?}"
        );
    }

    #[test]
    fn cached_ios_cancel_clears_device_command_sentinel() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_ios_hit_fixture();

        let effects = update(&mut state, Action::CachedIosRun(hit));
        assert!(
            matches!(
                effects.as_slice(),
                [Effect::LoadDevices {
                    kind: DeviceKind::Ios,
                    request_id: Some(_),
                }]
            ),
            "cached run should begin by loading iOS simulators; got {effects:?}"
        );

        let cancel_effects = update(&mut state, Action::ModalCancel);
        assert!(cancel_effects.is_empty());
        assert!(state.modal_stack.pending_cached_ios_run.is_none());
        assert!(state.modal_stack.pending_device_command.is_none());

        let late_effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Ios,
                request_id: Some(1),
                devices: vec![crate::domain::command::DeviceInfo {
                    id: "SIM-late".into(),
                    name: "iPhone 15".into(),
                }],
            },
        );

        assert!(
            late_effects.is_empty(),
            "late device enumeration after cancel must not start a normal run; got {late_effects:?}"
        );
        assert!(state.modal_stack.modal.is_none());
        assert!(state.modal_stack.pending_device_command.is_none());
        assert_eq!(slice_queue_len(&state, "wt-1"), 0);
    }

    #[test]
    fn cached_ios_device_selection_starts_metro_when_needed() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_ios_hit_fixture();
        state.modal_stack.pending_cached_ios_run =
            Some(pending_cached_ios_run_fixture("wt-1", hit.clone()));
        state.modal_stack.pending_device_command = Some(CommandSpec::UmpRunIos {
            device_id: String::new(),
            variant: Some(RunVariant::Local),
        });
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: vec![crate::domain::command::DeviceInfo {
                id: "SIM-1".into(),
                name: "iPhone 15".into(),
            }],
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            }),
            filter: String::new(),
        });

        let effects = update(&mut state, Action::ModalDeviceConfirm);

        assert!(state.modal_stack.pending_cached_ios_run.is_none());
        assert!(state.modal_stack.pending_device_command.is_none());
        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .and_then(|slice| slice.pending_cached_ios_launch.as_ref())
                .map(|pending| (&pending.device_id, &pending.cache_hit)),
            Some((&"SIM-1".to_string(), &hit))
        );
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::ScheduleAction(Action::SimulatorUsed(udid)) if udid == "SIM-1"
            )),
            "cached launch should record simulator usage; got {effects:?}"
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::SpawnMetro { .. })),
            "cached launch should start Metro when no port is running; got {effects:?}"
        );
    }

    #[test]
    fn cached_ios_device_selection_runs_yarn_before_shared_metro_when_stale() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        state.worktree_browser.worktrees[0].stale = true;
        let hit = cached_ios_hit_fixture();
        state.modal_stack.pending_cached_ios_run =
            Some(pending_cached_ios_run_fixture("wt-1", hit.clone()));
        state.modal_stack.pending_device_command = Some(CommandSpec::UmpRunIos {
            device_id: String::new(),
            variant: Some(RunVariant::Local),
        });
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: vec![crate::domain::command::DeviceInfo {
                id: "SIM-1".into(),
                name: "iPhone 15".into(),
            }],
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            }),
            filter: String::new(),
        });

        let effects = update(&mut state, Action::ModalDeviceConfirm);

        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .and_then(|slice| slice.pending_cached_ios_launch.as_ref())
                .map(|pending| (&pending.device_id, &pending.cache_hit)),
            Some((&"SIM-1".to_string(), &hit))
        );
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::SpawnTask {
                    worktree_id,
                    spec: CommandSpec::YarnInstall,
                    ..
                } if worktree_id == &WorktreeId("wt-1".into())
            )),
            "stale cached launch should enter the shared Metro dependency path and run yarn first; got {effects:?}"
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::SpawnMetro { .. })),
            "Metro must wait for yarn when the origin worktree is stale; got {effects:?}"
        );
    }

    #[test]
    fn cached_ios_stale_yarn_post_drain_starts_metro_for_origin_after_selection_changes() {
        let mut state = base_state();
        seed_two_worktrees(&mut state, "wt-A", "wt-B");
        let temp_root = std::env::temp_dir().join(format!(
            "ump-dash-post-drain-{}-{}",
            std::process::id(),
            crate::domain::task::TaskId::next().0
        ));
        let wt_a_path = temp_root.join("wt-A");
        std::fs::create_dir_all(wt_a_path.join("node_modules"))
            .expect("test worktree should be creatable");
        state.worktree_browser.worktrees[0].path = wt_a_path;
        state.worktree_browser.worktrees[0].stale = true;
        let hit = cached_ios_hit_fixture();
        state.modal_stack.pending_cached_ios_run =
            Some(pending_cached_ios_run_fixture("wt-A", hit.clone()));
        state.modal_stack.pending_device_command = Some(CommandSpec::UmpRunIos {
            device_id: String::new(),
            variant: Some(RunVariant::Local),
        });
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: vec![crate::domain::command::DeviceInfo {
                id: "SIM-origin".into(),
                name: "iPhone 15".into(),
            }],
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            }),
            filter: String::new(),
        });

        let effects = update(&mut state, Action::ModalDeviceConfirm);
        let yarn_task_id = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::SpawnTask {
                    task_id,
                    worktree_id,
                    spec: CommandSpec::YarnInstall,
                    ..
                } if worktree_id == &WorktreeId("wt-A".into()) => Some(*task_id),
                _ => None,
            })
            .expect("stale cached launch should dispatch YarnInstall for origin worktree");

        state
            .worktrees
            .get_mut(&WorktreeId("wt-A".into()))
            .expect("origin slice should exist")
            .task = Some(synthetic_task_record(
            yarn_task_id.0,
            CommandSpec::YarnInstall,
        ));

        state.worktree_browser.worktree_table_state.select(Some(1));

        let effects = update(
            &mut state,
            Action::CommandExited {
                task_id: yarn_task_id,
                status: crate::domain::task::ExitStatus::Success,
            },
        );

        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::SpawnMetro { worktree, .. } if worktree.ends_with("wt-A")
            )),
            "post-drain Metro start must stay bound to origin wt-A; got {effects:?}"
        );
        assert!(
            !effects.iter().any(|effect| matches!(
                effect,
                Effect::SpawnMetro { worktree, .. } if worktree.ends_with("wt-B")
            )),
            "selection changes must not reroute cached launch Metro start to wt-B; got {effects:?}"
        );
    }

    #[test]
    fn cached_ios_run_ignores_mismatched_device_enumeration() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_ios_hit_fixture();

        let effects = update(&mut state, Action::CachedIosRun(hit.clone()));
        assert!(matches!(
            effects.as_slice(),
            [Effect::LoadDevices {
                kind: DeviceKind::Ios,
                request_id: Some(1),
            }]
        ));

        let effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Android,
                request_id: None,
                devices: vec![crate::domain::command::DeviceInfo {
                    id: "android-stale".into(),
                    name: "Pixel".into(),
                }],
            },
        );

        assert!(
            effects.is_empty(),
            "stale devices must be ignored; got {effects:?}"
        );
        assert_eq!(
            state
                .modal_stack
                .pending_cached_ios_run
                .as_ref()
                .map(|run| (
                    run.worktree_id.clone(),
                    run.device_request_id,
                    run.cache_hit.clone(),
                )),
            Some((WorktreeId("wt-1".into()), 1, hit))
        );
        assert!(matches!(
            state.modal_stack.pending_device_command,
            Some(CommandSpec::UmpRunIos {
                ref device_id,
                variant: Some(RunVariant::Local),
            }) if device_id.is_empty()
        ));
        assert!(state.modal_stack.modal.is_none());
        assert_eq!(slice_queue_len(&state, "wt-1"), 0);
        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .and_then(|slice| slice.pending_cached_ios_launch.as_ref())
                .is_none()
        );
    }

    #[test]
    fn cached_ios_run_ignores_older_device_request() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_ios_hit_fixture();

        let effects = update(&mut state, Action::CachedIosRun(hit.clone()));
        assert!(matches!(
            effects.as_slice(),
            [Effect::LoadDevices {
                kind: DeviceKind::Ios,
                request_id: Some(1),
            }]
        ));

        let effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Ios,
                request_id: Some(0),
                devices: vec![crate::domain::command::DeviceInfo {
                    id: "SIM-old".into(),
                    name: "iPhone 14".into(),
                }],
            },
        );

        assert!(
            effects.is_empty(),
            "older cached request must be ignored; got {effects:?}"
        );
        assert_eq!(
            state
                .modal_stack
                .pending_cached_ios_run
                .as_ref()
                .map(|run| run.device_request_id),
            Some(1)
        );
        assert!(state.modal_stack.modal.is_none());
        assert_eq!(slice_queue_len(&state, "wt-1"), 0);
    }

    #[test]
    fn cached_ios_device_selection_launches_when_metro_already_running() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_ios_hit_fixture();
        register_ready_metro(&mut state, "wt-1", 19001);
        state.modal_stack.pending_cached_ios_run =
            Some(pending_cached_ios_run_fixture("wt-1", hit.clone()));
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: vec![crate::domain::command::DeviceInfo {
                id: "SIM-2".into(),
                name: "iPhone 15 Pro".into(),
            }],
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            }),
            filter: String::new(),
        });

        let effects = update(&mut state, Action::ModalDeviceConfirm);

        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::InstallAndLaunchCachedIosSimulator { worktree_id, request }
                    if worktree_id == &WorktreeId("wt-1".into())
                        && request.simulator_udid == "SIM-2"
                        && request.bundle_id == "com.aljazeera.test"
                        && request.app_path.as_path() == std::path::Path::new("/tmp/cached.app")
                        && request.metro_port == 19001
            )),
            "expected cached install/launch effect with selected simulator and metro port; got {effects:?}"
        );
    }

    #[test]
    fn cached_android_device_selection_launches_when_metro_already_running() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_android_hit_fixture();
        register_ready_metro(&mut state, "wt-1", 19001);
        state.modal_stack.pending_cached_android_run =
            Some(pending_cached_android_run_fixture("wt-1", hit.clone()));
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: vec![crate::domain::command::DeviceInfo {
                id: "emulator-5554".into(),
                name: "Pixel 9a".into(),
            }],
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            }),
            filter: String::new(),
        });

        let effects = update(&mut state, Action::ModalDeviceConfirm);

        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::InstallAndLaunchCachedAndroid { worktree_id, request }
                    if worktree_id == &WorktreeId("wt-1".into())
                        && request.device_id == "emulator-5554"
                        && request.application_id == "com.aljazeera.test"
                        && request.apk_path.as_path() == std::path::Path::new("/tmp/cached.apk")
                        && request.metro_port == 19001
            )),
            "expected cached Android install/launch effect with selected target and metro port; got {effects:?}"
        );
    }

    #[test]
    fn cached_android_device_selection_starts_metro_when_needed() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_android_hit_fixture();
        state.modal_stack.pending_cached_android_run =
            Some(pending_cached_android_run_fixture("wt-1", hit.clone()));
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: vec![crate::domain::command::DeviceInfo {
                id: "emulator-5554".into(),
                name: "Pixel 9a".into(),
            }],
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            }),
            filter: String::new(),
        });

        let effects = update(&mut state, Action::ModalDeviceConfirm);

        assert!(state.modal_stack.pending_cached_android_run.is_none());
        assert!(state.modal_stack.pending_device_command.is_none());
        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .and_then(|slice| slice.pending_cached_android_launch.as_ref())
                .map(|pending| (&pending.device_id, &pending.cache_hit)),
            Some((&"emulator-5554".to_string(), &hit))
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::SpawnMetro { .. })),
            "cached Android launch should start Metro when no port is running; got {effects:?}"
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::InstallAndLaunchCachedAndroid { .. })),
            "cached Android launch should wait for Metro before install; got {effects:?}"
        );
    }

    #[test]
    fn cached_android_device_selection_launches_when_metro_process_exists_before_ready_activity() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_android_hit_fixture();
        register_metro_without_activity(&mut state, "wt-1", 19007);
        state.modal_stack.pending_cached_android_run =
            Some(pending_cached_android_run_fixture("wt-1", hit.clone()));
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: vec![crate::domain::command::DeviceInfo {
                id: "emulator-5554".into(),
                name: "Pixel 9a".into(),
            }],
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            }),
            filter: String::new(),
        });

        let effects = update(&mut state, Action::ModalDeviceConfirm);

        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::InstallAndLaunchCachedAndroid { worktree_id, request }
                    if worktree_id == &WorktreeId("wt-1".into())
                        && request.device_id == "emulator-5554"
                        && request.metro_port == 19007
            )),
            "cached Android launch should use an already registered Metro process port even if the Ready activity was missed; got {effects:?}"
        );
        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .expect("slice should exist")
                .pending_cached_android_launch
                .is_none(),
            "Android launch should not stay parked when a live Metro handle already exists"
        );
    }

    #[test]
    fn cached_ios_device_selection_launches_when_metro_process_exists_before_ready_activity() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_ios_hit_fixture();
        register_metro_without_activity(&mut state, "wt-1", 19007);
        state.modal_stack.pending_cached_ios_run =
            Some(pending_cached_ios_run_fixture("wt-1", hit.clone()));
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: vec![crate::domain::command::DeviceInfo {
                id: "SIM-stale".into(),
                name: "iPhone 15 Pro".into(),
            }],
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            }),
            filter: String::new(),
        });

        let effects = update(&mut state, Action::ModalDeviceConfirm);

        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::InstallAndLaunchCachedIosSimulator { worktree_id, request }
                    if worktree_id == &WorktreeId("wt-1".into())
                        && request.simulator_udid == "SIM-stale"
                        && request.metro_port == 19007
            )),
            "cached launch should use an already registered Metro process port even if the Ready activity was missed; got {effects:?}"
        );
        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .expect("slice should exist")
                .pending_cached_ios_launch
                .is_none(),
            "launch should not stay parked when a live Metro handle already exists"
        );
    }

    #[test]
    fn cached_ios_device_selection_defers_when_metro_is_starting() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_ios_hit_fixture();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("slice should exist")
            .metro
            .reserve_start(19005);
        state.modal_stack.pending_cached_ios_run =
            Some(pending_cached_ios_run_fixture("wt-1", hit.clone()));
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: vec![crate::domain::command::DeviceInfo {
                id: "SIM-4".into(),
                name: "iPhone 15 Pro".into(),
            }],
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            }),
            filter: String::new(),
        });

        let effects = update(&mut state, Action::ModalDeviceConfirm);

        let pending = state
            .worktrees
            .get(&WorktreeId("wt-1".into()))
            .expect("slice should exist")
            .pending_cached_ios_launch
            .as_ref()
            .expect("cached iOS launch should wait for Metro Ready");
        assert_eq!(pending.device_id, "SIM-4");
        assert_eq!(pending.cache_hit, hit);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::InstallAndLaunchCachedIosSimulator { .. })),
            "cached launch should not run while Metro is only starting; got {effects:?}"
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::SpawnMetro { .. })),
            "cached launch should not spawn a duplicate Metro while one is starting; got {effects:?}"
        );
    }

    #[test]
    fn cached_android_device_selection_defers_when_metro_is_starting() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_android_hit_fixture();
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("slice should exist")
            .metro
            .reserve_start(19005);
        state.modal_stack.pending_cached_android_run =
            Some(pending_cached_android_run_fixture("wt-1", hit.clone()));
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: vec![crate::domain::command::DeviceInfo {
                id: "emulator-5554".into(),
                name: "Pixel 9a".into(),
            }],
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            }),
            filter: String::new(),
        });

        let effects = update(&mut state, Action::ModalDeviceConfirm);

        let pending = state
            .worktrees
            .get(&WorktreeId("wt-1".into()))
            .expect("slice should exist")
            .pending_cached_android_launch
            .as_ref()
            .expect("cached Android launch should wait for Metro Ready");
        assert_eq!(pending.device_id, "emulator-5554");
        assert_eq!(pending.cache_hit, hit);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::InstallAndLaunchCachedAndroid { .. })),
            "cached Android launch should not run while Metro is only starting; got {effects:?}"
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::SpawnMetro { .. })),
            "cached Android launch should not spawn a duplicate Metro while one is starting; got {effects:?}"
        );
    }

    #[test]
    fn cached_ios_launch_runs_after_deferred_metro_ready() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_ios_hit_fixture();
        {
            let slice = state
                .worktrees
                .get_mut(&WorktreeId("wt-1".into()))
                .expect("slice should exist");
            slice.metro.reserve_start(19002);
            slice.pending_cached_ios_launch =
                Some(crate::domain::native_cache::PendingCachedIosLaunch {
                    device_id: "SIM-3".into(),
                    cache_hit: hit.clone(),
                });
        }

        let effects = update(
            &mut state,
            Action::MetroActivityUpdate {
                worktree_id: "wt-1".into(),
                activity: crate::domain::metro::MetroActivity::Ready,
            },
        );

        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .expect("slice should exist")
                .pending_cached_ios_launch
                .is_none()
        );
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::InstallAndLaunchCachedIosSimulator { worktree_id, request }
                    if worktree_id == &WorktreeId("wt-1".into())
                        && request.simulator_udid == "SIM-3"
                        && request.bundle_id == "com.aljazeera.test"
                        && request.app_path.as_path() == std::path::Path::new("/tmp/cached.app")
                        && request.metro_port == 19002
            )),
            "expected deferred cached install/launch after Metro Ready; got {effects:?}"
        );
    }

    #[test]
    fn cached_android_launch_runs_after_deferred_metro_ready() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_android_hit_fixture();
        {
            let slice = state
                .worktrees
                .get_mut(&WorktreeId("wt-1".into()))
                .expect("slice should exist");
            slice.metro.reserve_start(19002);
            slice.pending_cached_android_launch =
                Some(crate::domain::native_cache::PendingCachedAndroidLaunch {
                    device_id: "emulator-5554".into(),
                    cache_hit: hit.clone(),
                });
        }

        let effects = update(
            &mut state,
            Action::MetroActivityUpdate {
                worktree_id: "wt-1".into(),
                activity: crate::domain::metro::MetroActivity::Ready,
            },
        );

        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .expect("slice should exist")
                .pending_cached_android_launch
                .is_none()
        );
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::InstallAndLaunchCachedAndroid { worktree_id, request }
                    if worktree_id == &WorktreeId("wt-1".into())
                        && request.device_id == "emulator-5554"
                        && request.application_id == "com.aljazeera.test"
                        && request.apk_path.as_path() == std::path::Path::new("/tmp/cached.apk")
                        && request.metro_port == 19002
            )),
            "expected deferred cached Android install/launch after Metro Ready; got {effects:?}"
        );
    }

    #[test]
    fn cached_ios_run_with_zero_devices_appends_error() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        state.modal_stack.pending_cached_ios_run = Some(pending_cached_ios_run_fixture(
            "wt-1",
            cached_ios_hit_fixture(),
        ));
        state.modal_stack.pending_device_command = Some(CommandSpec::UmpRunIos {
            device_id: String::new(),
            variant: Some(RunVariant::Local),
        });

        let effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Ios,
                request_id: Some(1),
                devices: vec![],
            },
        );

        assert!(effects.is_empty());
        assert!(state.modal_stack.pending_cached_ios_run.is_none());
        assert_eq!(
            slice_output(&state, "wt-1").last().map(String::as_str),
            Some("[error] no iOS simulators found for cached run")
        );
    }

    #[test]
    fn cached_android_run_with_zero_devices_appends_error() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        state.modal_stack.pending_cached_android_run = Some(pending_cached_android_run_fixture(
            "wt-1",
            cached_android_hit_fixture(),
        ));
        state.modal_stack.pending_device_command = Some(CommandSpec::UmpRunAndroid {
            device_id: String::new(),
            variant: Some(RunVariant::Local),
        });

        let effects = update(
            &mut state,
            Action::DevicesEnumerated {
                kind: DeviceKind::Android,
                request_id: Some(1),
                devices: vec![],
            },
        );

        assert!(effects.is_empty());
        assert!(state.modal_stack.pending_cached_android_run.is_none());
        assert_eq!(
            slice_output(&state, "wt-1").last().map(String::as_str),
            Some("[error] no Android devices found for cached run")
        );
    }

    #[test]
    fn selecting_target_opens_run_variant_picker_before_dispatch() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        state.modal_stack.modal = Some(ModalState::DevicePicker {
            devices: vec![
                crate::domain::command::DeviceInfo {
                    id: "emulator-5554".into(),
                    name: "Pixel 8".into(),
                },
                crate::domain::command::DeviceInfo {
                    id: "emulator-5556".into(),
                    name: "Pixel Tablet".into(),
                },
            ],
            selected: 1,
            pending_template: Box::new(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: None,
            }),
            filter: String::new(),
        });

        let effects = update(&mut state, Action::ModalDeviceConfirm);

        assert!(
            effects.is_empty(),
            "target selection should only open run-type picker; got {effects:?}"
        );
        assert!(matches!(
            state.modal_stack.modal,
            Some(ModalState::RunVariantPicker {
                selected: 0,
                pending_template,
                boot_android_emulator: false,
                ..
            }) if matches!(
                pending_template.as_ref(),
                CommandSpec::UmpRunAndroid { device_id, variant: None } if device_id == "emulator-5556"
            )
        ));
    }

    #[test]
    fn run_variant_picker_uses_local_dev_prod_order_and_confirm_queues_final_run() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        state.modal_stack.modal = Some(ModalState::RunVariantPicker {
            selected: 2,
            pending_template: Box::new(CommandSpec::UmpRunIos {
                device_id: "ios-udid-1".into(),
                variant: None,
            }),
            boot_android_emulator: false,
            cache_launch_supported: false,
            cached_variants: [false; 3],
        });

        let effects = update(&mut state, Action::ModalRunVariantConfirm);

        assert!(
            !effects.is_empty(),
            "fully selected UMP run should proceed into the metro prerequisite path"
        );
        let queued = state
            .worktrees
            .values()
            .next()
            .and_then(|slice| slice.queue.front())
            .cloned();
        assert_eq!(
            queued,
            Some(CommandSpec::UmpRunIos {
                device_id: "ios-udid-1".into(),
                variant: Some(RunVariant::Prod),
            })
        );
    }

    #[test]
    fn run_variant_navigation_wraps_in_local_dev_prod_order() {
        let mut state = base_state();
        state.modal_stack.modal = Some(ModalState::RunVariantPicker {
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunIos {
                device_id: "ios-udid-1".into(),
                variant: None,
            }),
            boot_android_emulator: false,
            cache_launch_supported: false,
            cached_variants: [false; 3],
        });

        let _ = update(&mut state, Action::ModalRunVariantPrev);
        assert!(matches!(
            state.modal_stack.modal,
            Some(ModalState::RunVariantPicker { selected: 2, .. })
        ));

        let _ = update(&mut state, Action::ModalRunVariantNext);
        assert!(matches!(
            state.modal_stack.modal,
            Some(ModalState::RunVariantPicker { selected: 0, .. })
        ));
    }

    #[test]
    fn uppercase_r_repeats_last_android_run_config_for_selected_worktree() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        state.modal_stack.modal = Some(ModalState::RunVariantPicker {
            selected: 1,
            pending_template: Box::new(CommandSpec::UmpRunAndroid {
                device_id: "emulator-5554".into(),
                variant: None,
            }),
            boot_android_emulator: false,
            cache_launch_supported: false,
            cached_variants: [false; 3],
        });

        let _ = update(&mut state, Action::ModalRunVariantConfirm);
        state.modal_stack.palette_mode = Some(PaletteMode::Android);

        assert_eq!(
            handle_key(&state, key('R')),
            Some(Action::CommandRunWithCache {
                spec: CommandSpec::UmpRunAndroid {
                    device_id: "emulator-5554".into(),
                    variant: Some(RunVariant::Dev),
                },
                cache_launch_supported: false,
            })
        );
    }

    #[test]
    fn available_android_avd_boot_waits_for_selected_avd_before_queued_run() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        state.modal_stack.modal = Some(ModalState::RunVariantPicker {
            selected: 0,
            pending_template: Box::new(CommandSpec::UmpRunAndroid {
                device_id: "avd:Pixel_9a".into(),
                variant: None,
            }),
            boot_android_emulator: true,
            cache_launch_supported: false,
            cached_variants: [false; 3],
        });

        let effects = update(&mut state, Action::ModalRunVariantConfirm);

        let command = effects.iter().find_map(|effect| match effect {
            Effect::SpawnTask {
                spec: CommandSpec::ShellCommand { command },
                ..
            } => Some(command.as_str()),
            _ => None,
        });

        let command = command.expect("booting an available AVD should spawn a shell command");
        assert!(
            command.contains("emulator -avd 'Pixel_9a'"),
            "boot command should launch the selected AVD, got {command}"
        );
        assert!(
            command.contains("adb -s \"$serial\" emu avd name"),
            "boot command should wait for the selected AVD serial, got {command}"
        );
        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .and_then(|slice| slice.queue.front())
                == Some(&CommandSpec::UmpRunAndroid {
                    device_id: "avd:Pixel_9a".into(),
                    variant: Some(RunVariant::Local),
                }),
            "boot flow should queue the final AVD run after the boot command"
        );
    }

    #[test]
    fn uppercase_r_uses_selected_worktree_run_history_only() {
        let mut state = base_state();
        seed_two_worktrees(&mut state, "wt-a", "wt-b");
        state.modal_stack.modal = Some(ModalState::RunVariantPicker {
            selected: 2,
            pending_template: Box::new(CommandSpec::UmpRunIos {
                device_id: "ios-wt-a".into(),
                variant: None,
            }),
            boot_android_emulator: false,
            cache_launch_supported: false,
            cached_variants: [false; 3],
        });

        let _ = update(&mut state, Action::ModalRunVariantConfirm);

        state.worktree_browser.worktree_table_state.select(Some(1));
        state.modal_stack.palette_mode = Some(PaletteMode::Ios);
        assert_eq!(handle_key(&state, key('R')), Some(Action::ModalCancel));

        state.worktree_browser.worktree_table_state.select(Some(0));
        state.modal_stack.palette_mode = Some(PaletteMode::Ios);
        assert_eq!(
            handle_key(&state, key('R')),
            Some(Action::CommandRunWithCache {
                spec: CommandSpec::UmpRunIos {
                    device_id: "ios-wt-a".into(),
                    variant: Some(RunVariant::Prod),
                },
                cache_launch_supported: false,
            })
        );
    }

    #[test]
    fn uppercase_r_ios_repeat_uses_matching_simulator_cache() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        register_ready_metro(&mut state, "wt-1", 19001);
        let mut hit = cached_ios_hit_fixture();
        hit.metadata.variant = RunVariant::Local.label().into();
        let slice = state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("active slice should exist");
        slice.ios_simulator_cache = IosSimulatorCacheState::Hit(Box::new(hit.clone()));
        slice.last_ios_run = Some(crate::domain::worktree_slice::LastRunConfig {
            device_id: "SIM-1".into(),
            variant: RunVariant::Local,
            cache_launch_supported: true,
        });
        state.modal_stack.palette_mode = Some(PaletteMode::Ios);

        let action = handle_key(&state, key('R')).expect("repeat should resolve");
        assert_eq!(
            action,
            Action::CommandRunWithCache {
                spec: CommandSpec::UmpRunIos {
                    device_id: "SIM-1".into(),
                    variant: Some(RunVariant::Local),
                },
                cache_launch_supported: true,
            }
        );

        let effects = update(&mut state, action);

        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::InstallAndLaunchCachedIosSimulator { worktree_id, request }
                    if worktree_id == &WorktreeId("wt-1".into())
                        && request.simulator_udid == "SIM-1"
                        && request.app_path == hit.artifact_path
                        && request.metro_port == 19001
            )),
            "iOS simulator repeat should launch matching cached artifact; got {effects:?}"
        );
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
        assert_eq!(
            slice_queue_len(&state, "wt-1"),
            0,
            "precondition: slice queue empty"
        );

        let _effects = update(
            &mut state,
            Action::CommandQueuePush(CommandSpec::YarnInstall),
        );
        let _effects = update(
            &mut state,
            Action::CommandQueuePush(CommandSpec::YarnPodInstall),
        );

        // D-21: slice-side queue assertions.
        assert_eq!(
            slice_queue_len(&state, "wt-1"),
            2,
            "slice queue must hold 2 items"
        );
        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .and_then(|s| s.queue.front()),
            Some(&CommandSpec::YarnInstall),
            "slice queue front must be YarnInstall"
        );
        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .and_then(|s| s.queue.back()),
            Some(&CommandSpec::YarnPodInstall),
            "slice queue back must be YarnPodInstall"
        );
    }

    #[test]
    fn command_exited_with_empty_queue_clears_running_command() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        // Simulate a running task in the slice (D-21: task lives in slice).
        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .unwrap()
            .task = Some(synthetic_task_record(1, CommandSpec::GitFetch));
        assert_eq!(
            slice_queue_len(&state, "wt-1"),
            0,
            "precondition: slice queue empty"
        );

        let _effects = update(
            &mut state,
            Action::CommandExited {
                task_id: crate::domain::task::TaskId(1),
                status: crate::domain::task::ExitStatus::Success,
            },
        );

        // D-21: slice-side assertion — task cleared after CommandExited.
        assert_no_running_task_anywhere(&state);
        assert_eq!(slice_queue_len(&state, "wt-1"), 0, "queue stays empty");
    }

    #[test]
    fn successful_ios_run_enqueues_native_cache_store() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let wid = WorktreeId("wt-1".into());
        state.worktrees.get_mut(&wid).unwrap().task = Some(synthetic_task_record(
            12,
            CommandSpec::UmpRunIos {
                device_id: "SIM-1".into(),
                variant: Some(crate::domain::command::RunVariant::Dev),
            },
        ));

        let effects = update(
            &mut state,
            Action::CommandExited {
                task_id: crate::domain::task::TaskId(12),
                status: crate::domain::task::ExitStatus::Success,
            },
        );

        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::StoreIosSimulatorCache {
                    worktree_id,
                    request,
                } if *worktree_id == wid
                    && request.worktree_path == std::path::Path::new("/tmp/wt-1")
                    && request.variant == "dev"
            )),
            "successful iOS run should enqueue native cache store; got {effects:?}"
        );
    }

    #[test]
    fn successful_android_run_enqueues_native_cache_store() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let wid = WorktreeId("wt-1".into());
        state.worktrees.get_mut(&wid).unwrap().task = Some(synthetic_task_record(
            14,
            CommandSpec::UmpRunAndroid {
                device_id: "emulator-5554".into(),
                variant: Some(crate::domain::command::RunVariant::Dev),
            },
        ));

        let effects = update(
            &mut state,
            Action::CommandExited {
                task_id: crate::domain::task::TaskId(14),
                status: crate::domain::task::ExitStatus::Success,
            },
        );

        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::StoreAndroidCache {
                    worktree_id,
                    request,
                } if *worktree_id == wid
                    && request.worktree_path == std::path::Path::new("/tmp/wt-1")
                    && request.variant == "dev"
            )),
            "successful Android run should enqueue native cache store; got {effects:?}"
        );
    }

    #[test]
    fn failed_ios_run_does_not_enqueue_native_cache_store() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let wid = WorktreeId("wt-1".into());
        state.worktrees.get_mut(&wid).unwrap().task = Some(synthetic_task_record(
            13,
            CommandSpec::UmpRunIos {
                device_id: "SIM-1".into(),
                variant: Some(crate::domain::command::RunVariant::Dev),
            },
        ));

        let effects = update(
            &mut state,
            Action::CommandExited {
                task_id: crate::domain::task::TaskId(13),
                status: crate::domain::task::ExitStatus::Failure { code: Some(1) },
            },
        );

        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::StoreIosSimulatorCache { .. })),
            "failed iOS run must not cache an artifact; got {effects:?}"
        );
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
        state
            .worktrees
            .get_mut(&wid)
            .unwrap()
            .queue
            .push_back(CommandSpec::YarnInstall);
        state
            .worktrees
            .get_mut(&wid)
            .unwrap()
            .queue
            .push_back(CommandSpec::YarnPodInstall);
        let effects = update(
            &mut state,
            Action::CommandExited {
                task_id: crate::domain::task::TaskId(2),
                status: crate::domain::task::ExitStatus::Success,
            },
        );

        // D-21: after draining the queue head, the slice task is cleared
        // (SpawnTask effect was emitted; the runtime will write back slice.task
        // when the effect resolves — not visible in a pure update() test).
        // The remaining YarnPodInstall stays in the slice queue.
        assert_eq!(
            slice_queue_len(&state, "wt-1"),
            1,
            "one item (YarnPodInstall) must remain in the slice queue after drain"
        );
        // Plan 14-06: dispatch_command now returns Effect::SpawnTask.
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::SpawnTask { .. })),
            "CommandExited drain must emit Effect::SpawnTask for the popped spec; got {effects:?}"
        );
    }
}

// =========================================================================
// Sub-module 3.5: Claude tab dispatch
// =========================================================================

mod claude_tab {
    use super::*;

    #[test]
    fn open_claude_code_opens_default_tab_without_suffix_prompt() {
        let mut state = base_state();
        state.app_config.multiplexer_available = true;
        seed_one_worktree_id(&mut state, "ump-dash");

        let effects = update(&mut state, Action::OpenClaudeCode);

        assert!(
            state.modal_stack.modal.is_none(),
            "OpenClaudeCode should not prompt for a custom suffix"
        );
        assert_eq!(
            effects.len(),
            1,
            "expected exactly one effect, got {effects:?}"
        );

        match &effects[0] {
            Effect::OpenInMultiplexer {
                worktree,
                name,
                command,
            } => {
                assert_eq!(worktree, &std::path::PathBuf::from("/tmp/ump-dash"));
                assert_eq!(name, "main-claude");
                assert_eq!(command, "claude --dangerously-skip-permissions");
            }
            other => panic!("expected OpenInMultiplexer effect, got {other:?}"),
        }
    }

    #[test]
    fn open_editor_terminal_mode_opens_configured_editor_in_multiplexer() {
        let mut state = base_state();
        state.app_config.multiplexer_available = true;
        state.app_config.editor = "vim".into();
        state.app_config.editor_in_terminal = true;
        seed_one_worktree_id(&mut state, "ump-dash");

        let effects = update(&mut state, Action::OpenEditor);

        match &effects[..] {
            [
                Effect::OpenInMultiplexer {
                    worktree,
                    name,
                    command,
                },
            ] => {
                assert_eq!(worktree, &std::path::PathBuf::from("/tmp/ump-dash"));
                assert_eq!(name, "main-editor");
                assert_eq!(command, "vim .");
            }
            other => panic!("expected OpenInMultiplexer effect, got {other:?}"),
        }
    }

    #[test]
    fn open_editor_terminal_mode_requires_multiplexer() {
        let mut state = base_state();
        state.app_config.multiplexer_available = false;
        state.app_config.editor = "vim".into();
        state.app_config.editor_in_terminal = true;
        seed_one_worktree_id(&mut state, "ump-dash");

        let effects = update(&mut state, Action::OpenEditor);

        assert!(effects.is_empty());
        assert_eq!(
            state.error_state.as_ref().map(|e| e.message.as_str()),
            Some("Cannot open editor: not inside a tmux, zellij, or Ghostty session")
        );
    }

    #[test]
    fn open_editor_external_mode_emits_quoted_external_command() {
        let mut state = base_state();
        state.app_config.editor = "emacsclient -c -n".into();
        state.app_config.editor_in_terminal = false;
        seed_one_worktree_id(&mut state, "ump dash");

        let effects = update(&mut state, Action::OpenEditor);

        match &effects[..] {
            [Effect::OpenExternalEditor { command }] => {
                assert_eq!(command, "emacsclient -c -n '/tmp/ump dash'");
            }
            other => panic!("expected OpenExternalEditor effect, got {other:?}"),
        }
    }

    #[test]
    fn open_editor_empty_config_shows_error() {
        let mut state = base_state();
        state.app_config.editor = "   ".into();
        state.app_config.editor_in_terminal = false;
        seed_one_worktree_id(&mut state, "ump-dash");

        let effects = update(&mut state, Action::OpenEditor);

        assert!(effects.is_empty());
        assert_eq!(
            state.error_state.as_ref().map(|e| e.message.as_str()),
            Some("Cannot open editor: configure the editor setting first")
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
        let worktrees = vec![make_worktree("wt-A", "main"), make_worktree("wt-B", "feat")];
        let _ = update(&mut state, Action::WorktreesLoaded(worktrees));
        assert!(state.worktrees.contains_key(&WorktreeId("wt-A".into())));
        assert!(state.worktrees.contains_key(&WorktreeId("wt-B".into())));
    }

    #[test]
    fn worktrees_loaded_starts_ios_cache_lookup_for_each_worktree() {
        let mut state = AppState::default();
        let worktrees = vec![make_worktree("wt-A", "main"), make_worktree("wt-B", "feat")];

        let effects = update(&mut state, Action::WorktreesLoaded(worktrees));

        assert!(matches!(
            state
                .worktrees
                .get(&WorktreeId("wt-A".into()))
                .unwrap()
                .ios_simulator_cache,
            crate::domain::native_cache::IosSimulatorCacheState::Checking
        ));
        assert!(matches!(
            state
                .worktrees
                .get(&WorktreeId("wt-B".into()))
                .unwrap()
                .ios_simulator_cache,
            crate::domain::native_cache::IosSimulatorCacheState::Checking
        ));

        let lookup_worktrees = effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::LookupIosSimulatorCache {
                    worktree_id,
                    worktree_path,
                } => Some((worktree_id.clone(), worktree_path.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            lookup_worktrees,
            vec![
                (
                    WorktreeId("wt-A".into()),
                    std::path::PathBuf::from("/tmp/wt-A")
                ),
                (
                    WorktreeId("wt-B".into()),
                    std::path::PathBuf::from("/tmp/wt-B")
                ),
            ]
        );
    }

    #[test]
    fn worktrees_loaded_starts_android_cache_lookup_for_each_worktree() {
        let mut state = AppState::default();
        let worktrees = vec![make_worktree("wt-A", "main"), make_worktree("wt-B", "feat")];

        let effects = update(&mut state, Action::WorktreesLoaded(worktrees));

        assert!(matches!(
            state
                .worktrees
                .get(&WorktreeId("wt-A".into()))
                .unwrap()
                .android_cache,
            crate::domain::native_cache::AndroidCacheState::Checking
        ));
        assert!(matches!(
            state
                .worktrees
                .get(&WorktreeId("wt-B".into()))
                .unwrap()
                .android_cache,
            crate::domain::native_cache::AndroidCacheState::Checking
        ));

        let lookup_worktrees = effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::LookupAndroidCache {
                    worktree_id,
                    worktree_path,
                } => Some((worktree_id.clone(), worktree_path.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            lookup_worktrees,
            vec![
                (
                    WorktreeId("wt-A".into()),
                    std::path::PathBuf::from("/tmp/wt-A")
                ),
                (
                    WorktreeId("wt-B".into()),
                    std::path::PathBuf::from("/tmp/wt-B")
                ),
            ]
        );
    }

    #[test]
    fn worktrees_loaded_marks_every_running_metro_worktree() {
        let mut state = AppState::default();

        #[derive(Debug)]
        struct FakeMetroHandle {
            pid: u32,
            worktree_id: String,
            port: u16,
        }
        impl crate::domain::ports::metro_port::MetroHandle for FakeMetroHandle {
            fn pid(&self) -> u32 {
                self.pid
            }
            fn worktree_id(&self) -> &str {
                &self.worktree_id
            }
            fn port(&self) -> u16 {
                self.port
            }
            fn send_stdin(&self, _bytes: Vec<u8>) -> anyhow::Result<()> {
                Ok(())
            }
            fn kill(self: Box<Self>) -> anyhow::Result<()> {
                Ok(())
            }
        }

        state
            .worktrees
            .entry(WorktreeId("wt-A".into()))
            .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
                id: WorktreeId("wt-A".into()),
                ..Default::default()
            })
            .metro
            .register(Box::new(FakeMetroHandle {
                pid: 9001,
                worktree_id: "wt-A".into(),
                port: 8081,
            }));
        state
            .worktrees
            .entry(WorktreeId("wt-B".into()))
            .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
                id: WorktreeId("wt-B".into()),
                ..Default::default()
            })
            .metro
            .register(Box::new(FakeMetroHandle {
                pid: 9002,
                worktree_id: "wt-B".into(),
                port: 8082,
            }));

        let mut worktrees = vec![make_worktree("wt-A", "main"), make_worktree("wt-B", "feat")];
        let _ = update(&mut state, Action::WorktreesLoaded(worktrees.clone()));
        worktrees = state.worktree_browser.worktrees;

        assert!(
            worktrees
                .iter()
                .all(|wt| wt.metro_status == WorktreeMetroStatus::Running)
        );
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
        state
            .worktrees
            .get_mut(&WorktreeId("wt-A".into()))
            .unwrap()
            .task = Some(synthetic_task_record(1, CommandSpec::YarnInstall));
        state
            .worktrees
            .get_mut(&WorktreeId("wt-B".into()))
            .unwrap()
            .task = Some(synthetic_task_record(
            2,
            CommandSpec::YarnJest {
                filter: String::new(),
            },
        ));

        assert_running_in(&state, "wt-A");
        assert_running_in(&state, "wt-B");

        let count_running = state
            .worktrees
            .values()
            .filter(|s| s.task.is_some())
            .count();
        assert_eq!(
            count_running, 2,
            "TASK-02 contract: parallel tasks across worktrees"
        );
    }

    /// COVER-01 / D-13 contract: MetroStart is scoped to the selected worktree.
    ///
    /// This is a unit-level restatement of the integration test in
    /// `tests/metro_single_instance.rs` (COVER-01). If accessing
    /// `FakeMetroHandle` from within `src/` is impractical without exposing
    /// a public test-helper type, this test is marked `#[ignore]` and
    /// coverage deferred to COVER-01.
    #[test]
    fn metro_start_on_a_while_metro_running_on_b_spawns_second_instance() {
        let mut state = base_state();
        seed_two_worktrees(&mut state, "wt-A", "wt-B");

        // Register a fake MetroHandle to simulate metro running in wt-B.
        #[derive(Debug)]
        struct FakeMetroHandle {
            pid: u32,
            worktree_id: String,
            port: u16,
        }
        impl crate::domain::ports::metro_port::MetroHandle for FakeMetroHandle {
            fn pid(&self) -> u32 {
                self.pid
            }
            fn worktree_id(&self) -> &str {
                &self.worktree_id
            }
            fn port(&self) -> u16 {
                self.port
            }
            fn send_stdin(&self, _bytes: Vec<u8>) -> anyhow::Result<()> {
                Ok(())
            }
            fn kill(self: Box<Self>) -> anyhow::Result<()> {
                Ok(())
            }
        }
        state
            .worktrees
            .get_mut(&WorktreeId("wt-B".into()))
            .expect("wt-B slice seeded")
            .metro
            .register(Box::new(FakeMetroHandle {
                pid: 9001,
                worktree_id: "wt-B".into(),
                port: 8081,
            }));
        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-B".into()))
                .unwrap()
                .metro
                .is_running(),
            "precondition: metro running in wt-B"
        );

        // Set active worktree to A (index 0) and dispatch MetroStart.
        state.worktree_browser.worktree_table_state.select(Some(0));
        // Skip external detection so the test stays synchronous.
        state.metro_state.skip_external_metro_check = true;
        let effects = update(&mut state, Action::MetroStart);

        assert!(
            !state.metro_state.pending_restart,
            "starting Metro on another worktree must not stop/restart the existing one"
        );
        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-B".into()))
                .unwrap()
                .metro
                .is_running(),
            "existing Metro must stay registered"
        );
        assert!(
            effects.iter().any(|effect| matches!(effect, Effect::SpawnMetro { worktree, .. } if worktree.ends_with("wt-A"))),
            "selected wt-A should get its own SpawnMetro effect; got {effects:?}"
        );
    }

    #[test]
    fn metro_exited_clears_only_matching_worktree_slice() {
        let mut state = base_state();
        seed_two_worktrees(&mut state, "wt-A", "wt-B");

        #[derive(Debug)]
        struct FakeMetroHandle {
            pid: u32,
            worktree_id: String,
            port: u16,
        }
        impl crate::domain::ports::metro_port::MetroHandle for FakeMetroHandle {
            fn pid(&self) -> u32 {
                self.pid
            }
            fn worktree_id(&self) -> &str {
                &self.worktree_id
            }
            fn port(&self) -> u16 {
                self.port
            }
            fn send_stdin(&self, _bytes: Vec<u8>) -> anyhow::Result<()> {
                Ok(())
            }
            fn kill(self: Box<Self>) -> anyhow::Result<()> {
                Ok(())
            }
        }

        state
            .worktrees
            .get_mut(&WorktreeId("wt-A".into()))
            .unwrap()
            .metro
            .register(Box::new(FakeMetroHandle {
                pid: 9001,
                worktree_id: "wt-A".into(),
                port: 8081,
            }));
        state
            .worktrees
            .get_mut(&WorktreeId("wt-B".into()))
            .unwrap()
            .metro
            .register(Box::new(FakeMetroHandle {
                pid: 9002,
                worktree_id: "wt-B".into(),
                port: 8082,
            }));

        let _ = update(&mut state, Action::MetroExited("wt-A".into()));

        assert!(
            !state
                .worktrees
                .get(&WorktreeId("wt-A".into()))
                .unwrap()
                .metro
                .is_running()
        );
        assert!(
            state
                .worktrees
                .get(&WorktreeId("wt-B".into()))
                .unwrap()
                .metro
                .is_running()
        );
    }

    #[test]
    fn metro_exited_clears_pending_cached_ios_launch() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_ios_hit_fixture();
        {
            let slice = state
                .worktrees
                .get_mut(&WorktreeId("wt-1".into()))
                .expect("slice should exist");
            slice.metro.reserve_start(19003);
            slice.pending_cached_ios_launch =
                Some(crate::domain::native_cache::PendingCachedIosLaunch {
                    device_id: "SIM-stale".into(),
                    cache_hit: hit,
                });
        }

        let _ = update(&mut state, Action::MetroExited("wt-1".into()));

        let slice = state
            .worktrees
            .get(&WorktreeId("wt-1".into()))
            .expect("slice should exist");
        assert!(slice.pending_cached_ios_launch.is_none());
        assert!(
            slice
                .output
                .iter()
                .any(|line| line == "[cached-ios error] Metro exited before cached launch"),
            "expected cached-ios Metro exit error in origin output; got {:?}",
            slice.output
        );

        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("slice should exist")
            .metro
            .reserve_start(19004);
        let effects = update(
            &mut state,
            Action::MetroActivityUpdate {
                worktree_id: "wt-1".into(),
                activity: crate::domain::metro::MetroActivity::Ready,
            },
        );

        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::InstallAndLaunchCachedIosSimulator { .. })),
            "stale cached iOS launch must not fire after later Metro Ready; got {effects:?}"
        );
    }

    #[test]
    fn metro_exited_clears_pending_cached_android_launch() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_android_hit_fixture();
        {
            let slice = state
                .worktrees
                .get_mut(&WorktreeId("wt-1".into()))
                .expect("slice should exist");
            slice.metro.reserve_start(19003);
            slice.pending_cached_android_launch =
                Some(crate::domain::native_cache::PendingCachedAndroidLaunch {
                    device_id: "emulator-5554".into(),
                    cache_hit: hit,
                });
        }

        let _ = update(&mut state, Action::MetroExited("wt-1".into()));

        let slice = state
            .worktrees
            .get(&WorktreeId("wt-1".into()))
            .expect("slice should exist");
        assert!(slice.pending_cached_android_launch.is_none());
        assert!(
            slice
                .output
                .iter()
                .any(|line| line == "[cached-android error] Metro exited before cached launch"),
            "expected cached-android Metro exit error in origin output; got {:?}",
            slice.output
        );

        state
            .worktrees
            .get_mut(&WorktreeId("wt-1".into()))
            .expect("slice should exist")
            .metro
            .reserve_start(19004);
        let effects = update(
            &mut state,
            Action::MetroActivityUpdate {
                worktree_id: "wt-1".into(),
                activity: crate::domain::metro::MetroActivity::Ready,
            },
        );

        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::InstallAndLaunchCachedAndroid { .. })),
            "stale cached Android launch must not fire after later Metro Ready; got {effects:?}"
        );
    }

    #[test]
    fn metro_spawn_failure_clears_pending_cached_android_launch() {
        let mut state = base_state();
        seed_one_worktree(&mut state);
        let hit = cached_android_hit_fixture();
        {
            let slice = state
                .worktrees
                .get_mut(&WorktreeId("wt-1".into()))
                .expect("slice should exist");
            slice.metro.reserve_start(19003);
            slice.pending_cached_android_launch =
                Some(crate::domain::native_cache::PendingCachedAndroidLaunch {
                    device_id: "emulator-5554".into(),
                    cache_hit: hit,
                });
        }

        let effects = update(
            &mut state,
            Action::MetroSpawnFailed {
                worktree_id: "wt-1".into(),
                message: "port unavailable".into(),
            },
        );

        let slice = state
            .worktrees
            .get(&WorktreeId("wt-1".into()))
            .expect("slice should exist");
        assert!(effects.is_empty());
        assert!(slice.pending_cached_android_launch.is_none());
        assert!(
            slice.output.iter().any(
                |line| line == "[cached-android error] Metro failed to start: port unavailable"
            ),
            "expected cached-android Metro spawn error in origin output; got {:?}",
            slice.output
        );
        assert!(state.error_state.is_some());
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
        state
            .worktrees
            .get_mut(&WorktreeId("wt-A".into()))
            .unwrap()
            .task = Some(synthetic_task_record(5, CommandSpec::YarnInstall));

        // Active worktree = B (UI has selected index 1 = wt-B).
        // seed_two_worktrees puts wt-A at 0 and wt-B at 1; select(1) = B.
        state.worktree_browser.worktree_table_state.select(Some(1));

        let _ = update(
            &mut state,
            Action::CommandOutputLine {
                task_id: crate::domain::task::TaskId(5),
                line: "from-A".into(),
            },
        );

        let a_out = slice_output(&state, "wt-A");
        let b_out = slice_output(&state, "wt-B");
        assert!(
            a_out.iter().any(|l| l == "from-A"),
            "D-08: line must land in slice_A (task owner); A={:?} B={:?}",
            a_out,
            b_out
        );
        assert!(
            !b_out.iter().any(|l| l == "from-A"),
            "D-08: line must NOT land in slice_B (not task owner); A={:?} B={:?}",
            a_out,
            b_out
        );
    }

    /// TASK-03 / D-11: CommandExited drains the slice-local queue of the
    /// exiting task, leaving other worktrees' queues untouched.
    #[test]
    fn command_exited_drains_slice_local_queue_not_other() {
        let mut state = base_state();
        seed_two_worktrees(&mut state, "wt-A", "wt-B");

        // Task on A; both slices have one queued item.
        state
            .worktrees
            .get_mut(&WorktreeId("wt-A".into()))
            .unwrap()
            .task = Some(synthetic_task_record(7, CommandSpec::GitFetch));
        state
            .worktrees
            .get_mut(&WorktreeId("wt-A".into()))
            .unwrap()
            .queue
            .push_back(CommandSpec::YarnInstall);
        state
            .worktrees
            .get_mut(&WorktreeId("wt-B".into()))
            .unwrap()
            .queue
            .push_back(CommandSpec::YarnPodInstall);

        let queue_b_before = slice_queue_len(&state, "wt-B");

        let _ = update(
            &mut state,
            Action::CommandExited {
                task_id: crate::domain::task::TaskId(7),
                status: crate::domain::task::ExitStatus::Success,
            },
        );

        // A's queue was drained (YarnInstall dispatched via SpawnTask effect).
        // slice.task is None post-exit; SpawnTask effect will populate it in runtime.
        assert_eq!(
            slice_queue_len(&state, "wt-A"),
            0,
            "D-11: A's queue must drain on CommandExited"
        );
        // B's queue untouched.
        assert_eq!(
            slice_queue_len(&state, "wt-B"),
            queue_b_before,
            "D-11: B's queue must not change"
        );
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

        let _ = update(
            &mut state,
            Action::CommandOutputLine {
                task_id: crate::domain::task::TaskId(99),
                line: "stale".into(),
            },
        );

        // P-3 contract: the stale line must not land in any slice's output.
        let any_slice_has_it = state
            .worktrees
            .values()
            .any(|s| s.output.iter().any(|l| l == "stale"));
        assert!(
            !any_slice_has_it,
            "P-3: stale output line must not contaminate any slice; \
             slices = {:?}",
            state
                .worktrees
                .iter()
                .map(|(k, v)| (k, v.output.len()))
                .collect::<Vec<_>>()
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
        let output_before: Vec<String> = state
            .worktrees
            .get(&wid)
            .unwrap()
            .output
            .iter()
            .cloned()
            .collect();

        let effects = update(&mut state, Action::CommandRun(CommandSpec::YarnInstall));

        // (a) No SpawnTask effect emitted.
        let has_spawn = effects
            .iter()
            .any(|e| matches!(e, Effect::SpawnTask { .. }));
        assert!(
            !has_spawn,
            "BlockNew must NOT emit Effect::SpawnTask; got {effects:?}"
        );

        // (b) slice.task is STILL Some with the original task_id (100).
        let task = state.worktrees.get(&wid).unwrap().task.as_ref();
        assert!(task.is_some(), "BlockNew must leave existing task in slice");
        assert_eq!(
            task.unwrap().id,
            crate::domain::task::TaskId(100),
            "BlockNew must preserve the original task_id"
        );

        // (c) slice.output is unchanged — no $ argv line, no [cancelled ...] line.
        let output_after: Vec<String> = state
            .worktrees
            .get(&wid)
            .unwrap()
            .output
            .iter()
            .cloned()
            .collect();
        assert_eq!(
            output_before, output_after,
            "BlockNew must not write any output line; before={output_before:?} after={output_after:?}"
        );
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
            CommandSpec::YarnJest {
                filter: "first".into(),
            },
        ));

        // Stage a TextInput modal whose template is YarnJest, buffer = "second".
        // ModalInputSubmit will compose CommandSpec::YarnJest { filter: "second" }
        // and call dispatch_command — the collision gate's true entry point.
        state.modal_stack.modal = Some(ModalState::TextInput {
            prompt: "Jest filter:".into(),
            buffer: "second".into(),
            pending_template: Box::new(CommandSpec::YarnJest {
                filter: String::new(),
            }),
        });

        let effects = update(&mut state, Action::ModalInputSubmit);

        // (a) Exactly one SpawnTask Effect with the NEW filter.
        let has_spawn_second = effects.iter().any(|e| matches!(
            e,
            Effect::SpawnTask { spec: CommandSpec::YarnJest { filter }, .. } if filter == "second"
        ));
        assert!(
            has_spawn_second,
            "CancelPrevious must emit Effect::SpawnTask for the NEW YarnJest dispatch; got {effects:?}"
        );

        // (b) slice.task is None — old record was taken; new record arrives
        //     asynchronously via task_handle_tx (not visible to synchronous update()).
        assert!(
            state.worktrees.get(&wid).unwrap().task.is_none(),
            "CancelPrevious must take the existing record from slice.task"
        );

        // (c) slice.output contains [cancelled by new dispatch].
        let output: Vec<String> = state
            .worktrees
            .get(&wid)
            .unwrap()
            .output
            .iter()
            .cloned()
            .collect();
        assert!(
            output.iter().any(|l| l == "[cancelled by new dispatch]"),
            "CancelPrevious must push [cancelled by new dispatch]; output={output:?}"
        );
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

        let effects = update(&mut state, Action::CommandRun(CommandSpec::YarnLint));

        // (a) Existing YarnInstall record is still in slice.task — different
        //     discriminant means no collision, no cancellation.
        let task = state.worktrees.get(&wid).unwrap().task.as_ref();
        assert!(
            task.is_some(),
            "existing YarnInstall task must remain — different discriminant"
        );
        assert_eq!(
            task.unwrap().id,
            crate::domain::task::TaskId(300),
            "original YarnInstall task_id must be preserved"
        );

        // (b) No [cancelled by new dispatch] line — gate did not fire.
        let output: Vec<String> = state
            .worktrees
            .get(&wid)
            .unwrap()
            .output
            .iter()
            .cloned()
            .collect();
        assert!(
            !output.iter().any(|l| l == "[cancelled by new dispatch]"),
            "no [cancelled by new dispatch] line when discriminants differ; output={output:?}"
        );

        // (c) SpawnTask Effect emitted for the new YarnLint (different discriminant
        //     means the dispatch flows through normally to allocate a new task).
        let has_spawn_lint = effects.iter().any(|e| {
            matches!(
                e,
                Effect::SpawnTask {
                    spec: CommandSpec::YarnLint,
                    ..
                }
            )
        });
        assert!(
            has_spawn_lint,
            "different-discriminant dispatch must emit Effect::SpawnTask for the new spec; got {effects:?}"
        );
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
        let output_before: Vec<String> = state
            .worktrees
            .get(&wid)
            .unwrap()
            .output
            .iter()
            .cloned()
            .collect();

        let effects = update(&mut state, Action::CommandRun(CommandSpec::GitPull));

        // (a) No SpawnTask effect emitted.
        let has_spawn = effects
            .iter()
            .any(|e| matches!(e, Effect::SpawnTask { .. }));
        assert!(
            !has_spawn,
            "git BlockNew must NOT emit Effect::SpawnTask; got {effects:?}"
        );

        // (b) Existing GitPull task still in slice.
        let task = state.worktrees.get(&wid).unwrap().task.as_ref();
        assert!(task.is_some(), "git BlockNew must preserve existing task");
        assert_eq!(
            task.unwrap().id,
            crate::domain::task::TaskId(400),
            "original GitPull task_id must be preserved"
        );

        // (c) Output unchanged.
        let output_after: Vec<String> = state
            .worktrees
            .get(&wid)
            .unwrap()
            .output
            .iter()
            .cloned()
            .collect();
        assert_eq!(
            output_before, output_after,
            "git BlockNew must not write any output line"
        );
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
        state
            .worktrees
            .get_mut(&wid)
            .unwrap()
            .queue
            .push_back(CommandSpec::YarnInstall);
        let output_before: Vec<String> = state
            .worktrees
            .get(&wid)
            .unwrap()
            .output
            .iter()
            .cloned()
            .collect();

        let _effects = update(&mut state, Action::CommandCancel);

        // (a) slice.task still Some with the SAME task_id (re-inserted).
        let task = state.worktrees.get(&wid).unwrap().task.as_ref();
        assert!(
            task.is_some(),
            "non-cancellable: record must be re-inserted"
        );
        assert_eq!(
            task.unwrap().id,
            crate::domain::task::TaskId(500),
            "non-cancellable: original task_id must be preserved"
        );

        // (b) Queue is unchanged — NOT cleared.
        assert_eq!(
            slice_queue_len(&state, "wt-1"),
            1,
            "non-cancellable: queue must NOT be cleared"
        );

        // (c) Output does NOT contain [cancelled] line.
        let output_after: Vec<String> = state
            .worktrees
            .get(&wid)
            .unwrap()
            .output
            .iter()
            .cloned()
            .collect();
        assert_eq!(
            output_before, output_after,
            "non-cancellable: no [cancelled] line written; before={output_before:?} after={output_after:?}"
        );
        assert!(
            !output_after.iter().any(|l| l == "[cancelled]"),
            "non-cancellable: explicit no-[cancelled] check"
        );
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
        state
            .worktrees
            .get_mut(&wid)
            .unwrap()
            .queue
            .push_back(CommandSpec::YarnLint);

        let _effects = update(&mut state, Action::CommandCancel);

        // (a) slice.task is None — record was taken.
        assert!(
            state.worktrees.get(&wid).unwrap().task.is_none(),
            "cancellable: slice.task must be cleared"
        );

        // (b) Queue is empty — cleared.
        assert_eq!(
            slice_queue_len(&state, "wt-1"),
            0,
            "cancellable: queue must be cleared"
        );

        // (c) Output contains [cancelled].
        let output: Vec<String> = state
            .worktrees
            .get(&wid)
            .unwrap()
            .output
            .iter()
            .cloned()
            .collect();
        assert!(
            output.iter().any(|l| l == "[cancelled]"),
            "cancellable: output must contain [cancelled]; got {output:?}"
        );
    }
}
