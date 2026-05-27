---
status: complete
phase: 16-live-ui-indicators
source: [16-01-SUMMARY.md, 16-02-SUMMARY.md]
started: 2026-05-26T18:55:31Z
updated: 2026-05-27T00:30:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Split Y/P cells (no slash)
expected: Worktree table renders Y and P as two independent cells (`Y P` with single-space separator; no `/`). Idle cells use staleness color, not yellow.
result: pass
notes: |
  - df85957: dropped metro icon column + Y/P space separator (flush `YP`).
  - 018cb1e: restored Y/P space separator after spinner-overlap regression
    discovered during UAT test 5 walkthrough — spinner glyph is
    east_asian_width=Ambiguous and overlapped P in user's terminal.
    Metro icon stays removed.

### 2. Y cell spinner during YarnInstall
expected: Trigger yarn install on a worktree (key `Y`). The Y cell on that row replaces the `Y` letter with a yellow rotating spinner (6 frames, ~150ms each). P cell on same row stays a plain `P`. When install finishes, Y cell returns to plain `Y` colored by `wt.stale`.
result: pass

### 3. P cell spinner during YarnPodInstall
expected: Trigger yarn pod install on a worktree (key `P`). The P cell on that row replaces the `P` letter with a yellow rotating spinner. Y cell on same row stays plain `Y`. When install finishes, P cell returns to plain `P` colored by `wt.stale_pods`.
result: pass

### 4. Task column for non-install tasks
expected: Trigger a non-install task on a worktree (e.g. jest, lint, run-ios). Rightmost task column shows `<spinner> <short-label> <elapsed>` (e.g. `◐ jest 3s`). Spinner yellow, glyph cycles every ~150ms. Y and P cells on the row remain idle (no spinner).
result: pass

### 5. Live elapsed advances each redraw
expected: While a non-install task runs, the elapsed value in the task column ticks forward on each ~250ms redraw — `0s` → `1s` → ... → `59s` → `1:00` → `1:01`. No need to press a key; advances on its own.
result: pass

### 6. Elapsed format boundary (Ns vs M:SS)
expected: Elapsed renders as `Ns` (e.g. `42s`) under 60 seconds; switches to `M:SS` (e.g. `1:00`, `12:03`) at 60 seconds and above. Minutes unpadded (no leading zero on minutes).
result: pass

### 7. Column alignment across all 6 spinner frames
expected: Watch the spinner cycle through all 6 frames. Y column, P column, branch column, ticket column, dir column, task column stay vertically aligned with idle rows on every frame. No column shift / drift / wobble between frames.
result: pass

### 8. Configurable spinner glyph set
expected: Default config renders circles (`◐ ◓ ◑ ◒`). Set `spinner_style = "braille"` in `config.toml`, restart `cargo run`, observe braille glyphs (`⠋ ⠙ ⠹ ⠸ ⠼ ⠴`) in spinner cells. Unknown value (e.g. `"foo"`) falls back to circles, no crash.
result: pass

## Summary

total: 8
passed: 8
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

[none yet]
