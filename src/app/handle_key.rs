//! Keyboard dispatch — pure function mapping (AppState, KeyEvent) -> Option<Action>.
//!
//! Plan 13-07 will replace the hard-coded match arms with a table-driven
//! KEYBINDINGS consumer (F-208). For Plan 13-06 this is a verbatim lift from
//! src/app.rs.

use super::state::{AppState, FocusedPanel, PaletteMode};
use crate::domain::action::Action;
use crate::domain::command::{CommandSpec, ModalState};
use ratatui::crossterm::event::{KeyCode, KeyEventKind};

/// Pure function: maps (state, key) → Action. No side effects.
/// Called from the event loop — keep it fast and allocation-free.
pub fn handle_key(state: &AppState, key: ratatui::crossterm::event::KeyEvent) -> Option<Action> {
    use KeyCode::*;

    // Guard: only process key-press events (prevents double-firing on Windows)
    if key.kind != KeyEventKind::Press {
        return None;
    }

    // --- MODAL INTERCEPTION — MUST be first (prevents key leak to navigation) ---
    if let Some(ref modal) = state.modal {
        return match modal {
            ModalState::Confirm { .. } => match key.code {
                Char('y') | Char('Y') => Some(Action::ModalConfirm),
                Char('n') | Char('N') | Esc => Some(Action::ModalCancel),
                _ => None,
            },
            ModalState::TextInput { .. } => match key.code {
                Esc => Some(Action::ModalCancel),
                Enter => Some(Action::ModalInputSubmit),
                Backspace => Some(Action::ModalInputBackspace),
                Char(c) => Some(Action::ModalInputChar(c)),
                _ => None,
            },
            ModalState::DevicePicker { .. } => match key.code {
                Esc => Some(Action::ModalCancel),
                Enter => Some(Action::ModalDeviceConfirm),
                Down => Some(Action::ModalDeviceNext),
                Up => Some(Action::ModalDevicePrev),
                Backspace => Some(Action::ModalInputBackspace),
                Char('j') => Some(Action::ModalDeviceNext),
                Char('k') => Some(Action::ModalDevicePrev),
                Char(c) if !c.is_ascii_control() => Some(Action::ModalInputChar(c)),
                _ => None,
            },
            ModalState::CleanToggle { .. } => match key.code {
                Char('n') => Some(Action::CleanToggleNodeModules),
                Char('p') => Some(Action::CleanTogglePods),
                Char('a') => Some(Action::CleanToggleAndroid),
                Char('i') => Some(Action::CleanToggleSyncAfter),
                Char('x') | Enter => Some(Action::CleanConfirm),
                Esc => Some(Action::ModalCancel),
                _ => None,
            },
            ModalState::SyncBeforeRun { .. } => match key.code {
                Char('y') | Char('Y') => Some(Action::SyncBeforeRunAccept),
                Char('n') | Char('N') | Esc => Some(Action::SyncBeforeRunDecline),
                _ => None,
            },
            ModalState::SyncBeforeMetro { .. } => match key.code {
                Char('y') | Char('Y') => Some(Action::SyncBeforeMetroAccept),
                Char('n') | Char('N') | Esc => Some(Action::SyncBeforeMetroDecline),
                _ => None,
            },
            ModalState::ExternalMetroConflict { pid, .. } => match key.code {
                Char('y') | Char('Y') | Enter => Some(Action::KillExternalMetro(*pid)),
                Char('n') | Char('N') | Esc => Some(Action::ModalCancel),
                _ => None,
            },
            ModalState::BranchPicker { .. } => match key.code {
                Enter => Some(Action::BranchPickerConfirm),
                Esc => Some(Action::ModalCancel),
                Down => Some(Action::BranchPickerNext),
                Up => Some(Action::BranchPickerPrev),
                Backspace => Some(Action::BranchPickerBackspace),
                Char(c) => Some(Action::BranchPickerFilter(c)),
                _ => None,
            },
        };
    }

    // --- PALETTE MODE ROUTING — after modal, before overlays ---
    if let Some(ref mode) = state.palette_mode {
        return match mode {
            PaletteMode::Android => match key.code {
                Char('d') => {
                    let mode_flag = state.android_mode.as_ref().map(|m| format!(" --mode {m}")).unwrap_or_default();
                    Some(Action::CommandRun(CommandSpec::ShellCommand {
                        command: format!("npx react-native run-android{mode_flag}"),
                    }))
                },
                Char('e') => Some(Action::CommandRun(CommandSpec::RnRunAndroid { device_id: String::new(), mode: state.android_mode.clone() })),
                Char('r') => Some(Action::CommandRun(CommandSpec::RnReleaseBuild)),
                Char('m') => Some(Action::StartSetAndroidMode),
                Esc => Some(Action::ModalCancel),
                _ => Some(Action::ModalCancel),
            },
            PaletteMode::Ios => match key.code {
                Char('d') => Some(Action::CommandRun(CommandSpec::RnRunIosDevice)),
                Char('e') => Some(Action::CommandRun(CommandSpec::RnRunIos { device_id: String::new() })),
                Char('p') => Some(Action::CommandRun(CommandSpec::YarnPodInstall)),
                Esc => Some(Action::ModalCancel),
                _ => Some(Action::ModalCancel),
            },
            PaletteMode::Yarn => match key.code {
                Char('i') => Some(Action::CommandRun(CommandSpec::YarnInstall)),
                Char('p') => Some(Action::CommandRun(CommandSpec::YarnPodInstall)),
                Char('u') => Some(Action::CommandRun(CommandSpec::YarnUnitTests)),
                Char('t') => Some(Action::CommandRun(CommandSpec::YarnCheckTypes)),
                Char('j') => Some(Action::CommandRun(CommandSpec::YarnJest { filter: String::new() })),
                Char('l') => Some(Action::CommandRun(CommandSpec::YarnLint)),
                Char('c') => Some(Action::OpenCleanMenu),
                Esc => Some(Action::ModalCancel),
                _ => Some(Action::ModalCancel),
            },
            PaletteMode::Git => match key.code {
                Char('f') => Some(Action::CommandRun(CommandSpec::GitFetch)),
                Char('p') => Some(Action::CommandRun(CommandSpec::GitPull)),
                Char('P') => Some(Action::CommandRun(CommandSpec::GitPush)),
                Char('X') => Some(Action::CommandRun(CommandSpec::GitResetHardFetch)),
                Char('b') => Some(Action::CommandRun(CommandSpec::GitCheckout { branch: String::new() })),
                Char('c') => Some(Action::CommandRun(CommandSpec::GitCheckoutNew { branch: String::new() })),
                Char('r') => Some(Action::CommandRun(CommandSpec::GitRebase { target: String::new() })),
                Esc => Some(Action::ModalCancel),
                _ => Some(Action::ModalCancel),
            },
            PaletteMode::Worktree => match key.code {
                Char('w') => Some(Action::WorktreeAdd),
                Char('d') => Some(Action::WorktreeRemove),
                Char('b') => Some(Action::WorktreeAddNewBranch),
                Esc => Some(Action::ModalCancel),
                _ => Some(Action::ModalCancel),
            },
        };
    }

    // --- OVERLAY MODES ---
    if state.show_help {
        return match key.code {
            Char('q') | Esc => Some(Action::DismissHelp),
            _ => None,
        };
    }

    if state.error_state.is_some() {
        return match key.code {
            Char('r') => Some(Action::RetryLastCommand),
            Char('q') | Esc => Some(Action::DismissError),
            _ => None,
        };
    }

    // --- FULLSCREEN: Tab exits fullscreen ---
    if state.fullscreen_panel.is_some()
        && key.code == Tab {
            return Some(Action::ToggleFullscreen);
        }

    // --- WORKTREE TABLE SPECIFIC ---
    if state.focused_panel == FocusedPanel::WorktreeTable {
        match key.code {
            Char('j') | Down => return Some(Action::WorktreeSelectNext),
            Char('k') | Up => return Some(Action::WorktreeSelectPrev),
            Char('a') => return Some(Action::EnterAndroidPalette),
            Char('i') => return Some(Action::EnterIosPalette),
            Char('y') => return Some(Action::EnterYarnPalette),
            Char('w') => return Some(Action::EnterWorktreePalette),
            Char('g') => return Some(Action::EnterGitPalette),
            Char('C') => return Some(Action::OpenClaudeCode),
            Char('T') => return Some(Action::OpenShellTab),
            Char('f') => return Some(Action::ToggleFullscreen),
            Char('!') => return Some(Action::StartShellCommand),
            Char('R') => {
                if state.metro.is_running() {
                    return Some(Action::MetroSendReload);
                } else {
                    return Some(Action::RefreshWorktrees);
                }
            }
            Char('J') => {
                if state.metro.is_running() {
                    return Some(Action::MetroSendDebugger);
                }
                // J does nothing when metro is not running
            }
            Esc => {
                if state.metro.is_running() {
                    return Some(Action::MetroStop);
                }
                // ESC does nothing on worktree table when metro is not running
            }
            Enter => return Some(Action::WorktreeSwitchToSelected),
            _ => {}
        }
    }

    // --- COMMAND OUTPUT SPECIFIC ---
    if state.focused_panel == FocusedPanel::CommandOutput {
        match key.code {
            Char('j') | Down => return Some(Action::CommandOutputScrollDown),
            Char('k') | Up => return Some(Action::CommandOutputScrollUp),
            Char('G') => return Some(Action::ScrollToBottom),
            Char('g') => {
                if state.pending_g {
                    return Some(Action::ScrollToTop);
                } else {
                    return Some(Action::SetPendingG);
                }
            }
            Char('X') => return Some(Action::CommandCancel),
            Char('C') => return Some(Action::CommandOutputClear),
            Char('f') => return Some(Action::ToggleFullscreen),
            _ => {}
        }
    }

    // --- NORMAL MODE ---
    match key.code {
        Char('q') => Some(Action::Quit),
        Char('?') | F(1) => Some(Action::ShowHelp),
        Char('/') => Some(Action::Search),
        Char('j') | Down => Some(Action::FocusDown),
        Char('k') | Up => Some(Action::FocusUp),
        Char('h') | Left => Some(Action::FocusLeft),
        Char('l') | Right => Some(Action::FocusRight),
        Tab => Some(Action::FocusNext),
        BackTab => Some(Action::FocusPrev),
        _ => None,
    }
}
