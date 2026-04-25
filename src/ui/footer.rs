use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use crate::{app::{AppState, FocusedPanel, PaletteMode}, domain::command::ModalState, ui::theme};

/// Renders the footer key hint bar. Always 1 line tall at the bottom of the layout.
/// Hints change based on which panel is focused and current app state — satisfies SHELL-02.
/// Dynamic: metro hints (R/J/Esc) only shown when metro is running. No static legend.
pub fn render_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let hints = key_hints_for(state);

    // Build hint spans (full-width, no legend)
    let hint_spans: Vec<Span> = hints.iter().flat_map(|(key, desc)| {
        vec![
            Span::styled(*key, theme::style_key_hint()),
            Span::raw(format!(" {desc}  ")),
        ]
    }).collect();

    let hint_line = Paragraph::new(Line::from(hint_spans));
    f.render_widget(hint_line, area);
}

/// Returns context-sensitive key hints for the current app state.
/// Priority order: palette mode → modal state → overlay modes → panel-specific hints.
fn key_hints_for(state: &AppState) -> Vec<(&'static str, &'static str)> {
    if state.show_help {
        return vec![("q/Esc", "close help")];
    }
    if state.error_state.is_some() {
        return vec![("r", "retry"), ("q/Esc", "dismiss")];
    }

    // Palette mode hints — checked before panel hints
    if let Some(ref mode) = state.modal_stack.palette_mode {
        return match mode {
            PaletteMode::Android => vec![
                ("d", "run-android"),
                ("e", "device list"),
                ("r", "release build"),
                ("m", "set mode"),
                ("Esc", "cancel"),
            ],
            PaletteMode::Ios => vec![
                ("d", "run-ios --device"),
                ("e", "simulator list"),
                ("p", "pod-install"),
                ("Esc", "cancel"),
            ],
            PaletteMode::Yarn => vec![
                ("i", "install"),
                ("p", "pod-install"),
                ("u", "unit-tests"),
                ("t", "check-types"),
                ("j", "jest"),
                ("l", "lint"),
                ("c", "clean…"),
                ("Esc", "cancel"),
            ],
            PaletteMode::Git => vec![
                ("f", "fetch"),
                ("p", "pull"),
                ("P", "push"),
                ("X", "reset hard"),
                ("b", "checkout"),
                ("c", "checkout -b"),
                ("r", "rebase"),
                ("Esc", "cancel"),
            ],
            PaletteMode::Worktree => vec![
                ("w", "add worktree"),
                ("d", "remove worktree"),
                ("b", "new branch worktree"),
                ("Esc", "cancel"),
            ],
        };
    }

    // Modal hints — checked after palette, before panel hints
    if let Some(ref modal) = state.modal_stack.modal {
        return match modal {
            ModalState::Confirm { .. } => vec![("Y", "confirm"), ("N/Esc", "cancel")],
            ModalState::TextInput { .. } => vec![("Enter", "submit"), ("Esc", "cancel")],
            ModalState::DevicePicker { .. } => {
                vec![("Enter", "select"), ("j/k", "navigate"), ("Esc", "cancel")]
            }
            ModalState::CleanToggle { .. } => vec![
                ("n", "node_modules"),
                ("p", "pods"),
                ("a", "android"),
                ("i", "sync after"),
                ("x/Enter", "confirm"),
                ("Esc", "cancel"),
            ],
            ModalState::SyncBeforeRun { .. } => vec![
                ("Y", "sync first"),
                ("N", "skip sync"),
                ("Esc", "cancel"),
            ],
            ModalState::SyncBeforeMetro { .. } => vec![
                ("Y", "sync first"),
                ("N", "skip sync"),
                ("Esc", "cancel"),
            ],
            ModalState::ExternalMetroConflict { .. } => vec![
                ("Y", "kill it"),
                ("N/Esc", "cancel"),
            ],
            ModalState::BranchPicker { .. } => vec![
                ("Enter", "select"),
                ("Up/Down", "navigate"),
                ("type", "filter"),
                ("Esc", "cancel"),
            ],
        };
    }

    // Common hints always shown in normal mode
    let common: Vec<(&str, &str)> = vec![("?/F1", "help"), ("q", "quit"), ("Tab", "panel")];

    let panel_hints: Vec<(&str, &str)> = match state.focused_panel {
        FocusedPanel::WorktreeTable => {
            let mut hints: Vec<(&'static str, &'static str)> = vec![
                ("a", "android"),
                ("i", "ios"),
                ("y", "yarn"),
                ("w", "worktree"),
                ("g", "git"),
            ];
            // Context-sensitive metro keys — only shown when metro is running (KEY-04)
            if state.metro.is_running() {
                hints.push(("R", "reload"));
                hints.push(("J", "debugger"));
                hints.push(("Esc", "stop metro"));
            }
            hints.push(("C", "claude"));
            hints.push(("T", "shell tab"));
            hints.push(("!", "shell"));
            hints.push(("Enter", "switch"));
            hints
        },
        FocusedPanel::CommandOutput => {
            let mut hints: Vec<(&'static str, &'static str)> = vec![
                ("j/k", "scroll"),
                ("f", "fullscreen"),
            ];
            if state.command_runner.running_command.is_some() {
                hints.push(("X", "cancel"));
            }
            hints
        },
    };

    // Panel hints first, then common hints
    let mut all = panel_hints;
    all.extend(common);
    all
}
