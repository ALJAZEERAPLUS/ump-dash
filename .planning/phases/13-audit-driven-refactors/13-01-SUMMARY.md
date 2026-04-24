---
phase: 13-audit-driven-refactors
plan: 01
subsystem: domain
tags: [refactor, domain, hexagonal, file-move, wave-a]
dependency_graph:
  requires:
    - .planning/phases/11-architecture-audit/AUDIT.md (F-002, F-003, F-005, F-100, F-103, F-106, F-107, F-110, F-300, F-301)
    - .planning/phases/13-audit-driven-refactors/13-PATTERNS.md
  provides:
    - src/domain/action.rs (Action enum — canonical TEA intent grammar)
    - src/domain/ports/{process_port,jira_port,multiplexer_port}.rs (3 port traits)
    - src/domain/jira.rs (pure extract_jira_key + 6 inline tests)
  affects:
    - src/app.rs (Action/Jira/Multiplexer import + type rewrites)
    - src/app/dispatch_tests.rs (Action import)
    - src/infra/{process,jira,multiplexer,command_runner}.rs (trait removal + import rewrites)
    - src/ui/panels.rs (jira extractor import — F-300 leak closed)
    - src/lib.rs (pub mod action removed)
    - src/domain/mod.rs (action, jira, ports submodules added)
    - src/event.rs (F-003 fall-through comment)
    - src/domain/command.rs (F-005 doc-count fix)
    - src/infra/mod.rs (F-100 cross-reference note)
    - tests/metro_single_instance.rs (Action import path)
    - .planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md (structural ratchet update)
tech_stack:
  added: []
  patterns:
    - Hexagonal ports live in `domain::ports::*_port` (trait-only files, one trait per file)
    - Adapter → port naming: `*Client` infra trait renamed to `*Port` domain trait
    - Pure domain helpers (like extract_jira_key) live alongside inline `#[cfg(test)] mod tests`
    - COVERAGE-THRESHOLDS.md structural-change entries: row migration preserved by moving the
      covering tests with the covered code (70% invariant migrates infra/jira.rs → domain/jira.rs)
key_files:
  created:
    - src/domain/action.rs
    - src/domain/jira.rs
    - src/domain/ports/mod.rs
    - src/domain/ports/process_port.rs
    - src/domain/ports/jira_port.rs
    - src/domain/ports/multiplexer_port.rs
  modified:
    - src/lib.rs
    - src/domain/mod.rs
    - src/domain/command.rs
    - src/domain/worktree.rs
    - src/app.rs
    - src/app/dispatch_tests.rs
    - src/event.rs
    - src/infra/mod.rs
    - src/infra/process.rs
    - src/infra/jira.rs
    - src/infra/multiplexer.rs
    - src/infra/command_runner.rs
    - src/ui/panels.rs
    - tests/metro_single_instance.rs
    - .planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md
  deleted:
    - src/action.rs  (renamed via git mv to src/domain/action.rs — 100% similarity)
decisions:
  - Single atomic commit per plan guidance — 21 files changed in one refactor commit.
    Every intermediate state would have been broken build anyway (removing `pub mod action`
    in lib.rs without a domain::action present => compile error), so stepwise commits
    would have added noise without rollback value.
  - ProcessPort kept `tokio::process::Child` in the signature per the AUDIT's pragmatic
    exception rule (mirrors existing domain/metro.rs:5-13 exception).
  - The ExternalMetroDetected(crate::infra::port::ExternalMetroInfo) variant in Action
    was left untouched — Plan 13-04 (F-102) renames that type and moves it to
    domain::ports::port_probe_port::ExternalProcessInfo. Per plan guidance.
  - COVERAGE-THRESHOLDS.md: infra/jira.rs threshold lowered from 70% to 0% and a new row
    domain/jira.rs added at 100% — this is a structural split (code + covering tests moved
    together), not a test regression. The 70% invariant rebinds to the new location.
metrics:
  started: "2026-04-24T10:15:02Z"
  completed: "2026-04-24T10:22:29Z"
  duration_minutes: 7
  tasks_completed: 1
  files_modified_count: 21
  lines_added: 230
  lines_removed: 148
---

# Phase 13 Plan 01: Move Action + 3 Traits + extract_jira_key to Domain — Summary

## One-Liner

Relocated the Action enum, three infra trait boundaries (ProcessClient/JiraClient/Multiplexer),
and the pure extract_jira_key helper into the domain layer; renamed the ports to *Port per
hexagonal convention; closed the UI→infra leak at panels.rs:71; batched as a single atomic
commit with zero behavioral change.

## What Landed

### New files (6)

| File | Purpose | Source |
|------|---------|--------|
| `src/domain/action.rs` | TEA `Action` enum (≈55 variants) | `git mv` from `src/action.rs` |
| `src/domain/jira.rs` | Pure `extract_jira_key` + 6 inline tests | Moved from `src/infra/jira.rs:90-175` |
| `src/domain/ports/mod.rs` | Ports module index (3 entries — others arrive in 13-03/04/05) | NEW |
| `src/domain/ports/process_port.rs` | `ProcessPort` trait (renamed from `ProcessClient`) | Trait block moved from `src/infra/process.rs:16-26` |
| `src/domain/ports/jira_port.rs` | `JiraPort` trait (renamed from `JiraClient`) | Trait block moved from `src/infra/jira.rs:22-30` |
| `src/domain/ports/multiplexer_port.rs` | `MultiplexerPort` trait (renamed from `Multiplexer`) | Trait block moved from `src/infra/multiplexer.rs:9-17` |

### Modified files (15)

| File | Nature of change |
|------|------------------|
| `src/lib.rs` | Removed `pub mod action;` line (Action now behind `pub mod domain`). |
| `src/domain/mod.rs` | Added `pub mod action; pub mod jira; pub mod ports;`. |
| `src/domain/command.rs` | F-005 — updated CommandSpec doc comment from "17 variants" → "23 variants". |
| `src/domain/worktree.rs` | Doc-comment reference `infra::jira::extract_jira_key()` → `domain::jira::extract_jira_key()`. |
| `src/app.rs` | 3 import/type rewrites: `use crate::action::Action` → domain; `Arc<dyn infra::jira::JiraClient>` → `Arc<dyn domain::ports::jira_port::JiraPort>`; `Box<dyn infra::multiplexer::Multiplexer>` → `Box<dyn domain::ports::multiplexer_port::MultiplexerPort>`. 3 call sites of `infra::jira::extract_jira_key` rewritten to `domain::jira::extract_jira_key`. Inner `use crate::infra::process::ProcessClient` rewritten to `use crate::domain::ports::process_port::ProcessPort`. |
| `src/app/dispatch_tests.rs` | `use crate::action::Action` → `use crate::domain::action::Action`. |
| `src/event.rs` | F-003 — added fall-through comment above the `_ => None` arm. |
| `src/infra/mod.rs` | F-100 — added NOTE cross-referencing Plan 13-05 for the last remaining action-import in command_runner.rs. |
| `src/infra/process.rs` | Removed `ProcessClient` trait block; added `use crate::domain::ports::process_port::ProcessPort;`; `impl ProcessClient for TokioProcessClient` → `impl ProcessPort for TokioProcessClient`. |
| `src/infra/jira.rs` | Removed `JiraClient` trait block; removed 73-line `extract_jira_key` fn + 6-test `mod tests`; added `use crate::domain::ports::jira_port::JiraPort;`; `impl JiraClient for HttpJiraClient` → `impl JiraPort for HttpJiraClient`. |
| `src/infra/multiplexer.rs` | Removed `Multiplexer` trait block; added `use crate::domain::ports::multiplexer_port::MultiplexerPort;`; two `impl Multiplexer for …` → `impl MultiplexerPort for …`; `detect_multiplexer() -> Option<Box<dyn Multiplexer>>` → `Option<Box<dyn MultiplexerPort>>`. |
| `src/infra/command_runner.rs` | `use crate::action::Action;` → `use crate::domain::action::Action;` (TEMPORARY per F-101; removed entirely in Plan 13-05 when `CommandEvent` replaces Action coupling). |
| `src/ui/panels.rs` | Line 71 — `crate::infra::jira::extract_jira_key` → `crate::domain::jira::extract_jira_key`. **F-300 UI→infra leak closed.** |
| `tests/metro_single_instance.rs` | Line 13 — `use rn_dash::action::Action` → `use rn_dash::domain::action::Action`. |
| `.planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md` | Added rows for 6 new files; removed `src/action.rs`; reduced `src/infra/jira.rs` invariant 70% → 0% (the 70% invariant migrates with the tests to the new `src/domain/jira.rs` row at 100%); added changelog entry. |

### Deleted files (1 — via rename)

- `src/action.rs` — shown by `git log` as a rename of `src/action.rs -> src/domain/action.rs`
  at 100% similarity (file content identical, only path changed).

## Import sites rewritten

| Pattern | Old path | New path | Occurrences |
|---------|----------|----------|-------------|
| `Action` import | `crate::action::Action` | `crate::domain::action::Action` | 3 (app.rs, command_runner.rs, dispatch_tests.rs) |
| `Action` import (test) | `rn_dash::action::Action` | `rn_dash::domain::action::Action` | 1 (tests/metro_single_instance.rs) |
| `extract_jira_key` call | `crate::infra::jira::extract_jira_key` | `crate::domain::jira::extract_jira_key` | 4 (app.rs ×3, ui/panels.rs ×1) |
| `JiraClient` trait reference | `crate::infra::jira::JiraClient` | `crate::domain::ports::jira_port::JiraPort` | 1 (app.rs field) |
| `Multiplexer` trait reference | `crate::infra::multiplexer::Multiplexer` | `crate::domain::ports::multiplexer_port::MultiplexerPort` | 1 (app.rs field) |
| `ProcessClient` inline import | `crate::infra::process::ProcessClient` | `crate::domain::ports::process_port::ProcessPort` | 1 (app.rs function body) |

## COVERAGE-THRESHOLDS.md changelog entry

Appended 2026-04-24 | 13 row:

> Plan 13-01 — action.rs moved to domain; 3 traits + extract_jira_key relocated; per-file
> ratchet rows updated per structural-change policy. `src/action.rs` deleted → replaced by
> new row `src/domain/action.rs` 0% (trait+enum moved verbatim, no new executable code).
> `src/infra/jira.rs` 70.18% → 0% is a structural split: the 6 `extract_jira_key*` inline
> tests migrated to new row `src/domain/jira.rs` 100% (same tests, new location). The 70%
> threshold invariant now binds to `domain/jira.rs`. Three new trait-only port files
> (`domain/ports/process_port.rs`, `jira_port.rs`, `multiplexer_port.rs`) added at 0% —
> trait definitions have no executable region and are floor-exempt. No test was removed;
> this is a pure file-move refactor.

Also updated the Invariant section: the `src/infra/jira.rs >= 70%` invariant was replaced
with `src/domain/jira.rs >= 100%` (the tests that enforce the invariant moved with the code).

## Verification

### Automated

| Check | Result |
|-------|--------|
| `cargo build --all-targets` | exit 0 |
| `cargo test --all-targets` | 49 tests passed (46 lib + 2 metro integration + 1 COVER-02 process-group); 6 of the lib tests are the relocated `domain::jira::tests::*` cases — all green at the new location |
| `cargo clippy --all-targets -- -D warnings` | exit 0 |
| `test -f src/domain/action.rs` | ✓ |
| `test ! -f src/action.rs` | ✓ |
| `test -f src/domain/jira.rs` | ✓ |
| `test -f src/domain/ports/mod.rs` | ✓ |
| `test -f src/domain/ports/process_port.rs` | ✓ |
| `test -f src/domain/ports/jira_port.rs` | ✓ |
| `test -f src/domain/ports/multiplexer_port.rs` | ✓ |
| `! rg 'use crate::action::Action' src/` | 0 hits |
| `! rg 'crate::infra::jira::extract_jira_key' src/` | 0 hits |
| `! rg 'crate::infra::jira' src/ui/` | 0 hits (**F-301 grep guard**) |
| `grep 'pub trait ProcessPort' src/domain/ports/process_port.rs` | match |
| `grep 'pub trait JiraPort' src/domain/ports/jira_port.rs` | match |
| `grep 'pub trait MultiplexerPort' src/domain/ports/multiplexer_port.rs` | match |
| `grep 'pub fn extract_jira_key' src/domain/jira.rs` | match |

### Shape Guards (per 13-VALIDATION.md)

| Guard | Target | Status |
|-------|--------|--------|
| **G-02** partial: `! rg 'crate::infra::jira' src/ui/` | 0 hits after 13-01 | ✓ PASS |
| **G-10** partial: `rg '^pub mod' src/domain/ports/mod.rs` | shows 3 modules | ✓ PASS (jira_port, multiplexer_port, process_port) |
| **G-15**: `test -f src/domain/action.rs && test ! -f src/action.rs` | both exit 0 | ✓ PASS |

## AUDIT Findings Closed

| Finding | Description | Status |
|---------|-------------|--------|
| F-002 | `src/action.rs` misplaced (should be in domain) | CLOSED |
| F-003 (Minor) | Catch-all fall-through in `event.rs` needs explanatory comment | CLOSED |
| F-005 (Minor) | CommandSpec doc under-counts variants (17 vs 23 actual) | CLOSED |
| F-100 (Minor) | `infra/mod.rs` claims "all behind trait boundaries" but command_runner still imports Action | CLOSED (cross-reference note added; full closure in Plan 13-05) |
| F-103 | `ProcessClient` trait lives in infra, should be domain port | CLOSED |
| F-106 | `JiraClient` trait lives in infra, should be domain port | CLOSED |
| F-107 | `extract_jira_key` pure fn misplaced in infra | CLOSED |
| F-110 | `Multiplexer` trait lives in infra, should be domain port | CLOSED |
| F-300 | UI→infra leak at `ui/panels.rs:71` | CLOSED |
| F-301 | `ui/mod.rs` doc-claim vs grep guard contradiction | CLOSED (now true by construction, G-02 partial passes) |

**Total:** 7 Critical/Major findings closed + 3 Minor tagalongs = 10 findings closed in a single commit.

## Deviations from Plan

**None** — plan executed exactly as written:

- All six substeps (STEP 1 through STEP 6) executed in the prescribed order.
- Single atomic commit (as the plan prescribed, since intermediate states would
  not have compiled anyway — removing `pub mod action;` without the domain
  replacement in place breaks the build).
- No auth gates, no checkpoints (this is a `type="auto"` task in a `wave: 1`
  autonomous plan).
- No auto-fix rules (Rules 1-3) fired; no architectural decisions needed (Rule 4).

The action.rs file move used `git mv` (per plan step 1.2) — git tracked the
change as a rename at 100% similarity, preserving history.

## Deferred Items

None. All in-scope work committed.

## Downstream Plans Unblocked

- **Plan 13-03** (MetroPort): `domain::ports` module now exists and the convention
  (one trait per file, `*_port.rs` naming) is established.
- **Plan 13-04** (3 new ports — PortProbePort, WorktreePort, DevicePort): same as above.
- **Plan 13-05** (F-101 CommandRunnerPort + CommandEvent): the temporary
  `use crate::domain::action::Action;` in `src/infra/command_runner.rs` is now the
  only infra→domain Action coupling. Plan 13-05 removes it via CommandEvent.
- **Plan 13-08** (Adapters struct): the in-place JiraPort + MultiplexerPort field
  type rewrites in app.rs already point at the final domain port types, so 13-08
  only needs to promote the fields into an injected Adapters struct — no further
  type surgery required.

## Threat Flags

None. This plan is a pure relocation + rename refactor. No new network endpoints,
auth paths, file access patterns, or schema changes at trust boundaries. The
`threat_model_disposition: accept_refactor_only` declared in the plan's frontmatter
holds: STRIDE register unchanged.

## Commits

| Hash | Description |
|------|-------------|
| `821b4e4` | refactor(13-01): relocate Action + 3 traits + extract_jira_key to domain |

## Self-Check: PASSED

- [x] Created file `src/domain/action.rs` — FOUND
- [x] Created file `src/domain/jira.rs` — FOUND
- [x] Created file `src/domain/ports/mod.rs` — FOUND
- [x] Created file `src/domain/ports/process_port.rs` — FOUND
- [x] Created file `src/domain/ports/jira_port.rs` — FOUND
- [x] Created file `src/domain/ports/multiplexer_port.rs` — FOUND
- [x] Deleted file `src/action.rs` — ABSENT (renamed, expected)
- [x] Commit `821b4e4` — FOUND in `git log --oneline`
- [x] `cargo test --all-targets` — 49 tests passed
- [x] `cargo clippy --all-targets -- -D warnings` — clean
- [x] G-02 partial, G-10 partial, G-15 — all pass
