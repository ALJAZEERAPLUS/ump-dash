# Phase 16: Live UI Indicators - Pattern Map

**Mapped:** 2026-05-22
**Files analyzed:** 2 (1 modify, 1 create)
**Analogs found:** 2 / 2

---

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `src/ui/panels.rs` | component (render) | request-response (per-frame render) | `src/ui/panels.rs` itself (lines 82-167) | exact — same file, extend existing pattern |
| `src/ui/indicators.rs` | utility (pure helper) | transform (Duration → &str / String) | `src/domain/refresh.rs` + `src/domain/jira.rs` | role-match — same pure-fn + inline `#[cfg(test)]` convention |

---

## Pattern Assignments

### `src/ui/panels.rs` (component, per-frame render — MODIFY)

**Analog:** Same file, existing span/cell construction at lines 82–167.

#### Imports pattern (lines 1–16) — no new imports needed

```rust
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
```

Add one `use` for the new helper module after the existing `ui::theme` import:
```rust
use crate::ui::indicators::{spinner_frame, format_elapsed, task_short_label};
```

#### Task lookup pattern (lines 208–210 in `render_command_output`) — copy verbatim into `render_worktree_table` row loop

```rust
// Already used in render_command_output. Copy the same call inside the
// `for wt in state.worktree_browser.worktrees.iter()` loop at ~line 67.
let task = crate::app::state::task_for_worktree(state, &wt.id);
```

This is the established per-row accessor. `task: Option<&TaskRecord>` — destructure once with `if let Some(record) = task { ... }` to avoid double-borrow (see Pitfall 2 in RESEARCH.md).

#### Current icon span construction (lines 82–98) — THIS IS WHAT GETS REPLACED

```rust
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
```

Replace with the D-02/D-03 split pattern — same `Span::styled` API, same Style construction, same `icon_spans` push convention:

```rust
// Metro indicator (unchanged)
if wt.metro_status == WorktreeMetroStatus::Running {
    icon_spans.push(Span::styled("\u{25B6} ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
} else {
    icon_spans.push(Span::raw("  "));
}

// Y cell: spinner if yarn-install running, else staleness color (D-02/D-09)
if let Some(record) = task {
    if matches!(&record.spec, crate::domain::command::CommandSpec::YarnInstall) {
        let frame = spinner_frame(record.started_at.elapsed());
        icon_spans.push(Span::styled(frame, Style::default().fg(Color::Yellow)));
    } else {
        let yarn_color = if wt.stale { Color::Red } else { Color::Green };
        icon_spans.push(Span::styled("Y", Style::default().fg(yarn_color)));
    }
} else {
    let yarn_color = if wt.stale { Color::Red } else { Color::Green };
    icon_spans.push(Span::styled("Y", Style::default().fg(yarn_color)));
}

// Space separator — replaces the slash (D-03)
icon_spans.push(Span::raw(" "));

// P cell: spinner if pod-install running, else staleness color (D-02/D-09)
if let Some(record) = task {
    if matches!(&record.spec, crate::domain::command::CommandSpec::YarnPodInstall) {
        let frame = spinner_frame(record.started_at.elapsed());
        icon_spans.push(Span::styled(frame, Style::default().fg(Color::Yellow)));
    } else {
        let pods_color = if wt.stale_pods { Color::Red } else { Color::Green };
        icon_spans.push(Span::styled("P", Style::default().fg(pods_color)));
    }
} else {
    let pods_color = if wt.stale_pods { Color::Red } else { Color::Green };
    icon_spans.push(Span::styled("P", Style::default().fg(pods_color)));
}
```

**Note on borrow:** `task` is `Option<&TaskRecord>` (a copy of the option holding an immutable reference). `if let Some(record) = task` does NOT move `task` — `Option<&T>` is `Copy`. Both Y and P arms can re-use `task` independently.

#### Task cell construction (new, before Row::new) — follows same String→Cell pattern as `ticket_display`

```rust
// Task column: spinner + short label + elapsed for non-yarn/pod tasks (D-04)
let task_cell: String = match task {
    Some(record) if !matches!(
        &record.spec,
        crate::domain::command::CommandSpec::YarnInstall
            | crate::domain::command::CommandSpec::YarnPodInstall
    ) => {
        let elapsed = record.started_at.elapsed();
        format!(
            "{} {} {}",
            spinner_frame(elapsed),
            task_short_label(&record.spec),
            format_elapsed(elapsed)
        )
    }
    _ => String::new(),
};
```

#### Current `Row::new` (lines 114–120) — 4-cell version to replace

```rust
rows.push(Row::new(vec![
    Cell::from(Line::from(icon_spans)),
    Cell::from(truncate(branch, 18)),
    Cell::from(ticket_display),
    Cell::from(dir_name),
])
.style(row_style));
```

Replace with 5-cell version:

```rust
rows.push(Row::new(vec![
    Cell::from(Line::from(icon_spans)),
    Cell::from(truncate(branch, 18)),
    Cell::from(ticket_display),
    Cell::from(dir_name),
    Cell::from(task_cell),  // new 5th column — empty String when no task
])
.style(row_style));
```

#### Metro detail row (lines 125–136) — MUST add 5th empty cell

Current (4 cells):
```rust
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
```

Add 5th cell:
```rust
let detail_row = Row::new(vec![
    Cell::from(""),
    Cell::from(""),
    Cell::from(Span::styled(
        format!("\u{2502} {activity}"),
        Style::default().fg(Color::Cyan),
    )),
    Cell::from(""),
    Cell::from(""),  // task column — always empty for detail rows
])
.style(Style::default().bg(Color::Rgb(0, 60, 0)));
```

#### Current `Table::new` Constraint array (lines 156–164) — 4-column version to replace

```rust
let table = Table::new(
    rows,
    [
        Constraint::Length(8),  // Status icons (metro + Y + /P)
        Constraint::Length(20), // Branch
        Constraint::Min(20),   // Ticket (merged number + title)
        Constraint::Length(16), // Dir
    ],
)
```

Replace with 5-column version:

```rust
let table = Table::new(
    rows,
    [
        Constraint::Length(8),  // Status icons (metro + Y + P)
        Constraint::Length(20), // Branch
        Constraint::Min(20),    // Ticket (merged number + title)
        Constraint::Length(16), // Dir
        Constraint::Length(20), // Task (◐ unit-tests 12:03 = 18 chars + 2 margin)
    ],
)
```

---

### `src/ui/indicators.rs` (utility, pure transform — CREATE)

**Analog:** `src/domain/refresh.rs` (pure fn + exhaustive match + inline `#[cfg(test)] mod tests`) AND `src/domain/jira.rs` (pure fn + inline `#[cfg(test)] mod tests` with boundary cases).

#### Module-level doc comment pattern (from `src/domain/refresh.rs` lines 1–5 and `src/domain/jira.rs` lines 1–6)

```rust
//! Pure display helpers for live task indicators.
//!
//! All functions are pure (no I/O, no mutable state) and safe to call from
//! the render path. Imports: `std::time::Duration` + `crate::domain::command::CommandSpec`.
//! No `crate::infra::` imports (G-02).
```

#### Imports pattern — follows G-02 rule (ui/ imports zero infra)

```rust
use std::time::Duration;
use crate::domain::command::CommandSpec;
```

This matches the `domain/refresh.rs` pattern (`use super::command::CommandSpec;` — same crate-relative reference, just absolute path form for a sibling-of-domain module).

#### Constant array pattern

```rust
/// Half-circle spinner frames. 6 frames, 150ms per frame → full rotation in 900ms.
/// Width risk: ◐ (U+25D0) and ◑ (U+25D1) have east_asian_width=A (Ambiguous).
/// Safe in tmux+iTerm2 (Western locale). Fall back to braille set if misaligned.
pub const SPINNER_FRAMES: [&str; 6] = ["◐", "◓", "◑", "◒", "◐", "◓"];
```

#### Pure fn pattern — `spinner_frame` (mirrors `refresh_needed` in structure)

```rust
/// Returns the current spinner glyph for a running task.
/// Frame index = elapsed.as_millis() / 150 % 6 (D-05).
pub fn spinner_frame(elapsed: Duration) -> &'static str {
    let idx = (elapsed.as_millis() / 150 % 6) as usize;
    SPINNER_FRAMES[idx]
}
```

#### Pure fn pattern — `format_elapsed` (mirrors `extract_jira_key` two-branch structure)

```rust
/// Formats elapsed duration: seconds under 60 → "42s"; 60+ → "M:SS" (D-08).
/// Examples: 0 → "0s", 59 → "59s", 60 → "1:00", 723 → "12:03".
pub fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}:{s:02}")
    }
}
```

#### Exhaustive match pattern — `task_short_label`

**The canonical analog is `collision_policy()` at `src/domain/command.rs` lines 172–206.**

That method:
- Has zero `_ =>` arm — every `CommandSpec` variant is named explicitly
- Uses `| CommandSpec::Foo` arm merging for groups
- Has a meta-test `collision_policy_covers_every_variant` (lines 501–562) that constructs all 23 variants and asserts `variants.len() == 23`

`task_short_label` MUST follow the same pattern — no `_ =>`, every variant named:

```rust
/// Short display code for the task column. Exhaustive — no `_ =>` arm.
/// Adding a new CommandSpec variant will fail to compile here (and in
/// collision_policy) simultaneously, enforcing variant coverage.
pub fn task_short_label(spec: &CommandSpec) -> &'static str {
    match spec {
        CommandSpec::YarnInstall          => "yarn",
        CommandSpec::YarnPodInstall       => "pods",
        CommandSpec::YarnJest { .. }      => "jest",
        CommandSpec::YarnLint             => "lint",
        CommandSpec::YarnCheckTypes       => "types",
        CommandSpec::YarnUnitTests        => "unit-tests",
        CommandSpec::RnRunAndroid { .. }  => "run-and",
        CommandSpec::RnRunIos { .. }      => "run-ios",
        CommandSpec::RnRunIosDevice       => "run-ios",
        CommandSpec::RnReleaseBuild       => "release",
        CommandSpec::AdbInstallApk        => "adb",
        CommandSpec::ShellCommand { .. }  => "shell",
        CommandSpec::RnCleanAndroid       => "clean-and",
        CommandSpec::RnCleanCocoapods     => "clean-pod",
        CommandSpec::RmNodeModules        => "rm-mods",
        CommandSpec::GitPull              => "pull",
        CommandSpec::GitPush              => "push",
        CommandSpec::GitFetch             => "fetch",
        CommandSpec::GitRebase { .. }     => "rebase",
        CommandSpec::GitResetHard         => "reset",
        CommandSpec::GitResetHardFetch    => "reset+f",
        CommandSpec::GitCheckout { .. }   => "co",
        CommandSpec::GitCheckoutNew { .. } => "co -b",
    }
}
```

#### Inline test pattern — `#[cfg(test)] mod tests` (mirrors `src/domain/refresh.rs` lines 71–248 and `src/domain/jira.rs` lines 40–end)

The project convention for pure helpers: inline `mod tests` at the bottom of the file, `use super::*;`, individual `#[test]` functions each testing a single case with a descriptive name. No separate test file. Follows `domain/refresh.rs` exactly:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // --- spinner_frame: boundary cases ---
    #[test] fn frame_at_0ms()    { assert_eq!(spinner_frame(Duration::from_millis(0)),   "◐"); }
    #[test] fn frame_at_149ms()  { assert_eq!(spinner_frame(Duration::from_millis(149)), "◐"); }
    #[test] fn frame_at_150ms()  { assert_eq!(spinner_frame(Duration::from_millis(150)), "◓"); }
    #[test] fn frame_at_749ms()  { assert_eq!(spinner_frame(Duration::from_millis(749)), "◐"); }
    #[test] fn frame_at_750ms()  { assert_eq!(spinner_frame(Duration::from_millis(750)), "◓"); }
    #[test] fn frame_wraps_at_900ms() { assert_eq!(spinner_frame(Duration::from_millis(900)), "◐"); }

    // --- format_elapsed: boundary cases (D-08) ---
    #[test] fn elapsed_0s()   { assert_eq!(format_elapsed(Duration::from_secs(0)),   "0s"); }
    #[test] fn elapsed_42s()  { assert_eq!(format_elapsed(Duration::from_secs(42)),  "42s"); }
    #[test] fn elapsed_59s()  { assert_eq!(format_elapsed(Duration::from_secs(59)),  "59s"); }
    #[test] fn elapsed_60s()  { assert_eq!(format_elapsed(Duration::from_secs(60)),  "1:00"); }
    #[test] fn elapsed_61s()  { assert_eq!(format_elapsed(Duration::from_secs(61)),  "1:01"); }
    #[test] fn elapsed_600s() { assert_eq!(format_elapsed(Duration::from_secs(600)), "10:00"); }
    #[test] fn elapsed_723s() { assert_eq!(format_elapsed(Duration::from_secs(723)), "12:03"); }

    // --- task_short_label: spot-check selected variants ---
    #[test]
    fn yarn_install_label()     { assert_eq!(task_short_label(&CommandSpec::YarnInstall), "yarn"); }
    #[test]
    fn unit_tests_label()       { assert_eq!(task_short_label(&CommandSpec::YarnUnitTests), "unit-tests"); }
    #[test]
    fn git_checkout_new_label() {
        assert_eq!(task_short_label(&CommandSpec::GitCheckoutNew { branch: "x".into() }), "co -b");
    }
    #[test]
    fn reset_hard_fetch_label() { assert_eq!(task_short_label(&CommandSpec::GitResetHardFetch), "reset+f"); }
    #[test]
    fn shell_label() {
        assert_eq!(task_short_label(&CommandSpec::ShellCommand { command: "".into() }), "shell");
    }
}
```

---

## Shared Patterns

### Span::styled coloring
**Source:** `src/ui/panels.rs` lines 87, 94, 97
**Apply to:** Y cell (yellow while spinning), P cell (yellow while spinning), static staleness cells.
```rust
// Yellow spinner glyph
Span::styled(frame, Style::default().fg(Color::Yellow))
// Static red staleness
Span::styled("Y", Style::default().fg(Color::Red))
// Static green fresh
Span::styled("Y", Style::default().fg(Color::Green))
```

### Multi-span cell construction
**Source:** `src/ui/panels.rs` line 115
**Apply to:** icon cell only (Y and P are added as separate spans to `icon_spans`; task column uses `Cell::from(String)`).
```rust
Cell::from(Line::from(icon_spans))
```

### Style with modifier
**Source:** `src/ui/panels.rs` line 87
**Apply to:** Metro indicator span only (BOLD).
```rust
Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
```

### Inline `#[cfg(test)] mod tests`
**Source:** `src/domain/refresh.rs` lines 71–248; `src/domain/jira.rs` lines 40+
**Apply to:** `src/ui/indicators.rs` — all pure helpers tested inline, no separate file.

### Exhaustive match (no `_ =>`)
**Source:** `src/domain/command.rs` `collision_policy()` at lines 172–206; meta-test at lines 501–562
**Apply to:** `task_short_label` in `src/ui/indicators.rs` — must list all 23 `CommandSpec` variants explicitly.

---

## Module Registration

### `src/ui/mod.rs` — add one line
**Source:** `src/ui/mod.rs` lines 7–12 (existing `pub mod` declarations)
```rust
// Current module declarations (lines 7-12):
pub mod footer;
pub mod help_overlay;
pub mod error_overlay;
pub mod modals;
pub mod panels;
pub mod theme;

// Add after line 12:
pub mod indicators;
```

---

## No Analog Found

All files have strong analogs. No entries in this section.

---

## Metadata

**Analog search scope:** `src/ui/`, `src/domain/`
**Files scanned:** `src/ui/panels.rs`, `src/ui/mod.rs`, `src/ui/theme.rs`, `src/ui/footer.rs`, `src/domain/command.rs`, `src/domain/refresh.rs`, `src/domain/jira.rs`
**Pattern extraction date:** 2026-05-22

**Critical line number pins (executor must verify before editing):**
- `panels.rs:82` — start of icon span block (comment `// Status icons:`)
- `panels.rs:86–90` — metro indicator spans
- `panels.rs:92–95` — yarn Y span
- `panels.rs:96–98` — pods /P span
- `panels.rs:114–120` — `rows.push(Row::new(vec![...]))` — 4-cell version
- `panels.rs:123–137` — metro detail row — 4-cell version
- `panels.rs:156–164` — `Table::new(rows, [...])` Constraint array — 4-column version
- `command.rs:172–206` — `collision_policy()` exhaustive match (template for `task_short_label`)
- `command.rs:248–274` — `label()` exhaustive match (second template reference)
- `ui/mod.rs:12` — last existing `pub mod` line (add `pub mod indicators;` after)
