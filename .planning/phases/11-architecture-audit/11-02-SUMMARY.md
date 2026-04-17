---
phase: 11-architecture-audit
plan: 02
subsystem: infra
tags: [audit, ousterhout, hexagonal, fowler-4-layer, infra-layer]

# Dependency graph
requires:
  - phase: 11-architecture-audit
    plan: 00
    provides: AUDIT.md skeleton with ## Module: infra/ anchor; 11-validate.sh harness; F-100..F-199 range allocation
  - phase: 11-architecture-audit
    plan: 01
    provides: root+domain sections populated; validator regex fix (committed by orchestrator as ab3937a); canonical "Why it's a problem:" phrasing now supported
provides:
  - AUDIT.md `## Module: infra/` section populated — 13 files scored + 12 findings (F-100..F-112)
  - F-101 (Critical) command_runner.rs imports `crate::action::Action` — clearest Fowler layer violation; drives Plan 11-05 Refactor Sequence entry
  - F-102..F-107, F-110 (Major) — six hexagonal-port-placement findings and one UI→infra leak; concrete `trait ` + `move ` recommendations
  - F-100, F-108, F-111, F-112 (Minor) — doc-claim correction, `is_inside_tmux` misplacement, persistence-accessor cohesion, tmux.rs deprecation-only flag (defer to Phase 13)
affects: [11-03-app-audit, 11-04-ui-audit, 11-05-cross-cutting, 13-refactor]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Per-file Ousterhout score block continues the 11-01 pattern (File / Public interface / Verdict / Justification)"
    - "Canonical 'Why it's a problem:' phrasing now valid (validator fix ab3937a)"

key-files:
  created:
    - .planning/phases/11-architecture-audit/11-02-SUMMARY.md
  modified:
    - .planning/phases/11-architecture-audit/AUDIT.md

key-decisions:
  - "F-101 graded Critical (not Major) per CONTEXT.md D-01 Aggressive calibration: `command_runner.rs:12` imports `crate::action::Action` — the Data Source layer knows a Service-layer messaging type. Qualifies as cross-layer leak on the critical path. Recommendation is concrete (`trait CommandOutputSink` in domain; `move` impl to infra adapter)."
  - "Six hexagonal-port Major findings (F-102..F-107, F-110) consolidated under a single pattern: 'trait belongs in domain/, not infra/' per Cockburn strict grading. Each has its own F-NNN with file-specific target shape so Plan 11-05's Refactor Sequence can stage them with clear Depends-on edges."
  - "F-112 (tmux.rs deprecation) flagged Minor with explicit 'DO NOT delete in Phase 11 — defer to Phase 13'. Preserves the audit-doesn't-refactor invariant (D-00)."
  - "F-111 (persistence-accessor proliferation) graded Minor rather than Major because the four small infra modules are each right-sized for their single caller; the cohesion improvement is a nice-to-have, not a refactor-cost <1-day forced move."
  - "F-109 skipped to preserve one-finding-per-concern rule — the original F-109 slot would have duplicated F-108's concern about is_inside_tmux placement."

patterns-established:
  - "Positive-example callouts are still scored at strict grading: e.g., multiplexer.rs is cited in PROJECT.md as a clean trait boundary (gets 'OK — positive example cited' verdict) but still earns F-110 Major under strict hexagonal placement grading"

requirements-completed: [ARCH-01, ARCH-02, ARCH-03]

# Metrics
duration: 33min (two timeouts + recovery)
completed: 2026-04-17
---

# Phase 11 Plan 02: Infra Audit Summary

**Thirteen files scored (all of `src/infra/`), 12 findings logged (1 Critical F-101, 7 Major F-102..F-107+F-110, 4 Minor F-100/F-108/F-111/F-112). `command_runner.rs:12` flagged as the single clearest Fowler layer violation in the codebase; six hexagonal port-placement findings consolidated for Plan 11-05's Refactor Sequence.**

## Files Audited (13)

| File | LOC | Verdict | Finding |
|------|-----|---------|---------|
| `infra/mod.rs` | 15 | OK (minimal re-export hub) | F-100 Minor |
| `infra/port.rs` | 66 | OK (shallow, hex candidate) | F-102 Major |
| `infra/process.rs` | 51 | OK (trait, wrong placement) | F-103 Major |
| `infra/worktrees.rs` | 348 | OK (mixed responsibilities) | F-104 Major |
| `infra/command_runner.rs` | 129 | Shallow — **clearest layer violation** | F-101 Critical |
| `infra/devices.rs` | 273 | OK (parsers deep, runners thin) | F-105 Major |
| `infra/jira.rs` | 175 | OK on trait; misplaced helpers | F-106, F-107 Major |
| `infra/jira_cache.rs` | ~60 | OK (tiny persistence helper) | — |
| `infra/config.rs` | ~90 | OK (appropriate config module) | — |
| `infra/multiplexer.rs` | ~170 | OK — positive example, wrong placement | F-110 Major |
| `infra/sim_history.rs` | ~60 | OK (small, purpose-built) | — |
| `infra/android_prefs.rs` | ~50 | OK (smallest persistence helper) | F-111 Minor (shared) |
| `infra/tmux.rs` | ~40 | Shallow — DEPRECATED | F-112 Minor |

## F-NNN IDs Assigned (F-100..F-112, range F-100..F-199)

| ID | Severity | File | Title | Concrete keyword |
|----|----------|------|-------|------------------|
| F-100 | Minor | infra/mod.rs | Doc-claim "All concrete implementations are behind trait boundaries" is not enforced | — |
| F-101 | **Critical** | infra/command_runner.rs | Imports `crate::action::Action` — Data Source layer knows Service-layer messaging type | `trait `, `move ` |
| F-102 | Major | infra/port.rs | Three free functions for external port probe — no hexagonal port trait | `trait `, `move ` |
| F-103 | Major | infra/process.rs | `ProcessClient` trait belongs in `domain/`, not `infra/` | `trait `, `move ` |
| F-104 | Major | infra/worktrees.rs | Eight free functions for git worktrees — no hexagonal port | `trait `, `move ` |
| F-105 | Major | infra/devices.rs | Pure parsers + async runners — no hexagonal port for device enumeration | `trait `, `move ` |
| F-106 | Major | infra/jira.rs | `JiraClient` trait belongs in `domain/`, not `infra/` | `trait `, `move ` |
| F-107 | Major | infra/jira.rs | `extract_jira_key` is pure domain logic called from `ui/panels.rs:71` — UI→infra leak | `move ` |
| F-108 | Minor | infra/jira.rs | `is_inside_tmux` lives in `infra/jira.rs` but is a multiplexer concern | `move ` |
| F-110 | Major | infra/multiplexer.rs | `Multiplexer` trait belongs in `domain/`, not `infra/` | `trait `, `move ` |
| F-111 | Minor | four small infra modules | Persistence-accessor proliferation — cohesion miss | — |
| F-112 | Minor | infra/tmux.rs | DEPRECATED per its own doc-comment — delete in Phase 13 (NOT Phase 11) | `move `/delete |

*F-109 intentionally skipped (would have duplicated F-108's concern).*

## Severity Distribution

- **Critical:** 1 (F-101)
- **Major:** 7 (F-102, F-103, F-104, F-105, F-106, F-107, F-110)
- **Minor:** 4 (F-100, F-108, F-111, F-112)

All Critical/Major recommendations contain D-08 concrete keywords (`trait `, `move `, or both).

## Notable Findings for Plan 11-05's Refactor Sequence

Plan 11-05 must include these in the `## Refactor Sequence` appendix (per D-09) with dependency edges:

1. **F-101** (Critical) — Introduce `trait CommandOutputSink` in `domain/`, `move` `command_runner.rs` impl to `infra/` adapter that implements the trait. Foundational; unblocks F-102 family.
2. **F-102..F-106, F-110** (Major, six-in-one pattern) — For each `infra/` module exposing a trait or free-fn set, `move` trait into `domain/port/<name>.rs`, keep impl in `infra/`. Can stage per-file; order suggested: process → jira → multiplexer → port → worktrees → devices.
3. **F-107** (Major) — `move` `extract_jira_key` from `infra/jira.rs` into `domain/jira.rs` (or `domain/text.rs`); update `ui/panels.rs:71` import.
4. **F-002** (Plan 11-01 Major, cross-plan) — `move src/action.rs → src/domain/action.rs`; F-101 fix eliminates the `infra/command_runner.rs:12` import so the file-move no longer needs a rewrite.

## Task Commits

1. **Task 1: Audit 7 larger infra files** — `8f13ca8` (docs) — port, process, worktrees, command_runner, devices, jira, mod; all Critical + 6 of 7 Major findings
2. **Task 2: Audit 6 small infra files** — `7d850c9` (docs) — config, jira_cache, multiplexer, sim_history, android_prefs, tmux; 1 remaining Major (F-110) + 4 Minor findings

## Validation Status

- `bash 11-validate.sh --module infra` — **exit 0** (OK: Phase 11 validation passed)
- `bash 11-validate.sh --self-test` — exit 0
- 23 per-file Verdict blocks across root/domain/infra modules now in AUDIT.md

## Deviations

1. **Two executor stream timeouts during Task 1.** First attempt produced nothing durable; second resumed-from-transcript attempt completed both task commits but timed out before writing this SUMMARY.md or updating STATE/ROADMAP. Orchestrator produced plan-completion artifacts inline since the underlying audit work (23 scores + 12 findings) was already committed cleanly on the branch.

2. **F-109 skipped.** Two adjacent concerns in `infra/jira.rs` (F-107 UI→infra leak, F-108 `is_inside_tmux` misplacement) consumed F-107 and F-108; F-109 skipped to keep one-finding-per-concern rule and to preserve a buffer slot if Plan 11-05 needs one in the F-100 range.

3. **Downstream plans unblocked without a pre-Plan-11-05 dependency graph rewrite.** F-101's Critical grading means Plan 11-05's Refactor Sequence gains one new top-level entry compared to what Plan 11-01's SUMMARY anticipated — Plan 11-05 was always going to assemble this list, so no coordination artifact needs editing between now and then.

## Handoff to Plan 11-03 (app.rs)

Plan 11-03 audits the single `src/app.rs` file (2,425 LOC, 41% of codebase). Per CONTEXT.md D-03, `app.rs` god-object cohesion and `update()` TEA purity must be graded at face value against the Aggressive rubric — both expected to be Critical. Per D-04, every Critical must come with a concrete target shape sketch.

The F-101 Critical from this plan is the first Critical in the phase — Plan 11-03 should align its expected 2+ Criticals with the same D-08 concreteness bar (recommendations must contain `trait `, `move `, `enum `, or `replace _ =>`).
