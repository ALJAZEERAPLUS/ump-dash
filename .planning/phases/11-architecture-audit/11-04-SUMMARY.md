---
phase: 11-architecture-audit
plan: 04
subsystem: ui
tags: [audit, ousterhout, hexagonal, fowler-4-layer, ui-layer, keybindings, d-14, drift-evidence]

# Dependency graph
requires:
  - phase: 11-architecture-audit
    plan: 00
    provides: AUDIT.md skeleton with ## Module: ui/ anchor; 11-validate.sh harness; F-300..F-399 range allocation
  - phase: 11-architecture-audit
    plan: 02
    provides: infra sections populated; F-107 extract_jira_key UI->infra leak graded Major (F-300 here is the symmetric UI-side of the same fix)
  - phase: 11-architecture-audit
    plan: 03
    provides: app sections populated; F-208 handle_key graded Major with concrete KeyBinding + KEYBINDINGS registry sketch (F-302 + F-303 here are UI-side definition sites #2 and #3)
provides:
  - AUDIT.md `## Module: ui/` section populated — 7 files scored (src/ui/{mod,panels,footer,help_overlay,error_overlay,modals,theme}.rs, 1,102 LOC total) + 4 Major findings (F-300..F-303)
  - F-300 (Major) ui/panels.rs:71 UI->infra hexagonal leak — symmetric two-line fix paired with Plan 11-02 F-107 (move extract_jira_key to domain + rewrite panels.rs import)
  - F-301 (Major) ui/mod.rs doc-claim contradiction — fix-the-code-not-the-doc; add `! rg 'crate::infra' src/ui/` grep guard after F-300 lands
  - F-302 (Major) ui/footer.rs::key_hints_for is D-14 keybinding definition site #2 — drift evidence captured (Yarn `c` differs across three sites; WorktreeTable `R` conditional dropped in footer)
  - F-303 (Major) ui/help_overlay.rs::render_help is D-14 keybinding definition site #3 — ~55 hand-coded Vec<Row> rows duplicating handle_key + footer
  - Positive verification for modals.rs: rg 'crate::app' src/ui/modals.rs returns empty — cited as Deep Ousterhout example
affects: [11-05-cross-cutting, 13-refactor]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Per-file Ousterhout score block continues the 11-01/11-02/11-03 pattern (File / Public interface / Verdict / Justification)"
    - "UI-side D-14 evidence completes the three-site map: F-208 (handle_key, Plan 11-03) + F-302 (footer.rs) + F-303 (help_overlay.rs) — ready for Plan 11-05 unified finalization"
    - "Plan 11-02 F-107 + Plan 11-04 F-300 established the 'symmetric two-side fix' pattern: the same misplaced helper produces a finding on each side of the layer boundary; Phase 13 fixes both at once via a single file move"

key-files:
  created:
    - .planning/phases/11-architecture-audit/11-04-SUMMARY.md
  modified:
    - .planning/phases/11-architecture-audit/AUDIT.md

key-decisions:
  - "F-300 graded Major per D-01 Aggressive (cross-layer leak on limited-scope pure function) rather than Critical — extract_jira_key is pure (no I/O), the fix is a one-line import rewrite, and the call site is a single line in panels.rs. Critical would imply multi-day refactor or runtime risk; neither applies. Pairs with Plan 11-02 F-107 as a symmetric two-side fix."
  - "F-301 graded Major rather than Minor per Pitfall 2 ('the comment IS the finding'): self-documented contracts that aren't enforced weaken layer discipline more than silent drift, because they create false reader confidence. Kept separate from F-300 so Phase 13 gets an explicit task to add the `! rg 'crate::infra' src/ui/` grep guard after the fix lands."
  - "F-302 and F-303 both graded Major (not Critical) per D-14's own calibration in RESEARCH.md §Keybinding Source-of-Truth Audit: 'evidence shows three definition sites already drifting — but not Critical because the drift hasn't caused a user-facing bug yet.' Same grading as F-208 (handle_key side)."
  - "F-302 and F-303 cross-reference F-208's struct KeyBinding + KEYBINDINGS registry sketch rather than restate it — avoids duplicate D-04 target shape text across three findings and keeps Plan 11-05's unified D-14 finalization as a consolidation (not a re-design)."
  - "modals.rs given a positive verification (Verdict: Deep) rather than a finding — confirms the layer boundary works where it works. Cited as a UI-side reference standard for narrow-interface deep-module (the textbook Ousterhout shape applied to rendering code)."
  - "theme.rs and error_overlay.rs received no findings: theme.rs is pure data (correctly shaped for its caller count); error_overlay.rs is a small single-purpose renderer with clean imports. Documenting 'no finding' is the audit's job per ARCH-01 literal coverage requirement (D-13)."
  - "Ousterhout Verdicts Shallow for footer.rs and help_overlay.rs are *knowledge-duplication* shallow, not interface-width shallow: both files have a 1-pub-fn interface, but the implementation is a hand-coded duplicate of data owned elsewhere (handle_key). Justified per Ousterhout's anti-pattern: shallow depth is determined by what the module hides — hiding nothing original is shallow regardless of impl LOC."

patterns-established:
  - "Shallow-by-duplication pattern: a module with a 1-item public interface is still Shallow if its implementation duplicates data owned by another module — the module hides nothing original. Used to grade footer.rs and help_overlay.rs against D-14."
  - "Symmetric two-side fix pattern: when a misplaced helper sits across a layer boundary, file it as a finding on each side (F-107 infra side + F-300 UI side) with cross-references. Phase 13 resolves both in a single file move."

requirements-completed: [ARCH-01, ARCH-02, ARCH-03]

# Metrics
duration: ~15min
completed: 2026-04-16
---

# Phase 11 Plan 04: UI Layer Architecture Audit Summary

**UI layer audit (7 files, 1,102 LOC) surfaced 4 Major findings — the UI->infra leak at panels.rs:71 paired with its mod.rs doc-claim contradiction, plus the footer.rs and help_overlay.rs D-14 keybinding evidence that finalizes the three-site map for Plan 11-05's unified registry finding.**

## Performance

- **Duration:** ~15 minutes
- **Started:** 2026-04-17T05:02:11Z (approx)
- **Completed:** 2026-04-17T05:17:00Z (approx)
- **Tasks:** 2 committed atomically
- **Files modified:** 1 (AUDIT.md — appended ui/ section)

## Accomplishments

- All 7 `src/ui/` files scored with Ousterhout Verdict lines (mod.rs 83 + panels.rs 267 + footer.rs 161 + help_overlay.rs 137 + error_overlay.rs 53 + modals.rs 376 + theme.rs 25 = 1,102 LOC)
- F-300 captures the single UI->infra leak in the codebase (`ui/panels.rs:71`) paired with Plan 11-02 F-107 as a symmetric two-line fix (both references resolve to the same `move extract_jira_key to domain/worktree.rs` operation in Phase 13)
- F-301 captures the `ui/mod.rs` doc-claim contradiction as a separate finding so Phase 13 adds an explicit `! rg 'crate::infra' src/ui/` grep guard after F-300 lands
- F-302 and F-303 complete the D-14 three-site evidence map (site #1 = `handle_key` via Plan 11-03 F-208; site #2 = `footer.rs` via F-302; site #3 = `help_overlay.rs` via F-303). Drift evidence confirmed and quoted: Yarn `c` description differs across all three sites; WorktreeTable `R` conditional is dropped in footer but captured in help_overlay
- `modals.rs` given a positive Ousterhout verification (Verdict: Deep) with explicit grep proof (`rg 'crate::app' src/ui/modals.rs` returns empty) — cited as the UI-side reference standard for narrow-interface deep-module
- `bash 11-validate.sh --module ui` exits 0

## Files Audited (7 files, 1,102 LOC)

| File | LOC | Verdict | Findings |
|------|-----|---------|----------|
| `src/ui/mod.rs` | 83 | Deep (for its role) | referenced by F-301 (doc-claim contradiction) |
| `src/ui/panels.rs` | 267 | OK on rendering; one UI->infra leak | F-300 Major |
| `src/ui/footer.rs` | 161 | Shallow (by-duplication) | F-302 Major |
| `src/ui/help_overlay.rs` | 137 | Shallow (by-duplication) | F-303 Major |
| `src/ui/error_overlay.rs` | 53 | OK | — |
| `src/ui/modals.rs` | 376 | Deep (positive reference) | — |
| `src/ui/theme.rs` | 25 | OK (pure data) | — |

## F-NNN IDs Assigned (F-300..F-303, range F-300..F-399)

| ID | Severity | Title | Concrete D-08 keyword |
|----|----------|-------|------------------------|
| F-300 | Major | `ui/panels.rs:71` calls `crate::infra::jira::extract_jira_key` directly — UI->infra hexagonal leak | `move ` (extract_jira_key to domain) |
| F-301 | Major | `ui/mod.rs` doc-claim contradicts actual imports — self-documented but unenforced layer contract | `move ` (same operation as F-300 resolves the contradiction) |
| F-302 | Major | `ui/footer.rs::key_hints_for` is D-14 keybinding definition site #2 | `move `, `replace ` (footer hints to iterate KEYBINDINGS; replace 15 inline tables) |
| F-303 | Major | `ui/help_overlay.rs::render_help` is D-14 keybinding definition site #3 | `move ` (help-overlay row generation to `keybindings::help_overlay_rows()`) |

(F-300..F-303, no duplicates, all within F-300..F-399 range. F-304..F-399 unused — available for Plan 11-05 if a cross-cutting finding needs a UI-adjacent ID, though Plan 11-05 uses F-400..F-499.)

## D-14 Evidence Captured (finalizes three-site map for Plan 11-05)

| Site | File | Reference | Finding | Captured by |
|------|------|-----------|---------|-------------|
| #1 | `src/app.rs::handle_key` | lines 260-478 (KeyCode -> Action) | F-208 | Plan 11-03 |
| #2 | `src/ui/footer.rs::key_hints_for` | lines 29-161 (5 palette + 8 modal + 2 panel hint tables; ~70 tuples) | F-302 | Plan 11-04 (this plan) |
| #3 | `src/ui/help_overlay.rs::render_help` | lines 17-112 (~55 hand-coded Vec<Row> rows, 9 sections) | F-303 | Plan 11-04 (this plan) |

Confirmed drift evidence (per RESEARCH §Drift evidence, re-verified via grep):
- **Yarn `c`:** footer.rs:60 `("c", "clean…")` vs help_overlay.rs:72 `"c" -> "Clean… (select targets: pods, android, node_modules)"` vs app.rs:360 `Char('c') => Some(Action::OpenCleanMenu)` — three sources, three different descriptions.
- **WorktreeTable `R`:** handle_key lines 421-427 conditional `MetroSendReload | RefreshWorktrees`; help_overlay.rs:40 correctly captures the conditional ("Reload metro (when running) / Refresh list"); footer.rs:135 drops it (just "reload"). The two UI sites disagree.

Plan 11-05 can now write the unified cross-cutting D-14 finding referencing all three F-IDs and quote this drift evidence without re-grepping.

## Severity Distribution

- **Critical:** 0
- **Major:** 4 (F-300, F-301, F-302, F-303) — each with concrete D-08 keyword in Recommendation
- **Minor:** 0

All Major Recommendations contain D-08 concrete keywords (`move` in all four; `replace ` also in F-302). Schema validation (`check_finding_schema`) passes cleanly for all 4 findings (verified via awk scan).

## Task Commits

1. **Task 1: Audit larger ui files (panels, footer, help_overlay, modals)** — `949dfcb` (docs) — 4 Ousterhout score blocks + F-300..F-303 Major findings
2. **Task 2: Audit small ui files (mod.rs, error_overlay, theme); validate --module ui** — `81603f9` (docs) — 3 additional Ousterhout score blocks; no new findings (mod.rs contradiction fully covered by F-301)

(Final metadata commit — SUMMARY.md, STATE.md, ROADMAP.md — is a separate commit below.)

## Decisions Made

- See frontmatter `key-decisions` section — 7 decisions logged covering severity-grading rationale, the symmetric-two-side-fix pattern, why F-301 is kept separate from F-300, why modals.rs received a positive verification rather than a null finding, and why theme.rs + error_overlay.rs received no findings (clean layer discipline; documenting absence is part of D-13 literal coverage).

## Deviations from Plan

None — plan executed exactly as written. All four mandatory findings (panels.rs:71 hexagonal leak, mod.rs doc-claim contradiction, footer.rs D-14 site, help_overlay.rs D-14 site) captured with the specified severity (Major) and concrete D-08 keyword in each Recommendation.

One minor adaptation: plan Task 1 action text specified finding `b` (mod.rs doc-claim) be captured in Task 1 even though mod.rs itself is Task 2's file. Followed the plan verbatim — F-301 is in Task 1's commit `949dfcb`, and Task 2 only adds the mod.rs Ousterhout score block (no new finding). This matches the plan's explicit guidance and avoids duplicating the contradiction.

## Issues Encountered

None. The UI layer is small (1,102 LOC across 7 files), and RESEARCH.md §"UI" table + §"Drift evidence" enumerated concrete line numbers for every target, making the audit mechanical (grep-verify, quote, grade).

The `.planning` directory is in `.gitignore`; commits used `git add -f` as required for this project's planning workflow.

## User Setup Required

None — Phase 11 is a read-only audit. No external services, no code changes.

## Next Phase Readiness

- **Plan 11-05 (cross-cutting) ready to proceed** with concrete inputs:
  - D-14 unified keybinding finding: cross-references F-208 (handle_key), F-302 (footer.rs), F-303 (help_overlay.rs); drift evidence already quoted in findings; Plan 11-03 F-208's `struct KeyBinding` + `KEYBINDINGS` registry sketch is the target shape to consolidate.
  - Refactor Sequence appendix (D-09): will list F-300, F-301, F-302, F-303 among the Majors. F-300 pairs with F-107 (Plan 11-02) as a single two-line fix; F-301 depends on F-300; F-302 + F-303 depend on F-208's KEYBINDINGS registry landing first.
  - Path correction (D-11): still pending in Plan 11-06.
- **Plan 11-06 (path correction)** unaffected by this plan.
- **Phase 13 (refactor)** now has UI-layer audit complete — zero Criticals in `src/ui/` means the UI refactor is cheap relative to the `src/app/` decomposition: a single `move extract_jira_key` import swap (F-300 + F-107 + F-301) plus a KEYBINDINGS-registry consumer rewrite (F-302 + F-303 + F-208).

## Validation Status

- `bash .planning/phases/11-architecture-audit/11-validate.sh --module ui` — **exit 0** (OK: Phase 11 validation passed)
- `--cross-cutting` and `--full` validators continue to report Plan 11-05 and Plan 11-06 work (Refactor Sequence assembly, catch-all enumeration, path correction) as expected failures — all out of scope for this plan.
- F-3NN concreteness (D-08): manual awk scan confirms every F-3NN Major Recommendation contains at least one of `trait`, `move`, `enum`, `replace _ =>`.
- F-3NN schema (D-06): manual awk scan confirms every F-3NN finding has all six fields (Location, Dimension, Symptom, Why's a problem, Recommendation, Phase 13 task hint).

## Self-Check: PASSED

- AUDIT.md exists at expected path: FOUND
- Commit `949dfcb` (Task 1): FOUND in git log
- Commit `81603f9` (Task 2): FOUND in git log
- `bash 11-validate.sh --module ui` exit code: 0 (PASS)
- All 4 F-3NN findings (F-300..F-303) present with complete D-06 schemas (all six fields) — verified via awk schema scan
- Every Major Recommendation contains a D-08 concrete keyword (`move` in all four; `replace` also in F-302) — verified via awk concreteness scan
- All 7 `src/ui/` files have Verdict lines under `## Module: ui/` — verified via `grep -c '^\*\*File:\*\* \`src/ui/'` = 7
- No duplicate F-NNN IDs across the full document — verified via `grep -oE '^### \[(...)\] F-[0-9]{3}' | sort | uniq -d` returns empty

---
*Phase: 11-architecture-audit*
*Completed: 2026-04-16*
