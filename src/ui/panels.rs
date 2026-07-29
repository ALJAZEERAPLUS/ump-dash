use crate::{
    app::AppState,
    domain::{dash_config::WorktreeTableColumn, worktree::WorktreeMetroStatus},
    ui::{
        indicators::{SpinnerStyle, format_elapsed, spinner_frame, task_short_label},
        theme,
    },
};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Cell, Paragraph, Row, Table},
};

/// Renders the application title bar with double border.
/// Available to layouts that need a standalone title.
#[allow(dead_code)]
pub fn render_title_bar(f: &mut Frame, area: Rect, state: &AppState) {
    let title = state
        .app_config
        .config
        .as_ref()
        .map(|c| c.app_title.as_str())
        .unwrap_or("UMP Dash");
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .title(format!(" {title} "))
        .title_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
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

fn worktree_column_constraint(column: WorktreeTableColumn) -> Constraint {
    match column {
        WorktreeTableColumn::Status => Constraint::Length(4),
        WorktreeTableColumn::Branch => Constraint::Length(20),
        WorktreeTableColumn::Ticket => Constraint::Min(20),
        WorktreeTableColumn::Dir => Constraint::Length(16),
        WorktreeTableColumn::Task => Constraint::Length(20),
        WorktreeTableColumn::CacheStatus => Constraint::Length(3),
        WorktreeTableColumn::Cache => Constraint::Length(8),
        WorktreeTableColumn::AndroidCacheStatus => Constraint::Length(3),
        WorktreeTableColumn::AndroidCache => Constraint::Length(8),
    }
}

fn worktree_column_header(column: WorktreeTableColumn) -> &'static str {
    match column {
        WorktreeTableColumn::Status => "",
        WorktreeTableColumn::Branch => "Branch",
        WorktreeTableColumn::Ticket => "Ticket",
        WorktreeTableColumn::Dir => "Dir",
        WorktreeTableColumn::Task => "Task",
        WorktreeTableColumn::CacheStatus => "iOS",
        WorktreeTableColumn::Cache => "iOS FP",
        WorktreeTableColumn::AndroidCacheStatus => "APK",
        WorktreeTableColumn::AndroidCache => "APK FP",
    }
}

fn activity_column_index(columns: &[WorktreeTableColumn]) -> Option<usize> {
    columns
        .iter()
        .position(|column| matches!(column, WorktreeTableColumn::Ticket))
        .or_else(|| {
            columns
                .iter()
                .position(|column| !matches!(column, WorktreeTableColumn::Status))
        })
        .or_else(|| (!columns.is_empty()).then_some(0))
}

fn metro_activity_label(
    activity: Option<&crate::domain::metro::MetroActivity>,
    port: u16,
) -> String {
    match activity {
        Some(activity) => format!("\u{2502} {activity} :{port}"),
        None => format!("\u{2502} Metro :{port}"),
    }
}

fn short_fingerprint(fingerprint: &str) -> String {
    let short = fingerprint.chars().take(8).collect::<String>();
    if short.is_empty() { "-".into() } else { short }
}

fn cache_column_label(slice: Option<&crate::domain::worktree_slice::WorktreeSlice>) -> String {
    match slice.map(|slice| &slice.ios_simulator_cache) {
        Some(crate::domain::native_cache::IosSimulatorCacheState::Hit(hit)) => {
            short_fingerprint(&hit.metadata.fingerprint)
        }
        Some(crate::domain::native_cache::IosSimulatorCacheState::Checking) => "...".into(),
        Some(crate::domain::native_cache::IosSimulatorCacheState::Error(_)) => "err".into(),
        Some(crate::domain::native_cache::IosSimulatorCacheState::Miss { fingerprint }) => {
            short_fingerprint(fingerprint)
        }
        Some(crate::domain::native_cache::IosSimulatorCacheState::Unknown) | None => "-".into(),
    }
}

fn android_cache_column_label(
    slice: Option<&crate::domain::worktree_slice::WorktreeSlice>,
) -> String {
    match slice.map(|slice| &slice.android_cache) {
        Some(crate::domain::native_cache::AndroidCacheState::Hit(hit)) => {
            short_fingerprint(&hit.metadata.fingerprint)
        }
        Some(crate::domain::native_cache::AndroidCacheState::Checking) => "...".into(),
        Some(crate::domain::native_cache::AndroidCacheState::Error(_)) => "err".into(),
        Some(crate::domain::native_cache::AndroidCacheState::Miss { fingerprint }) => {
            short_fingerprint(fingerprint)
        }
        Some(crate::domain::native_cache::AndroidCacheState::Unknown) | None => "-".into(),
    }
}

fn cache_status_column_label(
    slice: Option<&crate::domain::worktree_slice::WorktreeSlice>,
) -> String {
    match slice.map(|slice| &slice.ios_simulator_cache) {
        Some(crate::domain::native_cache::IosSimulatorCacheState::Hit(_)) => "\u{25cf}".into(),
        Some(crate::domain::native_cache::IosSimulatorCacheState::Checking) => "...".into(),
        Some(crate::domain::native_cache::IosSimulatorCacheState::Error(_)) => "!".into(),
        Some(crate::domain::native_cache::IosSimulatorCacheState::Miss { .. })
        | Some(crate::domain::native_cache::IosSimulatorCacheState::Unknown)
        | None => String::new(),
    }
}

fn android_cache_status_column_label(
    slice: Option<&crate::domain::worktree_slice::WorktreeSlice>,
) -> String {
    match slice.map(|slice| &slice.android_cache) {
        Some(crate::domain::native_cache::AndroidCacheState::Hit(_)) => "\u{25cf}".into(),
        Some(crate::domain::native_cache::AndroidCacheState::Checking) => "...".into(),
        Some(crate::domain::native_cache::AndroidCacheState::Error(_)) => "!".into(),
        Some(crate::domain::native_cache::AndroidCacheState::Miss { .. })
        | Some(crate::domain::native_cache::AndroidCacheState::Unknown)
        | None => String::new(),
    }
}

fn cache_status_column_style(
    slice: Option<&crate::domain::worktree_slice::WorktreeSlice>,
) -> Style {
    match slice.map(|slice| &slice.ios_simulator_cache) {
        Some(crate::domain::native_cache::IosSimulatorCacheState::Hit(_)) => {
            Style::default().fg(Color::Green)
        }
        Some(crate::domain::native_cache::IosSimulatorCacheState::Checking) => {
            Style::default().fg(Color::DarkGray)
        }
        Some(crate::domain::native_cache::IosSimulatorCacheState::Error(_)) => {
            Style::default().fg(Color::Red)
        }
        _ => Style::default(),
    }
}

fn android_cache_status_column_style(
    slice: Option<&crate::domain::worktree_slice::WorktreeSlice>,
) -> Style {
    match slice.map(|slice| &slice.android_cache) {
        Some(crate::domain::native_cache::AndroidCacheState::Hit(_)) => {
            Style::default().fg(Color::Green)
        }
        Some(crate::domain::native_cache::AndroidCacheState::Checking) => {
            Style::default().fg(Color::DarkGray)
        }
        Some(crate::domain::native_cache::AndroidCacheState::Error(_)) => {
            Style::default().fg(Color::Red)
        }
        _ => Style::default(),
    }
}

/// Renders the worktree table with structured columns.
pub fn render_worktree_table(f: &mut Frame, area: Rect, state: &mut AppState) {
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(theme::style_primary_border());

    if state.worktree_browser.worktrees.is_empty() {
        let placeholder = Paragraph::new("Loading worktrees...").block(block);
        f.render_widget(placeholder, area);
        return;
    }

    let mut rows: Vec<Row> = Vec::new();
    // Track visual row indices that are detail rows (not selectable worktree rows).
    let mut detail_row_indices: Vec<usize> = Vec::new();

    // Spinner glyph set — read once from config (default Circles). Pure UI choice.
    let spinner_style = state
        .app_config
        .config
        .as_ref()
        .map(|c| SpinnerStyle::from_config(&c.spinner_style))
        .unwrap_or_default();
    let columns = state
        .app_config
        .config
        .as_ref()
        .map(|c| c.columns.as_slice())
        .unwrap_or(&crate::domain::dash_config::DEFAULT_WORKTREE_COLUMNS);

    for wt in state.worktree_browser.worktrees.iter() {
        let branch = &wt.branch;

        // Extract ticket number from branch if possible
        let ticket_num = crate::domain::jira::extract_jira_key(branch, &state.jira.project_prefix)
            .unwrap_or_default();
        let title = wt.jira_title.as_deref().unwrap_or("");

        // Merged ticket display: "UMP-1234 Title text" or just one or the other
        let ticket_display = match (ticket_num.is_empty(), title.is_empty()) {
            (false, false) => format!("{ticket_num} {title}"),
            (false, true) => ticket_num,
            (true, false) => title.to_string(),
            (true, true) => String::new(),
        };

        let slice = state.worktrees.get(&wt.id);

        // Per-row running task lookup — Option<&TaskRecord> is Copy; safe to use twice below.
        let task = crate::app::state::task_for_worktree(state, &wt.id);
        let cache_cell = cache_column_label(slice);
        let cache_status_cell = cache_status_column_label(slice);
        let cache_status_style = cache_status_column_style(slice);
        let android_cache_cell = android_cache_column_label(slice);
        let android_cache_status_cell = android_cache_status_column_label(slice);
        let android_cache_status_style = android_cache_status_column_style(slice);

        // Status icons: Y (yarn) and P (pods) with color indicating freshness.
        // Metro state surfaces via row bg + detail row, not via an icon column.
        let mut icon_spans: Vec<Span> = Vec::new();

        // Y cell: yellow spinner if yarn-install running (D-02/D-09), else staleness color
        if let Some(record) = task {
            if matches!(
                &record.spec,
                crate::domain::command::CommandSpec::YarnInstall
            ) {
                let frame = spinner_frame(record.started_at.elapsed(), spinner_style);
                icon_spans.push(Span::styled(frame, Style::default().fg(Color::Yellow)));
            } else {
                let yarn_color = if wt.stale { Color::Red } else { Color::Green };
                icon_spans.push(Span::styled("Y", Style::default().fg(yarn_color)));
            }
        } else {
            let yarn_color = if wt.stale { Color::Red } else { Color::Green };
            icon_spans.push(Span::styled("Y", Style::default().fg(yarn_color)));
        }

        // Space separator between Y and P prevents spinner glyph overlap with P.
        icon_spans.push(Span::raw(" "));

        // P cell: yellow spinner if pod-install running (D-02/D-09), else staleness color
        if let Some(record) = task {
            if matches!(
                &record.spec,
                crate::domain::command::CommandSpec::YarnPodInstall
            ) {
                let frame = spinner_frame(record.started_at.elapsed(), spinner_style);
                icon_spans.push(Span::styled(frame, Style::default().fg(Color::Yellow)));
            } else {
                let pods_color = if wt.stale_pods {
                    Color::Red
                } else {
                    Color::Green
                };
                icon_spans.push(Span::styled("P", Style::default().fg(pods_color)));
            }
        } else {
            let pods_color = if wt.stale_pods {
                Color::Red
            } else {
                Color::Green
            };
            icon_spans.push(Span::styled("P", Style::default().fg(pods_color)));
        }

        let row_style = if wt.metro_status == WorktreeMetroStatus::Running {
            Style::default()
                .bg(Color::Rgb(0, 60, 0))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        // Extract dir name from path
        let dir_name = wt
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Task column: spinner + short label + elapsed for non-yarn/pod tasks (D-04/UI-02)
        // YarnInstall/YarnPodInstall animate in Y/P cells; task column empty for them.
        let task_cell: String = match task {
            Some(record)
                if !matches!(
                    &record.spec,
                    crate::domain::command::CommandSpec::YarnInstall
                        | crate::domain::command::CommandSpec::YarnPodInstall
                ) =>
            {
                let elapsed = record.started_at.elapsed();
                format!(
                    "{} {} {}",
                    spinner_frame(elapsed, spinner_style),
                    task_short_label(&record.spec),
                    format_elapsed(elapsed)
                )
            }
            _ => String::new(),
        };

        let row_cells = columns
            .iter()
            .map(|column| match column {
                WorktreeTableColumn::Status => Cell::from(Line::from(icon_spans.clone())),
                WorktreeTableColumn::Branch => Cell::from(truncate(branch, 18)),
                WorktreeTableColumn::Ticket => Cell::from(ticket_display.clone()),
                WorktreeTableColumn::Dir => Cell::from(dir_name.clone()),
                WorktreeTableColumn::Task => Cell::from(task_cell.clone()),
                WorktreeTableColumn::CacheStatus => {
                    Cell::from(Span::styled(cache_status_cell.clone(), cache_status_style))
                }
                WorktreeTableColumn::Cache => Cell::from(cache_cell.clone()),
                WorktreeTableColumn::AndroidCacheStatus => Cell::from(Span::styled(
                    android_cache_status_cell.clone(),
                    android_cache_status_style,
                )),
                WorktreeTableColumn::AndroidCache => Cell::from(android_cache_cell.clone()),
            })
            .collect::<Vec<_>>();

        rows.push(Row::new(row_cells).style(row_style));

        // If this worktree is running metro, add a detail row with activity and port.
        let slice_metro = slice.map(|slice| &slice.metro);
        if wt.metro_status == WorktreeMetroStatus::Running
            && let Some(metro) = slice_metro
            && let Some(port) = metro.running_port()
        {
            let mut detail_cells = columns.iter().map(|_| Cell::from("")).collect::<Vec<_>>();
            if let Some(idx) = activity_column_index(columns) {
                detail_cells[idx] = Cell::from(Span::styled(
                    metro_activity_label(metro.activity(), port),
                    Style::default().fg(Color::Cyan),
                ));
            }
            let detail_row =
                Row::new(detail_cells).style(Style::default().bg(Color::Rgb(0, 60, 0)));
            detail_row_indices.push(rows.len());
            rows.push(detail_row);
        }
    }

    // Use green highlight when the selected row is metro-active, gray otherwise
    let selected_idx = state
        .worktree_browser
        .worktree_table_state
        .selected()
        .unwrap_or(0);
    let selected_is_metro = state
        .worktree_browser
        .worktrees
        .get(selected_idx)
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

    let constraints = columns
        .iter()
        .copied()
        .map(worktree_column_constraint)
        .collect::<Vec<_>>();

    let header_cells = columns
        .iter()
        .copied()
        .map(|column| Cell::from(worktree_column_header(column)))
        .collect::<Vec<_>>();
    let header = Row::new(header_cells).style(
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );

    let table = Table::new(rows, constraints)
        .block(block)
        .header(header)
        .row_highlight_style(highlight_style);
    // No highlight_symbol — selection is conveyed by row bg only, so the
    // left gutter (`> ` for selected, blank for others) is not reserved.

    // Map logical selection index to visual index (offset by any detail rows inserted before it)
    if let Some(logical) = state.worktree_browser.worktree_table_state.selected() {
        let mut visual = logical;
        for &detail_idx in &detail_row_indices {
            if detail_idx <= visual {
                visual += 1;
            }
        }
        if visual != logical {
            state
                .worktree_browser
                .worktree_table_state
                .select(Some(visual));
        }
    }

    f.render_stateful_widget(
        table,
        area,
        &mut state.worktree_browser.worktree_table_state,
    );

    // Restore logical index after render so app state stays consistent
    if let Some(visual) = state.worktree_browser.worktree_table_state.selected() {
        let mut logical = visual;
        for &detail_idx in detail_row_indices.iter().rev() {
            if detail_idx <= logical && logical > 0 {
                logical -= 1;
            }
        }
        if logical != visual {
            state
                .worktree_browser
                .worktree_table_state
                .select(Some(logical));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::metro::MetroActivity;
    use crate::domain::native_cache::{
        ANDROID_APK_ARTIFACT_KIND, ANDROID_PLATFORM, AndroidCacheHit, AndroidCacheMetadata,
        AndroidCacheState, IOS_APP_ARTIFACT_KIND, IOS_SIMULATOR_PLATFORM, IosSimulatorCacheHit,
        IosSimulatorCacheMetadata, IosSimulatorCacheState,
    };
    use crate::domain::worktree_slice::WorktreeSlice;

    #[test]
    fn worktree_column_header_labels_match_configured_columns() {
        let labels = crate::domain::dash_config::DEFAULT_WORKTREE_COLUMNS
            .iter()
            .copied()
            .map(worktree_column_header)
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec!["", "Branch", "Ticket", "Dir", "Task", "iOS", "iOS FP"]
        );
        assert_eq!(
            worktree_column_header(WorktreeTableColumn::AndroidCacheStatus),
            "APK"
        );
        assert_eq!(
            worktree_column_header(WorktreeTableColumn::AndroidCache),
            "APK FP"
        );
    }

    #[test]
    fn metro_activity_label_shows_selected_port() {
        assert_eq!(
            metro_activity_label(Some(&MetroActivity::Ready), 8082),
            "\u{2502} Ready :8082"
        );
    }

    #[test]
    fn metro_activity_label_falls_back_to_port_when_activity_missing() {
        assert_eq!(metro_activity_label(None, 8083), "\u{2502} Metro :8083");
    }

    fn cache_hit(fingerprint: &str) -> IosSimulatorCacheHit {
        IosSimulatorCacheHit {
            metadata: IosSimulatorCacheMetadata {
                platform: IOS_SIMULATOR_PLATFORM.into(),
                fingerprint: fingerprint.into(),
                bundle_id: "com.example.app".into(),
                variant: "local".into(),
                created_at: "2026-06-01T00:00:00Z".into(),
                source_worktree: "/tmp/wt".into(),
                artifact_kind: IOS_APP_ARTIFACT_KIND.into(),
                storage_mode: "copy".into(),
                source_artifact_path: "/tmp/wt/build/app.app".into(),
                artifact_digest_algorithm: "sha256".into(),
                artifact_digest: "digest".into(),
            },
            artifact_path: "/tmp/cached.app".into(),
        }
    }

    fn android_cache_hit(fingerprint: &str) -> AndroidCacheHit {
        AndroidCacheHit {
            metadata: AndroidCacheMetadata {
                platform: ANDROID_PLATFORM.into(),
                fingerprint: fingerprint.into(),
                application_id: "com.example.app".into(),
                variant: "localDebugOptimized".into(),
                created_at: "2026-06-04T00:00:00Z".into(),
                source_worktree: "/tmp/wt".into(),
                artifact_kind: ANDROID_APK_ARTIFACT_KIND.into(),
                storage_mode: "copy".into(),
                source_artifact_path: "/tmp/wt/build/app.apk".into(),
                artifact_digest_algorithm: "sha256".into(),
                artifact_digest: "digest".into(),
            },
            artifact_path: "/tmp/cached.apk".into(),
        }
    }

    #[test]
    fn cache_column_label_shows_short_fingerprint_for_hits() {
        let slice = WorktreeSlice {
            ios_simulator_cache: IosSimulatorCacheState::Hit(Box::new(cache_hit(
                "abcdef1234567890",
            ))),
            ..Default::default()
        };

        assert_eq!(cache_column_label(Some(&slice)), "abcdef12");
    }

    #[test]
    fn cache_column_label_shows_miss_fingerprint_and_compact_statuses() {
        let checking = WorktreeSlice {
            ios_simulator_cache: IosSimulatorCacheState::Checking,
            ..Default::default()
        };
        let error = WorktreeSlice {
            ios_simulator_cache: IosSimulatorCacheState::Error("bad metadata".into()),
            ..Default::default()
        };
        let miss = WorktreeSlice {
            ios_simulator_cache: IosSimulatorCacheState::Miss {
                fingerprint: "0123456789abcdef".into(),
            },
            ..Default::default()
        };

        assert_eq!(cache_column_label(Some(&checking)), "...");
        assert_eq!(cache_column_label(Some(&error)), "err");
        assert_eq!(cache_column_label(Some(&miss)), "01234567");
        assert_eq!(cache_column_label(None), "-");
    }

    #[test]
    fn cache_status_column_label_shows_availability_light_only_for_hits() {
        let hit = WorktreeSlice {
            ios_simulator_cache: IosSimulatorCacheState::Hit(Box::new(cache_hit(
                "abcdef1234567890",
            ))),
            ..Default::default()
        };
        let miss = WorktreeSlice {
            ios_simulator_cache: IosSimulatorCacheState::Miss {
                fingerprint: "0123456789abcdef".into(),
            },
            ..Default::default()
        };
        let checking = WorktreeSlice {
            ios_simulator_cache: IosSimulatorCacheState::Checking,
            ..Default::default()
        };
        let error = WorktreeSlice {
            ios_simulator_cache: IosSimulatorCacheState::Error("bad metadata".into()),
            ..Default::default()
        };

        assert_eq!(cache_status_column_label(Some(&hit)), "\u{25cf}");
        assert_eq!(cache_status_column_label(Some(&miss)), "");
        assert_eq!(cache_status_column_label(Some(&checking)), "...");
        assert_eq!(cache_status_column_label(Some(&error)), "!");
        assert_eq!(cache_status_column_label(None), "");
    }

    #[test]
    fn android_cache_column_label_shows_fingerprint_and_compact_statuses() {
        let hit = WorktreeSlice {
            android_cache: AndroidCacheState::Hit(Box::new(android_cache_hit("abcdef1234567890"))),
            ..Default::default()
        };
        let miss = WorktreeSlice {
            android_cache: AndroidCacheState::Miss {
                fingerprint: "0123456789abcdef".into(),
            },
            ..Default::default()
        };
        let checking = WorktreeSlice {
            android_cache: AndroidCacheState::Checking,
            ..Default::default()
        };
        let error = WorktreeSlice {
            android_cache: AndroidCacheState::Error("bad metadata".into()),
            ..Default::default()
        };

        assert_eq!(android_cache_column_label(Some(&hit)), "abcdef12");
        assert_eq!(android_cache_column_label(Some(&miss)), "01234567");
        assert_eq!(android_cache_column_label(Some(&checking)), "...");
        assert_eq!(android_cache_column_label(Some(&error)), "err");
        assert_eq!(android_cache_column_label(None), "-");
    }

    #[test]
    fn android_cache_status_column_label_shows_availability_light_only_for_hits() {
        let hit = WorktreeSlice {
            android_cache: AndroidCacheState::Hit(Box::new(android_cache_hit("abcdef1234567890"))),
            ..Default::default()
        };
        let miss = WorktreeSlice {
            android_cache: AndroidCacheState::Miss {
                fingerprint: "0123456789abcdef".into(),
            },
            ..Default::default()
        };
        let checking = WorktreeSlice {
            android_cache: AndroidCacheState::Checking,
            ..Default::default()
        };
        let error = WorktreeSlice {
            android_cache: AndroidCacheState::Error("bad metadata".into()),
            ..Default::default()
        };

        assert_eq!(android_cache_status_column_label(Some(&hit)), "\u{25cf}");
        assert_eq!(android_cache_status_column_label(Some(&miss)), "");
        assert_eq!(android_cache_status_column_label(Some(&checking)), "...");
        assert_eq!(android_cache_status_column_label(Some(&error)), "!");
        assert_eq!(android_cache_status_column_label(None), "");
    }
}
