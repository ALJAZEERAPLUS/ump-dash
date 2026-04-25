//! Help overlay — KEYBINDINGS-driven keybinding table + hand-coded Icons legend.
//!
//! Plan 13-10 (F-303) closed: the ~100 lines of hand-coded `Vec<Row>`
//! keybinding tables that used to live here were replaced by a delegation to
//! `crate::app::keybindings::help_overlay_rows()`. Rows are grouped by the
//! `section` field on each `HelpRow` and a bold header row is inserted on
//! every section transition.
//!
//! The Icons legend at the bottom STAYS hand-coded — icons are not
//! keybindings (per AUDIT F-303 recommendation). They live below the
//! keybinding table and are appended after the registry-driven rows.

use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Row, Table},
    Frame,
};

use crate::app::keybindings::help_overlay_rows;

/// Renders the help overlay. Called from view() when state.show_help == true.
/// Uses Clear widget before the table to erase background panels behind the overlay.
/// Size: 70% width, 85% height to accommodate all keybinding sections.
pub fn render_help(f: &mut Frame) {
    let area = centered_rect(f.area(), 70, 85, 40, 10);

    let section_style = Style::default().add_modifier(Modifier::BOLD);
    let dim_style = Style::default().fg(Color::DarkGray);

    // Keybinding rows — read from the KEYBINDINGS registry. Group by section
    // and insert a bold header on every section transition.
    let rows_data = help_overlay_rows();
    let mut rendered: Vec<Row> = Vec::new();
    let mut current_section: &str = "";
    for hr in &rows_data {
        if hr.section != current_section {
            if !current_section.is_empty() {
                // Spacer row between sections.
                rendered.push(Row::new(vec!["", ""]).style(dim_style));
            }
            rendered.push(Row::new(vec![hr.section, ""]).style(section_style));
            current_section = hr.section;
        }
        rendered.push(Row::new(vec![hr.label, hr.desc]));
    }

    // Icons legend section — STAYS hand-coded per AUDIT F-303 (icons are not
    // keybindings — they describe table-cell glyphs in the worktree row).
    rendered.push(Row::new(vec!["", ""]).style(dim_style));
    rendered.push(Row::new(vec!["Icons", ""]).style(section_style));
    rendered.push(Row::new(vec!["\u{25B6}  (green)", "Metro is running"]));
    rendered.push(Row::new(vec![
        "\u{26A0}  (yellow)",
        "Stale dependencies (node_modules/pods)",
    ]));

    let table = Table::new(rendered, [Constraint::Length(28), Constraint::Fill(1)])
        .block(
            Block::default()
                .title(" Keybindings — q/Esc to close ")
                .borders(Borders::ALL),
        );

    // Clear MUST be rendered before the table — otherwise background panels show through
    f.render_widget(Clear, area);
    f.render_widget(table, area);
}

/// Computes a centered Rect within `area`, using percentage sizing with minimum dimensions.
/// Width is clamped to [min_w, area.width], height to [min_h, area.height].
fn centered_rect(area: Rect, percent_x: u16, percent_y: u16, min_w: u16, min_h: u16) -> Rect {
    let w = (area.width * percent_x / 100).clamp(min_w, area.width);
    let h = (area.height * percent_y / 100).clamp(min_h, area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}
