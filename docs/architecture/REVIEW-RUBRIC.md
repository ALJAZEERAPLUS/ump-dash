# Architecture Review Rubric

## Purpose

This rubric defines what the `ump-dash` architecture reviewer should flag. It is repo-local and should be read alongside `AGENTS.md`, `CLAUDE.md`, module docs, and `make arch-lint`.

The reviewer is a detection system. It should prefer fewer, evidence-backed findings over broad advice.

## Finding Categories

### dependency-boundary

Use when a module crosses a forbidden layer boundary.

Examples:

- `src/domain/**` imports `crate::infra`, `crate::ui`, Ratatui, `reqwest`, `tokio::process`, or process spawning APIs.
- `src/ui/**` imports `crate::infra`.
- `src/infra/**` imports app-owned `Action` instead of returning domain events or adapter results.
- `src/app/**` imports concrete infra adapters outside documented exceptions.

### purity

Use when app reducer code performs side effects directly.

Examples:

- `src/app/update.rs` calls `tokio::spawn`, `tokio::process`, `reqwest`, filesystem writes, or concrete command runners.
- state transitions depend on live IO instead of returned `Effect` values.

### state-locality

Use when state is stored at the wrong ownership level.

Examples:

- per-worktree task, output, Metro, or cache state is reintroduced as global app state.
- completed worktree slice ownership is bypassed by selected-worktree-only state.

### adapter-leak

Use when concrete infra details leak through an interface that should remain domain/app level.

Examples:

- domain types expose concrete adapter structs.
- app state stores concrete infra types instead of domain port objects or domain data.
- UI chooses behavior by inspecting filesystem or process state directly.

### test-gap

Use when an architecture-sensitive path lacks coverage that would catch a likely regression.

Examples:

- adding an `Effect` variant without a test proving `EffectRunner` interprets it.
- adding a `CommandSpec` without collision policy, refresh policy, label, keybinding, or queue behavior coverage.
- adding a new port without a fake or unit-level test proving app code can exercise it without concrete infra.

### deepening-opportunity

Use when a module is shallow or scattered enough that maintainers lose locality.

This is advisory unless tied to a concrete bug risk. It must explain what interface would become smaller or deeper and what tests would become clearer.

## Severity

### critical

Use only when the issue can corrupt user data, run unsafe destructive commands unintentionally, leak secrets, or break the main command loop broadly.

### high

Use for hard architecture violations that can bypass tests or cause cross-worktree behavior bugs.

Examples:

- reducer side effects in `update.rs`.
- UI or domain importing infra.
- per-worktree task routing regressing to selected-worktree-only behavior.

### medium

Use for likely drift with concrete maintenance or testability impact.

Examples:

- a new app flow bypasses an existing port fake, making tests rely on infra.
- duplicated orchestration spreads one workflow across several unrelated modules.

### low

Use for advisory deepening opportunities or missing minor coverage. Low findings still require file evidence and a minimal remediation.

## Accepted Exceptions

- `src/main.rs` may construct concrete infra adapters.
- integration tests under `tests/**` may import infra to verify real adapter behavior.
- documented temporary exceptions in `src/app/effect_runner.rs` are accepted only when the relevant `make arch-lint` whitelist still names them.
- infra modules may depend on `crate::domain::ports::*` to implement domain-owned ports.

## Required Finding Shape

Every finding must include:

- title
- category
- severity
- confidence from `0.0` to `1.0`
- file and line
- violated invariant
- evidence
- impact
- minimal remediation

JSON shape:

```json
{
  "title": "Reducer starts IO directly",
  "category": "purity",
  "severity": "high",
  "confidence": 0.95,
  "file": "src/app/update.rs",
  "line": 120,
  "violated_invariant": "update() mutates state and returns effects; it does not perform IO",
  "evidence": "The reducer calls tokio::spawn from an Action arm",
  "impact": "Tests can no longer validate flow decisions through returned effects, and side effects become hard to fake",
  "minimal_remediation": "Return an Effect variant and interpret it in EffectRunner"
}
```

## Reject These Findings

- Generic maintainability advice without a violated invariant.
- Style preferences unrelated to architecture.
- Findings without file evidence.
- Findings that ignore accepted exceptions.
- Duplicate findings for the same root cause.
- Broad rewrites where a small guard, test, or port adjustment would address the risk.

## Verification Expectations

Before reporting a finding, re-check the cited file and line. If the finding came from a subagent, the lead reviewer must verify it before including it in the final review.
