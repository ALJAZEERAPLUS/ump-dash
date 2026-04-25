//! UI layer — ratatui widgets, rendering, layout.
//! Imports: domain types and ratatui ONLY. Never imports infra directly.
//!
//! view() is the single render entry point called from app::run().
//! It accepts &mut AppState because render_stateful_widget requires &mut TableState.

pub mod footer;
pub mod help_overlay;
pub mod error_overlay;
pub mod modals;
pub mod panels;
pub mod theme;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use crate::app::{AppState, FocusedPanel};

/// Root render function. Called on every draw cycle from app::run().
/// Layout: top (command output) | bottom (worktree table) | footer
/// Fullscreen mode: single panel + footer, activated by ToggleFullscreen.
/// Overlays: rendered last so they layer on top of all base content.
pub fn view(f: &mut Frame, state: &mut AppState) {
    let area = f.area();

    // Fullscreen mode — render only the fullscreened panel + footer
    if let Some(panel) = state.worktree_browser.fullscreen_panel {
        let [main_area, footer_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .areas(area);

        if panel == FocusedPanel::CommandOutput {
            panels::render_command_output(f, main_area, state);
        }

        footer::render_footer(f, footer_area, state);

        // Overlays on top
        if state.show_help {
            help_overlay::render_help(f);
        }
        if let Some(ref error) = state.error_state {
            error_overlay::render_error(f, error);
        }
        if let Some(ref modal) = state.modal_stack.modal {
            modals::render_modal(f, modal);
        }
        return;
    }

    // Normal layout: top (command output) | bottom (worktree table) | footer
    let table_height = (state.worktree_browser.worktrees.len() as u16 + 3).max(5); // rows + borders + header
    let [top_area, table_area, footer_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),               // command output (flexible)
            Constraint::Length(table_height), // worktree table (fixed)
            Constraint::Length(1),            // footer
        ])
        .areas(area);

    // Top section: command output (full width)
    panels::render_command_output(f, top_area, state);

    // Bottom section: worktree table
    panels::render_worktree_table(f, table_area, state);

    // Footer
    footer::render_footer(f, footer_area, state);

    // Overlays last
    if state.show_help {
        help_overlay::render_help(f);
    }
    if let Some(ref error) = state.error_state {
        error_overlay::render_error(f, error);
    }
    if let Some(ref modal) = state.modal_stack.modal {
        modals::render_modal(f, modal);
    }
}
