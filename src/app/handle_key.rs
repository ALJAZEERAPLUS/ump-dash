//! Keyboard dispatch — pure function mapping (AppState, KeyEvent) -> Option<Action>.
//!
//! Plan 13-07 (F-208 + F-400 type half): the hard-coded match arms are
//! replaced by a walk over the `KEYBINDINGS` registry. The walker filters
//! bindings by `context_matches(&kb.context, state)` and returns the first
//! match's action.
//!
//! Two post-loop branches handle fallthroughs that cannot be expressed as
//! single KeyCode entries in the registry:
//!
//! 1. **Modal type-to-fill**: `TextInput`, `DevicePicker`, and `BranchPicker`
//!    modals accept arbitrary printable chars as filter/input text. The
//!    registry entries cover the fixed keys (Enter/Esc/arrows/Backspace); the
//!    post-loop branch forwards any other `Char(c)` to the appropriate
//!    ModalInputChar / BranchPickerFilter action.
//!
//! 2. **Palette context-fallback** (13-RESEARCH.md Pitfall 4): when a palette
//!    is open and no registered key matched, close the palette with
//!    `Action::ModalCancel`. Preserves the `_ => Some(Action::ModalCancel)`
//!    behavior of the pre-registry palette match arms.

use super::keybindings::{KEYBINDINGS, context_matches};
use super::state::AppState;
use crate::domain::action::Action;
use crate::domain::command::ModalState;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

/// Pure function: maps (state, key) → Action. No side effects.
/// Called from the event loop — keep it fast and allocation-free.
pub fn handle_key(state: &AppState, key: KeyEvent) -> Option<Action> {
    // Guard: only process key-press events (prevents double-firing on Windows)
    if key.kind != KeyEventKind::Press {
        return None;
    }

    // Primary dispatch: walk the KEYBINDINGS registry.
    for kb in KEYBINDINGS.iter() {
        if context_matches(&kb.context, state) && kb.key == key.code {
            return (kb.action)(state);
        }
    }

    // Fallthrough 1: modal char-consumers. TextInput/DevicePicker/BranchPicker/PR picker
    // accept arbitrary chars as input or filter text. The registry's explicit
    // entries handled the non-char keys; now route any remaining Char(c).
    //
    // Plan 13-09 (F-205): the outer match pivots on ModalState (exhaustive)
    // so Rust's exhaustiveness checker guards future ModalState additions —
    // the prior wildcard catch-all is gone. The inner if-let on KeyCode is
    // intentionally NOT exhaustive: KeyCode has dozens of variants
    // (function keys, modifiers, paste markers, etc.) and we only want
    // Char(c) here; everything else falls through to the post-match logic.
    if let Some(ref modal) = state.modal_stack.modal {
        match modal {
            ModalState::TextInput { .. } => {
                if let KeyCode::Char(c) = key.code {
                    return Some(Action::ModalInputChar(c));
                }
            }
            ModalState::DevicePicker { .. } => {
                if let KeyCode::Char(c) = key.code
                    && !c.is_ascii_control()
                {
                    // Note: 'j' and 'k' are registered as navigation keys
                    // — the registry walk above caught those. Any other
                    // char falls through here and becomes filter input.
                    return Some(Action::ModalInputChar(c));
                }
            }
            ModalState::BranchPicker { .. } => {
                if let KeyCode::Char(c) = key.code {
                    return Some(Action::BranchPickerFilter(c));
                }
            }
            ModalState::PullRequestPicker { .. } => {
                if let KeyCode::Char(c) = key.code
                    && !c.is_ascii_control()
                {
                    return Some(Action::PullRequestPickerFilter(c));
                }
            }
            // Intentionally no fallthrough — these modals do not consume
            // arbitrary char input here. (Their dismiss/confirm keys are
            // routed through the KEYBINDINGS registry above.)
            ModalState::Confirm { .. }
            | ModalState::RunVariantPicker { .. }
            | ModalState::CleanToggle { .. }
            | ModalState::SyncBeforeRun { .. }
            | ModalState::SyncBeforeMetro { .. }
            | ModalState::ExternalMetroConflict { .. }
            | ModalState::CommandLogs { .. }
            | ModalState::Info { .. } => { /* no-op */ }
        }
    }

    // Fallthrough 2: palette context-level close (Pitfall 4). Any unbound key
    // while a palette is open closes the palette.
    if state.modal_stack.palette_mode.is_some() {
        return Some(Action::ModalCancel);
    }

    None
}
