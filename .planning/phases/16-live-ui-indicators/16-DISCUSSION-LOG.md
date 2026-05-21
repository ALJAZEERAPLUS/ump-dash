# Phase 16: Live UI Indicators - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-22
**Phase:** 16-live-ui-indicators
**Areas discussed:** Grid layout, Spinner glyphs, Task labels, Elapsed format, Y/P split, Staleness color

---

## Area selection

User selected (multiSelect): Staleness color, Non-Y/P task indicator position. User added: "at this point I don't remember what those 6 spinners are and how are we gonna distinguish them and I assume non Y/P stuff are part of these? the question is how our grid will look like to clearly show what means what."

Clarified: the "6-frame spinner" is ONE animated glyph (6 frames = one rotation), not 6 distinct spinners. Category is conveyed by **position**, not by a different spinner. Reframed discussion around grid layout.

---

## Grid layout

| Option | Description | Selected |
|--------|-------------|----------|
| Split Y P + new task col | Y/P fixed cells; new rightmost task col shows spinner+label+elapsed for other categories | ✓ |
| Y P swap to category code | Y P pair morphs into spinner + 2-char code for non-yarn/pod tasks | |
| Spinner + label inline after dir | No new column; running task appended into dir cell | |

**User's choice:** Split Y P + new task col
**Notes:** Clean separation — Y/P only ever carry yarn/pod state; everything else lives in its own column. At most one spinner per row (collision rules → ≤1 task/slice).

---

## Spinner glyph set

| Option | Description | Selected |
|--------|-------------|----------|
| Braille rotate `⠋⠙⠹⠸⠼⠴` | Smooth, 1-cell wide, modern terminals | |
| Classic ASCII `|/-\|/` | Universal, zero font-risk | |
| Half-circles `◐◓◑◒◐◓` | Rotating ring look; variable-width risk | ✓ |

**User's choice:** Half-circles `◐ ◓ ◑ ◒ ◐ ◓`
**Notes:** Width risk flagged → executor verifies column alignment; swap to 1-cell-safe set if it breaks.

---

## Task labels (task column)

| Option | Description | Selected |
|--------|-------------|----------|
| Short codes | yarn / pods / jest / lint / types / run-ios / run-and / shell | ✓ |
| Full command names | yarn-install / pod-install / check-types / run-android | |
| CommandSpec Debug | `format!("{:?}", spec)` truncated | |

**User's choice:** Short codes
**Notes:** Fits narrow column. Label table must cover every CommandSpec discriminant.

---

## Elapsed time format

| Option | Description | Selected |
|--------|-------------|----------|
| Always M:SS | 0:05, 0:42, 1:34, 12:03 | |
| Always MM:SS | 00:05, 01:34, 12:03 (fixed width) | |
| Seconds under 60, then MM:SS | 5s, 42s, 1:34, 12:03 | ✓ |

**User's choice:** Seconds under 60, then M:SS
**Notes:** Compact for short tasks.

---

## Y/P split layout

| Option | Description | Selected |
|--------|-------------|----------|
| Drop slash, single space | `Y P` (Y, space, P) | ✓ |
| Keep slash separator | `Y/P`, spinner replaces letter only | |
| Adjacent no separator | `YP` tightest | |

**User's choice:** Drop slash, single space
**Notes:** Removes slash artifact from merged form; cleanest split.

---

## Staleness color (during/after spinner)

| Option | Description | Selected |
|--------|-------------|----------|
| Yellow during; restore on next render | No special clear logic; recomputes from wt.stale | ✓ |
| Yellow during; force green on Success | Optimistic green on successful exit | |
| Spinner color encodes staleness | Red/green spinner — diverges from yellow req | |

**User's choice:** Yellow during; restore on next render
**Notes:** Simplest; staleness re-emerges naturally next frame.

---

## Claude's Discretion

- Exact git-op short labels in task column
- Task column Constraint: fixed `Length` vs `Min`
- Location of spinner-frame + elapsed-format helpers in `ui/`
- Confirm only YarnInstall→Y, YarnPodInstall→P (default)

## Deferred Ideas

- Optimistic green-on-success for Y/P (rejected for simplicity)
- Spinner color encoding staleness (rejected — diverges from req)
- Inline full task name in row (deferred Phase 14)
- Task history per worktree (REQUIREMENTS §Future)
- Per-task progress (%) indicators (no task emits progress)
