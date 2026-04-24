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

use super::keybindings::{context_matches, KEYBINDINGS};
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

    // Fallthrough 1: modal char-consumers. TextInput/DevicePicker/BranchPicker
    // accept arbitrary chars as input or filter text. The registry's explicit
    // entries handled the non-char keys; now route any remaining Char(c).
    if let Some(ref modal) = state.modal {
        match (modal, key.code) {
            (ModalState::TextInput { .. }, KeyCode::Char(c)) => {
                return Some(Action::ModalInputChar(c));
            }
            (ModalState::DevicePicker { .. }, KeyCode::Char(c)) if !c.is_ascii_control() => {
                // Note: 'j' and 'k' are registered as navigation keys — the registry
                // walk above caught those. Any other char falls through here and
                // becomes filter input.
                return Some(Action::ModalInputChar(c));
            }
            (ModalState::BranchPicker { .. }, KeyCode::Char(c)) => {
                return Some(Action::BranchPickerFilter(c));
            }
            _ => {}
        }
    }

    // Fallthrough 2: palette context-level close (Pitfall 4). Any unbound key
    // while a palette is open closes the palette.
    if state.palette_mode.is_some() {
        return Some(Action::ModalCancel);
    }

    None
}
