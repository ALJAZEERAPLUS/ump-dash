# Repo-Local Architecture Review Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first repo-local architecture review system for `ump-dash`: durable guidance, a review rubric, a deterministic `arch-report`, a repo-local skill, and seeded evaluation cases.

**Architecture:** Keep hard facts and commands deterministic, then let the agent review only evidence-backed findings. `AGENTS.md` carries short always-loaded repo guidance, `docs/architecture/REVIEW-RUBRIC.md` carries the detailed review contract, `scripts/arch-report.sh` gathers evidence, and `.agents/skills/ump-dash-architecture-review/SKILL.md` defines the repeatable review workflow.

**Tech Stack:** Rust project tooling, Make, POSIX shell, `rg`, `cargo metadata`, `cargo test -- --list`, repo-local Codex skills.

---

## Scope Check

This plan implements one subsystem: the repo-local architecture review workflow. It does not add rust-analyzer/LSP integration, AST parsing, CI gates, or a Rust `xtask`. Those are intentionally excluded from this first working slice.

## File Structure

- Create `AGENTS.md`: portable repo entrypoint for architecture invariants, commands, and review expectations.
- Create `docs/architecture/REVIEW-RUBRIC.md`: detailed categories, severity rules, output schema, and accepted exceptions.
- Create `scripts/arch-report.sh`: deterministic human-readable report command.
- Modify `Makefile`: add `arch-report` to `.PHONY` and wire the target to the script.
- Create `.agents/skills/ump-dash-architecture-review/SKILL.md`: reusable review workflow that consumes the rubric and report.
- Create `tests/architecture_cases/cases.toml`: seeded positive and negative cases for reviewer evaluation.
- Create `tests/architecture_cases/README.md`: explains how to use the seeded cases.

Do not stage or commit the currently dirty Rust files unless the user explicitly asks. Every `git add` below names only files from this plan.

---

### Task 1: Add Portable Repo Architecture Guidance

**Files:**
- Create: `AGENTS.md`

- [ ] **Step 1: Verify the guidance file does not exist**

Run:

```bash
test ! -f AGENTS.md
```

Expected: command exits `0`.

- [ ] **Step 2: Create `AGENTS.md`**

Create `AGENTS.md` with this exact content:

```markdown
# UMP Dashboard Agent Guidance

## Repository Shape

`ump-dash` is a Rust + Ratatui terminal dashboard for managing UMP React Native worktrees, Metro, native build caches, devices, and related commands.

The main architecture layers are:

- `src/domain`: domain model, pure policy, and domain-owned port traits.
- `src/app`: TEA-style state mutation, key handling, effects, runtime, and adapter injection.
- `src/infra`: concrete process, filesystem, JIRA, multiplexer, cache, and device adapters.
- `src/ui`: Ratatui rendering over app/domain state.
- `src/main.rs`: CLI dispatch and composition root.

## Architecture Invariants

- `domain` owns interfaces and policy. It must not depend on `infra`, `ui`, Ratatui, process spawning, HTTP clients, or concrete adapters.
- `app` owns orchestration. `update()` mutates state and returns effects; it must not perform IO, spawn tasks, or call concrete infra APIs directly.
- `infra` owns concrete side effects and implements domain-owned ports.
- `ui` renders app/domain state and must not import `infra`.
- `main.rs` is allowed to construct concrete infra adapters and inject them into `app::Adapters`.
- Existing temporary exceptions must be documented in the guard or rubric before they are ignored.

## Required Commands

- Fast architecture guard: `make arch-lint`
- Architecture evidence report: `make arch-report`
- Rust tests: `cargo test`
- Rust linting: `cargo clippy --all-targets -- -D warnings`

## Review Guidelines

- Report findings only when backed by repo-local evidence.
- Start with bugs, architecture regressions, and missing tests.
- Each architecture finding must name the violated invariant, cite `file:line`, explain impact, and propose the smallest useful remediation.
- Separate hard violations from hypotheses and deepening opportunities.
- If no findings are found, say so and list any commands not run.
```

- [ ] **Step 3: Verify key guidance is present**

Run:

```bash
rg -n "Architecture Invariants|make arch-report|update\\(\\) mutates state and returns effects" AGENTS.md
```

Expected: three matching lines.

- [ ] **Step 4: Commit Task 1**

Run:

```bash
git add AGENTS.md
git commit -m "docs: add architecture agent guidance"
```

Expected: commit succeeds and does not include `src/app/dispatch_tests.rs` or `src/domain/command.rs`.

---

### Task 2: Add The Architecture Review Rubric

**Files:**
- Create: `docs/architecture/REVIEW-RUBRIC.md`

- [ ] **Step 1: Verify the rubric is absent**

Run:

```bash
test ! -f docs/architecture/REVIEW-RUBRIC.md
```

Expected: command exits `0`.

- [ ] **Step 2: Create the rubric directory**

Run:

```bash
mkdir -p docs/architecture
```

Expected: `docs/architecture` exists.

- [ ] **Step 3: Create `docs/architecture/REVIEW-RUBRIC.md`**

Create `docs/architecture/REVIEW-RUBRIC.md` with this exact content:

```markdown
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
```

- [ ] **Step 4: Verify rubric categories and schema**

Run:

```bash
rg -n "dependency-boundary|Required Finding Shape|Accepted Exceptions|Reject These Findings" docs/architecture/REVIEW-RUBRIC.md
```

Expected: at least four matching lines.

- [ ] **Step 5: Commit Task 2**

Run:

```bash
git add docs/architecture/REVIEW-RUBRIC.md
git commit -m "docs: add architecture review rubric"
```

Expected: commit succeeds and stages only the rubric file.

---

### Task 3: Add The Deterministic Architecture Report

**Files:**
- Create: `scripts/arch-report.sh`
- Modify: `Makefile`

- [ ] **Step 1: Verify `make arch-report` is not wired yet**

Run:

```bash
make arch-report
```

Expected: fails with a message like `No rule to make target 'arch-report'`.

- [ ] **Step 2: Create `scripts/arch-report.sh`**

Create `scripts/arch-report.sh` with this exact content:

```bash
#!/usr/bin/env bash
set -u

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT" || exit 1

STATUS=0

section() {
  printf '\n## %s\n' "$1"
}

run_required() {
  local label="$1"
  shift

  section "$label"
  if "$@"; then
    printf '%s_status=pass\n' "$label"
  else
    printf '%s_status=fail\n' "$label"
    STATUS=1
  fi
}

section "ump-dash architecture report"
printf 'repo=%s\n' "$ROOT"
printf 'commit=%s\n' "$(git rev-parse --short HEAD)"
printf 'branch=%s\n' "$(git rev-parse --abbrev-ref HEAD)"

run_required "arch-lint" make arch-lint

section "cargo metadata"
METADATA_PATH="${TMPDIR:-/tmp}/ump-dash-cargo-metadata.json"
if cargo metadata --no-deps --format-version 1 > "$METADATA_PATH"; then
  printf 'cargo_metadata_status=pass\n'
  printf 'cargo_metadata_path=%s\n' "$METADATA_PATH"
  printf 'cargo_metadata_bytes=%s\n' "$(wc -c < "$METADATA_PATH" | tr -d ' ')"
else
  printf 'cargo_metadata_status=fail\n'
  STATUS=1
fi

section "test inventory"
if cargo test -- --list; then
  printf 'test_inventory_status=pass\n'
else
  printf 'test_inventory_status=fail\n'
  STATUS=1
fi

section "largest rust files"
find src tests -name '*.rs' -print0 \
  | xargs -0 wc -l \
  | sort -nr \
  | sed -n '1,25p'

section "recent rust/doc churn"
git log --since='30 days ago' --name-only --pretty=format: -- src tests docs Makefile \
  | awk 'NF { count[$0]++ } END { for (file in count) print count[file], file }' \
  | sort -nr \
  | sed -n '1,25p'

section "boundary scan: app/ui/domain importing infra"
rg -n 'crate::infra::' src/app src/ui src/domain 2>/dev/null \
  | rg -v '^[^:]+:[0-9]+:\s*//' \
  | rg -v 'src/app/effect_runner\.rs.*(jira_cache|sim_history|task_handle)' \
  || true

section "boundary scan: infra importing app Action"
rg -n 'use crate::(domain::)?action|crate::domain::action::Action' src/infra 2>/dev/null || true

section "purity scan: app side-effect APIs"
rg -n 'tokio::spawn|spawn_blocking|tokio::process|reqwest|std::process::Command|Command::new' src/app 2>/dev/null || true

section "architecture hotspots"
for file in src/app/update.rs src/app/effect_runner.rs src/app/state.rs src/app/keybindings.rs src/domain/command.rs src/infra/native_cache.rs; do
  if [ -f "$file" ]; then
    printf '%s %s lines\n' "$file" "$(wc -l < "$file" | tr -d ' ')"
  fi
done

section "report result"
if [ "$STATUS" -eq 0 ]; then
  printf 'arch_report_status=pass\n'
else
  printf 'arch_report_status=fail\n'
fi

exit "$STATUS"
```

- [ ] **Step 3: Make the script executable**

Run:

```bash
chmod +x scripts/arch-report.sh
```

Expected: `scripts/arch-report.sh` is executable.

- [ ] **Step 4: Update `Makefile` phony targets**

In `Makefile`, change:

```make
.PHONY: cov cov-html cov-baseline cov-check arch-lint
```

to:

```make
.PHONY: cov cov-html cov-baseline cov-check arch-lint arch-report
```

- [ ] **Step 5: Add the `arch-report` target**

In `Makefile`, add this target immediately before `arch-lint:`:

```make
arch-report:
	@./scripts/arch-report.sh
```

- [ ] **Step 6: Run the report**

Run:

```bash
make arch-report
```

Expected:

- output starts with `## ump-dash architecture report`
- output includes `## arch-lint`
- output ends with `arch_report_status=pass`
- command exits `0`

- [ ] **Step 7: Commit Task 3**

Run:

```bash
git add Makefile scripts/arch-report.sh
git commit -m "chore: add architecture report command"
```

Expected: commit succeeds and stages only `Makefile` and `scripts/arch-report.sh`.

---

### Task 4: Add The Repo-Local Architecture Review Skill

**Files:**
- Create: `.agents/skills/ump-dash-architecture-review/SKILL.md`

- [ ] **Step 1: Verify the skill is absent**

Run:

```bash
test ! -f .agents/skills/ump-dash-architecture-review/SKILL.md
```

Expected: command exits `0`.

- [ ] **Step 2: Create the skill directory**

Run:

```bash
mkdir -p .agents/skills/ump-dash-architecture-review
```

Expected: directory exists.

- [ ] **Step 3: Create `SKILL.md`**

Create `.agents/skills/ump-dash-architecture-review/SKILL.md` with this exact content:

```markdown
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
```

- [ ] **Step 4: Verify skill metadata and required workflow**

Run:

```bash
rg -n "name: ump-dash-architecture-review|make arch-report|Finding Contract|Subagent Use" .agents/skills/ump-dash-architecture-review/SKILL.md
```

Expected: four matching lines.

- [ ] **Step 5: Commit Task 4**

Run:

```bash
git add .agents/skills/ump-dash-architecture-review/SKILL.md
git commit -m "docs: add architecture review skill"
```

Expected: commit succeeds and stages only the skill file.

---

### Task 5: Add Seeded Architecture Evaluation Cases

**Files:**
- Create: `tests/architecture_cases/cases.toml`
- Create: `tests/architecture_cases/README.md`

- [ ] **Step 1: Verify architecture cases are absent**

Run:

```bash
test ! -d tests/architecture_cases
```

Expected: command exits `0`.

- [ ] **Step 2: Create the cases directory**

Run:

```bash
mkdir -p tests/architecture_cases
```

Expected: directory exists.

- [ ] **Step 3: Create `tests/architecture_cases/cases.toml`**

Create `tests/architecture_cases/cases.toml` with this exact content:

```toml
[[case]]
id = "positive-ui-imports-infra"
kind = "positive"
category = "dependency-boundary"
severity = "high"
fixture = "Inject `use crate::infra::config;` into `src/ui/panels.rs`."
expected_file = "src/ui/panels.rs"
expected_invariant = "ui must not import infra"
accepted_aliases = ["ui depends on concrete infra", "rendering imports infrastructure"]

[[case]]
id = "positive-domain-imports-ratatui"
kind = "positive"
category = "dependency-boundary"
severity = "high"
fixture = "Inject `use ratatui::style::Color;` into `src/domain/worktree.rs`."
expected_file = "src/domain/worktree.rs"
expected_invariant = "domain must not depend on UI libraries"
accepted_aliases = ["domain imports Ratatui", "domain depends on rendering type"]

[[case]]
id = "positive-update-spawns-task"
kind = "positive"
category = "purity"
severity = "high"
fixture = "Inject `tokio::spawn(async {});` into an `Action` arm in `src/app/update.rs`."
expected_file = "src/app/update.rs"
expected_invariant = "update() returns effects and does not spawn tasks"
accepted_aliases = ["reducer performs side effect", "update.rs contains spawn primitive"]

[[case]]
id = "positive-infra-emits-action"
kind = "positive"
category = "dependency-boundary"
severity = "high"
fixture = "Inject `use crate::domain::action::Action;` into `src/infra/command_runner.rs`."
expected_file = "src/infra/command_runner.rs"
expected_invariant = "infra must not emit app/domain Action directly"
accepted_aliases = ["infra imports Action", "adapter leaks app event grammar"]

[[case]]
id = "positive-global-task-field-reintroduced"
kind = "positive"
category = "state-locality"
severity = "high"
fixture = "Add `pub running_command: Option<crate::domain::command::CommandSpec>,` to `src/app/state.rs`."
expected_file = "src/app/state.rs"
expected_invariant = "per-worktree task state must remain in WorktreeSlice"
accepted_aliases = ["global running command field", "task ownership moved out of worktree slice"]

[[case]]
id = "negative-main-composes-infra"
kind = "negative"
category = "accepted-exception"
severity = "none"
fixture = "`src/main.rs` constructs concrete infra adapters and stores them in `Adapters`."
expected_file = "src/main.rs"
expected_invariant = "main.rs is the composition root"
accepted_aliases = ["composition root exception"]

[[case]]
id = "negative-integration-test-imports-infra"
kind = "negative"
category = "accepted-exception"
severity = "none"
fixture = "`tests/process_group_cancel.rs` imports `ump_dash::infra::command_runner::TokioCommandRunner`."
expected_file = "tests/process_group_cancel.rs"
expected_invariant = "integration tests may import infra for real adapter coverage"
accepted_aliases = ["test imports infra intentionally"]

[[case]]
id = "negative-effect-runner-documented-exception"
kind = "negative"
category = "accepted-exception"
severity = "none"
fixture = "`src/app/effect_runner.rs` uses the documented `jira_cache`, `sim_history`, or `task_handle` whitelist while `make arch-lint` names the exception."
expected_file = "src/app/effect_runner.rs"
expected_invariant = "documented temporary exceptions are accepted only while the guard names them"
accepted_aliases = ["documented whitelist exception"]
```

- [ ] **Step 4: Create `tests/architecture_cases/README.md`**

Create `tests/architecture_cases/README.md` with this exact content:

```markdown
# Architecture Review Seed Cases

These cases benchmark the repo-local architecture reviewer as a detection system.

`cases.toml` contains positive cases the reviewer should flag and negative controls it should not flag. The cases are advisory fixtures, not CI gates.

## How To Use

1. Start from a clean disposable branch or worktree.
2. Apply one fixture from `cases.toml`.
3. Run `make arch-report`.
4. Run the `ump-dash-architecture-review` skill against the changed file.
5. Record whether the reviewer found the expected category and invariant.
6. Revert the fixture before applying the next case.

## Scoring

Track:

- schema validity
- precision
- recall
- high-severity recall
- false positives on negative cases
- duplicate finding rate
- vague finding rate
- location accuracy

The reviewer should not become a CI gate until deterministic checks are stable and negative controls produce low noise.
```

- [ ] **Step 5: Verify seeded cases**

Run:

```bash
rg -n "positive-update-spawns-task|negative-main-composes-infra|false positives on negative cases" tests/architecture_cases
```

Expected: three matching lines.

- [ ] **Step 6: Commit Task 5**

Run:

```bash
git add tests/architecture_cases/cases.toml tests/architecture_cases/README.md
git commit -m "test: add architecture review seed cases"
```

Expected: commit succeeds and stages only the architecture case files.

---

### Task 6: Full Verification

**Files:**
- No new files.

- [ ] **Step 1: Run the architecture lint guard**

Run:

```bash
make arch-lint
```

Expected: exits `0` with `arch-lint: PASS`.

- [ ] **Step 2: Run the architecture report**

Run:

```bash
make arch-report
```

Expected: exits `0` with `arch_report_status=pass`.

- [ ] **Step 3: Run targeted content verification**

Run:

```bash
rg -n "Architecture Invariants|Required Finding Shape|name: ump-dash-architecture-review|positive-ui-imports-infra" AGENTS.md docs/architecture/REVIEW-RUBRIC.md .agents/skills/ump-dash-architecture-review/SKILL.md tests/architecture_cases/cases.toml
```

Expected: at least four matching lines.

- [ ] **Step 4: Confirm only expected uncommitted files remain**

Run:

```bash
git status --short
```

Expected: either clean, or only the pre-existing user changes:

```text
 M src/app/dispatch_tests.rs
 M src/domain/command.rs
```

- [ ] **Step 5: Record final result**

If all commands above pass, report:

```text
Architecture review workflow first slice is implemented.
Verified: make arch-lint, make arch-report, rubric/skill/cases content scan.
Unrelated pre-existing changes remain: src/app/dispatch_tests.rs, src/domain/command.rs.
```

