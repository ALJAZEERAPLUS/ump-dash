# Repo-Local Architecture Review Agent Design

Date: 2026-06-08
Status: Ready for user review

## Context

`ump-dash` is a single Rust crate with intentional layer separation:

- `src/domain`: domain model, pure policy, and domain-owned port traits.
- `src/app`: TEA state mutation, key handling, effects, runtime, and adapter injection.
- `src/infra`: concrete process, filesystem, JIRA, multiplexer, cache, and device adapters.
- `src/ui`: Ratatui rendering over app/domain state.
- `src/main.rs`: composition root and CLI dispatch.

The repository already has architecture intent encoded in module docs,
`CLAUDE.md`, design specs, and `make arch-lint`. The current `arch-lint`
target is valuable because it checks repo-specific invariants such as app/UI
not importing infra, `update.rs` purity, single-source keybinding usage, and
removed global task fields staying deleted.

The desired architecture reviewer should not be a generic AI reviewer. It
should be a repo-local system that gathers deterministic evidence first, then
uses an agent to judge evidence-backed architectural drift.

## Goal

Create a repeatable architecture review workflow for this repository that:

1. Runs deterministic static checks and reports concrete architecture facts.
2. Gives the agent a compact, repo-specific architecture model.
3. Produces findings only when backed by code evidence.
4. Separates hard violations from hypotheses and deepening opportunities.
5. Can be benchmarked with seeded positive and negative cases before it gates CI.

## Non-Goals

- Generalizing this to other repositories.
- Building a fully autonomous multi-agent team as the first version.
- Making LSP or rust-analyzer a required CI dependency in the first version.
- Replacing `cargo test`, `cargo clippy`, or human review.
- Treating generated memories or uncommitted conversation state as source of truth.
- Failing CI on LLM-only opinions.

## Proposed Shape

The first version has four checked-in pieces:

```text
AGENTS.md or concise CLAUDE.md architecture section
.agents/skills/ump-dash-architecture-review/SKILL.md
docs/architecture/REVIEW-RUBRIC.md
scripts/arch-report.sh or xtask arch-report
```

`CLAUDE.md` already exists, so the least disruptive first step is to add a
short architecture-review section there or create `AGENTS.md` as the portable
entrypoint and keep `CLAUDE.md` as a thin local companion. The always-loaded
file should stay short: architecture layers, exact verification commands, and
review output rules.

The architecture review workflow belongs in a repo-local skill. The skill owns
the repeatable procedure: run the report, read the rubric, inspect relevant
files, and return findings in a strict schema. Detailed examples, seeded cases,
and long rubrics should live in referenced docs rather than the always-loaded
instruction file.

The report command owns deterministic evidence. It should be runnable without
an LLM and should output both human-readable text and machine-readable JSON
when practical.

## Static Evidence

Start with the existing `make arch-lint` guards and preserve their current
value. Add evidence collection in layers:

1. Existing grep guards:
   - app imports infra only through approved exceptions.
   - UI never imports infra.
   - infra does not depend on app `Action`.
   - `update.rs` does not spawn tasks or call process/network APIs.
   - removed global task fields do not reappear.

2. Cargo and compiler facts:
   - `cargo metadata --no-deps --format-version 1`.
   - `cargo test -- --list`.
   - `cargo clippy --all-targets --message-format=json` when requested.

3. Repository metrics:
   - line counts for largest modules.
   - test counts by module.
   - recent churn from `git log --name-only`.
   - hotspots such as `src/app/update.rs`, `src/app/effect_runner.rs`,
     `src/app/state.rs`, and `src/app/keybindings.rs`.

4. Later AST checks:
   - A small Rust checker using `syn` or a stable custom parser path to inspect
     `use` items and obvious fully qualified paths.
   - Enforce a module import matrix for `domain`, `app`, `infra`, `ui`, and
     `main.rs`.
   - Keep grep checks until AST checks prove they cover the same cases.

5. Later exploratory semantic checks:
   - rust-analyzer or LSP reference/call analysis for `Action`, `Effect`,
     `Adapters`, port traits, and `update`.
   - Use this for local reports and reviewer context first, not as a hard gate.

Cargo-level graph tools are useful for dependency policy but cannot enforce
`domain/app/infra/ui` module boundaries directly because this repository is one
crate, not a Cargo workspace with one crate per layer.

## Review Agent Workflow

The skill should run this workflow:

1. Read the architecture entrypoint (`AGENTS.md` if present, otherwise
   `CLAUDE.md`) and `docs/architecture/REVIEW-RUBRIC.md`.
2. Run `make arch-lint` and the architecture report command.
3. Inspect the changed files, or the requested subsystem when no diff is
   provided.
4. Classify each candidate as one of:
   - hard violation
   - likely drift
   - deepening opportunity
   - test gap
   - accepted exception
5. Verify every finding against file and line evidence.
6. Drop or downgrade any finding that cannot cite a repo-local invariant,
   source file, or test gap.
7. Return findings first, ordered by severity.

The finding schema:

```json
{
  "title": "Short issue title",
  "category": "dependency-boundary|purity|state-locality|adapter-leak|test-gap|deepening-opportunity",
  "severity": "low|medium|high|critical",
  "confidence": 0.0,
  "file": "src/app/update.rs",
  "line": 123,
  "violated_invariant": "update.rs returns effects and does not run IO directly",
  "evidence": "Concrete observation from the code or report",
  "impact": "Why this matters for locality, leverage, or correctness",
  "minimal_remediation": "Smallest plausible fix or follow-up check"
}
```

Plain-language review output should still follow the normal code review shape:
findings first, then open questions, then residual risks or test gaps.

## Multi-Agent Use

Use subagents sparingly and only for read-heavy independent work. The main
agent remains the lead reviewer and owns synthesis.

Useful worker scopes:

- Layer mapper: verify domain/app/infra/ui dependencies and exceptions.
- Orchestration reviewer: inspect `update`, `Effect`, `EffectRunner`, and
  `Adapters`.
- Testability reviewer: compare risky flows to unit and integration coverage.
- Deepening reviewer: identify shallow modules or misplaced seams.

Worker contract:

- Read-only.
- Stay inside assigned scope.
- Return evidence-backed findings only.
- Include `file:line` references where possible.
- Do not return raw logs or broad refactor proposals.

The lead reviewer deduplicates worker results, re-checks top findings, and
separates proven issues from hypotheses.

## Benchmark And Evaluation

Treat the reviewer as a detection system.

Seed cases under `tests/architecture_cases/` or an equivalent fixture folder:

- Positive cases:
  - `ui` imports `infra`.
  - `domain` imports Ratatui or process APIs.
  - `update.rs` performs IO instead of returning an effect.
  - infra emits `Action` directly.
  - a removed global task field reappears.

- Negative controls:
  - `main.rs` composing concrete infra adapters.
  - approved temporary exceptions in `effect_runner.rs`.
  - tests importing infra intentionally for integration coverage.
  - domain-owned ports referenced by infra adapters.

Each case records expected findings:

```text
case_id
base_sha
patch_or_fixture
expected_category
expected_severity
expected_file
expected_invariant
accepted_aliases
negative_traps
```

Score the reviewer with:

- schema validity
- precision
- recall
- high-severity recall
- false positives per negative case
- duplicate finding rate
- vague finding rate
- location accuracy
- run-to-run stability

The first benchmark should be local and advisory. CI gating should wait until
the deterministic checks are stable and the LLM review shows acceptable false
positive behavior.

## Report Command Design

Prefer an incremental command:

```text
make arch-report
```

or, if the logic grows, a Rust `xtask`:

```text
cargo run -p xtask -- arch-report
```

Initial `make arch-report` can wrap shell and existing tools:

- run `make arch-lint`
- collect `cargo metadata`
- list tests
- report largest files
- report known architecture hotspots
- report import-boundary grep results

If the report grows beyond simple shell, move it into Rust so the checker can
use structured parsing and emit stable JSON.

## Rollout

1. Document the architecture model and review rubric.
2. Add `arch-report` as non-gating local tooling.
3. Add the repo-local skill that consumes `arch-report`.
4. Create seeded positive and negative architecture cases.
5. Run the reviewer on the cases and record precision/recall.
6. Promote only deterministic checks to CI gates.
7. Keep LLM findings advisory until the benchmark shows low noise.

## Risks

- The reviewer may reward noisy generic advice. Mitigation: reject findings
  without repo-local evidence and a violated invariant.
- Grep checks can miss aliasing, re-exports, macros, and fully qualified paths.
  Mitigation: add AST checks after the shell report proves useful.
- LSP integration may add complexity before it adds reliability. Mitigation:
  keep rust-analyzer exploratory until the first report and benchmark exist.
- Seeded cases can overfit to known rules. Mitigation: include historical
  examples and negative controls.
- Always-loaded instructions can become bloated. Mitigation: keep facts in the
  entrypoint and move workflows/rubrics into skills and docs.

## Research Inputs

The design is consistent with current public guidance:

- OpenAI Codex manual: `AGENTS.md` for durable repo guidance, skills for
  repeatable workflows, hooks/rules for enforcement, and subagents for isolated
  parallel work.
- GitHub Copilot docs: repo-wide, path-specific, and agent instructions give
  Copilot repository-specific build, test, and validation context.
- Claude Code docs: skills package repeatable workflows, and subagents are best
  for focused isolated work with clear output contracts.
- Cursor rules docs: scoped project rules are preferable to large global
  instruction files.
- Aider conventions docs: project conventions can be loaded as read-only
  context.
- Rust tooling docs: `cargo metadata`, Clippy, rust-analyzer, and optional
  Rust parsing tools provide useful structured facts, but intra-crate layer
  boundaries require repo-specific analysis.

