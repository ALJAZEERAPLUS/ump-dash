# RN Dash

## What This Is

A Rust/Ratatui terminal UI dashboard for managing React Native worktrees. It provides a unified view of the currently running metro instance, all worktrees with their git/JIRA context, and quick access to git operations, RN commands, and Claude Code agents. Keyboard-driven with vim bindings and dynamic on-screen hints. Configurable for any React Native monorepo with git worktrees.

## Core Value

One place to see and control everything about your React Native worktrees — which one is running, what branch each is on, and execute any command without context-switching.

## Requirements

### Validated

- ✓ Running instance zone with metro status, log toggle, debugger/reload/restart controls — v1.0
- ✓ Worktree browser showing all worktrees with branch name, JIRA ticket title, and optional custom labels — v1.0
- ✓ JIRA integration via API token to auto-fetch ticket titles from branch names (UMP-XXXX pattern) — v1.0
- ✓ Git operations per worktree: reset --hard origin, pull, push, rebase, checkout, checkout -b — v1.0
- ✓ RN commands: clean (android/cocoapods), rm node_modules, yarn install, yarn start --reset-cache, yarn pod-install — v1.0
- ✓ UMP run commands: Android/iOS package scripts with target and run-type pickers — v1.0
- ✓ Metro interaction: open debugger (j), reload (r), kill and restart with --reset-cache — v1.0
- ✓ Testing/quality commands: yarn unit-tests, yarn jest [filter], yarn lint --quiet --fix, yarn check-types — v1.0
- ✓ Dependency staleness detection with hints, sync-before-run prompting — v1.0
- ✓ Worktree switching: kill metro in current worktree, auto-start in new one — v1.0
- ✓ Launch Claude Code in new tmux tab at a selected worktree — v1.0
- ✓ Custom labels per worktree/branch that override or accompany JIRA title — v1.0
- ✓ Vim-style keybindings with on-screen key hints — v1.0
- ✓ Only one metro instance running at a time across all worktrees — v1.0
- ✓ Command queue system with per-worktree output persistence — v1.0
- ✓ Multiplexer abstraction (tmux + zellij) — v1.0
- ✓ External metro conflict detection and resolution — v1.0
- ✓ Worktree creation and removal commands — v1.0
- ✓ Metro auto-prerequisite for RN run commands — v1.0

- ✓ Labels feature removed entirely — v1.1
- ✓ (s)ync renamed to (y)arn palette with clean commands absorbed — v1.1
- ✓ Worktree commands extracted from (g)it to lowercase (w)orktree palette — v1.1
- ✓ New worktree creation with interactive base branch picker — v1.1
- ✓ Context-sensitive metro keys (R/J/Esc), MetroRestart removed — v1.1
- ✓ Dynamic footer hints derived from available actions — v1.1
- ✓ Hardcoded AJ/UMP values extracted to DashConfig — v1.1
- ✓ MIT license, README, config example, Cargo.toml metadata — v1.1
- ✓ GitHub Actions CI + release workflow with prebuilt binaries (macOS signed+notarized, Linux) — v1.1
- ✓ TOML config format replaces JSON — v1.1
- ✓ `auto_sync` config param and `SyncBeforeMetro` modal — v1.2 (post-ship quick tasks)

- ✓ Architecture audit (ARCH-01..ARCH-06) — deep-module scoring, D-14 keybinding duplication, app.rs god-object findings — v1.3 Phase 11
- ✓ Coverage gate (COVER-01..COVER-04) — characterization tests for metro single-instance, POSIX process-group kill, TEA dispatch table; baseline coverage 12.84% line / 20.82% function with per-module `floor(baseline, 5)` thresholds — v1.3 Phase 12
- ✓ Audit-driven refactors (REFACTOR-01..REFACTOR-03) — 23 of 25 Critical/Major findings closed in code, 2 deferred with rationale (F-500 → Phase 14, F-501 → backlog); src/app.rs split into src/app/ (state/update/handle_key/runtime/effect_runner/adapters/effect/keybindings/dispatch_tests + 6 sub-structs); pure TEA `update(state, action) -> Vec<Effect>`; hexagonal injection via `pub struct Adapters` holding `Arc<dyn Port>` for all 8 domain ports; Recipe/Prerequisite/DependencyState in domain; CommandSpec::is_cancellable type-driven; 118-entry KEYBINDINGS registry consumed by handle_key + footer + help_overlay; 20 shape guards in `make arch-lint` enforcing architectural invariants — v1.3 Phase 13

### Active

_(See Current Milestone below — requirements defined in `.planning/REQUIREMENTS.md`)_

## Current Milestone: v1.3 Per-Worktree Tasks + Architecture Audit

**Goal:** Audit and refactor architecture where it has drifted from Ousterhout principles, then rework the task/command system to be per-worktree (bound, tracked, cancellable, parallel across worktrees) with live UI indicators.

**Target features:**
- Architecture audit against domain/infra/app/ui boundaries and Ousterhout deep-module criteria
- Refactor phases for each deviation surfaced by the audit
- Per-worktree task ownership — commands bound to a worktree instead of a global queue
- Parallel command execution across worktrees (metro stays single-instance globally)
- Individual command cancellation for yarn/clean/install, UMP Android/iOS runs, and tests (jest/lint/types). Git operations remain non-cancellable
- UI live indicators: worktree table row shows current task name + elapsed time; Y/P letters split (not merged) and replaced by a 6-frame rotating yellow spinner when the yarn-like or pod-like task is running. Run/test tasks get a similar animated indicator

### Out of Scope

- Mobile app or web UI — this is a terminal dashboard only
- Building or modifying the UMP React Native app itself — this tool manages it
- Real-time JIRA sync or ticket creation — read-only ticket title fetching
- Multi-user support — single-user tool

## Context

Shipped v1.1 Public Release — now at ~5,936 LOC Rust, published to GitHub as `rn-dash`.
Tech stack: Rust + Ratatui 0.30, tokio async runtime, crossterm, reqwest for JIRA, TOML config.
Architecture: TEA (The Elm Architecture) with domain/infra/app/ui separation.

- Works with any React Native monorepo (generalized from AJ/UMP in v1.1)
- Only one metro bundler can run at a time across all worktrees (enforced)
- User works in tmux or zellij, dedicating one window to this dashboard
- Branch naming convention configurable (default: JIRA-style PROJ-XXXX)
- Palette submenu keybinding scheme (a/i/x/y/g/w) with vim-style navigation
- Per-worktree command output persistence, FIFO command queue
- External metro conflict detection via port 8081 lsof
- Public GitHub release: MIT licensed, CI on macOS+Linux, tag-triggered prebuilt binaries (signed+notarized on macOS)

## Constraints

- **Tech stack**: Rust + Ratatui — no exceptions
- **Architecture**: Domain logic completely separated from UI and system concerns, following "A Philosophy of Software Design" by John Ousterhout
- **Environment**: macOS (primary), Linux (CI)
- **Config location**: Configurable, default ~/.config/rn-dash/ for JIRA token, preferences

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust + Ratatui for TUI | User preference, performance, type safety | ✓ Good — 5.5k LOC, fast startup, zero runtime crashes |
| Domain/UI/system separation | Ousterhout philosophy, testability, clarity | ✓ Good — clean module boundaries, deep modules |
| Kill + restart on worktree switch | Only one metro allowed, minimize manual steps | ✓ Good — seamless one-keystroke switching |
| Sync-before-run prompting | User-visible prompt replaced lazy auto-install | ✓ Good — more transparent than silent install |
| JIRA API with config token | Auto-fetch ticket titles for branch context | ✓ Good — Basic/Bearer auth, cached locally |
| ~/.config/ump-dash/ for config | XDG-style, separate from repo | ✓ Good — 0600 permissions on credentials |
| Multiplexer abstraction (tmux + zellij) | Support multiple terminal multiplexers | ✓ Good — clean trait boundary |
| Command queue (VecDeque) | Chain dependent commands, show queue count | ✓ Good — enables fetch-then-reset, release build flows |
| External metro conflict detection | Detect port 8081 already in use | ✓ Good — lsof-based PID lookup with kill prompt |
| Metro as prerequisite for RN runs | Auto-start metro before build commands | ✓ Good — prevents RN from spawning unmanaged metro |
| Remove labels feature | Unused in practice, added noise to domain/UI | ✓ Good — clean codebase, no regressions |
| Lowercase palette keys (w/d/b) | Consistency with other palettes, UAT feedback | ✓ Good — fixed in 08-04 gap closure |
| TOML over JSON for config | Better human readability and comments | ✓ Good — switched in 08-05 |
| Rename ump-dash → rn-dash | Generalize beyond AJ/UMP for public release | ✓ Good — shipped to GitHub |
| Config-driven repo paths, JIRA prefix | Remove hardcoded company-specific values | ✓ Good — works for any RN monorepo |
| macOS codesign + notarize in release | Avoid Gatekeeper friction for end users | ✓ Good — clean install experience |
| TEA purity: `update() -> Vec<Effect>` | Restore reducer purity by batching side effects as data | ✓ Good — Phase 13/F-201; testable without tokio, 17 dispatch tests preserved |
| Hexagonal `Adapters` injection | Decouple app/ from infra/ via `Arc<dyn Port>` for all 8 ports | ✓ Good — Phase 13/F-202; main.rs is sole composition root |
| Domain ports module (`domain::ports::*`) | Concentrate the port inventory in one place per Ousterhout | ✓ Good — Phase 13/F-102..F-110; 8 ports: ProcessPort/JiraPort/MultiplexerPort/MetroPort/PortProbePort/WorktreePort/DevicePort/CommandRunnerPort |
| Recipe + Prerequisite + DependencyState in domain | Replace 11 inline prereq sites with declarative pipeline data | ✓ Good — Phase 13/F-204; `Recipe::expand(&DependencyState) -> Vec<CommandSpec>` testable without tokio |
| Flat-enum `is_cancellable` predicate | Type-driven cancellation surface; 8 git variants false, 15 others true | ✓ Good — Phase 13/F-501-deferred; flat enum chosen over category-split |
| KEYBINDINGS registry (118 entries) | Single source of truth consumed by handle_key + footer + help_overlay | ✓ Good — Phase 13/F-208+F-302+F-303+F-400; D-14 keybinding duplication eliminated |
| 20 shape guards in `make arch-lint` | Architectural invariants enforced by grep — fail CI on regression | ✓ Good — Phase 13; G-01..G-20 active and passing |

## Current State

**Shipped:** v1.1 Public Release (2026-04-13) — app published to GitHub as `rn-dash`, CI + signed release binaries live. Post-ship quick tasks rolled into v1.2.0.

**v1.3 in progress:** Phases 11 (architecture-audit), 12 (coverage-gate), and 13 (audit-driven refactors) complete. 79 tests green, clippy clean, coverage baseline locked, all 23 Critical+Major AUDIT findings closed in code (2 deferred with rationale: F-500 → Phase 14, F-501 → backlog). Hexagonal architecture delivered: `update()` is pure, app/ has zero infra imports, all 8 domain ports scaffolded with Adapters injection. Phase 14 (Per-Worktree Task System Foundation) is unblocked and next.

## Next Milestone

_Active: v1.3 Per-Worktree Tasks + Architecture Audit (see Current Milestone above)._

Future candidate directions (deferred):
- Configurable keybinding overrides
- Theme / color customization
- Multi-project support (switch between RN repos)

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-04-23 — v1.3 Phase 12 (coverage-gate) complete; Phase 13 (audit-driven refactors) next*
