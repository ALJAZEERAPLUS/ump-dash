use ratatui::{
    layout::{Constraint, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Cell, Paragraph, Row, Scrollbar,
        ScrollbarOrientation,
        ScrollbarState, Table,
    },
    Frame,
};
use crate::{
    app::{AppState, FocusedPanel},
    domain::worktree::WorktreeMetroStatus,
    ui::theme,
};

/// Renders the application title bar with double border.
/// Only shown in normal (non-fullscreen) layout.
#[allow(dead_code)]
pub fn render_title_bar(f: &mut Frame, area: Rect, state: &AppState) {
    let title = state.config.as_ref()
        .map(|c| c.app_title.as_str())
        .unwrap_or("RN Dash");
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .title(format!(" {title} "))
        .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
    f.render_widget(block, area);
}

/// Truncates a string to max_width, appending "..." if truncated.
fn truncate(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        return s.to_string();
    }
    if max_width <= 3 {
        return s[..max_width].to_string();
    }
    format!("{}...", &s[..max_width - 3])
}

/// Renders the worktree table (bottom section) with structured columns.
pub fn render_worktree_table(f: &mut Frame, area: Rect, state: &mut AppState) {
    let border_style = if state.focused_panel == FocusedPanel::WorktreeTable {
        theme::style_focused_border()
    } else {
        theme::style_inactive_border()
    };

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .title(" Worktrees ")
        .title_style(Style::default().fg(Color::White))
        .border_style(border_style);

    if state.worktrees.is_empty() {
        let placeholder = Paragraph::new("Loading worktrees...").block(block);
        f.render_widget(placeholder, area);
        return;
    }

    let mut rows: Vec<Row> = Vec::new();
    // Track visual row indices that are detail rows (not selectable worktree rows).
    let mut detail_row_indices: Vec<usize> = Vec::new();

    for wt in state.worktrees.iter() {
        let branch = &wt.branch;

        // Extract ticket number from branch if possible
        let ticket_num = crate::domain::jira::extract_jira_key(branch, &state.jira_project_prefix).unwrap_or_default();
        let title = wt.jira_title.as_deref().unwrap_or("");

        // Merged ticket display: "UMP-1234 Title text" or just one or the other
        let ticket_display = match (ticket_num.is_empty(), title.is_empty()) {
            (false, false) => format!("{ticket_num} {title}"),
            (false, true) => ticket_num,
            (true, false) => title.to_string(),
            (true, true) => String::new(),
        };

        // Status icons: always show Y (yarn) and /P (pods) with color indicating freshness
        let mut icon_spans: Vec<Span> = Vec::new();

        // Metro indicator: play icon when running, space placeholder when not
        if wt.metro_status == WorktreeMetroStatus::Running {
            icon_spans.push(Span::styled("\u{25B6} ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
        } else {
            icon_spans.push(Span::raw("  "));
        }

        // Yarn staleness: Y always shown, green=fresh, red=stale
        let yarn_color = if wt.stale { Color::Red } else { Color::Green };
        icon_spans.push(Span::styled("Y", Style::default().fg(yarn_color)));

        // Pods staleness: /P always shown, green=fresh, red=stale
        let pods_color = if wt.stale_pods { Color::Red } else { Color::Green };
        icon_spans.push(Span::styled("/P", Style::default().fg(pods_color)));

        let row_style = if wt.metro_status == WorktreeMetroStatus::Running {
            Style::default()
                .bg(Color::Rgb(0, 60, 0))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        // Extract dir name from path
        let dir_name = wt.path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        rows.push(Row::new(vec![
            Cell::from(Line::from(icon_spans)),
            Cell::from(truncate(branch, 18)),
            Cell::from(ticket_display),
            Cell::from(dir_name),
        ])
        .style(row_style));

        // If this worktree is running metro and we have activity info, add a detail row
        if wt.metro_status == WorktreeMetroStatus::Running
            && let Some(ref activity) = state.metro.activity {
                let detail_row = Row::new(vec![
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(Span::styled(
                        format!("\u{2502} {activity}"),
                        Style::default().fg(Color::Cyan),
                    )),
                    Cell::from(""),
                ])
                .style(Style::default().bg(Color::Rgb(0, 60, 0)));
                detail_row_indices.push(rows.len());
                rows.push(detail_row);
        }
    }

    // Use green highlight when the selected row is metro-active, gray otherwise
    let selected_idx = state.worktree_table_state.selected().unwrap_or(0);
    let selected_is_metro = state.worktrees.get(selected_idx)
        .map(|wt| wt.metro_status == WorktreeMetroStatus::Running)
        .unwrap_or(false);

    let highlight_style = if selected_is_metro {
        Style::default()
            .bg(Color::Rgb(0, 80, 0))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),  // Status icons (metro + Y + /P)
            Constraint::Length(20), // Branch
            Constraint::Min(20),   // Ticket (merged number + title)
            Constraint::Length(16), // Dir
        ],
    )
    .block(block)
    .row_highlight_style(highlight_style)
    .highlight_symbol("> ");

    // Map logical selection index to visual index (offset by any detail rows inserted before it)
    if let Some(logical) = state.worktree_table_state.selected() {
        let mut visual = logical;
        for &detail_idx in &detail_row_indices {
            if detail_idx <= visual {
                visual += 1;
            }
        }
        if visual != logical {
            state.worktree_table_state.select(Some(visual));
        }
    }

    f.render_stateful_widget(table, area, &mut state.worktree_table_state);

    // Restore logical index after render so app state stays consistent
    if let Some(visual) = state.worktree_table_state.selected() {
        let mut logical = visual;
        for &detail_idx in detail_row_indices.iter().rev() {
            if detail_idx <= logical && logical > 0 {
                logical -= 1;
            }
        }
        if logical != visual {
            state.worktree_table_state.select(Some(logical));
        }
    }
}

/// Renders the command output pane (full top area) with real streaming output.
pub fn render_command_output(f: &mut Frame, area: Rect, state: &AppState) {
    let border_style = if state.focused_panel == FocusedPanel::CommandOutput {
        theme::style_focused_border()
    } else {
        theme::style_inactive_border()
    };

    // Title shows running command name, [running] indicator, and queue count
    let title = match &state.running_command {
        Some(spec) => {
            let queue_count = state.command_queue.len();
            if queue_count > 0 {
                format!(" Output — {} [running] (+{} queued) ", spec.label(), queue_count)
            } else {
                format!(" Output — {} [running] ", spec.label())
            }
        }
        None => {
            let queue_count = state.command_queue.len();
            if queue_count > 0 {
                format!(" Output ({queue_count} queued) ")
            } else {
                " Output ".to_string()
            }
        }
    };

    let lines: Vec<Line> = crate::app::active_output(state)
        .iter()
        .map(|l| Line::from(l.as_str()))
        .collect();

    let visible_height = area.height.saturating_sub(2) as usize; // subtract borders

    // Auto-scroll to bottom when scroll is 0 (default); manual scroll overrides
    let scroll_offset = crate::app::active_output_scroll(state);
    let scroll = if scroll_offset == 0 && !lines.is_empty() {
        lines.len().saturating_sub(visible_height)
    } else {
        scroll_offset
    };

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .title(title)
        .title_style(Style::default().fg(Color::White))
        .border_style(border_style);

    let paragraph = Paragraph::new(Text::from(lines.clone()))
        .block(block)
        .scroll((scroll as u16, 0));

    f.render_widget(paragraph, area);

    // Scrollbar — only rendered when content exceeds visible area
    if lines.len() > visible_height {
        let max_scroll = lines.len() - visible_height;
        let mut scrollbar_state = ScrollbarState::new(max_scroll).position(scroll.min(max_scroll));

        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}
