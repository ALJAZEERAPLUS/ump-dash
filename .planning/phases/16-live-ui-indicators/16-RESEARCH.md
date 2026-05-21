# Phase 16: Live UI Indicators — Research

**Researched:** 2026-05-22
**Domain:** Ratatui TUI rendering — Column layout, Span/Cell/Style, pure helper functions, zero new state
**Confidence:** HIGH (all key claims verified directly in source code)

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** New column layout: `[▶ icon][Y][P][branch][ticket+title][dir][task]`. The merged `Y/P` icon cell splits into independent `Y` and `P` glyphs.
- **D-02:** `Y` cell shows spinner only for running `YarnInstall`. `P` cell shows spinner only for running `YarnPodInstall`. Neither carries any other category's state.
- **D-03:** Y/P separator: drop the slash. Render `Y P` (Y, space, P) — 3 visual chars. When yarn runs: `◐ P`; when pod runs: `Y ◐`.
- **D-04:** New task column shows `◐ shortlabel elapsed` for every running task that is NOT `YarnInstall` or `YarnPodInstall`. Empty when no such task runs.
- **D-05:** Glyph set = `◐ ◓ ◑ ◒ ◐ ◓` (6 frames, `const SPINNER_FRAMES: [&str; 6]`). Frame index = `task.started_at.elapsed().as_millis() / 150 % 6`. Column constraint must account for potential variable width in some terminals.
- **D-06:** Spinner color = yellow. Constant while spinning — does not encode staleness.
- **D-07:** Short codes for task column: `yarn`/`pods`/`jest`/`lint`/`types`/`unit-tests`/`run-ios`/`run-and`/`shell`/clean variants; git ops = planner's discretion (short verb: `pull`, `push`, `rebase`, `fetch`, `reset`, `co`, `co-b`, `adb`). Every `CommandSpec` discriminant must have a label so no variant renders blank.
- **D-08:** Elapsed format: seconds under 60 → `42s`; 60+ → `M:SS` e.g. `1:34`, `12:03`. Minutes unpadded.
- **D-09:** While a spinner occupies Y or P: glyph is yellow, staleness color hidden for that cell.
- **D-10:** After task exits: Y/P cell recomputes from `wt.stale`/`wt.stale_pods` on next render. No explicit clear logic needed.

### Claude's Discretion

- Exact git-op short labels in the task column.
- Whether task column `Constraint` is fixed `Length` or `Min` — planner picks based on longest label+time (`◐ unit-tests 12:03` = 18 chars).
- Where spinner-frame helper + elapsed-format helper live in `ui/` (new `ui/indicators.rs` vs inline in `ui/panels.rs`).
- Exact mapping of which `CommandSpec` discriminants count as Y vs P vs task-column.

### Deferred Ideas (OUT OF SCOPE)

- Optimistic green-on-success for Y/P after install.
- Spinner color encoding staleness.
- Inline full task name in row.
- Task history per worktree.
- Per-task progress (%) indicators.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| UI-01 | Split the merged `Y/P` string into two independent cells. Each can independently show its letter or a spinner. | `render_worktree_table` lines 83-98 verified: current single `"Y"` + `"/P"` spans; splitting = replacing with two independent span branches per D-02/D-03. |
| UI-02 | While yarn-family task runs, Y cell shows rotating 6-frame yellow spinner. Same for P when pod-family runs. Every other running task gets spinner + label + elapsed in new task column. Static when idle. | `task_for_worktree(state, &wt.id)` returns `Option<&TaskRecord>`. `TaskRecord.spec: CommandSpec` discriminates Y vs P vs task-column. `TaskRecord.started_at: Instant` drives frame index and elapsed. |
| UI-03 | Live elapsed counter in render path from `started_at.elapsed()`. No mutable tick state in `AppState`. | 250ms tick at `runtime.rs:31` already drives redraw. Render reads `Instant::elapsed()` per frame — zero new state needed. |
</phase_requirements>

---

## Summary

Phase 16 is a pure render modification: split the existing merged `Y/P` icon span into two independent cells, add a new rightmost "task" column, and wire both to the per-worktree `TaskRecord` already populated by Phases 14/15. The 250ms tick at `runtime.rs:31` already drives redraws — no new timers, channels, or `AppState` fields are needed for live animation.

The single file that changes is `src/ui/panels.rs`. The render loop at lines 67-137 iterates `state.worktree_browser.worktrees` (a `Vec<Worktree>`). Each `Worktree` has an `id: WorktreeId` field; the helper `crate::app::state::task_for_worktree(state, &wt.id)` (already called in the same file at line 210) returns `Option<&TaskRecord>`. `TaskRecord` provides `spec: CommandSpec` (to discriminate which column gets the spinner) and `started_at: Instant` (to compute both the frame index and the elapsed display).

The `Constraint` array at lines 158-163 is the only structural change: the existing 4-column array becomes a 5-column array. The icon column width stays at `Length(8)` — the current content is 5 visual chars (`▶ ` + `Y` + `/P`) and the new content is also 5 (`▶ ` + `Y` + ` ` + `P`), so no width change is needed for the icon column.

The one technical risk is the half-circle glyphs (`◐ ◓ ◑ ◒`) having `east_asian_width = A` (Ambiguous) for two of the four frames. Ambiguous glyphs render as 1-cell wide in standard Western terminals (tmux + iTerm2) but 2-cell wide in CJK terminals. The planner must note this; a safe fallback is the braille spinner set (`⠋⠙⠹⠸⠼⠴`), which has `east_asian_width = N` (Neutral = always 1 cell) for all 6 frames.

**Primary recommendation:** Add `ui/indicators.rs` with two pure helpers (`spinner_frame(elapsed: Duration) -> &'static str` and `format_elapsed(elapsed: Duration) -> String`) plus a short-label lookup. Wire into `render_worktree_table` per-row. Extend the `Constraint` array with one `Constraint::Length(20)` for the task column (covers `◐ unit-tests 12:03` = 18 chars with 2 chars margin). All helpers are `#[cfg(test)]`-testable without a terminal.

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Spinner animation (frame selection) | Frontend / UI render | — | Pure function of `Duration`; no network, no I/O. Must stay in `ui/` to satisfy G-02. |
| Elapsed time formatting | Frontend / UI render | — | Pure string formatting of `Duration`. Same G-02 rationale. |
| Short label lookup per CommandSpec | Frontend / UI render | Domain (read-only) | Label table is display-only. Could live in `domain/command.rs` as a `short_label()` method but G-02 says `ui/` imports zero infra — domain imports are fine. Keeping it in `ui/indicators.rs` avoids domain churn. |
| Per-row task state access | App state (read path) | UI read-only consumer | `task_for_worktree(state, &wt.id)` is the domain-sanctioned accessor; render calls it, does not own state. |
| Redraw frequency (250ms tick) | App / runtime | — | Already exists at `runtime.rs:31`; no change. |
| Staleness coloring (Y/P when idle) | Frontend / UI render | — | `wt.stale` / `wt.stale_pods` flags read directly in render; no change to source of truth. |

---

## Ground-Truth Code Inventory

### Current `render_worktree_table` (panels.rs lines 44–196)

**Exact current span construction (lines 83-98):**
```rust
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

**Phase 16 target span construction (icon cell):**
```rust
// Metro indicator (unchanged)
if wt.metro_status == WorktreeMetroStatus::Running {
    icon_spans.push(Span::styled("▶ ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
} else {
    icon_spans.push(Span::raw("  "));
}

// Y cell: spinner if yarn-install running, else staleness color
match task.map(|t| &t.spec) {
    Some(CommandSpec::YarnInstall) => {
        let frame = spinner_frame(task.unwrap().started_at.elapsed());
        icon_spans.push(Span::styled(frame, Style::default().fg(Color::Yellow)));
    }
    _ => {
        let yarn_color = if wt.stale { Color::Red } else { Color::Green };
        icon_spans.push(Span::styled("Y", Style::default().fg(yarn_color)));
    }
}

// Space separator (replaces the slash)
icon_spans.push(Span::raw(" "));

// P cell: spinner if pod-install running, else staleness color
match task.map(|t| &t.spec) {
    Some(CommandSpec::YarnPodInstall) => {
        let frame = spinner_frame(task.unwrap().started_at.elapsed());
        icon_spans.push(Span::styled(frame, Style::default().fg(Color::Yellow)));
    }
    _ => {
        let pods_color = if wt.stale_pods { Color::Red } else { Color::Green };
        icon_spans.push(Span::styled("P", Style::default().fg(pods_color)));
    }
}
```

**Exact current `Row::new` (lines 114-120):**
```rust
rows.push(Row::new(vec![
    Cell::from(Line::from(icon_spans)),
    Cell::from(truncate(branch, 18)),
    Cell::from(ticket_display),
    Cell::from(dir_name),
])
.style(row_style));
```

**Phase 16 target `Row::new`:**
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

**Exact current `Constraint` array (lines 158-163):**
```rust
let table = Table::new(
    rows,
    [
        Constraint::Length(8),  // Status icons (metro + Y + /P)
        Constraint::Length(20), // Branch
        Constraint::Min(20),    // Ticket (merged number + title)
        Constraint::Length(16), // Dir
    ],
)
```

**Phase 16 target `Constraint` array:**
```rust
let table = Table::new(
    rows,
    [
        Constraint::Length(8),  // Status icons (metro + Y + P) — width unchanged
        Constraint::Length(20), // Branch
        Constraint::Min(20),    // Ticket (merged number + title)
        Constraint::Length(16), // Dir
        Constraint::Length(20), // Task (◐ unit-tests 12:03 = 18 chars + 2 margin)
    ],
)
```

**Detail row (lines 125-136) — must also add a 5th empty cell:**
The metro detail row currently has 4 cells. After the change it must have 5:
```rust
Row::new(vec![
    Cell::from(""), Cell::from(""), Cell::from(Span::styled(...)), Cell::from(""),
    Cell::from(""),  // new empty task cell
])
```

---

## CommandSpec Variant Inventory (ALL 23 variants)

**Source: `src/domain/command.rs` lines 9-44, verified directly.**

| Variant | Column | Short Label | Category |
|---------|--------|-------------|----------|
| `YarnInstall` | Y cell (spinner) | `yarn` (only if routed to task col) | Yarn install |
| `YarnPodInstall` | P cell (spinner) | `pods` (only if routed to task col) | Pod install |
| `YarnJest { filter }` | task col | `jest` | Test/quality |
| `YarnLint` | task col | `lint` | Test/quality |
| `YarnCheckTypes` | task col | `types` | Test/quality |
| `YarnUnitTests` | task col | `unit-tests` | Test/quality |
| `RnRunAndroid { .. }` | task col | `run-and` | RN run |
| `RnRunIos { .. }` | task col | `run-ios` | RN run |
| `RnRunIosDevice` | task col | `run-ios` | RN run (auto) |
| `RnReleaseBuild` | task col | `release` | RN build |
| `AdbInstallApk` | task col | `adb` | ADB |
| `ShellCommand { .. }` | task col | `shell` | Shell |
| `RnCleanAndroid` | task col | `clean-and` | Clean |
| `RnCleanCocoapods` | task col | `clean-pod` | Clean |
| `RmNodeModules` | task col | `rm-mods` | Clean |
| `GitPull` | task col | `pull` | Git (non-cancellable but can still run) |
| `GitPush` | task col | `push` | Git |
| `GitFetch` | task col | `fetch` | Git |
| `GitRebase { .. }` | task col | `rebase` | Git |
| `GitResetHard` | task col | `reset` | Git |
| `GitResetHardFetch` | task col | `reset+f` | Git |
| `GitCheckout { .. }` | task col | `co` | Git |
| `GitCheckoutNew { .. }` | task col | `co -b` | Git |

**Note on git ops in task column:** Git variants have `is_cancellable() == false` and `collision_policy() == BlockNew`, but they CAN run and have a `TaskRecord` in the slice. They should still show in the task column while running (no spinner is shown per D-04; the task column also shows the spinner for all task-col entries). The cancel button is a no-op for these, but the display still shows elapsed + label. The planner should confirm whether git ops (non-cancellable) should show the spinner glyph — D-04 says "every running task that is not yarn-install or pod-install" which includes git ops, so they DO get the spinner in the task column.

**Total: 23 variants — matches `collision_policy_covers_every_variant` meta-test assertion.**

---

## Phase 14/15 Hand-off Integrity Verification

**All claims from CONTEXT.md canonical refs verified directly in source:**

### `TaskRecord` (src/domain/task.rs lines 41-50) — VERIFIED
```rust
pub struct TaskRecord {
    pub id: TaskId,
    pub spec: CommandSpec,
    pub started_at: Instant,        // ← render reads .elapsed() from here
    pub handle: Box<dyn TaskHandle>,
}
```
Fields `spec` and `started_at` exist exactly as CONTEXT.md claims. [VERIFIED: src/domain/task.rs]

### `WorktreeSlice` (src/domain/worktree_slice.rs lines 27-34) — VERIFIED
```rust
pub struct WorktreeSlice {
    pub id: WorktreeId,
    pub task: Option<TaskRecord>,   // ← Some while task running, None when idle
    pub queue: VecDeque<CommandSpec>,
    pub output: VecDeque<String>,
    pub output_scroll: usize,
    pub post_drain: Option<Box<Action>>,
}
```
`task: Option<TaskRecord>` exists exactly as claimed. [VERIFIED: src/domain/worktree_slice.rs]

### `task_for_worktree` (src/app/state.rs lines 290-295) — VERIFIED
```rust
pub fn task_for_worktree<'a>(
    state: &'a AppState,
    id: &crate::domain::worktree::WorktreeId,
) -> Option<&'a crate::domain::task::TaskRecord> {
    state.worktrees.get(id).and_then(|s| s.task.as_ref())
}
```
Signature confirmed. Already called in `panels.rs` via `crate::app::state::task_for_worktree(state, id)` at line 210. [VERIFIED: src/app/state.rs]

### `Worktree.id` (src/domain/worktree.rs line 24) — VERIFIED
```rust
pub id: WorktreeId,
```
The render loop iterates `state.worktree_browser.worktrees` (Vec<Worktree>) — each `wt` has `wt.id: WorktreeId` which can be passed to `task_for_worktree`. [VERIFIED: src/domain/worktree.rs]

### 250ms tick (src/app/runtime.rs line 31) — VERIFIED
```rust
let mut tick = tokio::time::interval(std::time::Duration::from_millis(250));
```
Already drives `terminal.draw(|f| crate::ui::view(f, &mut state))` at line 63. No new tick needed. [VERIFIED: src/app/runtime.rs]

---

## Architecture Patterns

### System Architecture Diagram

```
[250ms tokio tick] ──────────────────────────────────────────► terminal.draw()
                                                                      │
                                                                      ▼
                                                           ui::view(f, &mut state)
                                                                      │
                                                                      ▼
                                                    panels::render_worktree_table(f, area, state)
                                                                      │
                                          ┌───────────────────────────┤
                                          │ for wt in worktree_browser.worktrees
                                          │       │
                                          │       ├── task_for_worktree(state, &wt.id)
                                          │       │         │
                                          │       │   Option<&TaskRecord>
                                          │       │         │
                                          │       │   None ─┼─► static Y/P letter + staleness color
                                          │       │         │
                                          │       │   Some(record) ─► match record.spec
                                          │       │                         │
                                          │       │         YarnInstall ────┼──► spinner in Y cell (yellow)
                                          │       │         YarnPodInstall ─┼──► spinner in P cell (yellow)
                                          │       │         everything else ┼──► task col: spinner+label+elapsed
                                          │       │                         │
                                          │       │   record.started_at.elapsed()
                                          │       │         │
                                          │       │   spinner_frame(elapsed) → &'static str
                                          │       │   format_elapsed(elapsed) → String
                                          │       │
                                          │       └── Row::new([icon_cell, branch, ticket, dir, task_cell])
                                          │
                                          └── Table::new(rows, [L(8), L(20), Min(20), L(16), L(20)])
```

### Recommended Project Structure

```
src/ui/
├── panels.rs       — render_worktree_table (primary change site)
├── indicators.rs   — NEW: spinner_frame(), format_elapsed(), task_short_label()
├── theme.rs        — no change
├── footer.rs       — no change
├── help_overlay.rs — no change
├── modals.rs       — no change
└── mod.rs          — add `pub mod indicators;`
```

### Pattern 1: Pure Helper in ui/indicators.rs

**What:** Self-contained pure functions with inline `#[cfg(test)] mod tests`.
**When to use:** Any display logic that takes only domain values and returns strings/styles — no I/O, no mutable state.

```rust
// Source: mirrors domain/refresh.rs and domain/task.rs inline-test convention

use std::time::Duration;
use crate::domain::command::CommandSpec;

pub const SPINNER_FRAMES: [&str; 6] = ["◐", "◓", "◑", "◒", "◐", "◓"];

/// Returns the current spinner frame glyph for a running task.
/// Frame index = elapsed.as_millis() / 150 % 6.
pub fn spinner_frame(elapsed: Duration) -> &'static str {
    let idx = (elapsed.as_millis() / 150 % 6) as usize;
    SPINNER_FRAMES[idx]
}

/// Formats an elapsed duration as seconds (under 60) or M:SS (60+).
/// Examples: 5 → "5s", 59 → "59s", 60 → "1:00", 723 → "12:03".
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

/// Returns the short display code for the task column.
/// Every CommandSpec variant MUST have a label (exhaustive match).
pub fn task_short_label(spec: &CommandSpec) -> &'static str {
    match spec {
        CommandSpec::YarnInstall     => "yarn",
        CommandSpec::YarnPodInstall  => "pods",
        CommandSpec::YarnJest { .. } => "jest",
        CommandSpec::YarnLint        => "lint",
        CommandSpec::YarnCheckTypes  => "types",
        CommandSpec::YarnUnitTests   => "unit-tests",
        CommandSpec::RnRunAndroid { .. } => "run-and",
        CommandSpec::RnRunIos { .. }     => "run-ios",
        CommandSpec::RnRunIosDevice      => "run-ios",
        CommandSpec::RnReleaseBuild      => "release",
        CommandSpec::AdbInstallApk       => "adb",
        CommandSpec::ShellCommand { .. } => "shell",
        CommandSpec::RnCleanAndroid      => "clean-and",
        CommandSpec::RnCleanCocoapods    => "clean-pod",
        CommandSpec::RmNodeModules       => "rm-mods",
        CommandSpec::GitPull             => "pull",
        CommandSpec::GitPush             => "push",
        CommandSpec::GitFetch            => "fetch",
        CommandSpec::GitRebase { .. }    => "rebase",
        CommandSpec::GitResetHard        => "reset",
        CommandSpec::GitResetHardFetch   => "reset+f",
        CommandSpec::GitCheckout { .. }  => "co",
        CommandSpec::GitCheckoutNew { .. } => "co -b",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // spinner_frame boundary tests
    #[test] fn frame_at_0ms_is_index_0() { assert_eq!(spinner_frame(Duration::from_millis(0)), "◐"); }
    #[test] fn frame_at_149ms_is_index_0() { assert_eq!(spinner_frame(Duration::from_millis(149)), "◐"); }
    #[test] fn frame_at_150ms_is_index_1() { assert_eq!(spinner_frame(Duration::from_millis(150)), "◓"); }
    #[test] fn frame_at_749ms_is_index_4() { assert_eq!(spinner_frame(Duration::from_millis(749)), "◐"); }
    #[test] fn frame_at_750ms_is_index_5() { assert_eq!(spinner_frame(Duration::from_millis(750)), "◓"); }
    #[test] fn frame_at_900ms_wraps_to_index_0() { assert_eq!(spinner_frame(Duration::from_millis(900)), "◐"); }

    // format_elapsed boundary tests
    #[test] fn elapsed_0s() { assert_eq!(format_elapsed(Duration::from_secs(0)), "0s"); }
    #[test] fn elapsed_42s() { assert_eq!(format_elapsed(Duration::from_secs(42)), "42s"); }
    #[test] fn elapsed_59s() { assert_eq!(format_elapsed(Duration::from_secs(59)), "59s"); }
    #[test] fn elapsed_60s() { assert_eq!(format_elapsed(Duration::from_secs(60)), "1:00"); }
    #[test] fn elapsed_61s() { assert_eq!(format_elapsed(Duration::from_secs(61)), "1:01"); }
    #[test] fn elapsed_600s() { assert_eq!(format_elapsed(Duration::from_secs(600)), "10:00"); }
    #[test] fn elapsed_723s() { assert_eq!(format_elapsed(Duration::from_secs(723)), "12:03"); }

    // task_short_label exhaustiveness (every variant covered)
    #[test] fn yarn_install_label() { assert_eq!(task_short_label(&CommandSpec::YarnInstall), "yarn"); }
    #[test] fn unit_tests_label_longest() { assert_eq!(task_short_label(&CommandSpec::YarnUnitTests), "unit-tests"); }
    #[test] fn git_checkout_new_label() {
        assert_eq!(task_short_label(&CommandSpec::GitCheckoutNew { branch: "x".into() }), "co -b");
    }
}
```

### Pattern 2: Per-Row Task Lookup in render_worktree_table

```rust
// Within the `for wt in state.worktree_browser.worktrees.iter()` loop:
let task = crate::app::state::task_for_worktree(state, &wt.id);
```

This pattern is already established in the same file (`render_command_output` at line 210). No new import needed.

### Pattern 3: Multi-Span Cell (existing, confirmed in panels.rs)

```rust
// Ratatui 0.30: Cell::from(Line::from(vec![Span, Span, ...])) [VERIFIED: panels.rs:115]
Cell::from(Line::from(icon_spans))
```

For the task column, a simple `Cell::from(String)` or `Cell::from(Span::styled(...))` suffices since the task cell is a single styled span. Using `Line::from(vec![Span, ...])` is only needed if mixing multiple styles in one cell (e.g., spinner in yellow, label in default, elapsed in dim).

### Anti-Patterns to Avoid

- **Storing frame index in AppState:** UI-03 explicitly forbids any mutable tick counter. The frame index is recomputed each render from `started_at.elapsed().as_millis() / 150 % 6`.
- **Calling `Instant::now()` in `update()`:** The reducer must stay pure. `Instant::elapsed()` is called only in the render path.
- **Importing `crate::infra` from `ui/`:** G-02 is enforced by `make arch-lint` via `rg 'crate::infra::' src/ui/`. `ui/indicators.rs` imports only `std::time::Duration` and `crate::domain::command::CommandSpec`.
- **Adding a new `_ =>` catch-all to `task_short_label`:** Defeats the compile-time exhaustiveness guard. Every variant must be named explicitly (same pattern as `collision_policy()`).
- **Forgetting to add a 5th cell to the metro detail row (lines 125-136):** The detail row currently has 4 cells. After the column change it must have 5 (add `Cell::from("")` for the task column), otherwise ratatui will render misaligned columns.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Spinner animation | Custom timer or `AppState` field | `Instant::elapsed().as_millis() / 150 % 6` in render | Stateless formula; 250ms tick already drives redraws |
| Elapsed formatting | Complex string builder | Simple `if secs < 60 { "{s}s" } else { "{m}:{s:02}" }` | D-08 is a two-branch pure function; no library needed |
| Glyph animation library | `throbber-widgets-tui` or similar | `const SPINNER_FRAMES: [&str; 6]` | Explicitly out of scope per REQUIREMENTS.md; avoids MSRV bump to 1.88 |
| Column width calculation | Runtime measurement | `Constraint::Length(20)` chosen from longest label analysis | `◐ unit-tests 12:03` = 18 chars; Length(20) gives 2-char margin |

**Key insight:** This phase has no algorithmic complexity — the hard work (task state, concurrency, cancellation) is done. This is entirely UI wiring.

---

## Half-Circle Glyph Width Risk (D-05)

**Risk level:** LOW for standard Western terminals; MEDIUM for CJK-locale terminals.

**Unicode analysis (verified via Python `unicodedata` module):**

| Glyph | Unicode | east_asian_width | Width in Western term | Width in CJK term |
|-------|---------|------------------|----------------------|-------------------|
| `◐` | U+25D0 | A (Ambiguous) | 1 cell | 2 cells |
| `◓` | U+25D3 | N (Neutral) | 1 cell | 1 cell |
| `◑` | U+25D1 | A (Ambiguous) | 1 cell | 2 cells |
| `◒` | U+25D2 | N (Neutral) | 1 cell | 1 cell |

Two of the four unique frames (`◐` and `◑`) are Ambiguous. In standard macOS + tmux + iTerm2 (the project's target environment per CLAUDE.md notes), Ambiguous glyphs render as 1 cell. The column constraint will be correct for the primary target.

**Verification approach:** After implementing, run the binary in the target terminal (tmux + iTerm2) and confirm that the Y/P cells maintain 1-cell width across all 6 animation frames. Visual inspection is the only reliable check — Unicode tables do not give a terminal-specific answer.

**Safe fallback set (if alignment breaks):** Braille spinner `["⠋", "⠙", "⠹", "⠸", "⠼", "⠴"]` — all 6 glyphs have `east_asian_width = N` (Neutral, always 1 cell). Swapping requires only changing `SPINNER_FRAMES` in `ui/indicators.rs`; no layout logic changes. [ASSUMED — braille rendering in the specific terminal has not been tested]

**ASCII fallback (last resort):** `["-", "\\", "|", "/", "-", "\\"]` — `east_asian_width = Na` (Narrow, always 1 cell). Less visually appealing but guaranteed safe.

---

## Common Pitfalls

### Pitfall 1: Missing 5th Cell in Metro Detail Row
**What goes wrong:** Ratatui `Table` requires every `Row` to have the same number of cells as the `Constraint` array. The metro activity detail row (panels.rs lines 125-136) currently has 4 cells. Adding a 5th column constraint without adding a 5th cell to the detail row will cause a panic or rendering artifact.
**Why it happens:** The detail row is in a separate `if` block and is easy to miss when updating the main row.
**How to avoid:** Search for ALL `Row::new` calls in `render_worktree_table` and add `Cell::from("")` to each.
**Warning signs:** Runtime panic mentioning row/column mismatch, or misaligned columns in the rendered table.

### Pitfall 2: Borrowing `task` Twice in the Spinner Match
**What goes wrong:** The spinner match on `task.map(|t| &t.spec)` borrows `task`. Using `task.unwrap()` inside the match arm will cause a double-borrow compile error.
**Why it happens:** `Option::map` borrows; `unwrap()` moves.
**How to avoid:** Destructure `task` once before the match:
```rust
let task = crate::app::state::task_for_worktree(state, &wt.id);
if let Some(record) = task {
    let frame = spinner_frame(record.started_at.elapsed());
    let label = task_short_label(&record.spec);
    // ...
}
```
Or match on `task` directly with `if let Some(record) = task { match &record.spec { ... } }`.

### Pitfall 3: `task_short_label` Match Non-Exhaustive After Future Variant Addition
**What goes wrong:** A future `CommandSpec` variant (e.g., Phase N adds `GitWorktreeAdd`) silently falls through to a `_ => "?"` catch-all if one was written.
**Why it happens:** Convenience — writing `_ => "?"` avoids updating the match when adding variants.
**How to avoid:** NO `_ =>` arm. Exhaustive match, same as `collision_policy()`. Adding a new `CommandSpec` variant will then fail to compile at `task_short_label`, `collision_policy`, and the drift-guard meta-test simultaneously — three compile-error layers.

### Pitfall 4: Wrong `wt.id` Scope for task_for_worktree
**What goes wrong:** `task_for_worktree(state, &wt.id)` needs a shared borrow of `state`. The render function takes `state: &mut AppState` (needed for `render_stateful_widget`). Rust allows shared borrows alongside mutable borrows as long as they don't overlap — but the borrow checker will reject simultaneous `&state` (for `task_for_worktree`) and `&mut state.worktree_browser.worktree_table_state` (for `render_stateful_widget`).
**Why it happens:** The mutable borrow for `render_stateful_widget` happens AFTER the row-building loop, so there is no actual overlap — the borrow checker should accept it since the shared borrows drop before the mutable borrow begins.
**How to avoid:** Ensure `task_for_worktree` is only called inside the row-building loop (lines 67-137), well before the `render_stateful_widget` call at line 182. Keep the pattern identical to the existing `task_for_worktree` call at line 210 (which is inside a different function, `render_command_output`, but same principle).

### Pitfall 5: `arch-lint` G-02 — ui/ Must Not Import infra
**What goes wrong:** If `ui/indicators.rs` accidentally imports anything from `crate::infra::`, `make arch-lint` fails on the G-02 check: `rg 'crate::infra::' src/ui/`.
**Why it happens:** Domain types like `CommandSpec` live in `crate::domain::`, not `crate::infra::`. The import `use crate::domain::command::CommandSpec;` is correct. Only infra imports trigger G-02.
**How to avoid:** `ui/indicators.rs` imports: `std::time::Duration` + `crate::domain::command::CommandSpec`. Nothing from `crate::infra::`. Run `make arch-lint` as a sanity check after adding the file.

---

## Column Width Analysis

```
Longest task cell: "◐ unit-tests 12:03" = 18 chars
Constraint::Length(20) → 2-char margin, safe for all labels
```

**All task-column labels with max elapsed (`12:03`):**

| Label | Cell content | Length |
|-------|-------------|--------|
| `unit-tests` | `◐ unit-tests 12:03` | 18 |
| `run-ios` | `◐ run-ios 12:03` | 15 |
| `run-and` | `◐ run-and 12:03` | 15 |
| `rm-mods` | `◐ rm-mods 12:03` | 15 |
| `rebase` | `◐ rebase 12:03` | 14 |
| `release` | `◐ release 12:03` | 15 |
| `types` | `◐ types 12:03` | 13 |
| `clean-and` | `◐ clean-and 12:03` | 17 |
| `clean-pod` | `◐ clean-pod 12:03` | 17 |
| `reset+f` | `◐ reset+f 12:03` | 15 |
| `co -b` | `◐ co -b 12:03` | 13 |
| All others | ≤ 13 chars | ≤13 |

**Conclusion:** `Constraint::Length(20)` is sufficient for all labels. `Constraint::Min(16)` would also work (16 > max 18 is wrong — use Min(18) or Length(20)). `Length(20)` is clean and predictable; recommended.

**Icon column (no width change needed):**
- Current: `"  "` (2) + `"Y"` (1) + `"/P"` (2) = 5 chars in `Length(8)` — 3 chars headroom
- New: `"  "` (2) + `"Y"` or `"◐"` (1) + `" "` (1) + `"P"` or `"◐"` (1) = 5 chars in `Length(8)` — same
- No width change required for icon column.

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Global `running_command: Option<CommandSpec>` | Per-worktree `WorktreeSlice.task: Option<TaskRecord>` | Phase 14 | render accesses `task_for_worktree(state, &wt.id)` per row |
| No task UI | Spinner + elapsed in dedicated columns | Phase 16 | requires column layout change |
| Merged `Y/P` span | Independent `Y` cell + `P` cell | Phase 16 | icon column `Constraint::Length(8)` unchanged |
| No task column | New rightmost task column | Phase 16 | `Constraint::Length(20)` added |

**Deprecated/outdated in this phase:**
- `"/P"` span (2 chars with slash): replaced by `" "` (space separator) + `"P"` letter.
- The comment `// Status icons (metro + Y + /P)` on the constraint becomes `// Status icons (metro + Y + P)`.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Braille glyphs `⠋⠙⠹⠸⠼⠴` all render as 1 cell in tmux+iTerm2 | Half-Circle Width Risk | Fallback spinner looks different than expected; layout still correct |
| A2 | Git ops (non-cancellable) should still show spinner+label+elapsed in task column while running | CommandSpec Variant Inventory | If git ops should show nothing, task_short_label for git variants is dead code; no breakage |
| A3 | `Constraint::Length(20)` is wide enough that terminal resize never clips `unit-tests 12:03` | Column Width Analysis | `◐ unit-tests 12:03` clips at 18+ chars; use `Min(18)` instead if resizing is common |

**All other claims in this research were verified directly in source code.**

---

## Open Questions

1. **Should git ops (non-cancellable) show a spinner in the task column while running?**
   - What we know: D-04 says "every running task that is not yarn-install or pod-install" — git ops are running tasks.
   - What's unclear: Whether showing a spinner for non-cancellable tasks confuses users (they can't cancel it anyway).
   - Recommendation: Show it — D-04's wording is clear and it's useful to see that a git pull is running.

2. **Where to place `task_short_label`: `ui/indicators.rs` or `domain/command.rs`?**
   - What we know: Both are valid. `domain/command.rs` already has `label()` (long human-readable). Adding `short_label()` would put both labels together. `ui/indicators.rs` satisfies G-02 trivially.
   - What's unclear: Whether having two label methods on `CommandSpec` is confusing vs. keeping display logic out of domain.
   - Recommendation: `ui/indicators.rs` — the short codes are display-only, not semantic identifiers.

---

## Environment Availability

Step 2.6: SKIPPED — this phase is code-only changes. No external tools, databases, or CLIs are required to implement it. The existing `cargo build` / `cargo test` / `make arch-lint` toolchain is sufficient and is already verified working (Phase 15 complete).

---

## Validation Architecture

`workflow.nyquist_validation` key absent from `.planning/config.json` → treated as enabled.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness (`cargo test`) |
| Config file | none — standard Cargo integration |
| Quick run command | `cargo test -p rn-dash indicators 2>&1 \| tail -20` |
| Full suite command | `cargo test --all-targets 2>&1 \| tail -30` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| UI-01 | `Y` and `P` render as independent glyphs (not merged `Y/P`) | unit (pure) | `cargo test -p rn-dash -q -- indicators::tests` | ❌ Wave 0 — new `ui/indicators.rs` |
| UI-02 | Spinner frame index formula is correct at boundaries (0ms, 149ms, 150ms, 750ms, 900ms) | unit (pure) | `cargo test -p rn-dash -q -- indicators::tests::frame_at` | ❌ Wave 0 |
| UI-02 | `task_short_label` covers all 23 `CommandSpec` variants (no blank) | unit (pure) | `cargo test -p rn-dash -q -- indicators::tests::` | ❌ Wave 0 |
| UI-03 | `format_elapsed` boundary cases: 59s→`"59s"`, 60s→`"1:00"`, 600s→`"10:00"` | unit (pure) | `cargo test -p rn-dash -q -- indicators::tests::elapsed` | ❌ Wave 0 |
| ALL | `make arch-lint` passes (G-02: ui/ imports zero infra) | integration | `make arch-lint` | ✅ exists |
| ALL | Existing tests continue to pass | regression | `cargo test --all-targets 2>&1 \| grep -E 'FAILED\|error'` | ✅ exists |

### Sampling Rate

- **Per task commit:** `cargo test -p rn-dash -q -- indicators 2>&1 | tail -10`
- **Per wave merge:** `cargo test --all-targets 2>&1 | tail -20`
- **Phase gate:** `cargo test --all-targets` clean + `make arch-lint` PASS before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `src/ui/indicators.rs` — covers UI-01/UI-02/UI-03 helper tests (spinner_frame, format_elapsed, task_short_label)
- [ ] `src/ui/mod.rs` — add `pub mod indicators;`

*(No new test files in `tests/` directory needed — pure helpers tested inline per project convention.)*

---

## Security Domain

No security-sensitive code in this phase. Phase 16 is render-only: it reads `Duration` and `CommandSpec` values from already-validated state and formats strings. No user input is processed, no network calls are made, no secrets are handled.

ASVS categories do not apply. Security enforcement is not relevant to a pure display phase.

---

## Sources

### Primary (HIGH confidence)
- `src/ui/panels.rs` — verified current column structure, Constraint array, icon span construction, detail row format
- `src/domain/command.rs` — verified all 23 `CommandSpec` variants, `label()`, `is_cancellable()`, `collision_policy()`
- `src/domain/task.rs` — verified `TaskRecord { id, spec, started_at, handle }` fields
- `src/domain/worktree_slice.rs` — verified `WorktreeSlice.task: Option<TaskRecord>`
- `src/app/state.rs` — verified `task_for_worktree` signature
- `src/domain/worktree.rs` — verified `Worktree.id: WorktreeId`
- `src/app/runtime.rs` — verified 250ms tick at line 31
- `Cargo.lock` — confirmed ratatui = 0.30.0
- `.planning/config.json` — confirmed nyquist_validation absent (treated as enabled)
- `Makefile` — verified G-02 arch-lint check: `rg 'crate::infra::' src/ui/`

### Secondary (MEDIUM confidence)
- Unicode `unicodedata` module (Python 3) — east_asian_width values for `◐◓◑◒` and braille glyphs
- Manual arithmetic — elapsed format boundary values, column width calculations

### Tertiary (LOW confidence — flagged as [ASSUMED])
- Braille glyph rendering in tmux+iTerm2: listed as A1 in Assumptions Log

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — ratatui 0.30.0 confirmed in Cargo.lock; all APIs verified in existing panels.rs usage
- Architecture: HIGH — single render file, Phase 14/15 data structures verified in source
- Pitfalls: HIGH — derived from direct code reading of borrow patterns and arch-lint rules
- Glyph width risk: MEDIUM — Unicode tables clear; terminal rendering behavior assumed for target env

**Research date:** 2026-05-22
**Valid until:** 2026-06-22 (stable domain; ratatui 0.30.x API is stable)
