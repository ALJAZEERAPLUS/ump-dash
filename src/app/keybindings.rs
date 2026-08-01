//! Unified keybinding registry (F-400 + F-208 + F-302 + F-303 type half).
//!
//! Single source of truth for every keypress dispatched by the TUI. Three
//! consumers (Plan 13-10 wires the latter two):
//!   1. `handle_key` walks the registry to resolve a KeyEvent to an Action.
//!   2. `footer::render_footer` reads `footer_hints_for(state)` (Plan 13-10).
//!   3. `help_overlay::render_help` reads `help_overlay_rows()` (Plan 13-10).
//!
//! The `context` field encodes when a binding is active. The handle_key
//! walker filters by `context_matches(&kb.context, state)`.
//!
//! The catch-all modal inputs (TextInput/DevicePicker/BranchPicker accept
//! arbitrary chars as input/filter text) cannot be encoded as a single
//! KeyCode match, so `handle_key` performs that fallthrough AFTER the
//! registry walk. The palette context-level fallback
//! `_ => Some(Action::ModalCancel)` (13-RESEARCH.md Pitfall 4) is also a
//! post-loop branch in handle_key.

#![allow(dead_code)]

use super::state::{AppState, PaletteMode, active_worktree_id};
use crate::domain::action::Action;
use crate::domain::command::{CommandSpec, ModalState};
use crate::domain::worktree_slice::LastRunConfig;
use ratatui::crossterm::event::KeyCode;

#[derive(Debug, Clone)]
pub enum BindingContext {
    /// Matches unconditionally (currently unused — kept for future broad-scope bindings).
    Always,
    /// Top-level (no modal, palette, or overlay).
    Normal,
    /// The worktree table is the app's primary surface.
    WorktreeTable,
    /// Printable input is editing the worktree table filter.
    WorktreeFilter,
    /// A specific palette is open.
    Palette(PaletteMode),
    /// A specific modal is open.
    Modal(ModalKind),
    /// A specific overlay is active.
    Overlay(OverlayKind),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModalKind {
    Confirm,
    TextInput,
    DevicePicker,
    RunVariantPicker,
    CleanToggle,
    SyncBeforeRun,
    SyncBeforeMetro,
    ExternalMetroConflict,
    BranchPicker,
    PullRequestPicker,
    CommandLogs,
    Info,
}

#[derive(Debug, Clone, Copy)]
pub enum OverlayKind {
    Help,
    Error,
}

/// A single keybinding entry. `action` is a fn-pointer that reads AppState to
/// produce an Action (supports context-sensitive actions like the conditional
/// 'R' reload/refresh key). `visible` filters footer display.
pub struct KeyBinding {
    pub key: KeyCode,
    pub label: &'static str,
    pub short_desc: &'static str,
    pub long_desc: &'static str,
    pub context: BindingContext,
    pub action: fn(&AppState) -> Option<Action>,
    pub visible: fn(&AppState) -> bool,
}

/// Row returned by `help_overlay_rows()` — Plan 13-10 renders these in the help overlay.
pub struct HelpRow {
    pub section: &'static str,
    pub label: &'static str,
    pub desc: &'static str,
}

// ---------------------------------------------------------------------------
// The registry.
// ---------------------------------------------------------------------------

pub const KEYBINDINGS: &[KeyBinding] = &[
    // ==== Modal: Confirm (Y/N/Esc) ====
    KeyBinding {
        key: KeyCode::Char('y'),
        label: "Y",
        short_desc: "confirm",
        long_desc: "Confirm destructive action",
        context: BindingContext::Modal(ModalKind::Confirm),
        action: |_| Some(Action::ModalConfirm),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('Y'),
        label: "Y",
        short_desc: "confirm",
        long_desc: "Confirm destructive action",
        context: BindingContext::Modal(ModalKind::Confirm),
        action: |_| Some(Action::ModalConfirm),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('n'),
        label: "N",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::Confirm),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('N'),
        label: "N",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::Confirm),
        action: |_| Some(Action::ModalCancel),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::Confirm),
        action: |_| Some(Action::ModalCancel),
        visible: |_| false,
    },
    // ==== Modal: TextInput (Enter/Backspace/Esc — Char handled post-loop) ====
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::TextInput),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Enter,
        label: "Enter",
        short_desc: "submit",
        long_desc: "Submit text",
        context: BindingContext::Modal(ModalKind::TextInput),
        action: |_| Some(Action::ModalInputSubmit),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Backspace,
        label: "Backspace",
        short_desc: "backspace",
        long_desc: "Delete character",
        context: BindingContext::Modal(ModalKind::TextInput),
        action: |_| Some(Action::ModalInputBackspace),
        visible: |_| false,
    },
    // ==== Modal: DevicePicker (Enter/Esc/j/k/arrows/Backspace; Char handled post-loop) ====
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::DevicePicker),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Enter,
        label: "Enter",
        short_desc: "select",
        long_desc: "Select device",
        context: BindingContext::Modal(ModalKind::DevicePicker),
        action: |_| Some(Action::ModalDeviceConfirm),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Down,
        label: "Down",
        short_desc: "next",
        long_desc: "Next device",
        context: BindingContext::Modal(ModalKind::DevicePicker),
        action: |_| Some(Action::ModalDeviceNext),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Up,
        label: "Up",
        short_desc: "prev",
        long_desc: "Previous device",
        context: BindingContext::Modal(ModalKind::DevicePicker),
        action: |_| Some(Action::ModalDevicePrev),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Backspace,
        label: "Backspace",
        short_desc: "backspace",
        long_desc: "Delete filter character",
        context: BindingContext::Modal(ModalKind::DevicePicker),
        action: |_| Some(Action::ModalInputBackspace),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('j'),
        label: "j/k",
        short_desc: "navigate",
        long_desc: "Navigate device list",
        context: BindingContext::Modal(ModalKind::DevicePicker),
        action: |_| Some(Action::ModalDeviceNext),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('k'),
        label: "k",
        short_desc: "prev",
        long_desc: "Previous device",
        context: BindingContext::Modal(ModalKind::DevicePicker),
        action: |_| Some(Action::ModalDevicePrev),
        visible: |_| false,
    },
    // ==== Modal: RunVariantPicker (Enter/Esc/j/k/arrows) ====
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::RunVariantPicker),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Enter,
        label: "Enter",
        short_desc: "select",
        long_desc: "Select run type",
        context: BindingContext::Modal(ModalKind::RunVariantPicker),
        action: |_| Some(Action::ModalRunVariantConfirm),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Down,
        label: "Down",
        short_desc: "next",
        long_desc: "Next run type",
        context: BindingContext::Modal(ModalKind::RunVariantPicker),
        action: |_| Some(Action::ModalRunVariantNext),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Up,
        label: "Up",
        short_desc: "prev",
        long_desc: "Previous run type",
        context: BindingContext::Modal(ModalKind::RunVariantPicker),
        action: |_| Some(Action::ModalRunVariantPrev),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('j'),
        label: "j/k",
        short_desc: "navigate",
        long_desc: "Navigate run types",
        context: BindingContext::Modal(ModalKind::RunVariantPicker),
        action: |_| Some(Action::ModalRunVariantNext),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('k'),
        label: "k",
        short_desc: "prev",
        long_desc: "Previous run type",
        context: BindingContext::Modal(ModalKind::RunVariantPicker),
        action: |_| Some(Action::ModalRunVariantPrev),
        visible: |_| false,
    },
    // ==== Modal: CleanToggle ====
    KeyBinding {
        key: KeyCode::Char('n'),
        label: "n",
        short_desc: "node_modules",
        long_desc: "Toggle node_modules",
        context: BindingContext::Modal(ModalKind::CleanToggle),
        action: |_| Some(Action::CleanToggleNodeModules),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('p'),
        label: "p",
        short_desc: "pods",
        long_desc: "Toggle pods",
        context: BindingContext::Modal(ModalKind::CleanToggle),
        action: |_| Some(Action::CleanTogglePods),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('a'),
        label: "a",
        short_desc: "android",
        long_desc: "Toggle android",
        context: BindingContext::Modal(ModalKind::CleanToggle),
        action: |_| Some(Action::CleanToggleAndroid),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('i'),
        label: "i",
        short_desc: "sync after",
        long_desc: "Toggle sync after clean",
        context: BindingContext::Modal(ModalKind::CleanToggle),
        action: |_| Some(Action::CleanToggleSyncAfter),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('x'),
        label: "x/Enter",
        short_desc: "confirm",
        long_desc: "Confirm clean",
        context: BindingContext::Modal(ModalKind::CleanToggle),
        action: |_| Some(Action::CleanConfirm),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Enter,
        label: "Enter",
        short_desc: "confirm",
        long_desc: "Confirm clean",
        context: BindingContext::Modal(ModalKind::CleanToggle),
        action: |_| Some(Action::CleanConfirm),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::CleanToggle),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    // ==== Modal: SyncBeforeRun ====
    KeyBinding {
        key: KeyCode::Char('y'),
        label: "Y",
        short_desc: "sync first",
        long_desc: "Sync dependencies before run",
        context: BindingContext::Modal(ModalKind::SyncBeforeRun),
        action: |_| Some(Action::SyncBeforeRunAccept),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('Y'),
        label: "Y",
        short_desc: "sync first",
        long_desc: "Sync dependencies before run",
        context: BindingContext::Modal(ModalKind::SyncBeforeRun),
        action: |_| Some(Action::SyncBeforeRunAccept),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('n'),
        label: "N",
        short_desc: "skip sync",
        long_desc: "Skip sync and run anyway",
        context: BindingContext::Modal(ModalKind::SyncBeforeRun),
        action: |_| Some(Action::SyncBeforeRunDecline),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('N'),
        label: "N",
        short_desc: "skip sync",
        long_desc: "Skip sync and run anyway",
        context: BindingContext::Modal(ModalKind::SyncBeforeRun),
        action: |_| Some(Action::SyncBeforeRunDecline),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Skip sync and run anyway",
        context: BindingContext::Modal(ModalKind::SyncBeforeRun),
        action: |_| Some(Action::SyncBeforeRunDecline),
        visible: |_| true,
    },
    // ==== Modal: SyncBeforeMetro ====
    KeyBinding {
        key: KeyCode::Char('y'),
        label: "Y",
        short_desc: "sync first",
        long_desc: "Sync dependencies before metro",
        context: BindingContext::Modal(ModalKind::SyncBeforeMetro),
        action: |_| Some(Action::SyncBeforeMetroAccept),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('Y'),
        label: "Y",
        short_desc: "sync first",
        long_desc: "Sync dependencies before metro",
        context: BindingContext::Modal(ModalKind::SyncBeforeMetro),
        action: |_| Some(Action::SyncBeforeMetroAccept),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('n'),
        label: "N",
        short_desc: "skip sync",
        long_desc: "Skip sync and start metro anyway",
        context: BindingContext::Modal(ModalKind::SyncBeforeMetro),
        action: |_| Some(Action::SyncBeforeMetroDecline),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('N'),
        label: "N",
        short_desc: "skip sync",
        long_desc: "Skip sync and start metro anyway",
        context: BindingContext::Modal(ModalKind::SyncBeforeMetro),
        action: |_| Some(Action::SyncBeforeMetroDecline),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Skip sync and start metro anyway",
        context: BindingContext::Modal(ModalKind::SyncBeforeMetro),
        action: |_| Some(Action::SyncBeforeMetroDecline),
        visible: |_| true,
    },
    // ==== Modal: ExternalMetroConflict (Y/Enter kills PID from state) ====
    KeyBinding {
        key: KeyCode::Char('y'),
        label: "Y",
        short_desc: "kill it",
        long_desc: "Kill the external metro process",
        context: BindingContext::Modal(ModalKind::ExternalMetroConflict),
        action: external_metro_kill_action,
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('Y'),
        label: "Y",
        short_desc: "kill it",
        long_desc: "Kill the external metro process",
        context: BindingContext::Modal(ModalKind::ExternalMetroConflict),
        action: external_metro_kill_action,
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Enter,
        label: "Enter",
        short_desc: "kill it",
        long_desc: "Kill the external metro process",
        context: BindingContext::Modal(ModalKind::ExternalMetroConflict),
        action: external_metro_kill_action,
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('n'),
        label: "N",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::ExternalMetroConflict),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('N'),
        label: "N",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::ExternalMetroConflict),
        action: |_| Some(Action::ModalCancel),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::ExternalMetroConflict),
        action: |_| Some(Action::ModalCancel),
        visible: |_| false,
    },
    // ==== Modal: BranchPicker (Enter/Esc/arrows/Backspace; Char handled post-loop) ====
    KeyBinding {
        key: KeyCode::Enter,
        label: "Enter",
        short_desc: "select",
        long_desc: "Select branch",
        context: BindingContext::Modal(ModalKind::BranchPicker),
        action: |_| Some(Action::BranchPickerConfirm),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::BranchPicker),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Down,
        label: "Up/Down",
        short_desc: "navigate",
        long_desc: "Navigate branch list",
        context: BindingContext::Modal(ModalKind::BranchPicker),
        action: |_| Some(Action::BranchPickerNext),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Up,
        label: "Up",
        short_desc: "prev",
        long_desc: "Previous branch",
        context: BindingContext::Modal(ModalKind::BranchPicker),
        action: |_| Some(Action::BranchPickerPrev),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Backspace,
        label: "Backspace",
        short_desc: "backspace",
        long_desc: "Delete filter character",
        context: BindingContext::Modal(ModalKind::BranchPicker),
        action: |_| Some(Action::BranchPickerBackspace),
        visible: |_| false,
    },
    // ==== Modal: PullRequestPicker (Enter/Esc/arrows/Tab/Backspace; Char handled post-loop) ====
    KeyBinding {
        key: KeyCode::Enter,
        label: "Enter",
        short_desc: "select",
        long_desc: "Select PR",
        context: BindingContext::Modal(ModalKind::PullRequestPicker),
        action: |_| Some(Action::PullRequestPickerConfirm),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Cancel",
        context: BindingContext::Modal(ModalKind::PullRequestPicker),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Tab,
        label: "Tab",
        short_desc: "filter",
        long_desc: "Cycle PR filter",
        context: BindingContext::Modal(ModalKind::PullRequestPicker),
        action: |_| Some(Action::PullRequestPickerNextFilter),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Down,
        label: "Up/Down",
        short_desc: "navigate",
        long_desc: "Navigate PR list",
        context: BindingContext::Modal(ModalKind::PullRequestPicker),
        action: |_| Some(Action::PullRequestPickerNext),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Up,
        label: "Up",
        short_desc: "prev",
        long_desc: "Previous PR",
        context: BindingContext::Modal(ModalKind::PullRequestPicker),
        action: |_| Some(Action::PullRequestPickerPrev),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Backspace,
        label: "Backspace",
        short_desc: "backspace",
        long_desc: "Delete search character",
        context: BindingContext::Modal(ModalKind::PullRequestPicker),
        action: |_| Some(Action::PullRequestPickerBackspace),
        visible: |_| false,
    },
    // ==== Modal: CommandLogs ====
    KeyBinding {
        key: KeyCode::Char('j'),
        label: "j/k",
        short_desc: "scroll",
        long_desc: "Scroll command logs",
        context: BindingContext::Modal(ModalKind::CommandLogs),
        action: |_| Some(Action::CommandLogsScrollDown),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Down,
        label: "Down",
        short_desc: "scroll down",
        long_desc: "Scroll command logs down",
        context: BindingContext::Modal(ModalKind::CommandLogs),
        action: |_| Some(Action::CommandLogsScrollDown),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('k'),
        label: "k",
        short_desc: "scroll up",
        long_desc: "Scroll command logs up",
        context: BindingContext::Modal(ModalKind::CommandLogs),
        action: |_| Some(Action::CommandLogsScrollUp),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Up,
        label: "Up",
        short_desc: "scroll up",
        long_desc: "Scroll command logs up",
        context: BindingContext::Modal(ModalKind::CommandLogs),
        action: |_| Some(Action::CommandLogsScrollUp),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('G'),
        label: "G",
        short_desc: "bottom",
        long_desc: "Follow newest command logs",
        context: BindingContext::Modal(ModalKind::CommandLogs),
        action: |_| Some(Action::CommandLogsScrollBottom),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('l'),
        label: "l",
        short_desc: "close",
        long_desc: "Close command logs",
        context: BindingContext::Modal(ModalKind::CommandLogs),
        action: |_| Some(Action::ModalCancel),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('q'),
        label: "q",
        short_desc: "close",
        long_desc: "Close command logs",
        context: BindingContext::Modal(ModalKind::CommandLogs),
        action: |_| Some(Action::ModalCancel),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "close",
        long_desc: "Close command logs",
        context: BindingContext::Modal(ModalKind::CommandLogs),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    // ==== Modal: Info ====
    KeyBinding {
        key: KeyCode::Enter,
        label: "Enter",
        short_desc: "ok",
        long_desc: "Dismiss message",
        context: BindingContext::Modal(ModalKind::Info),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "dismiss",
        long_desc: "Dismiss message",
        context: BindingContext::Modal(ModalKind::Info),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    // ==== Palette: Android ====
    KeyBinding {
        key: KeyCode::Char('r'),
        label: "r",
        short_desc: "run",
        long_desc: "Run Android via UMP script",
        context: BindingContext::Palette(PaletteMode::Android),
        action: |_| {
            Some(Action::CommandRun(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: None,
            }))
        },
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('R'),
        label: "R",
        short_desc: "repeat",
        long_desc: "Repeat last Android run",
        context: BindingContext::Palette(PaletteMode::Android),
        action: repeat_last_android_run,
        visible: has_last_android_run,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Close palette",
        context: BindingContext::Palette(PaletteMode::Android),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    // ==== Palette: iOS ====
    KeyBinding {
        key: KeyCode::Char('p'),
        label: "p",
        short_desc: "pod-install",
        long_desc: "yarn pod-install",
        context: BindingContext::Palette(PaletteMode::Ios),
        action: |_| Some(Action::CommandRun(CommandSpec::YarnPodInstall)),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('r'),
        label: "r",
        short_desc: "run",
        long_desc: "Run iOS via UMP script",
        context: BindingContext::Palette(PaletteMode::Ios),
        action: |_| {
            Some(Action::CommandRun(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: None,
            }))
        },
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('R'),
        label: "R",
        short_desc: "repeat",
        long_desc: "Repeat last iOS run",
        context: BindingContext::Palette(PaletteMode::Ios),
        action: repeat_last_ios_run,
        visible: has_last_ios_run,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Close palette",
        context: BindingContext::Palette(PaletteMode::Ios),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    // ==== Palette: Yarn ====
    KeyBinding {
        key: KeyCode::Char('i'),
        label: "i",
        short_desc: "install",
        long_desc: "yarn install",
        context: BindingContext::Palette(PaletteMode::Yarn),
        action: |_| Some(Action::CommandRun(CommandSpec::YarnInstall)),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('p'),
        label: "p",
        short_desc: "pod-install",
        long_desc: "yarn pod-install",
        context: BindingContext::Palette(PaletteMode::Yarn),
        action: |_| Some(Action::CommandRun(CommandSpec::YarnPodInstall)),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('u'),
        label: "u",
        short_desc: "unit-tests",
        long_desc: "yarn unit-tests",
        context: BindingContext::Palette(PaletteMode::Yarn),
        action: |_| Some(Action::CommandRun(CommandSpec::YarnUnitTests)),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('t'),
        label: "t",
        short_desc: "check-types",
        long_desc: "yarn check-types --incremental",
        context: BindingContext::Palette(PaletteMode::Yarn),
        action: |_| Some(Action::CommandRun(CommandSpec::YarnCheckTypes)),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('j'),
        label: "j",
        short_desc: "jest",
        long_desc: "yarn jest <filter>",
        context: BindingContext::Palette(PaletteMode::Yarn),
        action: |_| {
            Some(Action::CommandRun(CommandSpec::YarnJest {
                filter: String::new(),
            }))
        },
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('l'),
        label: "l",
        short_desc: "lint",
        long_desc: "yarn lint --quiet --fix",
        context: BindingContext::Palette(PaletteMode::Yarn),
        action: |_| Some(Action::CommandRun(CommandSpec::YarnLint)),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('c'),
        label: "c",
        short_desc: "clean…",
        long_desc: "Clean submenu (pods/android/node_modules)",
        context: BindingContext::Palette(PaletteMode::Yarn),
        action: |_| Some(Action::OpenCleanMenu),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Close palette",
        context: BindingContext::Palette(PaletteMode::Yarn),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    // ==== Palette: Git ====
    KeyBinding {
        key: KeyCode::Char('f'),
        label: "f",
        short_desc: "fetch",
        long_desc: "git fetch --all --tags",
        context: BindingContext::Palette(PaletteMode::Git),
        action: |_| Some(Action::CommandRun(CommandSpec::GitFetch)),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('p'),
        label: "p",
        short_desc: "pull",
        long_desc: "git pull",
        context: BindingContext::Palette(PaletteMode::Git),
        action: |_| Some(Action::CommandRun(CommandSpec::GitPull)),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('P'),
        label: "P",
        short_desc: "push",
        long_desc: "git push",
        context: BindingContext::Palette(PaletteMode::Git),
        action: |_| Some(Action::CommandRun(CommandSpec::GitPush)),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('X'),
        label: "X",
        short_desc: "reset hard",
        long_desc: "git fetch + reset --hard origin/<branch>",
        context: BindingContext::Palette(PaletteMode::Git),
        action: |_| Some(Action::CommandRun(CommandSpec::GitResetHardFetch)),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Close palette",
        context: BindingContext::Palette(PaletteMode::Git),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    // ==== Palette: Worktree ====
    KeyBinding {
        key: KeyCode::Char('c'),
        label: "c",
        short_desc: "checkout",
        long_desc: "Checkout worktree",
        context: BindingContext::Palette(PaletteMode::Worktree),
        action: |_| Some(Action::WorktreeAdd),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('n'),
        label: "n",
        short_desc: "new branch",
        long_desc: "New branch + worktree",
        context: BindingContext::Palette(PaletteMode::Worktree),
        action: |_| Some(Action::WorktreeAddNewBranch),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('r'),
        label: "r",
        short_desc: "review",
        long_desc: "Review PR",
        context: BindingContext::Palette(PaletteMode::Worktree),
        action: |_| Some(Action::ReviewOpen),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Close palette",
        context: BindingContext::Palette(PaletteMode::Worktree),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    // ==== Palette: Open ====
    KeyBinding {
        key: KeyCode::Char('c'),
        label: "c",
        short_desc: "claude",
        long_desc: "Open Claude Code (tmux/zellij/Ghostty)",
        context: BindingContext::Palette(PaletteMode::Open),
        action: |_| Some(Action::OpenClaudeCode),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('e'),
        label: "e",
        short_desc: "editor",
        long_desc: "Open configured editor at worktree",
        context: BindingContext::Palette(PaletteMode::Open),
        action: |_| Some(Action::OpenEditor),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('f'),
        label: "f",
        short_desc: "finder",
        long_desc: "Open selected worktree in Finder",
        context: BindingContext::Palette(PaletteMode::Open),
        action: |_| Some(Action::OpenFinder),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('t'),
        label: "t",
        short_desc: "shell tab",
        long_desc: "Open shell tab at worktree",
        context: BindingContext::Palette(PaletteMode::Open),
        action: |_| Some(Action::OpenShellTab),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('j'),
        label: "j",
        short_desc: "debugger",
        long_desc: "Metro debugger (when running)",
        context: BindingContext::Palette(PaletteMode::Open),
        action: |s| {
            if metro_running(s) {
                Some(Action::MetroSendDebugger)
            } else {
                Some(Action::ModalCancel)
            }
        },
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "cancel",
        long_desc: "Close palette",
        context: BindingContext::Palette(PaletteMode::Open),
        action: |_| Some(Action::ModalCancel),
        visible: |_| true,
    },
    // ==== Overlay: Help ====
    KeyBinding {
        key: KeyCode::Char('q'),
        label: "q/Esc",
        short_desc: "close help",
        long_desc: "Close help overlay",
        context: BindingContext::Overlay(OverlayKind::Help),
        action: |_| Some(Action::DismissHelp),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "close help",
        long_desc: "Close help overlay",
        context: BindingContext::Overlay(OverlayKind::Help),
        action: |_| Some(Action::DismissHelp),
        visible: |_| false,
    },
    // ==== Overlay: Error ====
    KeyBinding {
        key: KeyCode::Char('r'),
        label: "r",
        short_desc: "retry",
        long_desc: "Retry last command",
        context: BindingContext::Overlay(OverlayKind::Error),
        action: |_| Some(Action::RetryLastCommand),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('q'),
        label: "q/Esc",
        short_desc: "dismiss",
        long_desc: "Dismiss error",
        context: BindingContext::Overlay(OverlayKind::Error),
        action: |_| Some(Action::DismissError),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "dismiss",
        long_desc: "Dismiss error",
        context: BindingContext::Overlay(OverlayKind::Error),
        action: |_| Some(Action::DismissError),
        visible: |_| false,
    },
    // ==== Worktree filter input ====
    KeyBinding {
        key: KeyCode::Enter,
        label: "Enter",
        short_desc: "apply",
        long_desc: "Apply worktree filter",
        context: BindingContext::WorktreeFilter,
        action: |_| Some(Action::WorktreeFilterApply),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "clear",
        long_desc: "Clear worktree filter",
        context: BindingContext::WorktreeFilter,
        action: |_| Some(Action::WorktreeFilterClear),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Backspace,
        label: "Backspace",
        short_desc: "delete",
        long_desc: "Delete filter character",
        context: BindingContext::WorktreeFilter,
        action: |_| Some(Action::WorktreeFilterBackspace),
        visible: |_| true,
    },
    // ==== Worktree table ====
    KeyBinding {
        key: KeyCode::Char('j'),
        label: "j/k",
        short_desc: "navigate",
        long_desc: "Navigate worktrees",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::WorktreeSelectNext),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Down,
        label: "Down",
        short_desc: "next",
        long_desc: "Next worktree",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::WorktreeSelectNext),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('k'),
        label: "k",
        short_desc: "prev",
        long_desc: "Previous worktree",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::WorktreeSelectPrev),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Up,
        label: "Up",
        short_desc: "prev",
        long_desc: "Previous worktree",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::WorktreeSelectPrev),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('+'),
        label: "+/-",
        short_desc: "worktree",
        long_desc: "Add/remove worktree",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::EnterWorktreePalette),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('-'),
        label: "-",
        short_desc: "remove",
        long_desc: "Remove worktree",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::WorktreeRemove),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('g'),
        label: "g",
        short_desc: "git",
        long_desc: "Git submenu",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::EnterGitPalette),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('y'),
        label: "y",
        short_desc: "yarn",
        long_desc: "Yarn submenu",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::EnterYarnPalette),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('i'),
        label: "i",
        short_desc: "ios",
        long_desc: "iOS submenu",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::EnterIosPalette),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('a'),
        label: "a",
        short_desc: "android",
        long_desc: "Android submenu",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::EnterAndroidPalette),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('o'),
        label: "o",
        short_desc: "open",
        long_desc: "Open submenu",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::EnterOpenPalette),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('R'),
        label: "R",
        short_desc: "reload",
        long_desc: "Reload metro (when running) / Refresh list",
        context: BindingContext::WorktreeTable,
        action: |s| {
            if metro_running(s) {
                Some(Action::MetroSendReload)
            } else {
                Some(Action::RefreshWorktrees)
            }
        },
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Esc,
        label: "Esc",
        short_desc: "stop metro",
        long_desc: "Stop metro (when running)",
        context: BindingContext::WorktreeTable,
        action: |s| {
            if metro_running(s) {
                Some(Action::MetroStop)
            } else {
                None
            }
        },
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('l'),
        label: "l",
        short_desc: "logs",
        long_desc: "Open selected worktree command logs",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::OpenCommandLogs),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('X'),
        label: "X",
        short_desc: "cancel",
        long_desc: "Cancel selected worktree command",
        context: BindingContext::WorktreeTable,
        action: |s| selected_task_cancellable(s).then_some(Action::CommandCancel),
        visible: selected_task_cancellable,
    },
    KeyBinding {
        key: KeyCode::Enter,
        label: "Enter",
        short_desc: "metro",
        long_desc: "Use selected worktree for Metro",
        context: BindingContext::WorktreeTable,
        action: |_| Some(Action::WorktreeSwitchToSelected),
        visible: |_| false,
    },
    // ==== Global bindings ====
    KeyBinding {
        key: KeyCode::Char('q'),
        label: "q",
        short_desc: "quit",
        long_desc: "Quit the application",
        context: BindingContext::Normal,
        action: |_| Some(Action::Quit),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::Char('?'),
        label: "?/F1",
        short_desc: "help",
        long_desc: "Open keybinding help",
        context: BindingContext::Normal,
        action: |_| Some(Action::ShowHelp),
        visible: |_| true,
    },
    KeyBinding {
        key: KeyCode::F(1),
        label: "F1",
        short_desc: "help",
        long_desc: "Open keybinding help",
        context: BindingContext::Normal,
        action: |_| Some(Action::ShowHelp),
        visible: |_| false,
    },
    KeyBinding {
        key: KeyCode::Char('/'),
        label: "/",
        short_desc: "filter",
        long_desc: "Filter worktrees",
        context: BindingContext::Normal,
        action: |_| Some(Action::Search),
        visible: |_| false,
    },
];

// ---------------------------------------------------------------------------
// Action closures that need AppState introspection.
// ---------------------------------------------------------------------------

fn external_metro_kill_action(state: &AppState) -> Option<Action> {
    match state.modal_stack.modal {
        Some(ModalState::ExternalMetroConflict { pid, .. }) => Some(Action::KillExternalMetro(pid)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Visibility helpers.
// ---------------------------------------------------------------------------

fn metro_running(state: &AppState) -> bool {
    state
        .worktree_browser
        .selected_worktree()
        .and_then(|wt| state.worktrees.get(&wt.id))
        .map(|slice| slice.metro.is_running())
        .unwrap_or(false)
}

fn selected_task_cancellable(state: &AppState) -> bool {
    active_worktree_id(state)
        .and_then(|id| state.worktrees.get(&id))
        .and_then(|slice| slice.task.as_ref())
        .is_some_and(|task| task.spec.is_cancellable())
}

// ---------------------------------------------------------------------------
// Context matching.
// ---------------------------------------------------------------------------

pub fn context_matches(ctx: &BindingContext, state: &AppState) -> bool {
    // Overlays take precedence over everything else.
    let in_overlay = state.show_help || state.error_state.is_some();
    let in_modal = state.modal_stack.modal.is_some();
    let in_palette = state.modal_stack.palette_mode.is_some();
    let in_worktree_filter = state.worktree_browser.filter_input_active;
    match ctx {
        BindingContext::Always => true,
        BindingContext::Normal | BindingContext::WorktreeTable => {
            !in_modal && !in_palette && !in_overlay && !in_worktree_filter
        }
        BindingContext::WorktreeFilter => {
            !in_modal && !in_palette && !in_overlay && in_worktree_filter
        }
        BindingContext::Palette(p) => {
            !in_modal && !in_overlay && state.modal_stack.palette_mode.as_ref() == Some(p)
        }
        BindingContext::Modal(k) => matches_modal_kind(state.modal_stack.modal.as_ref(), *k),
        BindingContext::Overlay(OverlayKind::Help) => state.show_help,
        BindingContext::Overlay(OverlayKind::Error) => {
            !state.show_help && state.error_state.is_some()
        }
    }
}

fn matches_modal_kind(modal: Option<&ModalState>, kind: ModalKind) -> bool {
    matches!(
        (modal, kind),
        (Some(ModalState::Confirm { .. }), ModalKind::Confirm)
            | (Some(ModalState::TextInput { .. }), ModalKind::TextInput)
            | (
                Some(ModalState::DevicePicker { .. }),
                ModalKind::DevicePicker
            )
            | (
                Some(ModalState::RunVariantPicker { .. }),
                ModalKind::RunVariantPicker
            )
            | (Some(ModalState::CleanToggle { .. }), ModalKind::CleanToggle)
            | (
                Some(ModalState::SyncBeforeRun { .. }),
                ModalKind::SyncBeforeRun
            )
            | (
                Some(ModalState::SyncBeforeMetro { .. }),
                ModalKind::SyncBeforeMetro
            )
            | (
                Some(ModalState::ExternalMetroConflict { .. }),
                ModalKind::ExternalMetroConflict
            )
            | (
                Some(ModalState::BranchPicker { .. }),
                ModalKind::BranchPicker
            )
            | (
                Some(ModalState::PullRequestPicker { .. }),
                ModalKind::PullRequestPicker
            )
            | (Some(ModalState::CommandLogs { .. }), ModalKind::CommandLogs)
            | (Some(ModalState::Info { .. }), ModalKind::Info)
    )
}

// ---------------------------------------------------------------------------
// Palette-specific action closures (moved to fns since const fn pointers
// cannot capture state).
// ---------------------------------------------------------------------------

fn active_last_run_config(
    state: &AppState,
    pick: fn(&crate::domain::worktree_slice::WorktreeSlice) -> Option<&LastRunConfig>,
) -> Option<&LastRunConfig> {
    let id = active_worktree_id(state)?;
    let slice = state.worktrees.get(&id)?;
    pick(slice)
}

fn last_android_run(state: &AppState) -> Option<&LastRunConfig> {
    active_last_run_config(state, |slice| slice.last_android_run.as_ref())
}

fn last_ios_run(state: &AppState) -> Option<&LastRunConfig> {
    active_last_run_config(state, |slice| slice.last_ios_run.as_ref())
}

fn has_last_android_run(state: &AppState) -> bool {
    last_android_run(state).is_some()
}

fn has_last_ios_run(state: &AppState) -> bool {
    last_ios_run(state).is_some()
}

fn repeat_last_android_run(state: &AppState) -> Option<Action> {
    Some(
        last_android_run(state).map_or(Action::ModalCancel, |config| {
            Action::CommandRun(CommandSpec::UmpRunAndroid {
                device_id: config.device_id.clone(),
                variant: Some(config.variant),
            })
        }),
    )
}

fn repeat_last_ios_run(state: &AppState) -> Option<Action> {
    Some(
        last_ios_run(state).map_or(Action::ModalCancel, |config| {
            Action::CommandRun(CommandSpec::UmpRunIos {
                device_id: config.device_id.clone(),
                variant: Some(config.variant),
            })
        }),
    )
}

// ---------------------------------------------------------------------------
// Consumer helpers used by Plan 13-10 (footer.rs / help_overlay.rs rewrites).
// ---------------------------------------------------------------------------

/// Returns the set of (label, short_desc) pairs relevant to the current
/// context. De-duplicates on label so that aliases (j/k for up/down) collapse
/// to a single hint row.
pub fn footer_hints_for(state: &AppState) -> Vec<(&'static str, &'static str)> {
    let mut seen_labels: Vec<&'static str> = Vec::new();
    let mut hints: Vec<(&'static str, &'static str)> = Vec::new();
    for kb in KEYBINDINGS.iter() {
        if !context_matches(&kb.context, state) {
            continue;
        }
        if !(kb.visible)(state) {
            continue;
        }
        if seen_labels.contains(&kb.label) {
            continue;
        }
        seen_labels.push(kb.label);
        hints.push((kb.label, kb.short_desc));
    }
    hints
}

/// Returns rows for the help overlay grouped by section (context-derived).
/// Plan 13-10's `help_overlay::render_help` consumes this.
pub fn help_overlay_rows() -> Vec<HelpRow> {
    let mut rows: Vec<HelpRow> = Vec::new();
    let mut seen: Vec<(&'static str, &'static str)> = Vec::new();
    for kb in KEYBINDINGS.iter() {
        let section = section_for_context(&kb.context);
        if seen.contains(&(section, kb.label)) {
            continue;
        }
        seen.push((section, kb.label));
        rows.push(HelpRow {
            section,
            label: kb.label,
            desc: kb.long_desc,
        });
    }
    rows
}

fn section_for_context(ctx: &BindingContext) -> &'static str {
    match ctx {
        BindingContext::Always | BindingContext::Normal => "Global",
        BindingContext::WorktreeTable => "Worktree Table",
        BindingContext::WorktreeFilter => "Worktree Filter  (/)",
        BindingContext::Palette(PaletteMode::Android) => "Android  (a>)",
        BindingContext::Palette(PaletteMode::Ios) => "iOS  (i>)",
        BindingContext::Palette(PaletteMode::Yarn) => "Yarn  (y>)",
        BindingContext::Palette(PaletteMode::Git) => "Git  (g>)",
        BindingContext::Palette(PaletteMode::Worktree) => "Worktree  (+>)",
        BindingContext::Palette(PaletteMode::Open) => "Open  (o>)",
        BindingContext::Modal(_) => "Modal",
        BindingContext::Overlay(_) => "Overlay",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keybindings_entry_count_meets_lower_bound() {
        // Plan 13-07 expects ≥60 entries (conservative lower bound; ~80 expected).
        assert!(
            KEYBINDINGS.len() >= 60,
            "KEYBINDINGS has {} entries; expected at least 60",
            KEYBINDINGS.len()
        );
    }

    #[test]
    fn every_palette_has_escape() {
        // Each of the 5 palettes must have an Esc binding so the walker finds a
        // close action even before the post-loop fallback (Pitfall 4).
        for palette in [
            PaletteMode::Android,
            PaletteMode::Ios,
            PaletteMode::Yarn,
            PaletteMode::Git,
            PaletteMode::Worktree,
            PaletteMode::Open,
        ] {
            let has_esc = KEYBINDINGS.iter().any(|kb| {
                matches!(kb.context, BindingContext::Palette(ref p) if std::mem::discriminant(p) == std::mem::discriminant(&palette))
                    && kb.key == KeyCode::Esc
            });
            assert!(has_esc, "palette {palette:?} missing Esc binding");
        }
    }
}
