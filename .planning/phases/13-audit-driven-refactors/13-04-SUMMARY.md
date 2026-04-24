---
phase: 13-audit-driven-refactors
plan: 04
subsystem: domain-ports
tags: [refactor, domain, trait-def, adapter-shell, wave-b, hexagonal]
wave: 3
depends_on: [13-01, 13-03]
requirements_addressed: [REFACTOR-01]

dependency_graph:
  requires:
    - "crate::domain::worktree::Worktree (already in domain)"
    - "crate::domain::command::DeviceInfo (already in domain)"
    - "async_trait crate (existing workspace dep)"
  provides:
    - "crate::domain::ports::port_probe_port::PortProbePort trait"
    - "crate::domain::ports::port_probe_port::ExternalProcessInfo struct"
    - "crate::domain::ports::worktree_port::WorktreePort trait"
    - "crate::domain::ports::device_port::DevicePort trait + DeviceKind enum"
    - "crate::infra::port::LsofPortProbe adapter"
    - "crate::infra::worktrees::GitWorktreeAdapter adapter"
    - "crate::infra::devices::AdbXcrunDevices adapter"
  affects:
    - "Plan 13-05: will define CommandRunnerPort; follows same trait-def + adapter-shell pattern"
    - "Plan 13-08: constructs Adapters struct; rewires app.rs call sites from free fns to trait methods"

tech_stack:
  added: []
  patterns:
    - "Adapter-shell pattern: free fn remains + thin wrapping struct delegates to it (no behavioral change)"
    - "Name migration at the boundary: infra keeps ExternalMetroInfo; domain exposes ExternalProcessInfo; adapter converts"

key_files:
  created:
    - "src/domain/ports/port_probe_port.rs"
    - "src/domain/ports/worktree_port.rs"
    - "src/domain/ports/device_port.rs"
  modified:
    - "src/domain/ports/mod.rs (added 3 pub mod lines, alphabetised)"
    - "src/domain/action.rs (ExternalMetroDetected payload: infra::ExternalMetroInfo -> domain::ports::port_probe_port::ExternalProcessInfo)"
    - "src/infra/port.rs (+LsofPortProbe impl PortProbePort)"
    - "src/infra/worktrees.rs (+GitWorktreeAdapter impl WorktreePort)"
    - "src/infra/devices.rs (+AdbXcrunDevices impl DevicePort)"
    - "src/app.rs (2 construction sites build ExternalProcessInfo from ExternalMetroInfo)"
    - "src/app/effect.rs (stub DeviceKind removed; canonical import added)"

decisions:
  - "ExternalMetroInfo in src/infra/port.rs is preserved (not renamed) — its free-fn callers in app.rs still use it until Plan 13-08 routes through the port. Only the domain type was renamed, matching the plan's migration strategy."
  - "check_stale / check_stale_pods / parse_worktree_porcelain stay as free fns — they are internal helpers, not worktree CRUD ops (plan note L216-218 respected)."
  - "Physical iOS devices (list_ios_physical_devices) intentionally NOT exposed via DevicePort yet — current consumers only need Android + iOS simulator families. A future DeviceKind::IosPhysical variant can be added without breaking existing callers."
  - "The Action::ExternalMetroDetected pattern-match site at src/app.rs:712 requires no type change — field access (info.pid, info.working_dir) is identical on both the old and new payload types, so the destructure body is untouched."

metrics:
  duration: "3m 23s"
  tasks_completed: 1
  files_changed: 10
  files_created: 3
  lines_added: 235
  lines_removed: 13
  completed_date: "2026-04-24"
---

# Phase 13 Plan 04: PortProbePort + WorktreePort + DevicePort Trait Definitions + Adapter Shells

One-liner: Define three hexagonal ports (F-102/F-104/F-105) with adapter shells wrapping existing
infra free fns, renaming ExternalMetroInfo -> ExternalProcessInfo at the domain boundary so
Plan 13-08 can inject them via the Adapters struct without further type churn.

## Context

Plan 13-08 will construct an `Adapters` injection struct and rewire app.rs's 43+
`crate::infra::*` call sites to go through trait methods. For that to be mechanical, the
traits and their one-and-only production adapters have to exist first. This plan lands them.

Zero behavioural change: the free fns in infra remain primary; the adapters just delegate.
Plan 13-08 flips the delegation — the free fns will be inlined into the adapter method
bodies or deleted once all call sites route through the trait.

## What Landed

### Three new port trait files

| File | Trait | Struct / Enum | Methods |
| --- | --- | --- | --- |
| `src/domain/ports/port_probe_port.rs` | `PortProbePort` (async) | `ExternalProcessInfo { pid, working_dir }` | `port_is_free(port) -> bool`, `detect_external(port) -> Option<ExternalProcessInfo>`, `kill_process(pid) -> Result<()>` |
| `src/domain/ports/worktree_port.rs` | `WorktreePort` (async) | — | `list`, `remove`, `add`, `add_new_branch`, `list_remote_branches` (all take `&Path` for `repo_root`) |
| `src/domain/ports/device_port.rs` | `DevicePort` (async) | `DeviceKind { Android, Ios }` | `list(kind) -> Vec<DeviceInfo>` |

### Three adapter shells in infra

| File | Adapter | Wraps |
| --- | --- | --- |
| `src/infra/port.rs` | `LsofPortProbe` | `port_is_free`, `detect_external_metro`, `kill_process` (converts `ExternalMetroInfo` -> `ExternalProcessInfo` inside `detect_external`) |
| `src/infra/worktrees.rs` | `GitWorktreeAdapter` | 5 pub async fns (identity delegation) |
| `src/infra/devices.rs` | `AdbXcrunDevices` | Dispatches `DeviceKind::Android` -> `list_android_devices`, `DeviceKind::Ios` -> `list_ios_simulators` |

### Name migration at the domain boundary

- `src/domain/action.rs` — `ExternalMetroDetected` variant payload type changed from
  `crate::infra::port::ExternalMetroInfo` to
  `crate::domain::ports::port_probe_port::ExternalProcessInfo`.
- Two construction sites in `src/app.rs` (lines ~612 and ~2137) build the domain type
  explicitly from the infra type's fields.
- The single pattern-match site in `src/app.rs` (line 712) required no change — field
  names `pid` + `working_dir` are identical on both types.
- `src/infra/port.rs` still exposes `ExternalMetroInfo` and `detect_external_metro`
  (rename-in-place deferred until 13-08 eliminates those direct callers).

### DeviceKind stub removal

- `src/app/effect.rs` previously had a stub `pub enum DeviceKind { Android, Ios }` with a
  TODO comment from Plan 13-03 saying "Plan 13-04 moves this to
  `crate::domain::ports::device_port::DeviceKind`".
- That stub is now deleted; replaced by
  `use crate::domain::ports::device_port::DeviceKind;` at the top of the file.
- `Effect::LoadDevices { kind: DeviceKind }` + the `effect_variants_compile` test that
  constructs `Effect::LoadDevices { kind: DeviceKind::Android }` both continue to
  compile — confirming the canonical type is structurally identical to the stub.

## Signature Divergences from 13-PATTERNS.md

None. All three trait method signatures match the PATTERNS.md sketches exactly. The
existing free-fn signatures in infra were already `&Path`/`&str` reference-based, which
is what the patterns specified.

## Call Site Conversion Count (Action::ExternalMetroDetected)

Three grep hits in `src/app.rs` for `Action::ExternalMetroDetected(`:

1. Line ~614 (metro start path): **converted** — builds `ExternalProcessInfo` from `info`.
2. Line ~712 (pattern match in update loop): **unchanged** — field access only, identical shape.
3. Line ~2139 (startup check): **converted** — same pattern as site 1.

Net: two conversions, one untouched.

## Effect.rs Downstream Impact

Beyond the simple import swap + stub delete (5-line diff total), no other changes were
needed. The `Effect::LoadDevices` variant, the compile-only test, and the variant-index
match in `effect_has_at_least_fifteen_variants` all continue to reference `DeviceKind`
(now resolved through the imported canonical type). Because the old stub and new
canonical type are structurally identical (`Debug + Clone + Copy + PartialEq` + two
unit variants), no code change cascaded beyond the import.

## Deviations from Plan

None. Plan executed exactly as written.

## Verification Results

```
cargo build --all-targets       -> 0 (unoptimised dev profile, 20.50s)
cargo test --all-targets        -> 0 (70 + 0 + 2 + 1 = 73 tests; 0 failures)
cargo clippy --all-targets -- -D warnings -> 0 (no warnings)
make arch-lint                  -> 0 (PASS; G-10 satisfied — 7 pub mod lines in ports/mod.rs)
```

Acceptance criteria checklist (all 11 pass):

- [x] 3 new trait files exist under src/domain/ports/
- [x] Each defines its `pub trait *Port` (grep-verified for all 3)
- [x] `pub struct ExternalProcessInfo` exists in port_probe_port.rs
- [x] Adapter shells exist: LsofPortProbe, GitWorktreeAdapter, AdbXcrunDevices
- [x] src/domain/action.rs ExternalMetroDetected uses ExternalProcessInfo
- [x] src/app/effect.rs imports canonical DeviceKind; no stub enum
- [x] cargo build --all-targets exits 0
- [x] cargo test --all-targets exits 0 (all prior tests survive)
- [x] cargo clippy --all-targets -- -D warnings exits 0
- [x] make arch-lint exits 0 (G-10 satisfied with 7 pub mod lines)
- [x] ExternalMetroInfo in src/infra/port.rs still exists (callers unchanged)
- [x] Free fns in infra (list_worktrees etc.) still exist (adapters wrap, do not replace)

## Unlocks

- **Plan 13-05** can now land `CommandRunnerPort` using the same adapter-shell pattern.
  No import conflict since `command_runner.rs` doesn't touch the 3 ports added here.
- **Plan 13-08** has all 7 of its required port traits registered
  (`ProcessPort` + `JiraPort` + `MultiplexerPort` + `MetroPort` from earlier waves,
  plus `PortProbePort` + `WorktreePort` + `DevicePort` from this plan). Plan 13-08 will
  add the 8th (`CommandRunnerPort` from 13-05) and then construct the `Adapters` struct.

## Commits

| Hash | Message |
| --- | --- |
| d9cd8c8 | feat(13-04): add PortProbePort + WorktreePort + DevicePort + adapter shells per F-102/F-104/F-105 |

## Self-Check: PASSED

File existence verified:
- FOUND: src/domain/ports/port_probe_port.rs
- FOUND: src/domain/ports/worktree_port.rs
- FOUND: src/domain/ports/device_port.rs
- FOUND: src/domain/ports/mod.rs (updated)
- FOUND: src/domain/action.rs (updated)
- FOUND: src/infra/port.rs (LsofPortProbe appended)
- FOUND: src/infra/worktrees.rs (GitWorktreeAdapter appended)
- FOUND: src/infra/devices.rs (AdbXcrunDevices appended)
- FOUND: src/app.rs (2 construction sites converted)
- FOUND: src/app/effect.rs (stub removed; canonical imported)

Commit verified: d9cd8c8 present in git log.
