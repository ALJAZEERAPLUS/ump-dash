---
phase: 11-architecture-audit
plan: 00
subsystem: infra
tags: [audit, bash, validation, ripgrep, ousterhout]

# Dependency graph
requires:
  - phase: 10-context-gathering
    provides: 11-CONTEXT.md with D-01..D-14 decisions, 11-RESEARCH.md with codebase inventory and prerequisite locations, 11-VALIDATION.md with full assertion map
provides:
  - AUDIT.md skeleton with H1 + five module H2s + Cross-Cutting H2 + Refactor Sequence H2
  - 11-validate.sh harness enforcing every 11-VALIDATION.md assertion
  - F-NNN ID range allocation (11-01..11-05 plans get disjoint 100-wide ranges)
  - Known-good append anchors so Wave 1 plans do not touch top-level structure
affects: [11-01-domain-audit, 11-02-infra-audit, 11-03-app-audit, 11-04-ui-audit, 11-05-cross-cutting, 11-06-path-correction, 12-cover, 13-refactor]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Machine-checkable Markdown deliverable (doc + validator script pair)"
    - "Subcommand-based validation (--self-test, --module <name>, --cross-cutting, full)"
    - "Finding schema with 6 mandatory fields for mechanical extraction"

key-files:
  created:
    - .planning/phases/11-architecture-audit/AUDIT.md
    - .planning/phases/11-architecture-audit/11-validate.sh
    - .planning/phases/11-architecture-audit/11-00-SUMMARY.md
  modified: []

key-decisions:
  - "F-NNN ID ranges allocated per plan: 11-01 domain F-001..F-099, 11-02 infra F-100..F-199, 11-03 app F-200..F-299, 11-04 ui F-300..F-399, 11-05 cross F-400..F-499"
  - "Validator script uses REPO_ROOT via dirname+cd so it works from any cwd"
  - "Self-test mode skips coverage/catch-all/prereq/D-14 checks so empty skeleton is green at Wave 0 commit"
  - "check_recommendation_concreteness uses whitespace-terminated keywords ('trait ', 'move ', 'enum ', 'replace _ =') so 'configuration' does not falsely match 'trait' etc."
  - "check_keybinding_finding has 50-line window after the keybinding F-NNN header for handle_key/footer.rs/help_overlay.rs triple — matches D-14 evidence discipline"

patterns-established:
  - "Finding schema: ### [Critical|Major|Minor] F-NNN: <title> + 6 required bullet fields (Location/Dimension/Symptom/Why/Recommendation/Phase-13-hint)"
  - "Ousterhout Verdict line discipline: every covered file gets a 'Verdict:' line within 30 lines of first mention"
  - "Refactor Sequence cross-reference: every Critical+Major F-NNN must reappear by ID in the ## Refactor Sequence appendix (D-09)"

requirements-completed: [ARCH-06]

# Metrics
duration: 4min
completed: 2026-04-16
---

# Phase 11 Plan 00: Validation Harness + AUDIT.md Skeleton Summary

**Bash validation harness (11 checks across 4 subcommand modes) plus structurally-complete AUDIT.md skeleton, making every Wave 1+ plan mechanically verifiable.**

## Performance

- **Duration:** ~4 min
- **Started:** 2026-04-16T15:55:49Z
- **Completed:** 2026-04-16T15:59:25Z
- **Tasks:** 2
- **Files created:** 2

## Accomplishments

- AUDIT.md skeleton with every required H1/H2/H3 anchor from CONTEXT.md §D-05
- 11-validate.sh implementing all 11 assertions from 11-VALIDATION.md §Validation Map
- Self-test (`bash 11-validate.sh --self-test`) exits 0 in ~33ms against the empty skeleton
- F-NNN ID range allocation embedded in the AUDIT.md preamble so every downstream plan knows its range
- Module subcommand (`--module <name>`) validated: reports coverage + Verdict gaps per file (correctly fails on empty skeleton — exactly the signal Wave 1 plans rely on)
- Usage errors return exit code 2; green runs return 0; validation failures return 1

## Task Commits

Each task committed atomically:

1. **Task 1: Create AUDIT.md skeleton with all required structural headers** — `526a488` (docs)
2. **Task 2: Create 11-validate.sh validation harness implementing every assertion in 11-VALIDATION.md** — `7652acf` (feat)

## Files Created/Modified

- `.planning/phases/11-architecture-audit/AUDIT.md` — Audit document skeleton; H1 preamble with severity calibration (D-01) and F-NNN range map, five module H2s (root, domain, infra, app, ui) each with Critical/Major/Minor H3 subheaders, Cross-Cutting H2 with four H3 subsections (catch-all, prerequisite, hexagonal, keybinding), Refactor Sequence H2 placeholder. Zero findings present (Wave 1 plans append).
- `.planning/phases/11-architecture-audit/11-validate.sh` — Executable bash harness (265 lines). Inline 31-file inventory; four mode dispatch (`--self-test`, `--module <name>`, `--cross-cutting`, default full); 11 check functions; depends only on bash/rg/grep/find/awk/sed/diff.

## F-NNN ID Range Allocation (downstream plan reference)

| Plan  | Module        | Range           | Notes |
|-------|---------------|-----------------|-------|
| 11-01 | domain + root | F-001 .. F-099  | 5 domain files + 5 root files (main/tui/event/action/app) |
| 11-02 | infra         | F-100 .. F-199  | 13 infra files — expect highest finding density (port discipline) |
| 11-03 | app           | F-200 .. F-299  | src/app.rs only; must include D-04 target shapes for Criticals |
| 11-04 | ui            | F-300 .. F-399  | 7 ui files + initial keybinding evidence (D-14) |
| 11-05 | cross-cutting | F-400 .. F-499  | Catch-all enumeration, prerequisite locations, cross-module hex, D-14 finalization |
| 11-06 | path correction | n/a           | Touches ROADMAP.md + REQUIREMENTS.md; no F-NNN findings (D-11) |

Validator enforces uniqueness (`sort | uniq -d` returns empty) so collisions are caught at plan-submit time.

## Validator Behavior Matrix (verified at execution time)

| Mode                       | Expected Exit | Actual | Notes                                                      |
|----------------------------|---------------|--------|------------------------------------------------------------|
| `--self-test`              | 0             | 0      | Skeleton-only checks (structure + F-NNN uniqueness) pass  |
| `--module domain`          | 1             | 1      | Fails because no findings yet — coverage gap is correct signal |
| `--module notreal`         | 2             | 2      | Usage error — unknown module name                          |
| `--bogus-flag`             | 2             | 2      | Usage error — unknown flag                                 |
| (no args, full)            | 1             | 1      | Coverage + catch-all + prereq + D-14 fail — expected per plan §verification |

Runtime: ~33ms on self-test (full mode also sub-second). Well under the 5-second budget.

## Decisions Made

- **F-NNN ID range allocation:** Embedded the per-plan 100-wide ranges directly in the AUDIT.md preamble so every downstream plan auditor sees the contract without reading CONTEXT.md.
- **Self-test skip list:** `--self-test` deliberately skips coverage, catch-all, prerequisite, and D-14 checks. Rationale: those only become satisfiable after Wave 1+ plans append findings; gating Wave 0 on them would block this plan from committing.
- **Module-mode verdict window (30 lines):** D-13 asks for a Verdict line "within 20 lines of the file mention". Plan specified 30-line window — kept the plan's value since Wave 1 auditors may use a multi-paragraph score block above the Verdict line.
- **Catch-all check uses `rg` (not `grep`):** Matches the plan verbatim; `rg` is already a project dependency per 11-VALIDATION.md.

## Deviations from Plan

None — plan executed exactly as written. Both files match the plan's inline specifications byte-for-byte (modulo harmless whitespace). The script's subcommands, check functions, file inventory arrays, and error/success paths all conform to the plan.

## Issues Encountered

- **`.planning/` is gitignored:** Phase context anticipated this (verification section: "committed to git (or noted skipped_gitignored if `.planning/` is gitignored — that is expected per phase context"). Resolution: used `git add -f` for both commits, consistent with prior phases whose `.planning/milestones/*` files are tracked.

## Self-Check: PASSED

- `.planning/phases/11-architecture-audit/AUDIT.md` — FOUND (verified via `test -f`)
- `.planning/phases/11-architecture-audit/11-validate.sh` — FOUND and executable (`-rwxr-xr-x`)
- Commit `526a488` (Task 1 AUDIT.md skeleton) — FOUND in `git log --oneline`
- Commit `7652acf` (Task 2 11-validate.sh harness) — FOUND in `git log --oneline`
- `bash 11-validate.sh --self-test` — exit 0 confirmed (33ms runtime)
- All required structural headers present (H1 + 5 module H2 + Cross-Cutting H2 + Refactor Sequence H2 + 15 severity H3 + 4 cross-cutting H3)

## Next Phase Readiness

- Wave 1 plans (11-01 domain, 11-02 infra, 11-03 app, 11-04 ui) can begin in parallel; each plan appends findings under its module H2 and validates with `bash 11-validate.sh --module <name>`
- Wave 2 plan 11-05 (cross-cutting) depends on Wave 1 completion (needs Critical/Major IDs for Refactor Sequence population)
- Wave 2 plan 11-06 (path correction) is independent and can run any time — validated by `check_path_correction`
- No blockers

---
*Phase: 11-architecture-audit*
*Completed: 2026-04-16*
