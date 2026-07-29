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
pub mod indicators;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use crate::app::AppState;

/// Root render function. Called on every draw cycle from app::run().
/// Layout: worktree table | footer.
/// Overlays: rendered last so they layer on top of all base content.
pub fn view(f: &mut Frame, state: &mut AppState) {
    let area = f.area();

    let [table_area, footer_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .areas(area);

    panels::render_worktree_table(f, table_area, state);
    footer::render_footer(f, footer_area, state);

    // Overlays last
    if state.show_help {
        help_overlay::render_help(f);
    }
    if let Some(ref error) = state.error_state {
        error_overlay::render_error(f, error);
    }
    if let Some(ref modal) = state.modal_stack.modal {
        modals::render_modal(f, modal, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        metro::MetroActivity,
        worktree::{Worktree, WorktreeId, WorktreeMetroStatus},
        worktree_slice::WorktreeSlice,
    };
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn primary_view_renders_table_activity_without_retained_command_logs() {
        let worktree_id = WorktreeId("wt-1".into());
        let mut state = AppState::default();
        state.worktree_browser.worktrees.push(Worktree {
            id: worktree_id.clone(),
            path: "/tmp/wt-1".into(),
            branch: "feature/table-only".into(),
            head_sha: "0000000".into(),
            metro_status: WorktreeMetroStatus::Running,
            jira_title: None,
            stale: false,
            stale_pods: false,
            jira_key: None,
        });
        let mut slice = WorktreeSlice {
            id: worktree_id.clone(),
            ..Default::default()
        };
        slice.metro.reserve_start(8081);
        slice.metro.record_activity(MetroActivity::Ready);
        slice.append_log("RETAINED_LOG_MUST_NOT_RENDER");
        state.worktrees.insert(worktree_id, slice);

        let backend = TestBackend::new(100, 15);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| view(frame, &mut state))
            .expect("primary view should render");
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("feature/table-only"));
        assert!(rendered.contains("Ready"));
        assert!(rendered.contains("8081"));
        assert!(!rendered.contains("RETAINED_LOG_MUST_NOT_RENDER"));
        assert!(!rendered.contains("Output"));
    }

    #[test]
    fn command_logs_modal_opens_at_bottom_and_can_render_older_lines() {
        let worktree_id = WorktreeId("wt-logs".into());
        let mut state = AppState::default();
        state.worktree_browser.worktrees.push(Worktree {
            id: worktree_id.clone(),
            path: "/tmp/wt-logs".into(),
            branch: "feature/log-modal".into(),
            head_sha: "0000000".into(),
            metro_status: WorktreeMetroStatus::Stopped,
            jira_title: None,
            stale: false,
            stale_pods: false,
            jira_key: None,
        });
        state
            .worktree_browser
            .worktree_table_state
            .select(Some(0));
        let mut slice = WorktreeSlice {
            id: worktree_id.clone(),
            ..Default::default()
        };
        for index in 0..30 {
            slice.append_log(format!("modal-log-{index:02}"));
        }
        state.worktrees.insert(worktree_id.clone(), slice);
        state.modal_stack.modal = Some(crate::domain::command::ModalState::CommandLogs {
            worktree_id,
            scroll_from_bottom: 0,
        });

        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| view(frame, &mut state))
            .expect("log modal should render");
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Logs — feature/log-modal (30 lines)"));
        assert!(rendered.contains("modal-log-29"));
        assert!(!rendered.contains("modal-log-00"));

        if let Some(crate::domain::command::ModalState::CommandLogs {
            scroll_from_bottom,
            ..
        }) = state.modal_stack.modal.as_mut()
        {
            *scroll_from_bottom = 30;
        }
        terminal
            .draw(|frame| view(frame, &mut state))
            .expect("scrolled log modal should render");
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("modal-log-00"));
        assert!(!rendered.contains("modal-log-29"));
    }
}
