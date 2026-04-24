#![allow(dead_code)]
use ratatui::crossterm::event::Event as CrosstermEvent;

/// Internal event type wrapping crossterm. Allows future addition of app-generated events
/// (e.g., background task completion) without coupling callers to crossterm types.
#[derive(Debug)]
pub enum Event {
    Key(ratatui::crossterm::event::KeyEvent),
    Resize(u16, u16),
    Tick,
}

/// Convert a crossterm Event to our internal Event type.
/// Returns None for event types we don't handle (mouse, paste, focus, etc.).
pub fn from_crossterm(ev: CrosstermEvent) -> Option<Event> {
    use CrosstermEvent::*;
    match ev {
        Key(k) => Some(Event::Key(k)),
        Resize(w, h) => Some(Event::Resize(w, h)),
        // intentionally unhandled: Mouse, Paste, FocusGained, FocusLost
        // (legitimate fall-through — these crossterm event kinds have no
        //  semantic meaning in this TUI).
        _ => None,
    }
}
