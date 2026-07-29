//! Color constants and Style definitions. No logic — pure data.
#![allow(dead_code)]
use ratatui::style::{Color, Modifier, Style};

pub const COLOR_PRIMARY_BORDER: Color = Color::Cyan;
pub const COLOR_ERROR_BORDER: Color = Color::Red;
pub const COLOR_FOOTER_KEY: Color = Color::Cyan;
pub const COLOR_FOOTER_BG: Color = Color::Reset;

pub fn style_primary_border() -> Style {
    Style::default().fg(COLOR_PRIMARY_BORDER)
}

pub fn style_error_border() -> Style {
    Style::default().fg(COLOR_ERROR_BORDER)
}

pub fn style_key_hint() -> Style {
    Style::default().fg(COLOR_FOOTER_KEY).add_modifier(Modifier::BOLD)
}
