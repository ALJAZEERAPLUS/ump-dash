---
phase: 11-architecture-audit
plan: 03
subsystem: app
tags: [audit, ousterhout, hexagonal, fowler-4-layer, tea-purity, god-object, app-layer, effect-enum, prerequisite-logic, keybindings]

# Dependency graph
requires:
  - phase: 11-architecture-audit
    plan: 00
    provides: AUDIT.md skeleton with ## Module: app/ anchor; 11-validate.sh harness; F-200..F-299 range allocation
  - phase: 11-architecture-audit
    plan: 01
    provides: root+domain sections populated; F-004 MetroHandle tokio leak graded Major (coordinates with F-203 here)
  - phase: 11-architecture-audit
    plan: 02
    provides: infra sections populated; F-101 command_runner Action import graded Critical; six hexagonal port-placement Majors (F-102..F-107, F-110) that F-202 here consumes as port definitions
provides:
  - AUDIT.md `## Module: app/` section populated — 1 file scored (src/app.rs, 2,425 LOC) + 11 findings (F-200..F-210, F-299 was placeholder only)
  - F-200 (Critical) god-object split of app.rs into src/app/{state,update,effect_runner,handle_key,runtime}.rs — concrete 5-file layout
  - F-201 (Critical) TEA impurity in update() with 20 tokio::spawn sites — concrete pub enum Effect sketch with 15+ named variants
  - F-202 (Critical) app→infra direct coupling with 43 crate::infra refs — concrete Adapters struct with trait objects consuming Plan 11-02's ports
  - F-203 (Critical) async metro helpers (7 fns, 218 LOC) moved to src/infra/metro.rs implementing new trait MetroPort in src/domain/ports/metro_port.rs
  - F-204 (Major) inline prerequisite logic at 11 sites — concrete pub enum Prerequisite + pub enum Recipe sketch per REFACTOR-03
  - F-205 (Major) catch-all arms in app.rs — replace _ => {} at 1140/1153 with exhaustive ModalState enumeration
  - F-208 (Major) D-14 keybinding source-of-truth: handle_key as one of three definition sites — concrete struct KeyBinding + KEYBINDINGS registry sketch
  - F-209 (Major) AppState 39-field overexposure — concrete 6-7 sub-struct grouping
  - F-206, F-207, F-210 (Minor) — absorbed by F-201, F-203, F-202 respectively; listed for completeness
affects: [11-04-ui-audit, 11-05-cross-cutting, 13-refactor, 14-per-worktree-tasks]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Per-file Ousterhout score block continues the 11-01/11-02 pattern (File / Public interface / Verdict / Justification)"
    - "Criticals in this plan include concrete D-04 target shape sketches (Effect enum with 15+ variants; app/ submodule 5-file layout; MetroPort 4-method trait; Adapters struct with 8 trait-object fields)"
    - "Cross-referenced Plan 11-02's six port extractions (F-102..F-107, F-110) and Plan 11-01's F-004 MetroHandle finding as prerequisite context for Phase 13 sequencing"

key-files:
  created:
    - .planning/phases/11-architecture-audit/11-03-SUMMARY.md
  modified:
    - .planning/phases/11-architecture-audit/AUDIT.md

key-decisions:
  - "F-200 god-object graded Critical per D-03 face-value Aggressive calibration: 2,425 LOC / 39-field AppState / >=9 unrelated responsibilities clears the '>5 unrelated responsibilities' Critical threshold without ambiguity. D-04 target shape names the 5 new files explicitly (state/update/effect_runner/handle_key/runtime.rs)."
  - "F-201 TEA impurity graded Critical per D-03 face-value: 20 in-function tokio::spawn and spawn_blocking sites (17 spawn + 3 spawn_blocking, verified via grep). D-04 target shape is a 15+ variant pub enum Effect covering every kind of side effect update() currently performs; the new effect_runner.rs consumes Vec<Effect> and performs the spawn calls."
  - "F-202 app→infra coupling graded Critical per D-01 (cross-layer leak on critical path): 43 crate::infra references. D-04 target shape is an Adapters struct holding Arc<dyn Port> for 8 ports (CommandRunnerPort, MetroPort, PortProbePort, WorktreePort, DevicePort, JiraPort, Multiplexer, PersistencePort) — consuming Plan 11-02's port extractions as the trait definitions."
  - "F-203 metro-helpers colocation graded Critical because it blocks F-202's clean injection (no MetroPort trait to inject without this move). Introduces a new trait MetroPort in domain/ports/metro_port.rs with 4 methods (start, send_stdin, kill, http_post); moves 218 LOC into infra/metro.rs::TokioMetroAdapter; pure helpers parse_metro_line + extract_percent stay as private module fns in the adapter."
  - "F-204 inline prerequisite logic at 11 verified sites with 5 coordinating boolean flags (45 in-file references) + VecDeque command_queue — graded Major (not Critical) because the refactor is contingent on F-201 landing first (update() must be pure for Recipe::expand to have a deterministic place to execute). D-04 target shape pairs pub enum Prerequisite and pub enum Recipe."
  - "F-208 D-14 keybinding finding anchored on handle_key as one of three definition sites; Plan 11-04 will capture footer.rs::key_hints_for and help_overlay.rs::render_help; Plan 11-05 finalizes the unified cross-cutting finding. The F-208 Recommendation already contains the full KeyBinding struct + KEYBINDINGS registry sketch so Plan 11-05's finalization is a consolidation, not a re-design."
  - "F-206, F-207, F-210 graded Minor rather than separate Majors because each is structurally absorbed by a sibling Critical (F-201, F-203, F-202 respectively). Listed separately so the cross-reference is explicit and Phase 13 sequencing knows not to stage them independently."
  - "F-299 placeholder at top of AUDIT.md (F-NNN range advertisement) is not a finding — confirmed no actual F-299: finding header appears in the doc."

patterns-established:
  - "Critical findings pair a face-value severity grade with a concrete D-04 target shape sketch (file paths, trait signatures, enum variants) so Phase 13 plans from AUDIT.md rather than re-deciding architecture"
  - "Minors absorbed by a sibling Critical are listed but explicitly deferred ('Do not action independently — fix is absorbed by F-NNN implementation') so Phase 13 sequencing is unambiguous"

requirements-completed: [ARCH-01, ARCH-02, ARCH-03]

# Metrics
duration: ~20min
completed: 2026-04-16
---

# Phase 11 Plan 03: app.rs Architecture Audit Summary

**Single-file audit of `src/app.rs` (2,425 LOC, 41% of codebase) produced 4 Critical findings with concrete D-04 target shapes — god-object split into 5-file `src/app/` submodule, pure `update()` returning `Vec<Effect>` via a 15+ variant enum, Adapters struct injecting 8 domain ports at startup, and a new `MetroPort` trait extracting the 218 LOC of colocated async metro helpers into `src/infra/metro.rs`.**

## Performance

- **Duration:** ~20 minutes
- **Started:** 2026-04-16T17:00:00Z (approx)
- **Completed:** 2026-04-16T17:20:00Z (approx)
- **Tasks:** 2 committed atomically
- **Files modified:** 1 (AUDIT.md — appended app/ section)

## Accomplishments

- `src/app.rs` scored at module level: **Shallow / God-object** verdict with documented evidence (39 pub fields on AppState, ~18 top-level items, 20 tokio::spawn sites inside update(), 43 crate::infra references, 218 LOC of async metro helpers colocated with Service code)
- Four Critical findings with concrete D-04 target shape sketches — Phase 13 can plan the largest refactor in the v1.3 milestone without re-deciding architecture
- Four Major findings with D-08 concrete keywords (`enum`, `replace _ =>`, `struct`, `move`)
- Three Minor findings explicitly marked as "absorbed by sibling Critical — do not action independently" so Phase 13 sequencing stays unambiguous
- Cross-references in place to Plan 11-01 F-004 (MetroHandle tokio leak coordinates with F-203), Plan 11-02 F-101..F-110 (port extractions consumed by F-202), Plan 11-04 (footer.rs + help_overlay.rs keybinding sites), Plan 11-05 (prerequisite enumeration, catch-all enumeration, D-14 finalization)
- `bash 11-validate.sh --module app` exits 0

## Files Audited (1 file, 2,425 LOC)

| File | LOC | Verdict | Findings |
|------|-----|---------|----------|
| `src/app.rs` | 2,425 | **Shallow / God-object** | F-200 Critical, F-201 Critical, F-202 Critical, F-203 Critical, F-204 Major, F-205 Major, F-208 Major, F-209 Major, F-206 Minor, F-207 Minor, F-210 Minor |

## F-NNN IDs Assigned (F-200..F-210, range F-200..F-299)

| ID | Severity | Title | Concrete D-08 keyword |
|----|----------|-------|------------------------|
| F-200 | **Critical** | `app.rs` is a 2,425-LOC god-object with ≥9 unrelated responsibilities | `move ` (app.rs into app/ submodule) |
| F-201 | **Critical** | `update()` directly invokes `tokio::spawn` 20 times — TEA purity violation | `enum ` (pub enum Effect sketch) |
| F-202 | **Critical** | `app.rs` depends on concrete `crate::infra::*` modules instead of domain ports | `trait ` (8 ports via Adapters struct) |
| F-203 | **Critical** | Async metro helpers colocated with Service code — no MetroPort trait | `trait `, `move ` (trait MetroPort + move to infra/metro.rs) |
| F-204 | Major | Inline prerequisite/ordering logic scattered across 11 sites in `update()` (ARCH-05) | `enum ` (pub enum Prerequisite + pub enum Recipe) |
| F-205 | Major | Catch-all match arms in `app.rs` drop inputs without exhaustive coverage (ARCH-04) | `replace _ =>` (exhaustive ModalState enumeration) |
| F-206 | Minor | Recursive `update()` self-dispatch at 7 sites — temporal decomposition | absorbed by F-201 |
| F-207 | Minor | Private `metro_http_post` helper — belongs on MetroPort | absorbed by F-203 |
| F-208 | **Major** | `handle_key` is one of three keybinding definition sites — D-14 | `struct `, `move ` (struct KeyBinding + KEYBINDINGS registry) |
| F-209 | Major | AppState exposes 39 pub fields — Ousterhout "Overexposure" | `struct ` (6-7 sub-struct grouping) |
| F-210 | Minor | Startup wiring inline in `run()` — not a problem today | absorbed by F-202 |

(F-200 .. F-210, no duplicates, no collisions, all within F-200..F-299 range.)

## Severity Distribution

- **Critical:** 4 (F-200, F-201, F-202, F-203) — each with concrete D-04 target shape sketch
- **Major:** 4 (F-204, F-205, F-208, F-209) — each with concrete D-08 keyword in Recommendation
- **Minor:** 3 (F-206, F-207, F-210) — each explicitly absorbed by a sibling Critical

All Critical/Major Recommendations contain D-08 concrete keywords (`trait `, `move `, `enum `, `replace _ =>`, or `struct `). Schema validation (`check_finding_schema`) passes cleanly for all 11 findings.

## Notable Findings for Plan 11-05's Refactor Sequence

Plan 11-05 must include the 8 Critical/Major findings below in the `## Refactor Sequence` appendix (per D-09) with dependency edges:

1. **F-200** (Critical) — Split `src/app.rs` into `src/app/{state,update,effect_runner,handle_key,runtime}.rs`. **Foundational; blocks F-201, F-202, F-203, F-204, F-208, F-209.** Stage first in Phase 13.
2. **F-201** (Critical) — Define `pub enum Effect` (15+ variants) in `src/app/effect.rs`; refactor `update()` to return `(AppState, Vec<Effect>)`; implement `effect_runner.rs`. Depends on F-200.
3. **F-202** (Critical) — Create `src/app/adapters.rs::Adapters` struct; remove every `crate::infra::*` from `src/app/`. Depends on F-200 + Plan 11-02's port extractions (F-101, F-102, F-103, F-104, F-105, F-106, F-110) + a new PersistencePort (Plan 11-02 F-111).
4. **F-203** (Critical) — Create `src/domain/ports/metro_port.rs::trait MetroPort`; create `src/infra/metro.rs::TokioMetroAdapter`; move `app.rs:2209-2425` into the adapter. Depends on F-200 and coordinates with Plan 11-01 F-004 (MetroHandle tokio leak — the new opaque MetroHandle type replaces the leaky struct).
5. **F-204** (Major) — Introduce `pub enum Prerequisite` + `pub enum Recipe` in `src/domain/`; replace the 11 inline ordering sites in `update()`. Depends on F-201 (update() must be pure first).
6. **F-205** (Major) — Replace `_ => {}` literal arms at `src/app.rs:1140` and `:1153` with exhaustive ModalState enumeration. Low-risk — can land early or late.
7. **F-208** (Major) — Create `src/app/keybindings.rs::KEYBINDINGS` registry; refactor `handle_key`, `footer.rs::key_hints_for`, `help_overlay.rs::render_help` to read from it. Depends on F-200.
8. **F-209** (Major) — Group AppState 39 pub fields into 6-7 sub-structs (`MetroState`, `WorktreeBrowserState`, `CommandRunnerState`, `ModalStackState`, `PendingFlags`, `AppConfigState`). Depends on F-200 + F-204 (the PendingFlags struct shrinks after Recipe lands).

## Task Commits

1. **Task 1: Ousterhout score for app.rs + 4 Critical findings (F-200..F-203)** — `0e5deb9` (docs) — god-object split, TEA purity Effect enum, Adapters struct hexagonal injection, MetroPort trait extraction
2. **Task 2: 4 Major + 3 Minor findings (F-204..F-210)** — `f05768c` (docs) — prerequisite Recipe enum, catch-all exhaustive arms, KeyBinding registry, AppState sub-struct grouping, 3 absorbed Minors

(Final metadata commit — SUMMARY.md, STATE.md, ROADMAP.md — is a separate commit below.)

## Decisions Made

- See frontmatter `key-decisions` section — 8 decisions logged covering the rationale for each severity grading, why certain findings are Major rather than Critical (contingency on F-201), and why 3 Minors were kept separate from their absorbing Criticals (to preserve cross-reference traceability for Phase 13).

## Deviations from Plan

None — plan executed exactly as written. The one minor adaptation: Task 1 cross-referenced F-208 (keybinding) and F-209 (AppState sub-structs) inside F-200's Recommendation before those findings existed; this is intentional forward-referencing within a single-plan ID range and causes no validator issue (ID range F-200..F-299 is reserved for this plan, so forward refs are internally consistent).

## Issues Encountered

None. The file is large (2,425 LOC) but the RESEARCH.md enumeration of concrete line numbers for every target category (20 tokio::spawn sites, 11 prerequisite locations, 5 catch-all arms, 43 infra references) made the audit mechanical — grep-verify, quote, grade.

## User Setup Required

None — Phase 11 is a read-only audit. No external services, no code changes.

## Next Phase Readiness

- Plan 11-04 (ui audit) ready to proceed — will score 7 UI files and capture the footer.rs + help_overlay.rs keybinding sites that F-208 cross-references
- Plan 11-05 (cross-cutting) ready to proceed — will enumerate every catch-all arm and prerequisite location that F-204 and F-205 cite, and finalize the unified D-14 keybinding finding by consolidating F-208 + the Plan 11-04 evidence
- Phase 13 (refactor) has a concrete blueprint for the largest refactor in the milestone: 5-file `src/app/` split → pure `update()` with Effect enum → Adapters injection → MetroPort extraction → Recipe for prerequisites → KeyBinding registry → AppState sub-structs, in that dependency order

## Validation Status

- `bash .planning/phases/11-architecture-audit/11-validate.sh --module app` — **exit 0** (OK: Phase 11 validation passed)
- Full-mode validation correctly reports expected failures in ui/ coverage, Refactor Sequence assembly, and cross-cutting catch-all enumeration — all of which are Plan 11-04 / 11-05 work, not in scope for this plan

## Self-Check: PASSED

- AUDIT.md exists at expected path: FOUND
- Commit `0e5deb9` (Task 1): FOUND in git log
- Commit `f05768c` (Task 2): FOUND in git log
- `bash 11-validate.sh --module app` exit code: 0 (PASS)
- All 11 F-2NN findings (F-200..F-210) present with complete D-06 schemas (all six fields) — verified via awk schema scan
- Every Critical/Major Recommendation contains D-08 concrete keyword (`trait`, `move`, `enum`, `replace _ =>`, or `struct`)

---
*Phase: 11-architecture-audit*
*Completed: 2026-04-16*
