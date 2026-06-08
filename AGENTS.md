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
