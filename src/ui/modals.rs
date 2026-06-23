//! Modal overlay renderers. Each modal type uses Clear + centered_rect pattern.
//! render_modal() is the single dispatch point called from view().

use crate::domain::command::ModalState;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

/// Dispatch to the correct modal renderer based on the current ModalState.
pub fn render_modal(f: &mut Frame, modal: &ModalState) {
    match modal {
        ModalState::Confirm { prompt, .. } => render_confirm_modal(f, prompt),
        ModalState::TextInput { prompt, buffer, .. } => render_text_input_modal(f, prompt, buffer),
        ModalState::DevicePicker {
            devices,
            selected,
            filter,
            ..
        } => render_device_picker_modal(f, devices, *selected, filter),
        ModalState::RunVariantPicker { selected, .. } => {
            render_run_variant_picker_modal(f, *selected)
        }
        ModalState::CleanToggle { options } => render_clean_modal(f, options),
        ModalState::SyncBeforeRun {
            run_command,
            needs_yarn,
            needs_pods,
        } => render_sync_prompt(f, run_command, *needs_yarn, *needs_pods),
        ModalState::SyncBeforeMetro {
            needs_yarn,
            needs_pods,
        } => render_sync_before_metro(f, *needs_yarn, *needs_pods),
        ModalState::ExternalMetroConflict { pid, working_dir } => {
            render_external_metro_modal(f, *pid, working_dir)
        }
        ModalState::BranchPicker {
            branches,
            selected,
            filter,
        } => render_branch_picker_modal(f, branches, *selected, filter),
        ModalState::PullRequestPicker {
            pull_requests,
            selected,
            search,
            filter,
        } => render_pull_request_picker_modal(f, pull_requests, *selected, search, *filter),
        ModalState::Info { message } => render_info_modal(f, message),
    }
}

fn render_run_variant_picker_modal(f: &mut Frame, selected: usize) {
    let area = centered_rect(f.area(), 40, 35, 28, 6);
    let block = Block::default()
        .title(" Select Run Type ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let items: Vec<ListItem> = crate::domain::command::RunVariant::ALL
        .iter()
        .map(|variant| ListItem::new(Line::from(variant.label())))
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ")
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Always);

    let mut ls = ListState::default();
    ls.select(Some(
        selected.min(crate::domain::command::RunVariant::ALL.len() - 1),
    ));

    f.render_widget(Clear, area);
    f.render_stateful_widget(list, area, &mut ls);
}

/// Renders a yes/no confirmation modal with a red border.
fn render_confirm_modal(f: &mut Frame, prompt: &str) {
    let area = centered_rect(f.area(), 50, 25, 40, 5);

    let lines = vec![
        Line::from(Span::raw(prompt)),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "[Y]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" confirm    "),
            Span::styled("[N/Esc]", Style::default().fg(Color::Red)),
            Span::raw(" cancel"),
        ]),
    ];

    let block = Block::default()
        .title(" Confirm ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Renders a text input modal with a cyan border.
fn render_text_input_modal(f: &mut Frame, prompt: &str, buffer: &str) {
    let area = centered_rect(f.area(), 60, 25, 40, 6);

    let lines = vec![
        Line::from(Span::raw(prompt)),
        Line::from(Span::styled(
            format!("{buffer}_"),
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "[Enter]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" submit    "),
            Span::styled("[Esc]", Style::default().fg(Color::Red)),
            Span::raw(" cancel"),
        ]),
    ];

    let block = Block::default()
        .title(" Input ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Renders a device picker modal with a green border, list selection, and type-to-filter.
fn render_device_picker_modal(
    f: &mut Frame,
    devices: &[crate::domain::command::DeviceInfo],
    selected: usize,
    filter: &str,
) {
    let area = centered_rect(f.area(), 60, 60, 40, 7);

    // Apply filter (case-insensitive substring match)
    let filtered: Vec<&crate::domain::command::DeviceInfo> = if filter.is_empty() {
        devices.iter().collect()
    } else {
        let lower = filter.to_lowercase();
        devices
            .iter()
            .filter(|d| d.name.to_lowercase().contains(&lower))
            .collect()
    };

    // Title shows filter text when active
    let title = if filter.is_empty() {
        " Select Device ".to_string()
    } else {
        format!(" Select Device — filter: {filter} ")
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    if filtered.is_empty() {
        let msg = if devices.is_empty() {
            "No devices found."
        } else {
            "No devices match filter."
        };
        let para = Paragraph::new(vec![
            Line::from(msg),
            Line::from(""),
            Line::from(vec![
                Span::styled("[Esc]", Style::default().fg(Color::Red)),
                Span::raw(" cancel    "),
                Span::styled("[Backspace]", Style::default().fg(Color::Yellow)),
                Span::raw(" clear filter"),
            ]),
        ])
        .block(block);

        f.render_widget(Clear, area);
        f.render_widget(para, area);
        return;
    }

    // Clamp selected index to filtered list length
    let clamped_selected = selected.min(filtered.len() - 1);

    let items: Vec<ListItem> = filtered
        .iter()
        .map(|d| ListItem::new(Line::from(d.name.as_str())))
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ")
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Always);

    let mut ls = ListState::default();
    ls.select(Some(clamped_selected));

    f.render_widget(Clear, area);
    f.render_stateful_widget(list, area, &mut ls);
}

/// Renders the clean toggle modal with checkboxes for each clean option.
fn render_clean_modal(f: &mut Frame, options: &crate::domain::command::CleanOptions) {
    let area = centered_rect(f.area(), 50, 60, 40, 10);

    f.render_widget(Clear, area);

    let checkbox = |checked: bool| if checked { "[x]" } else { "[ ]" };

    let text = vec![
        Line::from(Span::styled(
            " Clean Options ",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(
            "  n  {} node_modules (rm -rf)",
            checkbox(options.node_modules)
        )),
        Line::from(format!(
            "  p  {} CocoaPods (react-native clean)",
            checkbox(options.pods)
        )),
        Line::from(format!(
            "  a  {} Android (react-native clean)",
            checkbox(options.android)
        )),
        Line::from(format!(
            "  i  {} Sync after clean (yarn + pods)",
            checkbox(options.sync_after)
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  x/Enter = confirm  Esc = cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Clean ");

    f.render_widget(Paragraph::new(text).block(block), area);
}

/// Renders the sync-before-run prompt when a stale worktree is detected.
fn render_sync_prompt(
    f: &mut Frame,
    run_command: &crate::domain::command::CommandSpec,
    needs_yarn: bool,
    needs_pods: bool,
) {
    let area = centered_rect(f.area(), 60, 40, 40, 8);

    f.render_widget(Clear, area);

    let sync_desc = match (needs_yarn, needs_pods) {
        (true, true) => "yarn install + pod-install",
        (true, false) => "yarn install",
        (false, true) => "pod-install",
        (false, false) => "nothing", // unreachable — modal only shown when at least one is stale
    };

    let text = vec![
        Line::from(Span::styled(
            " Stale Dependencies ",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  Worktree is stale. Sync before running?"),
        Line::from(format!(
            "  Will run: {} -> {}",
            sync_desc,
            run_command.label()
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Y = sync first  N = skip  Esc = cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Sync ");

    f.render_widget(Paragraph::new(text).block(block), area);
}

/// Renders the sync-before-metro prompt when a stale worktree is about to start metro via Enter.
fn render_sync_before_metro(f: &mut Frame, needs_yarn: bool, needs_pods: bool) {
    let area = centered_rect(f.area(), 60, 40, 40, 8);
    f.render_widget(Clear, area);

    let sync_desc = match (needs_yarn, needs_pods) {
        (true, true) => "yarn install + pod-install",
        (true, false) => "yarn install",
        (false, true) => "pod-install",
        (false, false) => "nothing",
    };

    let text = vec![
        Line::from(Span::styled(
            " Stale Dependencies ",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  Worktree is stale. Sync before starting metro?"),
        Line::from(format!("  Will run: {} -> start metro", sync_desc)),
        Line::from(""),
        Line::from(Span::styled(
            "  Y = sync first  N = skip  Esc = cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Sync ");

    f.render_widget(Paragraph::new(text).block(block), area);
}

/// Renders the external metro conflict modal with PID and working directory info.
fn render_external_metro_modal(f: &mut Frame, pid: u32, working_dir: &str) {
    let area = centered_rect(f.area(), 60, 30, 40, 9);

    f.render_widget(Clear, area);

    let text = vec![
        Line::from(Span::styled(
            " External Metro Detected ",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  Another process is using port 8081".to_string()),
        Line::from(format!("  PID: {pid}")),
        Line::from(format!("  Directory: {working_dir}")),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "[Y]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Kill it    "),
            Span::styled("[N/Esc]", Style::default().fg(Color::Red)),
            Span::raw(" Cancel"),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Double)
        .border_style(Style::default().fg(Color::Red))
        .title(" External Metro ");

    f.render_widget(Paragraph::new(text).block(block), area);
}

/// Renders a branch picker modal with filterable list. Used by w>B new-branch worktree flow.
fn render_branch_picker_modal(f: &mut Frame, branches: &[String], selected: usize, filter: &str) {
    let area = centered_rect(f.area(), 60, 60, 40, 7);

    // Apply filter (case-insensitive substring match)
    let filtered: Vec<&String> = if filter.is_empty() {
        branches.iter().collect()
    } else {
        let lower = filter.to_lowercase();
        branches
            .iter()
            .filter(|b| b.to_lowercase().contains(&lower))
            .collect()
    };

    // Title shows filter text when active
    let title = if filter.is_empty() {
        " Select Base Branch ".to_string()
    } else {
        format!(" Select Base Branch — filter: {filter} ")
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    if filtered.is_empty() {
        let msg = if branches.is_empty() {
            "No remote branches found."
        } else {
            "No branches match filter."
        };
        let para = Paragraph::new(vec![
            Line::from(msg),
            Line::from(""),
            Line::from(vec![
                Span::styled("[Esc]", Style::default().fg(Color::Red)),
                Span::raw(" cancel    "),
                Span::styled("[Backspace]", Style::default().fg(Color::Yellow)),
                Span::raw(" clear filter"),
            ]),
        ])
        .block(block);

        f.render_widget(Clear, area);
        f.render_widget(para, area);
        return;
    }

    // Clamp selected index to filtered list length
    let clamped_selected = selected.min(filtered.len() - 1);

    let items: Vec<ListItem> = filtered
        .iter()
        .map(|b| ListItem::new(Line::from(b.as_str())))
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ")
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Always);

    let mut ls = ListState::default();
    ls.select(Some(clamped_selected));

    f.render_widget(Clear, area);
    f.render_stateful_widget(list, area, &mut ls);
}

fn render_pull_request_picker_modal(
    f: &mut Frame,
    pull_requests: &[crate::domain::review::PullRequest],
    selected: usize,
    search: &str,
    filter: crate::domain::review::PullRequestFilter,
) {
    let area = centered_rect(f.area(), 72, 70, 52, 10);
    let lower = search.to_lowercase();
    let filtered: Vec<&crate::domain::review::PullRequest> = pull_requests
        .iter()
        .filter(|pr| {
            search.is_empty()
                || pr.title.to_lowercase().contains(&lower)
                || pr.author.to_lowercase().contains(&lower)
        })
        .collect();

    let title = format!(" Review PRs [{}]  Search: {} ", filter.label(), search);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    if filtered.is_empty() {
        let msg = if pull_requests.is_empty() {
            "No pull requests loaded."
        } else {
            "No pull requests match search."
        };
        let para = Paragraph::new(vec![
            Line::from(msg),
            Line::from(""),
            Line::from(vec![
                Span::styled("[Tab]", Style::default().fg(Color::Yellow)),
                Span::raw(" filter    "),
                Span::styled("[Esc]", Style::default().fg(Color::Red)),
                Span::raw(" cancel"),
            ]),
        ])
        .block(block);
        f.render_widget(Clear, area);
        f.render_widget(para, area);
        return;
    }

    let items: Vec<ListItem> = filtered
        .iter()
        .map(|pr| {
            ListItem::new(Line::from(format!(
                "#{} {}  @{}  {}",
                pr.number, pr.title, pr.author, pr.head_ref_name
            )))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ")
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Always);

    let mut ls = ListState::default();
    ls.select(Some(selected.min(filtered.len() - 1)));

    f.render_widget(Clear, area);
    f.render_stateful_widget(list, area, &mut ls);
}

fn render_info_modal(f: &mut Frame, message: &str) {
    let area = centered_rect(f.area(), 58, 25, 42, 5);
    let para = Paragraph::new(vec![
        Line::from(message),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(Color::Green)),
            Span::raw(" ok    "),
            Span::styled("[Esc]", Style::default().fg(Color::Red)),
            Span::raw(" dismiss"),
        ]),
    ])
    .block(
        Block::default()
            .title(" Notice ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)),
    );

    f.render_widget(Clear, area);
    f.render_widget(para, area);
}

/// Computes a centered Rect within `area`, using percentage sizing with minimum dimensions.
/// Width is clamped to [min_w, area.width], height to [min_h, area.height].
/// Follows the same independent-copy pattern as help_overlay.rs and error_overlay.rs.
fn centered_rect(area: Rect, percent_x: u16, percent_y: u16, min_w: u16, min_h: u16) -> Rect {
    let w = (area.width * percent_x / 100).clamp(min_w, area.width);
    let h = (area.height * percent_y / 100).clamp(min_h, area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}
