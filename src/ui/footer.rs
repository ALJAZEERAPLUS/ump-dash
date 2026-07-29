//! Footer key-hint bar — thin wrapper around the KEYBINDINGS registry.
//!
//! Plan 13-10 (F-302) closed: the ~130 lines of hand-coded context-branching
//! tables that used to live here were replaced by a delegation to
//! `crate::app::keybindings::footer_hints_for(state)`. The registry walker
//! filters every binding by `context_matches(&kb.context, state)` and by
//! `(kb.visible)(state)`, so context-sensitive hints (palette, modal,
//! overlay, task-specific) flow from the single source of truth.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::{app::{keybindings::footer_hints_for, AppState}, ui::theme};

/// Renders the footer key hint bar. Always 1 line tall at the bottom of the layout.
/// Hints change with the current app state — satisfies SHELL-02.
/// Dynamic: metro hints (R/J/Esc) only shown when metro is running. No static legend.
pub fn render_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let hints = footer_hints_for(state);

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
