---
name: ump-dash-architecture-review
description: Review this repo's Rust architecture for domain/app/infra/ui drift, reducer purity, adapter leaks, state-locality regressions, and missing architecture tests. Use when asked to review architecture, run an architecture audit, or evaluate architectural drift in ump-dash.
---

# UMP Dash Architecture Review

Use this skill for repo-local architecture review of `ump-dash`. This is not a generic Rust review. Findings must be backed by `AGENTS.md`, `CLAUDE.md`, `docs/architecture/REVIEW-RUBRIC.md`, module docs, `make arch-lint`, `make arch-report`, or direct source evidence.

## Inputs

- Current git diff or requested subsystem.
- `AGENTS.md` for short architecture invariants.
- `docs/architecture/REVIEW-RUBRIC.md` for categories, severity, accepted exceptions, and finding shape.
- `make arch-report` output for deterministic evidence.

## Workflow

1. Read `AGENTS.md`, `CLAUDE.md`, and `docs/architecture/REVIEW-RUBRIC.md`.
2. Run `make arch-report`.
3. If reviewing a diff, inspect only changed files plus directly related modules. If reviewing a subsystem, inspect that subsystem and its layer edges.
4. Classify candidates as `hard violation`, `likely drift`, `deepening opportunity`, `test gap`, or `accepted exception`.
5. Verify every candidate against source evidence before reporting it.
6. Drop any candidate that lacks a violated invariant, file evidence, impact, and minimal remediation.
7. Return findings first, ordered by severity. If there are no findings, say that explicitly and list commands not run.

## Categories

Use only categories from `docs/architecture/REVIEW-RUBRIC.md`:

- `dependency-boundary`
- `purity`
- `state-locality`
- `adapter-leak`
- `test-gap`
- `deepening-opportunity`

## Finding Contract

Each finding must include:

- title
- category
- severity
- confidence
- file and line
- violated invariant
- evidence
- impact
- minimal remediation

Use this JSON-compatible shape for structured output when requested:

```json
{
  "title": "Short issue title",
  "category": "dependency-boundary",
  "severity": "high",
  "confidence": 0.95,
  "file": "src/ui/panels.rs",
  "line": 1,
  "violated_invariant": "ui must not import infra",
  "evidence": "The file imports crate::infra directly",
  "impact": "Rendering becomes coupled to concrete side effects and cannot be tested through app/domain state alone",
  "minimal_remediation": "Move the required data into app/domain state or through an existing domain port"
}
```

## Subagent Use

Use read-only subagents only when the review has independent slices. Good slices are:

- layer mapper
- orchestration reviewer
- testability reviewer
- deepening reviewer

Workers must return concise evidence-backed findings with file references. The lead reviewer must re-check every worker finding before reporting it.

## Anti-Patterns

- Do not report generic maintainability advice.
- Do not re-litigate documented accepted exceptions.
- Do not suggest broad rewrites where a small guard, test, or port adjustment addresses the risk.
- Do not treat LLM opinion as a CI gate.
- Do not stage or modify files during a read-only architecture review.
