# Phase 13: Audit-Driven Refactors — Research

**Researched:** 2026-04-24
**Domain:** Rust + Ratatui architecture refactor — TEA purity, hexagonal ports-and-adapters, god-object decomposition, domain-level command orchestration
**Confidence:** HIGH

## Summary

Phase 13 resolves every Critical and Major finding from `AUDIT.md` (12 findings across
4 domains — god-object split, TEA purity, hexagonal dependency inversion, domain-level
prerequisite representation) plus REFACTOR-02 (`is_cancellable()`) and REFACTOR-03
(`Prerequisite` + `Recipe` domain types). The audit already supplies D-04 target
shapes for every Critical finding and a 23-step Refactor Sequence partitioned into
4 dependency-waved groups (A foundational / B infra adapters / C app rewiring /
D UI rewiring). This research does NOT re-derive shapes — it answers the planning
questions the audit deliberately left open: plan carving, atomicity boundaries,
wave parallelism, test coverage of risk surface, and the F-500 / F-501 punted
decisions.

The coverage gate (Phase 12) is green; 49 tests assert metro single-instance
(D-09 two-layer), process-group kill, and 17 TEA dispatch invariants. Those tests
plus `floor(baseline, 5)` per-file ratchets are Phase 13's regression trip-wire.
Specific risk assessment per Critical in §Risk Surface Map below.

**Primary recommendation:** 10 plans in 5 waves. Land F-002 + F-103/106/110 + F-201 +
F-204 type-halves (all pure type definitions) in parallel as Wave A, then REFACTOR-02
tiny pure add, then F-200 app-split, then F-201/F-202/F-203/F-204 consumer-halves
each as their own plan, then the UI/keybinding consumer rewires (F-208/302/303 via
F-400) as Wave D. Do **not** land F-501 category-split. Do **not** land F-500
WorktreeSlice. Both are deferred per the AUDIT-ADDENDUM routing table — Phase 13
stays true to the locked Phase 11 scope and leaves those for `/gsd-discuss-phase 14`.

## User Constraints (from CONTEXT.md)

> **No CONTEXT.md exists for Phase 13 yet** — this research is the standalone/integrated
> entry point. Constraints below are derived from ROADMAP.md, REQUIREMENTS.md, AUDIT.md,
> AUDIT-ADDENDUM.md, CLAUDE.md, and STATE.md as the authoritative substitutes.

### Locked Decisions (from authoritative sources)

- **From REQUIREMENTS.md REFACTOR-01..03:**
  - Resolve ALL Critical + Major findings from ARCH-01..ARCH-05 (Minor MAY defer to backlog with rationale per D-02)
  - COVER-01..COVER-04 MUST be green before any refactor touches modified code (Phase 12 is complete — satisfied)
  - `CommandSpec::is_cancellable()` returns `false` for all git-porcelain variants, `true` for all others
  - Command prerequisites/ordering represented abstractly in domain (REFACTOR-03 pick (a) Prerequisite graph OR (b) Recipe type — AUDIT F-204 already picks (b) Recipe + Prerequisite together)

- **From ROADMAP.md Phase 13 Success Criteria:**
  - No new Critical/Major regressions introduced
  - `cargo test` + `cargo clippy -D warnings` both green after refactor
  - Dispatcher reads prerequisite ordering from domain, not inline `update()`

- **From AUDIT-ADDENDUM.md routing table:**
  - **F-500 `WorktreeSlice`: OUT OF SCOPE for Phase 13** — Phase 14 concern; preserve current `AppState` shape during F-200 split
  - **F-501 Command category-split: base audit flat-enum wins unless discuss-phase overrides** — default to flat-enum predicate for REFACTOR-02

- **From CLAUDE.md:**
  - YOLO mode — do not ask for confirmation at workflow gates
  - `check-types` uses `--incremental` flag (already wired — `CommandSpec::YarnCheckTypes::to_argv`)
  - `/clear` between phases; re-read CLAUDE.md + `/gsd:progress` after clear

- **From REQUIREMENTS.md Out of Scope (hard-locked exclusions that Phase 13 MUST NOT violate):**
  - No `throbber-widgets-tui` dep (MSRV bump)
  - No `arch_test_core` (AGPL incompatible with MIT)
  - No `cargo-modules` / `cargo-depgraph` / `cargo-deny` CI integration
  - No broader unit/integration test expansion beyond COVER-01..04
  - Cancellation of git operations FORBIDDEN (data integrity) — enforced type-level by `is_cancellable()`

### Claude's Discretion

- **F-400 keybinding registry placement:** `src/keybindings.rs` (root) vs `src/app/keybindings.rs` (after F-200 split). Auditor's recommendation: app-level (depends on AppState for conditional actions). See §Open Questions → Q1.
- **Domain port submodule convention:** `domain::ports::{process_port,metro_port,...}` nested module, OR flat `domain::{ProcessPort,MetroPort,...}`. Audit AUDIT.md uses `domain::ports::*` consistently in recommendations (F-101/102/103/104/105/106/110) — default to that convention.
- **Which Minors ride along:** F-005 (doc drift), F-003 (event.rs fall-through comment), F-006 (needs_text_input catch-all), F-007 (defer — REFACTOR-02 territory), F-008 (refresh fall-through), F-009 (defer — Phase 14+16 concern), F-100 (doc claim fix), F-108 (is_inside_tmux relocation), F-111 (persistence — defer), F-112 (tmux.rs delete), F-206/207/210 (fold into F-201/F-203 naturally). Default strategy per §Minor Tagalongs Table below.
- **Atomic plan boundary policy:** intermediate state may keep both trait definition in domain AND legacy call sites until the wave's "consumer half" lands. But every PLAN's end-state must compile + clippy-clean + test-green. See §Atomicity Boundaries.
- **Error-handling idioms in new ports:** `anyhow::Result<T>` (consistent with existing codebase — verified `grep -c 'anyhow' src/infra/*.rs` = every I/O module uses anyhow).

### Deferred Ideas (OUT OF SCOPE — do not plan)

- **F-500** WorktreeSlice — Phase 14 scoping decision [CITED: AUDIT-ADDENDUM.md:42-50]
- **F-501** Category-split `Command { Git(..), Yarn(..), Rn(..), Shell(..) }` — keep flat `CommandSpec` enum; REFACTOR-02 lands as flat-enum predicate per base audit F-007 recommendation [CITED: AUDIT-ADDENDUM.md:92-97]
- **F-111** Persistence-port consolidation — defer to backlog; re-evaluate in Phase 16 when task history persistence lands [CITED: AUDIT.md F-111]
- **F-009** `Worktree` struct split (jira enrichment vs identity) — defer to Phase 14/16 per audit recommendation [CITED: AUDIT.md F-009]
- **F-007** `is_cancellable()` mention in Phase 11 — belongs HERE as REFACTOR-02, do not re-file
- **arch_test_core runtime fitness functions** — AGPL (already out-of-scope)
- **Configurable keybindings / theme customization / multi-project support** — future milestones
- **`cargo-modules` / `cargo-depgraph` / `cargo-deny` in CI** — post-milestone

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| REFACTOR-01 | All Critical + Major findings from ARCH-01..ARCH-05 resolved (Minor may defer with rationale) | See §Critical & Major Findings Map (12 findings) + §Refactor Sequence → Plans translation. 10 plans × 5 waves below. COVER-01..04 tests guard metro + process-group + dispatch surfaces during refactor. |
| REFACTOR-02 | `CommandSpec::is_cancellable()` predicate added — `false` for git-porcelain, `true` for others (type-driven, not convention) | See §Don't Hand-Roll → "Type-driven cancellability". Implementation sketch inline. 6 git variants return false; all 17 others return true. Lands as Plan 13-02 (tiny standalone after Wave A foundational extractions). |
| REFACTOR-03 | Command prerequisites/ordering in domain — pick (a) Prerequisite graph OR (b) Recipe type; dispatcher reads from domain not inline `update()` | See §Architecture Patterns → Pattern 2 Recipe. Base audit AUDIT.md F-204 picks **both** (`Prerequisite` enum + `Recipe` enum composed together). Lands as Plan 13-04 (type half) + Plan 13-08 (consumer half collapsing 11 inline sites). |

## Project Constraints (from CLAUDE.md)

- **YOLO mode:** auto-approve plans, research, verification; do NOT prompt at workflow gates
- **`check-types` uses `--incremental`:** already encoded in `CommandSpec::YarnCheckTypes` — do not regress during refactor
- **Branch labels are per-branch** (persist across worktrees) — not relevant to Phase 13 (labels feature was REMOVED per v1.1), but listed in CLAUDE.md
- **Metro logs only stream when a filter is applied** — metro doesn't stream by default anymore; means `MetroActivity` parsing path is filter-gated (relevant for F-203 `TokioMetroAdapter` — preserve this filter behavior)
- **`/clear` between phases:** research must stand on its own for post-clear planners — this document MUST be self-contained

## Architectural Responsibility Map

Phase 13 is a restructuring phase — every capability is REDISTRIBUTING existing code,
not adding new domains. Map below is the post-Phase-13 target.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| TEA message/intent grammar (`Action` enum) | Domain | App (dispatch) | Action IS domain vocabulary per AUDIT F-002; lives in `src/domain/action.rs` |
| Command spec types + predicates (`CommandSpec`, `is_cancellable`, `needs_metro`, `needs_text_input`, `is_destructive`, `label`) | Domain | — | Already in `domain/command.rs`; REFACTOR-02 adds `is_cancellable()` here |
| Command orchestration (`Prerequisite`, `Recipe`, `Recipe::expand`) | Domain | App (dispatcher consumer) | New `src/domain/pipeline.rs` per F-204; dispatcher reads domain types only |
| Effect grammar (`Effect` enum) | App (or domain) | Infra (effect runner dispatches) | New `src/app/effect.rs` per F-201 — app-layer because it references CommandSpec + worktree paths + adapter ports |
| Key binding registry (`KeyBinding`, `KEYBINDINGS`) | App | UI (consumer) | App-level per audit F-400 recommendation — depends on AppState for conditional actions; UI reads via `footer_hints_for(&state)` / `help_overlay_rows()` |
| Process spawn port (`ProcessPort`) | Domain (trait) | Infra (adapter) | Relocate trait from infra to `domain::ports::process_port` per F-103 |
| Metro lifecycle port (`MetroPort`) | Domain (trait + opaque `MetroHandle`) | Infra (`TokioMetroAdapter`) | NEW trait per F-203 + F-004; absorbs the 7 in-app.rs async helpers |
| Command runner port (`CommandRunnerPort`, `CommandEvent`) | Domain (trait) | Infra (`TokioCommandRunner`) | NEW per F-101; removes `Action` import from infra |
| JIRA port (`JiraPort`) + pure `extract_jira_key` | Domain (trait + pure fn) | Infra (`HttpJiraClient` adapter) | F-106 + F-107 + F-300 symmetric three-side fix; pure fn lives in `src/domain/jira.rs` |
| Multiplexer port (`MultiplexerPort`) | Domain (trait) | Infra (`TmuxAdapter`, `ZellijAdapter`) | Relocate per F-110 |
| Worktree/device/port-probe ports | Domain (3 traits) | Infra (3 adapters) | NEW per F-102/F-104/F-105 |
| App state (AppState + sub-structs) | App | — | Split into `src/app/state.rs` with sub-structs per F-209 grouping |
| Update logic (pure `update(state, action) -> Vec<Effect>`) | App | — | `src/app/update.rs` after F-200 + F-201 consumer |
| Effect interpretation (tokio::spawn dispatch) | App (runner) | Infra (adapter calls) | `src/app/effect_runner.rs` after F-200 |
| Event loop (`run()`) | App | Infra (startup-time adapter construction) | `src/app/runtime.rs` after F-200 |
| Key dispatch (handle_key) | App | — | `src/app/handle_key.rs` after F-200; reads `KEYBINDINGS` after F-208 |
| Rendering (widgets, layout) | UI | Domain (reads types only) | Already UI-clean; refactor closes last UI→infra leak (F-300 `extract_jira_key`) |

**Why this matters:** every Phase 13 plan has its tier listed up-front — the plan-checker
catches a task that tries to put an adapter impl in `src/app/` or a trait in `src/infra/`.

## Standard Stack

Phase 13 is a REFACTOR phase — no new deps. Table below confirms what's already
wired and flags what MUST NOT be added.

### Core (already in Cargo.toml)

| Crate | Version | Purpose | Already used in codebase | Verified |
|-------|---------|---------|--------------------------|----------|
| ratatui | 0.30 | TUI widgets + crossterm re-export | `src/ui/`, `src/app.rs` | [VERIFIED: Cargo.toml:22] |
| crossterm | 0.29 | Event stream (via `ratatui::crossterm`) | `src/app.rs`, `src/event.rs` | [VERIFIED: Cargo.toml:25] |
| tokio | 1.49 full | Async runtime, channels, process, signal | 27 `tokio::spawn` sites in app.rs today | [VERIFIED: Cargo.toml:28] |
| futures | 0.3 | StreamExt trait for EventStream | `src/app.rs:5` | [VERIFIED: Cargo.toml:31] |
| anyhow | 1 | Error propagation in infra/domain | every infra/*.rs | [VERIFIED: Cargo.toml:34] |
| thiserror | 2 | Error type derivation | (available but lightly used) | [VERIFIED: Cargo.toml:35] |
| color-eyre | 0.6 | Main-level error handling | `src/main.rs` | [VERIFIED: Cargo.toml:36] |
| tracing | 0.1 | Structured logging (file only — never stdout in TUI) | every module | [VERIFIED: Cargo.toml:39] |
| async-trait | 0.1 | `#[async_trait]` for async trait methods | `src/infra/process.rs`, `src/infra/jira.rs` | [VERIFIED: Cargo.toml:44] |
| serde / serde_json / toml | 1 / 1 / 0.8 | Config + cache persistence | `src/infra/{config,jira_cache,sim_history,android_prefs}.rs` | [VERIFIED: Cargo.toml:47-49] |
| reqwest | 0.12 rustls-tls | HTTP (JIRA + metro control endpoints) | `src/infra/jira.rs` + `src/app.rs:2411` (F-203 moves this) | [VERIFIED: Cargo.toml:52] |
| libc | 0.2 | POSIX process group kill | `src/app.rs:2259+` (F-203 moves this) | [VERIFIED: Cargo.toml:55] |

### Dev-dependencies (already in Cargo.toml)

| Crate | Version | Purpose | Verified |
|-------|---------|---------|----------|
| tokio | 1.49 (macros, rt-multi-thread, process, time, io-util, sync) | `#[tokio::test]` + timeout + channels in integration tests | [VERIFIED: Cargo.toml:61] |
| anyhow | 1 | dev error handling | [VERIFIED: Cargo.toml:62] |

### Alternatives (NOT to introduce)

| Instead of | Could Use | Tradeoff | Recommendation |
|------------|-----------|----------|----------------|
| `Arc<dyn Port>` | Generics `<P: Port>` | Generics compile faster, avoid vtable dispatch; trait objects allow runtime swap and smaller code | **Use `Arc<dyn Port>`** — audit F-202 recommends this; matches existing `Box<dyn Multiplexer>` + `Arc<dyn JiraClient>` pattern [VERIFIED: src/app.rs:120,130] |
| `Box<dyn Port>` | `Arc<dyn Port>` | Box = single owner; Arc = cheap clone, safe across tasks | **`Arc<dyn Port>`** for ports that may be cloned into spawned tasks (CommandRunner, MetroPort — spawning reads clone these into Effect runner); `Box<dyn Port>` for singletons never crossed-thread (Multiplexer — already Box) |
| `#[async_trait]` traits | Native AFIT (async-fn-in-trait, Rust 1.75+) | AFIT is native + faster; `#[async_trait]` boxes futures | **Keep `#[async_trait]`** — existing code uses it (ProcessClient, JiraClient); mixing conventions is churn. Rust edition 2024 supports AFIT but codebase consistency wins |
| `tokio::task::JoinHandle` on `Effect` | Drop pattern via `kill_on_drop(true)` | Drop pattern is simpler; JoinHandle allows explicit abort() | **Both** — effect runner holds JoinHandle for per-task cancel (Phase 15 dependency); `kill_on_drop` is safety net |
| `static KEYBINDINGS: &[KeyBinding]` | `once_cell::Lazy<Vec<KeyBinding>>` | Static const is zero-cost; Lazy allows runtime init | **`const KEYBINDINGS: &[KeyBinding]`** — audit F-400 specifies `const`; avoid new dep |
| `throbber-widgets-tui` | inline `SPINNER_FRAMES` const | MSRV bump to 1.88 — out of scope per REQUIREMENTS.md | N/A for Phase 13 (Phase 16 concern) |

**Installation:** No new deps. Verify with:

```bash
cargo tree --depth 1 | sort -u > /tmp/deps-before.txt
# after Phase 13 implementation:
cargo tree --depth 1 | sort -u > /tmp/deps-after.txt
diff /tmp/deps-before.txt /tmp/deps-after.txt
# Expected: no differences (ASSUMED — but the Refactor Sequence is all relocations, no new crates)
```

**Version verification:** rustc 1.94.1, cargo 1.94.1, edition 2024 [VERIFIED: BASELINE-COVERAGE.md:4]. `check-types` uses `--incremental` (CLAUDE.md mandate) [VERIFIED: src/domain/command.rs:87].

## Architecture Patterns

### System Architecture Diagram — Post-Phase-13 Target

```
                        User key press
                             |
                             v
                   ratatui EventStream (infra — ratatui::crossterm)
                             |
                             v
                   src/app/runtime.rs::run()  ← owns Adapters<Arc<dyn *Port>>
                             |
                             v
                src/app/handle_key.rs::handle_key(&AppState, KeyEvent)
                             |
                             v
                walk KEYBINDINGS  ← src/app/keybindings.rs (single source of truth)
                             |
                             v
                       Option<Action>  ← src/domain/action.rs
                             |
                             v
                src/app/update.rs::update(&mut state, action) -> Vec<Effect>
                 (pure — NO tokio::spawn; returns effects as data)
                             |
                             v
                       Vec<Effect>  ← src/app/effect.rs
                             |
                             v
            src/app/effect_runner.rs::run_effects(Vec<Effect>, Adapters, tx)
                             |
              +--------------+--------------+--------------+
              |              |              |              |
              v              v              v              v
       Arc<dyn CR>   Arc<dyn Metro>  Arc<dyn WT>    Arc<dyn Jira>
              |              |              |              |
              v              v              v              v
    infra::command_    infra::metro::  infra::        infra::
    runner::Tokio      TokioMetro     worktrees::    jira::
    CommandRunner       Adapter       GitWorktree    HttpJiraClient
                                      Adapter
                             |
                             v
                    CommandEvent / MetroActivity / Worktree events
                             |
                             v (through mpsc channels into runtime.rs)
                             |
                    translated back to Action
                             |
                             v
                   next update() iteration

Rendering path (independent of event loop):
  src/app/runtime.rs::run() calls terminal.draw(|f| ui::view(f, &mut state))
  ui::view reads &AppState → panels + footer + overlays
  panels::render_worktree_table reads EXTRACT_JIRA_KEY from domain/jira.rs (post-F-107)
  footer::render_footer calls keybindings::footer_hints_for(&state) (post-F-208)
  help_overlay::render_help calls keybindings::help_overlay_rows() (post-F-208)

Test guards (invariant trip-wires):
  tests/metro_single_instance.rs  — MetroStart-while-running flow
  tests/process_group_kill.rs     — PGID reap (cfg'd Linux/macOS)
  src/app/dispatch_tests.rs       — 17 palette+modal+queue tests
  src/domain/metro.rs::tests      — register_twice panic + clear→register
  src/domain/refresh.rs::tests    — 17 refresh-rule tests
  src/infra/jira.rs::tests (→ moves to domain/jira.rs) — 6 extract_jira_key tests
```

### Component Responsibilities

| Component | File (post-Phase-13) | Responsibility |
|-----------|----------------------|----------------|
| Root binary | `src/main.rs` | Construct adapters → `Arc<dyn *Port>` → build `Adapters` struct → `app::run(terminal, adapters)` |
| Lib crate root | `src/lib.rs` | `pub mod {action(→domain),app,domain,event,infra,tui,ui}` re-export |
| Action grammar | `src/domain/action.rs` (moved from `src/action.rs`) | The single TEA intent enum; imported by `app/` only |
| Command spec | `src/domain/command.rs` | `CommandSpec` + `is_cancellable()` (new) + existing predicates |
| Pipeline types | `src/domain/pipeline.rs` (new) | `Prerequisite`, `Recipe`, `Recipe::expand(&DependencyState) -> Vec<CommandSpec>` |
| JIRA pure helper | `src/domain/jira.rs` (new) | `extract_jira_key` + 6 tests (moved from `infra/jira.rs`) |
| Metro lifecycle types | `src/domain/metro.rs` | `MetroActivity`, `MetroStatus`, `MetroManager` (Option-based single-instance guard preserved); `MetroHandle` REPLACED by opaque trait per F-004 |
| Ports | `src/domain/ports/{command_runner,metro,process,jira,multiplexer,worktree,device,port_probe}_port.rs` | 8 `pub trait *Port` + supporting types (`CommandEvent`, opaque `MetroHandle`, `ExternalProcessInfo`) |
| Effect enum | `src/app/effect.rs` | `pub enum Effect { SpawnCommand, StartMetro, MetroHttpPost, DetectExternalMetro, KillProcess, LoadDevices, ListWorktrees, ...15+ variants }` |
| AppState | `src/app/state.rs` | `AppState` with 6-7 sub-structs (`MetroState`, `WorktreeBrowserState`, `CommandRunnerState`, `ModalStackState`, `PendingFlags`, `AppConfigState`) per F-209; `pub(crate)` inner fields |
| Pure update | `src/app/update.rs` | `pub fn update(&mut AppState, Action) -> Vec<Effect>` — NO tokio::spawn |
| Effect runner | `src/app/effect_runner.rs` | `pub struct EffectRunner { adapters: Adapters, tx: UnboundedSender<Action> }` + `run_effects(Vec<Effect>)` — owns all tokio::spawn calls |
| Key dispatch | `src/app/handle_key.rs` | `handle_key(&AppState, KeyEvent) -> Option<Action>` — walks KEYBINDINGS |
| Key registry | `src/app/keybindings.rs` | `pub struct KeyBinding`, `pub const KEYBINDINGS: &[KeyBinding]`, `handle_key`, `footer_hints_for`, `help_overlay_rows` |
| Event loop | `src/app/runtime.rs` | `pub async fn run(terminal, adapters)` — the event loop; wires up mpsc channels |
| Adapters struct | `src/app/adapters.rs` (or part of runtime.rs) | `pub struct Adapters { command_runner, metro, port_probe, worktrees, devices, jira, multiplexer, persistence }` holding `Arc<dyn Port>` trait objects |
| Infra adapters | `src/infra/{command_runner,metro,process,jira,multiplexer,worktrees,devices,port,android_prefs,sim_history,jira_cache,config}.rs` | Each implements the corresponding `domain::ports::*Port` trait; zero imports of `crate::action` or `crate::app` |
| UI | `src/ui/{panels,footer,help_overlay,modals,error_overlay,theme,mod}.rs` | Widget rendering; reads from `KEYBINDINGS` (post-F-302/F-303); zero imports of `crate::infra::*` (post-F-300) |

### Pattern 1: Pure TEA update returning `Vec<Effect>`

**What:** `update()` signature becomes `fn update(state: &mut AppState, action: Action) -> Vec<Effect>`. The 20 inline `tokio::spawn` call sites become `effects.push(Effect::...)` returns. The `effect_runner.rs` interprets effects into actual tokio::spawn calls at a single boundary.

**When to use:** Every state mutation path in update(). No exception — if an Action today calls `tokio::spawn`, it MUST return an `Effect` variant post-refactor.

**Example (verified against audit AUDIT.md F-201 target shape):**

```rust
// src/app/effect.rs (new)
// Source: AUDIT.md F-201 recommendation — 15+ variants, sketched verbatim
pub enum Effect {
    // Metro lifecycle
    DetectExternalMetro { port: u16 },
    SpawnMetro { worktree: PathBuf },
    MetroHttpPost { url: String, body: String },
    KillProcess { pid: u32 },

    // Commands
    SpawnCommand { spec: CommandSpec, cwd: PathBuf, branch: String },
    LoadDevices { kind: DeviceKind },

    // Worktrees
    ListWorktrees,
    RemoveWorktree { path: PathBuf },
    AddWorktree { branch: String },
    AddWorktreeNewBranch { new: String, base: String },
    ListRemoteBranches,

    // Persistence (spawn_blocking sites)
    SaveJiraCache(HashMap<String, String>),
    SaveAndroidMode(String),
    RecordSimUsed(String),

    // External processes
    OpenInMultiplexer { worktree: PathBuf, name: String, command: String },

    // JIRA
    FetchJiraTitles { keys: Vec<String> },
}

// src/app/update.rs (post-F-200/F-201 split)
pub fn update(state: &mut AppState, action: Action) -> Vec<Effect> {
    let mut effects = Vec::new();
    match action {
        Action::MetroStart => {
            if state.metro.is_running() {
                state.pending_restart = true;
                // recursive self-dispatch replaced by Effect::ScheduleAction after F-201
                effects.extend(update(state, Action::MetroStop));
                return effects;
            }
            if !state.skip_external_metro_check {
                effects.push(Effect::DetectExternalMetro { port: 8081 });
                return effects;
            }
            state.metro.set_starting();
            if let Some(path) = state.active_worktree_path.clone() {
                effects.push(Effect::SpawnMetro { worktree: path });
            }
        }
        // ... every Action arm returns effects instead of calling tokio::spawn
        _ => {}
    }
    effects
}
```

**Pitfall:** the 7+ recursive `update()` self-dispatch sites in current app.rs (F-206) will naturally become `effects.extend(update(state, next_action))` OR an explicit `Effect::ScheduleAction(Action)` variant. The audit says this "falls out of F-201" — that's correct IF the planner remembers that recursive calls must ALSO flow through the effect plumbing so the runner can log/replay/intercept them.

[CITED: AUDIT.md F-201 + F-206]

### Pattern 2: `Recipe` + `Prerequisite` as domain-level orchestration

**What:** Two domain enums in `src/domain/pipeline.rs` that encode the command-dependency rules currently spread across 11 inline sites in `update()` (F-204 enumerates: lines 843-887, 890, 949-953, 956-960, 1014, 1463-1499, 1622-1635, 1684-1705, 1713, 1722-1753, 657-674).

**When to use:** Every `update()` arm that currently pushes onto `command_queue` OR sets one of the 5 boolean coordination flags (`pending_restart`, `pending_switch_path`, `pending_metro_run`, `pending_metro_after_sync`, `skip_external_metro_check`). After F-204 consumer half, ALL of these collapse into `Recipe::expand()` returns.

**Example (verified against AUDIT.md F-204 target shape + 11-RESEARCH.md §"Recommended target shape for D-04"):**

```rust
// src/domain/pipeline.rs (new)
// Source: AUDIT.md F-204 + 11-RESEARCH.md sketch (auditor promotes to Phase 13 spec)

/// Precondition a CommandSpec requires before it can run.
pub enum Prerequisite {
    MetroRunning,
    DependenciesFresh { yarn: bool, pods: bool },
}

impl CommandSpec {
    /// Derive prerequisites from the variant — replaces `needs_metro()` inline checks.
    /// `needs_metro` stays for backward compat but is a wrapper around prerequisites().
    pub fn prerequisites(&self) -> Vec<Prerequisite> {
        match self {
            CommandSpec::RnRunAndroid { .. }
            | CommandSpec::RnRunIos { .. }
            | CommandSpec::RnRunIosDevice
            | CommandSpec::RnReleaseBuild => vec![Prerequisite::MetroRunning],
            // sync prereqs come from Recipe::SyncThenRun — not per-variant
            _ => vec![],
        }
    }
}

/// A single-command or multi-step dispatch unit. Dispatcher reads Recipe, never inline logic.
pub enum Recipe {
    Single(CommandSpec),
    Sequence(Vec<CommandSpec>),          // GitResetHardFetch, RnReleaseBuild+AdbInstall
    Clean(CleanOptions),                 // RnCleanCocoapods → RnCleanAndroid → RmNodeModules → [YarnInstall, YarnPodInstall if sync_after]
    SyncThenRun(CommandSpec),            // [YarnInstall, YarnPodInstall if iOS] → run_cmd
    SyncThenStartMetro,                  // [YarnInstall, YarnPodInstall] → MetroStart
    ReleaseBuildAndInstall,              // [RnReleaseBuild, AdbInstallApk]
    GitFetchThenReset,                   // [GitFetch, GitResetHard]
}

/// Staleness snapshot used by Recipe::expand to decide which sync commands are needed.
pub struct DependencyState {
    pub stale_yarn: bool,
    pub stale_pods: bool,
    pub is_ios_target: bool,
}

impl Recipe {
    /// Expand the recipe into a linear sequence of CommandSpec — dispatcher calls this once.
    pub fn expand(&self, deps: &DependencyState) -> Vec<CommandSpec> {
        match self {
            Recipe::Single(cmd) => vec![cmd.clone()],
            Recipe::Sequence(cmds) => cmds.clone(),
            Recipe::Clean(opts) => {
                let mut v = Vec::new();
                if opts.pods { v.push(CommandSpec::RnCleanCocoapods); }
                if opts.android { v.push(CommandSpec::RnCleanAndroid); }
                if opts.node_modules { v.push(CommandSpec::RmNodeModules); }
                if opts.sync_after { v.push(CommandSpec::YarnInstall); v.push(CommandSpec::YarnPodInstall); }
                v
            }
            Recipe::SyncThenRun(cmd) => {
                let mut v = Vec::new();
                if deps.stale_yarn { v.push(CommandSpec::YarnInstall); }
                if deps.stale_pods && deps.is_ios_target { v.push(CommandSpec::YarnPodInstall); }
                v.push(cmd.clone());
                v
            }
            Recipe::SyncThenStartMetro => {
                let mut v = Vec::new();
                if deps.stale_yarn { v.push(CommandSpec::YarnInstall); }
                if deps.stale_pods { v.push(CommandSpec::YarnPodInstall); }
                v // dispatcher knows to follow with MetroStart
            }
            Recipe::ReleaseBuildAndInstall => vec![CommandSpec::RnReleaseBuild, CommandSpec::AdbInstallApk],
            Recipe::GitFetchThenReset => vec![CommandSpec::GitFetch, CommandSpec::GitResetHard],
        }
    }
}
```

**Pitfall:** the 5 coordination boolean flags on `AppState` do NOT all die at once. Some encode "in-flight after effect dispatch" rather than "prerequisite ordering" (e.g., `pending_restart` after MetroStop-then-MetroStart). The F-204 consumer plan must enumerate which flags the `Recipe` replaces vs which become Effect state. Recommend keeping `pending_restart` alive initially (post-MetroExited behavior), and folding `pending_metro_run` / `pending_metro_after_sync` / `pending_switch_path` into `Recipe` variants + `Effect::ScheduleAction` chains. `skip_external_metro_check` ties to the metro-lifecycle state machine — consider moving it to `MetroState` sub-struct per F-209.

[CITED: AUDIT.md F-204 + 11-RESEARCH.md:378-406]

### Pattern 3: `KeyBinding` registry with context-filtered iteration

**What:** Single `&'static [KeyBinding]` const + a `BindingContext` enum. Three consumers (`handle_key`, `footer::render_footer`, `help_overlay::render_help`) iterate the slice filtered by context.

**When to use:** After F-400 type-half lands. Every keybinding in the codebase is one row in the registry.

**Example (verified against AUDIT.md F-400 target shape):**

```rust
// src/app/keybindings.rs (new — app-level per F-400 recommendation)
use crate::action::Action;  // becomes crate::domain::action::Action after F-002
use crate::app::{AppState, FocusedPanel, PaletteMode};
use crate::domain::command::ModalState;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug, Clone, Copy)]
pub enum BindingContext {
    Always,
    Normal,
    WorktreeTable,
    CommandOutput,
    Palette(PaletteMode),
    Modal(ModalKind),
    Overlay(OverlayKind),
}

#[derive(Debug, Clone, Copy)]
pub enum ModalKind {
    Confirm, TextInput, DevicePicker, CleanToggle,
    SyncBeforeRun, SyncBeforeMetro, ExternalMetroConflict, BranchPicker,
}

#[derive(Debug, Clone, Copy)]
pub enum OverlayKind { Help, Error }

#[derive(Debug)]
pub struct KeyBinding {
    pub key: KeyCode,
    pub label: &'static str,       // footer label, e.g. "c"
    pub short_desc: &'static str,  // footer hint, e.g. "clean…"
    pub long_desc: &'static str,   // help overlay description
    pub context: BindingContext,
    /// Produce the Action for the current state. Returns None when key is bound
    /// but conditionally inapplicable (e.g., `R` on WorktreeTable: metro-reload or refresh).
    pub action: fn(&AppState) -> Option<Action>,
    /// Optional visibility predicate for footer hints (R/J/Esc only shown when metro running).
    pub visible: fn(&AppState) -> bool,
}

pub const KEYBINDINGS: &[KeyBinding] = &[
    // Normal mode
    KeyBinding { key: KeyCode::Char('q'), label: "q", short_desc: "quit", long_desc: "Quit the application",
                 context: BindingContext::Normal, action: |_| Some(Action::Quit), visible: |_| true },
    KeyBinding { key: KeyCode::Char('?'), label: "?/F1", short_desc: "help", long_desc: "Open keybinding help",
                 context: BindingContext::Normal, action: |_| Some(Action::ShowHelp), visible: |_| true },
    // ... ~80 entries covering every current key in handle_key + footer + help_overlay
    // WorktreeTable R — the conditional one that drifted across the three sites
    KeyBinding { key: KeyCode::Char('R'), label: "R", short_desc: "reload",
                 long_desc: "Reload metro (when running) / Refresh list",
                 context: BindingContext::WorktreeTable,
                 action: |s| if s.metro.is_running() { Some(Action::MetroSendReload) } else { Some(Action::RefreshWorktrees) },
                 visible: |_| true },
    // ... etc
];

pub fn handle_key(state: &AppState, key: KeyEvent) -> Option<Action> {
    if key.kind != KeyEventKind::Press { return None; }
    for kb in KEYBINDINGS.iter() {
        if context_matches(&kb.context, state) && kb.key == key.code {
            return (kb.action)(state);
        }
    }
    None
}

pub fn footer_hints_for(state: &AppState) -> Vec<(&'static str, &'static str)> {
    KEYBINDINGS.iter()
        .filter(|kb| context_matches(&kb.context, state) && (kb.visible)(state))
        .map(|kb| (kb.label, kb.short_desc))
        .collect()
}

pub fn help_overlay_rows() -> Vec<HelpRow> {
    // Group by context.section() — returns section headers + rows per group
    // ...
}

fn context_matches(ctx: &BindingContext, state: &AppState) -> bool {
    match ctx {
        BindingContext::Always => true,
        BindingContext::Normal => state.modal.is_none() && state.palette_mode.is_none() && !state.show_help && state.error_state.is_none(),
        BindingContext::WorktreeTable => state.focused_panel == FocusedPanel::WorktreeTable && state.modal.is_none() && state.palette_mode.is_none(),
        BindingContext::CommandOutput => state.focused_panel == FocusedPanel::CommandOutput && state.modal.is_none() && state.palette_mode.is_none(),
        BindingContext::Palette(p) => state.palette_mode.as_ref() == Some(p),
        BindingContext::Modal(k) => matches_modal_kind(state.modal.as_ref(), *k),
        BindingContext::Overlay(OverlayKind::Help) => state.show_help,
        BindingContext::Overlay(OverlayKind::Error) => state.error_state.is_some(),
    }
}
```

**Pitfall:** The existing palette `_ => Some(Action::ModalCancel)` fallback (src/app.rs:344,351,362,373,380) means "any unbound key in palette mode closes the palette." That's a CONTEXT-LEVEL fallback, not a per-key one. The registry design must encode this either as a synthetic `KeyBinding { key: KeyCode::Char('*'), ... }` wildcard per palette OR as a post-loop fallback: `if state.palette_mode.is_some() { return Some(Action::ModalCancel); }`. Recommend the latter — simpler; preserves exact current behavior.

[CITED: AUDIT.md F-400 + F-208 + F-302 + F-303 + 11-RESEARCH.md:441-475]

### Pattern 4: `Adapters` struct + trait-object dependency injection

**What:** `run()` in runtime.rs constructs concrete adapters once, builds an `Adapters` struct of trait objects, passes it through. `update()` stays pure — only `effect_runner` borrows the adapters.

**Example (verified against AUDIT.md F-202 target shape):**

```rust
// src/app/adapters.rs (new — or part of runtime.rs)
// Source: AUDIT.md F-202 recommendation — verbatim struct

use std::sync::Arc;
use crate::domain::ports::*;

pub struct Adapters {
    pub command_runner: Arc<dyn CommandRunnerPort>,
    pub metro: Arc<dyn MetroPort>,
    pub port_probe: Arc<dyn PortProbePort>,
    pub worktrees: Arc<dyn WorktreePort>,
    pub devices: Arc<dyn DevicePort>,
    pub jira: Option<Arc<dyn JiraPort>>,       // Optional — None if no token configured
    pub multiplexer: Option<Arc<dyn MultiplexerPort>>,  // Optional — None outside tmux/zellij
    pub persistence: Arc<dyn PersistencePort>,  // IF F-111 lands; otherwise skip this field
}

// src/app/runtime.rs::run
pub async fn run(mut terminal: ratatui::DefaultTerminal) -> color_eyre::Result<()> {
    // Step 1: load config (was inline at app.rs:2079 today)
    let config = crate::infra::config::load_config().ok();

    // Step 2: construct concrete adapters
    let adapters = Adapters {
        command_runner: Arc::new(crate::infra::command_runner::TokioCommandRunner),
        metro: Arc::new(crate::infra::metro::TokioMetroAdapter::new()),
        port_probe: Arc::new(crate::infra::port::LsofPortProbe),
        worktrees: Arc::new(crate::infra::worktrees::GitWorktreeAdapter),
        devices: Arc::new(crate::infra::devices::AdbXcrunDevices),
        jira: config.as_ref().and_then(|c| build_jira_client(c).map(|j| Arc::new(j) as Arc<dyn JiraPort>)),
        multiplexer: crate::infra::multiplexer::detect_multiplexer().map(Arc::from),
        // persistence: Arc::new(crate::infra::persistence::FilePersistence) if F-111 lands
    };

    let mut state = AppState::default();
    // wire channels, event loop...
    // state.config stays but becomes Arc<DashConfig> perhaps — cleaner
}
```

**Pitfall:** `AppState` currently holds `Option<crate::infra::config::DashConfig>` (src/app.rs:134). Config is DATA, not a port — it doesn't need to become an Arc<dyn Port>. Keep the DashConfig struct in `infra/config.rs` (per audit — no finding filed against it); AppState holds it directly OR moves it into the `AppConfigState` sub-struct per F-209. Don't over-port.

[CITED: AUDIT.md F-202]

### Recommended Project Structure (post-Phase-13 — target)

```
src/
├── action.rs → DELETED (content moves to domain/action.rs per F-002)
├── app.rs → DELETED (split per F-200)
├── app/
│   ├── mod.rs            — re-exports: pub use state::*; pub use runtime::run;
│   ├── state.rs          — struct AppState (6-7 sub-structs per F-209)
│   ├── update.rs         — pub fn update(state, action) -> Vec<Effect> (pure)
│   ├── effect.rs         — pub enum Effect (15+ variants)
│   ├── effect_runner.rs  — pub struct EffectRunner + run_effects()
│   ├── handle_key.rs     — pub fn handle_key via KEYBINDINGS
│   ├── keybindings.rs    — KeyBinding, KEYBINDINGS const, context-filter helpers
│   ├── adapters.rs       — pub struct Adapters { Arc<dyn *Port>... }  (optional — may inline in runtime.rs)
│   ├── runtime.rs        — pub async fn run(terminal, adapters) — event loop
│   └── dispatch_tests.rs — EXISTING — preserves 17 COVER-03 tests; MUST keep compiling
├── domain/
│   ├── mod.rs            — pub mod {action, command, metro, refresh, worktree, jira, pipeline, ports}
│   ├── action.rs         — NEW (moved from root/action.rs per F-002)
│   ├── command.rs        — CommandSpec + is_cancellable() (new per REFACTOR-02)
│   ├── metro.rs          — MetroActivity, MetroStatus, MetroManager (trait MetroHandle per F-004, tokio types REMOVED)
│   ├── refresh.rs        — EXEMPLARY — keep 100% coverage
│   ├── worktree.rs       — unchanged (F-009 deferred)
│   ├── jira.rs           — NEW (extract_jira_key + 6 tests, moved from infra/jira.rs per F-107)
│   ├── pipeline.rs       — NEW (Prerequisite + Recipe per F-204 + REFACTOR-03)
│   └── ports/            — NEW module
│       ├── mod.rs
│       ├── command_runner_port.rs   — CommandRunnerPort + CommandEvent
│       ├── metro_port.rs            — MetroPort + opaque MetroHandle
│       ├── process_port.rs          — ProcessPort (relocated from infra/process.rs)
│       ├── jira_port.rs             — JiraPort (relocated from infra/jira.rs)
│       ├── multiplexer_port.rs      — MultiplexerPort (relocated from infra/multiplexer.rs)
│       ├── worktree_port.rs         — WorktreePort (new)
│       ├── device_port.rs           — DevicePort (new)
│       └── port_probe_port.rs       — PortProbePort + ExternalProcessInfo (new)
├── infra/
│   ├── mod.rs            — 12 or 13 modules (adds metro.rs); updated doc-claim
│   ├── command_runner.rs — TokioCommandRunner impl CommandRunnerPort (NO Action import)
│   ├── metro.rs          — NEW — TokioMetroAdapter impl MetroPort (absorbs 218 LOC from app.rs)
│   ├── process.rs        — TokioProcessClient impl domain::ports::ProcessPort
│   ├── jira.rs           — HttpJiraClient impl JiraPort (extract_jira_key REMOVED)
│   ├── multiplexer.rs    — Tmux/ZellijAdapter impl MultiplexerPort
│   ├── worktrees.rs      — GitWorktreeAdapter impl WorktreePort (+ private parsers)
│   ├── devices.rs        — AdbXcrunDevices impl DevicePort (+ private parsers)
│   ├── port.rs           — LsofPortProbe impl PortProbePort
│   ├── config.rs         — unchanged (DashConfig is DATA, not a port)
│   ├── android_prefs.rs  — unchanged (Minor F-111 deferred)
│   ├── sim_history.rs    — unchanged
│   ├── jira_cache.rs     — unchanged
│   └── tmux.rs → DELETED per F-112
├── ui/
│   ├── mod.rs            — unchanged doc-claim (now accurate)
│   ├── panels.rs         — imports from domain::jira instead of infra::jira (F-300 consumer)
│   ├── footer.rs         — calls keybindings::footer_hints_for (F-302)
│   ├── help_overlay.rs   — calls keybindings::help_overlay_rows (F-303)
│   ├── modals.rs         — unchanged (audit: deep, clean)
│   ├── error_overlay.rs  — unchanged
│   └── theme.rs          — unchanged
├── event.rs              — unchanged (optional: expand _ => None comment per F-003)
├── tui.rs                — unchanged
├── lib.rs                — updated re-exports (pub mod action → domain::action; add `pub mod domain::ports`)
└── main.rs               — passes terminal → runtime::run(terminal)

tests/                    — UNCHANGED structure; paths in imports may need tweak
├── common/mod.rs         — fake_metro_handle helper (unchanged signature; impl MAY change if MetroHandle becomes trait per F-004 — need Arc<dyn MetroHandle> fake)
├── metro_single_instance.rs  — keep green (tests rn_dash::app::update signature)
└── process_group_kill.rs — keep green (cfg'd to Linux+macOS; tests raw libc::kill)
```

### Anti-Patterns to Avoid

- **Effect enum becomes an impurity loophole.** `Effect::RunArbitraryClosure(Box<dyn Fn>)` defeats the purpose. Every Effect variant must be plain data (no closures, no task handles) so update() stays replayable.
- **Trait-object granularity mismatch.** Do not define `trait FilesystemPort` that's a grab-bag. Each port maps to one external concern (see audit's 8-port inventory in §Hexagonal port violations cross-module table). Avoid merging.
- **Pattern-match 8 modals in keybindings fallback.** The handle_key refactor must NOT enumerate palette × 23 command-specs in the const. Use conditional `action: fn(&AppState) -> Option<Action>` to encode conditionals.
- **Splitting app.rs into `app/{state,update,effect_runner,handle_key,runtime}.rs` without preserving `src/app/dispatch_tests.rs`.** That test file lives inside the app module and tests `super::*`. The split MUST preserve `#[cfg(test)] mod dispatch_tests;` at the correct parent module (likely `src/app/mod.rs` or via `src/app/update.rs`). Failing to do so breaks 17 passing tests — Phase 12 regression.
- **Moving `MetroHandle` to a trait WITHOUT updating `tests/common/mod.rs::fake_metro_handle`.** The test helper currently constructs a concrete MetroHandle with tokio channels. If F-004 makes MetroHandle a trait, the helper must be updated in the same atomic plan. Otherwise integration tests break.
- **Treating the `_ => Some(Action::ModalCancel)` palette fallback as per-key instead of per-context.** Enumerate every palette key explicitly + add a context-level fallback in handle_key. Missing this loses the "press any unbound palette key to close palette" UX.
- **Deleting `command_queue` entirely in the F-204 consumer plan.** Today `command_queue` also serves as the mechanism for queueing CommandOutput-line bursts during dispatch. It is both a prerequisite-ordering queue AND a dispatch FIFO. The F-204 consumer plan replaces the prerequisite-ordering use case; the FIFO use case may need to persist (or be lifted into Phase 14's per-worktree queue). Do not delete wholesale.

## Critical & Major Findings Map

| F-NNN | Sev | Dim | Blocks | Blocked by | Test coverage (Phase 12) | Risk level | Plan placement |
|-------|-----|-----|--------|------------|--------------------------|------------|----------------|
| F-002 | Major | Fowler | F-101 | — | action grammar implicit in dispatch_tests + metro integration | Low (file move + 2 imports) | 13-01 (Wave A) |
| F-101 | **Critical** | Hex | F-202 | F-002, F-103, F-201 | `dispatch_tests::command_queue::*` (3 tests), metro_single_instance (2 tests) — command_runner untested directly (0% coverage baseline) | High (20-LOC flow + 49 tests — but command_runner itself is 0% coverage; relies on transitive test) | 13-06 (Wave C) |
| F-103 | Major | Hex | F-101 | — | 0% coverage on process.rs; indirect via metro integration | Low (trait relocation, no impl change) | 13-01 (Wave A — piggyback) |
| F-106 | Major | Hex | F-107 | F-103 | 70% coverage on jira.rs (6 extract_jira_key tests) | Low (trait relocation) | 13-01 (Wave A — piggyback) |
| F-110 | Major | Hex | — | F-103 | 0% coverage on multiplexer.rs | Low (trait relocation) | 13-01 (Wave A — piggyback) |
| F-107 + F-300 + F-301 | Major × 3 | Hex | — | F-106 | 70% coverage on jira.rs covers `extract_jira_key` — MUST preserve 6 tests | Low (one move + two imports; symmetric fix) | 13-01 (Wave A — piggyback) |
| F-102 / F-104 / F-105 | Major × 3 | Hex | F-202 | F-103 | 0% coverage on port/worktrees/devices — no direct test | Medium (3 new ports; adapter shells still call existing infra fns) | 13-03 (Wave A-B bridge) |
| F-201 (types) | **Critical** | TEA | F-200, F-101, F-203, F-204 consumer | F-002 | Pure data — no test needed for enum definition | Trivial | 13-03 (Wave A) |
| F-204 (types) | Major | Prereq | F-204 consumer | — | Pure data — `Prerequisite` + `Recipe` enums | Trivial | 13-03 (Wave A) |
| F-004 | Major | Hex/Ousterhout | F-203 | F-002, F-103 | register_twice test + metro integration (5 tests) — MetroHandle shape changes | **High** (test helper in tests/common/mod.rs must update atomically) | 13-03 (Wave A — paired with F-203 trait def) |
| F-400 (registry) | Major | Ousterhout/D-14 | F-208, F-302, F-303 | F-002 | No direct coverage — keybindings are tested transitively via dispatch_tests | Low (type + const) | 13-05 (Wave B) |
| REFACTOR-02 | — | Req | — | — | `is_cancellable()` is a predicate — add inline tests in command.rs mirroring existing predicates | Trivial | 13-02 (Wave A-B, after F-002) |
| F-200 (split) | **Critical** | Ousterhout | F-201 consumer, F-202, F-208, F-209 | F-002, F-201 types, F-400 (optional) | 5 tests in metro_single_instance + 17 dispatch_tests + 3 metro.rs inline + 1 PGID = 26 tests depend on `rn_dash::app::update` symbol at crate-relative path | **Highest LOC churn** | 13-06 (Wave C) or split across 13-06 (move files) + 13-07 (rewire imports) |
| F-201 (consumer) | **Critical** | TEA | F-204 consumer, F-206 | F-200, F-201 types | All 26 tests — they currently call `update(&mut state, Action::*, &metro_tx, &handle_tx)`; new signature is `update(&mut state, Action::*) -> Vec<Effect>`; tests must update to assert on Vec<Effect> | **Highest behavior surface** — every arm changes | 13-07 (Wave C) |
| F-202 (consumer) | **Critical** | Hex | — | F-200, F-101, F-102, F-104, F-105, F-106, F-110, F-203 | No direct test; verified by grep guard `rg 'crate::infra::' src/app/` → 0 matches | Medium (43 import sites) | 13-08 (Wave C) |
| F-203 (consumer) | **Critical** | Hex/Fowler | — | F-200, F-004, F-201 types | 5 tests in metro_single_instance + register panic + PGID — MetroHandle type shift risk | **High** — tokio channel plumbing moves | 13-07 or 13-08 (Wave C) |
| F-204 (consumer) | Major | Prereq | F-209 | F-200, F-201 consumer, F-204 types | dispatch_tests (command_queue 3 tests) + metro_single_instance pending_restart assertions | Medium (11 inline sites + 5 flags) | 13-09 (Wave C-D bridge) |
| F-205 | Major | Catch-All | — | F-200 | No specific test; Rust exhaustiveness is the guard post-fix | Low (mechanical substitution at 4 lines) | 13-09 or piggyback on 13-07 |
| F-209 | Major | Ousterhout | — | F-200, F-204 consumer | All 26 tests touch AppState fields — each sub-struct split breaks transitively | **High** — field-access churn; but each sub-struct move is mechanical | 13-10 (Wave C end) |
| F-208 | Major | Ousterhout | — | F-200, F-400 | 17 dispatch_tests — each calls handle_key with a KeyEvent; new handle_key walks KEYBINDINGS | Medium | 13-10 (Wave D) |
| F-302 | Major | Ousterhout/D-14 | — | F-400 | 0% coverage on footer.rs — no test | Low (function rewrite) | 13-10 (Wave D) |
| F-303 | Major | Ousterhout/D-14 | — | F-400 | 0% coverage on help_overlay.rs — no test | Low (function rewrite) | 13-10 (Wave D) |

**Summary:** 12 Critical/Major findings (some bundled: F-107+F-300+F-301, F-102+F-104+F-105). Plus REFACTOR-02 (tiny) + REFACTOR-03 = F-204 (already counted). **10 total plan boundaries recommended** (see §Recommended Target Plans for the full carving).

## Risk Surface Map — What Can Break + What Guards It

| Refactor | What can break | Existing test guard | Gap | Mitigation |
|----------|----------------|---------------------|-----|-----------|
| F-200 app-split | `rn_dash::app::update` path changes; 26 tests import it | `metro_single_instance.rs` (uses `rn_dash::app::{update, AppState}`), `dispatch_tests.rs` (uses `super::*`) | Moving `update` to `src/app/update.rs` keeps the symbol path `rn_dash::app::update` IF `src/app/mod.rs` has `pub use update::update;`. Otherwise tests fail-fast with "unresolved import" — good trip-wire | Ensure `src/app/mod.rs` re-exports every public item the tests import. Grep test crate imports: `rg 'rn_dash::app::' tests/ src/app/dispatch_tests.rs` — 6 distinct imports: `update`, `AppState`, `FocusedPanel`, `PaletteMode`, `active_worktree_id`/`active_output`/`active_output_scroll`. ALL must remain reachable |
| F-201 consumer (update pure) | `update()` signature changes from `(..., tx, htx)` → `-> Vec<Effect>`; all 26 test call sites must update | All 26 tests | **The tests currently pass `metro_tx` / `handle_tx` channels to update() — they hold receivers to prevent "channel closed" panics per 12-RESEARCH.md Pitfall 10.** After F-201, update() no longer takes channels — the EFFECT RUNNER does. Tests must be rewritten to (a) call update() → get Vec<Effect>, (b) optionally invoke effect_runner on those effects with a fake Adapters | **This is the single largest test-update surface in the phase.** Mitigation: update all test call sites in the same plan as F-201 consumer. Tests do not need to invoke effect_runner — they can assert on the returned Vec<Effect>, which is strictly more expressive than the current tokio::spawn side-effect observation |
| F-202 consumer (trait-object injection) | 43 `crate::infra::*` references in app.rs today become trait-method calls | Indirect via all 26 tests | Tests currently construct `AppState::default()` which doesn't exercise infra calls. Post-F-202, `AppState` holds `Adapters` OR effect_runner holds them — if AppState holds them, `AppState::default()` must build a default Adapters struct (problematic — needs test adapters). If effect_runner holds them, AppState::default() is unchanged → tests unaffected at the type level | Recommend **effect_runner owns Adapters, AppState does not.** Keeps AppState::default() unchanged and tests stable. The current `jira_client: Option<Arc<dyn JiraClient>>` field on AppState is an anti-pattern per F-202 — move it into Adapters |
| F-203 consumer (metro→infra) | 218 LOC of metro helpers move from app.rs → infra/metro.rs; tokio::spawn wiring relocates | `metro_single_instance.rs` (2 tests), `src/domain/metro.rs::tests` (3 tests) | MetroHandle shape changes (F-004 trait). `tests/common/mod.rs::fake_metro_handle` returns concrete `MetroHandle` today. If F-004 makes MetroHandle a trait, fake_metro_handle must return `Box<dyn MetroHandle>` OR a `FakeMetroHandle` concrete type | Plan 13-03 (F-004 + F-203 types) MUST also update `tests/common/mod.rs::fake_metro_handle` in the same commit. Otherwise integration tests break compilation |
| F-204 consumer (Recipe) | 11 inline prereq sites collapse; 5 boolean flags partially collapse | `metro_single_instance.rs` asserts `pending_restart` — this flag likely STAYS (restart is metro-lifecycle, not prereq); `dispatch_tests::command_queue::{push,drain}` tests assert on command_queue membership — if Recipe replaces command_queue, these tests break | `pending_restart` survives F-204 (stays in `MetroState` per F-209). `command_queue` semantics change: today it's a FIFO of CommandSpecs; post-F-204, it becomes a FIFO of expanded Recipes' leftovers (dispatcher runs Recipe::expand → dispatches first, queues rest). Tests either (a) update to use Recipe-based assertions OR (b) keep testing `command_queue` as now-derived observable | Recommend **option (a) — update dispatch_tests to Recipe-based assertions**, since the semantic is the new contract; the queue is an implementation detail post-F-204 |
| F-004 (MetroHandle trait) | `tests/common/mod.rs::fake_metro_handle` and `src/domain/metro.rs::dummy_handle` both construct concrete MetroHandle today | 5 metro tests | Trait-object construction diverges | Recommend `pub struct TokioMetroHandle` (concrete prod impl) + `pub struct FakeMetroHandle` (test helper in `src/domain/metro.rs::testing`) implementing a trait. `MetroManager::handle: Option<Box<dyn MetroHandle>>`. Test helper updates atomically with F-004 plan |
| F-101 (CommandRunnerPort) | `use crate::action::Action` dies from command_runner.rs; signature changes to return `UnboundedReceiver<CommandEvent>` | 0% coverage on command_runner.rs directly; indirectly via dispatch_tests::command_queue | High-risk subtle bug potential: the `build_argv` GitResetHard special case at command_runner.rs:119 must survive the move. `spawn_command_task(spec, path, branch, tx)` → `TokioCommandRunner::spawn(spec, path, branch) -> UnboundedReceiver<CommandEvent>` | Enumerate the behavior: (1) argv construction with `build_argv` GitResetHard branch preservation, (2) `.kill_on_drop(true)` preservation, (3) stdout+stderr concurrent streaming, (4) exit signal, (5) spawn-failure error surfacing. Call this out in Plan 13-06 checklist |
| F-400 + F-208 keybinding registry | `handle_key` implementation rewrites; 80+ entries in KEYBINDINGS | 17 dispatch_tests | **Palette `_ => Some(Action::ModalCancel)` fallback** is a per-CONTEXT fallback, not per-key. Registry must preserve this as either a wildcard entry or a post-loop context-fallback. Drift observed in AUDIT.md already (R → reload vs R → refresh conditional). | Per §Pattern 3 Pitfall — preserve as post-loop context-fallback. Test 13-dispatch's `every_yarn_palette_key` test is the guard. |
| F-205 exhaustive modal arms | `ModalInputChar` / `ModalInputBackspace` at app.rs:1140, 1153 — replace `_ => {}` with exhaustive named match | dispatch_tests modal_dismissal (8 tests) — asserts `state.modal == None` post-dismissal | No specific assertion on which chars are dropped per modal; Rust exhaustiveness catches missing arms at compile time (trivial). | Free win post-F-200. Fold into 13-07 or later |
| F-209 sub-struct grouping | Every `state.field` reader at ~450+ call sites updates to `state.sub.field` | All 26 tests + every UI rendering path | **The single most mechanically invasive change** — but the change is mechanical (sed-style rename) + cleanly covered by compiler errors | Do this LAST (Plan 13-10) so all other refactors see the flat AppState. Tests break immediately at compile time if field access paths drift — use compiler to drive the fix |

**Coverage floor invariants (MUST preserve from COVERAGE-THRESHOLDS.md):**

- `src/domain/refresh.rs >= 100%` — 17 inline tests; none of Phase 13 touches this file
- `src/domain/metro.rs >= 70%` — register_twice + register_clear tests survive F-004 trait refactor (helper moves)
- `src/infra/jira.rs >= 70%` — the 6 `extract_jira_key` tests **MOVE with the function** to `src/domain/jira.rs` per F-107. This changes WHICH file carries the coverage but preserves the test count. `infra/jira.rs` baseline drops post-move (acceptable structural change per COVERAGE-THRESHOLDS.md §"Phase 13+ may LOWER a row's baseline (refactor removes code)"). `domain/jira.rs` becomes new row.
- `src/infra/android_prefs.rs >= 55%` — not touched by Phase 13
- `src/app.rs >= 10%` — F-200 split REMOVES `src/app.rs` entirely. New threshold rows appear for `src/app/{state,update,effect,effect_runner,handle_key,keybindings,runtime}.rs`. Policy: treat as structural change per COVERAGE-THRESHOLDS.md changelog; update thresholds in same commit as the split.
- `src/domain/command.rs >= 5%` — REFACTOR-02 adds `is_cancellable()` with inline tests → coverage goes UP, threshold may ratchet

**Grep-based shape guards (to add in Phase 13 validation):**

```bash
# After Phase 13 lands, ALL of these must pass:
! rg 'use crate::action::Action' src/infra/                    # F-101 — infra never imports Action
! rg 'crate::infra::' src/app/                                 # F-202 — app never imports infra
! rg 'crate::infra::' src/ui/                                  # F-300 — ui never imports infra
! rg 'use crate::app' src/domain/                              # domain never imports app
! rg 'tokio::spawn' src/app/update.rs                          # F-201 — update() is pure
! rg 'use (ratatui|crossterm)' src/domain/                     # domain has no UI deps
grep -q 'pub enum Effect' src/app/effect.rs                    # F-201 — Effect exists
grep -q 'pub fn is_cancellable' src/domain/command.rs          # REFACTOR-02
grep -q 'pub enum Recipe' src/domain/pipeline.rs               # REFACTOR-03
grep -q 'pub trait CommandRunnerPort' src/domain/ports/        # F-101
grep -q 'pub trait MetroPort' src/domain/ports/                # F-203
grep -q 'pub const KEYBINDINGS' src/app/keybindings.rs         # F-400
test ! -f src/action.rs                                        # F-002 moved
test ! -f src/app.rs                                           # F-200 split
test ! -f src/infra/tmux.rs                                    # F-112 deleted
```

## Recommended Target Plans — 10 plans in 5 waves

> D-02 permits Minor deferrals with rationale. This carving addresses all 12 Critical+Major
> findings plus REFACTOR-02/03 in 10 atomic plans. Each plan's end-state compiles,
> `cargo clippy -D warnings` is green, and `cargo test` passes.

### Wave A — Foundational type/trait extractions (parallel-safe)

**Plan 13-01: `domain::action` + `domain::ports` skeleton + trait relocations**
- Move `src/action.rs` → `src/domain/action.rs` (F-002)
- Update `src/app.rs:2` + `src/infra/command_runner.rs:12` imports → `crate::domain::action::Action`
- Create `src/domain/ports/mod.rs`
- Move `ProcessClient` trait → `domain::ports::process_port::ProcessPort` (F-103), keep `TokioProcessClient` in infra
- Move `Multiplexer` trait → `domain::ports::multiplexer_port::MultiplexerPort` (F-110), keep adapters in infra
- Move `JiraClient` trait → `domain::ports::jira_port::JiraPort` (F-106), keep `HttpJiraClient` in infra
- Move pure `extract_jira_key` + 6 tests → `src/domain/jira.rs` (F-107)
- Update `src/ui/panels.rs:71` import → `crate::domain::jira::extract_jira_key` (F-300; F-301 becomes true by construction)
- Update `src/lib.rs` re-exports
- Piggyback Minors: F-003 (event.rs comment), F-005 (command.rs doc), F-100 (infra/mod.rs doc-claim)
- **Atomicity:** File moves + import rewrites; everything compiles after single atomic commit
- **Risk:** Low — pure relocation; 6 existing jira tests verify extract_jira_key behavior preserves
- **Duration estimate:** Medium (1 hr) — 5 traits + 1 enum + 1 pure fn + 6 tests moved; ~10 files touched

**Plan 13-02: REFACTOR-02 `CommandSpec::is_cancellable()`**
- Add `pub fn is_cancellable(&self) -> bool` to `impl CommandSpec` in `src/domain/command.rs`
- Add inline tests — at minimum: one test per command family asserting the predicate value (7 tests: git-false, yarn-true, rn-run-true, rn-clean-true, rm-node-modules-true, adb-install-true, shell-true)
- Flat-enum predicate (NOT category split per AUDIT-ADDENDUM F-501 decision)
- **Atomicity:** Single file change + tests
- **Risk:** Trivial — pure addition; no consumer yet (consumers land in Phase 15)
- **Duration estimate:** Small (<30min)

**Plan 13-03: `Effect` enum + `Prerequisite`/`Recipe` types + `MetroPort` trait + `MetroHandle` trait (F-004)**
- Create `src/app/effect.rs` with `pub enum Effect { ... }` (15+ variants per F-201) — NO consumers yet
- Create `src/domain/pipeline.rs` with `pub enum Prerequisite`, `pub enum Recipe`, `impl Recipe { pub fn expand() }`, `pub struct DependencyState` (F-204 type half + REFACTOR-03)
- Create `src/domain/ports/metro_port.rs` with `pub trait MetroPort` (4-5 methods), `pub enum MetroActivity` re-export or relocation, opaque `pub trait MetroHandle` (F-004 + F-203 type half)
- Update `src/domain/metro.rs`: replace `pub struct MetroHandle { pub pid, worktree_id, stdin_tx, stream_task, stdin_task, kill_tx }` with `pub trait MetroHandle { fn pid(&self) -> u32; fn worktree_id(&self) -> &str; fn send_stdin(&self, bytes: Vec<u8>) -> anyhow::Result<()>; fn kill(self: Box<Self>) -> anyhow::Result<()>; }`. `MetroManager::handle: Option<Box<dyn MetroHandle>>`.
- Update `tests/common/mod.rs::fake_metro_handle` to return `Box<dyn MetroHandle>` via a test-only `FakeMetroHandle` struct (either in tests/common/mod.rs or in a new `src/domain/metro.rs::testing` module)
- Update `src/domain/metro.rs::tests::dummy_handle` same way
- `TokioMetroHandle` does NOT exist yet (lands in 13-07 with F-203 consumer)
- Preserve the `register_twice` panic test (F-004 refactor is transparent at that level)
- **Atomicity:** Types + trait defs + test-helper updates — everything compiles; 5 metro tests stay green
- **Risk:** Medium (test helper must change atomically)
- **Duration estimate:** Medium (1-1.5 hr) — trait design + test migration; validate with `cargo test --test metro_single_instance` + `cargo test --lib domain::metro`

### Wave B — New port traits + CommandRunnerPort adapter

**Plan 13-04: New domain ports — `WorktreePort`, `DevicePort`, `PortProbePort` + adapter shells**
- Define `src/domain/ports/{worktree_port,device_port,port_probe_port}.rs` (3 new traits per F-102/F-104/F-105)
- Move `ExternalMetroInfo` → rename to `ExternalProcessInfo` in `src/domain/ports/port_probe_port.rs` (per F-102)
- Adapter shells: `infra::worktrees::GitWorktreeAdapter`, `infra::devices::AdbXcrunDevices`, `infra::port::LsofPortProbe` — each wraps the existing free functions in their own module
- No app-side rewiring yet (consumer in 13-08)
- Free functions in infra stay callable during transition (do not delete yet) — or do delete if adapter-shell call path works end-to-end: do delete (less dead code)
- `src/app.rs` still calls the free functions; compilation passes
- **Alternative:** Merge with 13-03 if schedule tight. Recommended separate for plan-atomicity
- **Atomicity:** 3 traits + 3 adapters; no consumer changes
- **Risk:** Low — new traits over existing behavior
- **Duration estimate:** Medium (1 hr)

**Plan 13-05: F-400 `KeyBinding` + `KEYBINDINGS` registry (type half)**
- Decide placement — recommend `src/app/keybindings.rs` (auditor's recommendation per F-400)
- **But F-200 split hasn't landed yet.** Options:
  - (a) Create `src/app/keybindings.rs` in anticipation — requires `src/app/mod.rs` to exist, which requires app.rs to be a directory. Conflict.
  - (b) Create `src/keybindings.rs` at root for now; relocate to `src/app/keybindings.rs` in Plan 13-06 F-200 split commit.
  - (c) Delay until AFTER F-200 split.
- **Recommend (c):** swap 13-05 and 13-06 in the sequence. Define `KeyBinding` types as part of the post-F-200 plan 13-07 (handle_key lives in `src/app/handle_key.rs` after split).
- **Revised Wave B: only 13-04.** 13-05 content folds into 13-07 (Wave C).

### Wave C — The hard wave — app split + TEA purity + hexagonal injection

**Plan 13-06: F-200 app.rs split** — structural lift-and-shift
- Create `src/app/{mod.rs, state.rs, update.rs, effect_runner.rs, handle_key.rs, runtime.rs, adapters.rs}`
- `src/app/state.rs`: `AppState` struct + `Default` impl + `FocusedPanel` + `ErrorState` + `PaletteMode` + `active_worktree_id/_output/_scroll` helpers
- `src/app/update.rs`: `pub fn update(state, action, metro_tx, handle_tx)` — **signature unchanged** from today; the entire ~1520-line function body moves verbatim (F-201 consumer comes later)
- `src/app/handle_key.rs`: `pub fn handle_key` — moves verbatim (F-208 consumer comes later)
- `src/app/runtime.rs`: `pub async fn run(terminal)` — moves verbatim; the 7 async metro helpers (`spawn_metro_task`, etc.) stay here temporarily (F-203 consumer moves them out)
- `src/app/effect_runner.rs`: STUB — empty struct + empty impl (F-201 consumer populates)
- `src/app/adapters.rs`: STUB — empty struct (F-202 consumer populates)
- `src/app/mod.rs`: re-export everything tests import: `pub use state::{AppState, FocusedPanel, PaletteMode, ErrorState, active_worktree_id, active_output, active_output_scroll}; pub use update::update; pub use runtime::run; pub use handle_key::handle_key;` and `#[cfg(test)] pub mod dispatch_tests;`
- Delete `src/app.rs`
- Update `src/lib.rs` — `pub mod app;` still works (now points to directory)
- Update `src/main.rs` — `use rn_dash::{app, tui}` still works
- **Atomicity:** Massive file reorganization; nothing changes behaviorally. Tests should all pass unchanged
- **Risk:** **HIGHEST LOC CHURN** — ~2500 lines moved across 6 files. Risk is spurious breakage from name-clash or missing re-exports
- **Duration estimate:** Large (2-3 hr) — do this as ONE atomic plan with careful file-by-file migration; after each file-move commit, `cargo test && cargo clippy -- -D warnings`

**Plan 13-07: F-201 consumer (update becomes pure) + F-203 consumer (metro helpers → infra) + F-208 consumer (handle_key reads KEYBINDINGS) + F-400 registry impl**
- Rewrite `src/app/update.rs`: signature becomes `pub fn update(state: &mut AppState, action: Action) -> Vec<Effect>` — every `tokio::spawn` site returns an Effect variant; recursive `update()` self-dispatch sites either inline-extend or return `Effect::ScheduleAction(Action)` (new Effect variant)
- Populate `src/app/effect_runner.rs`: `pub struct EffectRunner { adapters, tx, htx }` + `pub async fn run_effects(&self, effects: Vec<Effect>)` — each variant translates to adapter call OR tokio::spawn OR direct invocation
- Move 7 async metro helpers from `src/app/runtime.rs` (temporarily there post-F-200) → `src/infra/metro.rs::TokioMetroAdapter` — implement `MetroPort` trait from 13-03
- Create `src/app/keybindings.rs`: `KeyBinding` struct + `KEYBINDINGS: &[KeyBinding]` const (~80 entries translated from current handle_key) + `pub fn footer_hints_for`, `pub fn help_overlay_rows` helpers
- Rewrite `src/app/handle_key.rs`: `pub fn handle_key(&AppState, KeyEvent) -> Option<Action>` walks KEYBINDINGS filtered by context
- Update all 26 test call sites: dispatch_tests (17) assert on `Vec<Effect>` returned by update(); metro_single_instance (2) same
- **Atomicity:** THE behavior-surface plan. All 26 tests must pass with new assertions
- **Risk:** **HIGHEST BEHAVIOR-SURFACE** — every action arm affected
- **Duration estimate:** Large (3-4 hr) — may split across multiple commits within single plan. After each Action family migration, run `cargo test -- --nocapture` with the affected tests

**Plan 13-08: F-202 consumer (hexagonal injection) + F-101 consumer (CommandRunnerPort)**
- Define `src/domain/ports/command_runner_port.rs`: `pub enum CommandEvent { OutputLine(String), Exited(ExitStatus) }` + `pub trait CommandRunnerPort { fn spawn(&self, spec, cwd, branch) -> UnboundedReceiver<CommandEvent>; }`
- Rewrite `src/infra/command_runner.rs`: `pub struct TokioCommandRunner` implementing CommandRunnerPort; **REMOVE `use crate::action::Action`**; body preserves `build_argv` GitResetHard special case + `.kill_on_drop(true)`; stdout/stderr streaming becomes `CommandEvent::OutputLine` sends
- Populate `src/app/adapters.rs`: `pub struct Adapters { command_runner, metro, worktrees, devices, port_probe, jira, multiplexer }` with `Arc<dyn Port>` trait objects
- Update `src/app/runtime.rs::run`: construct concrete adapters + build Adapters struct
- Update `src/app/effect_runner.rs`: receive `&Adapters`; translate `Effect::SpawnCommand { spec, cwd, branch }` into `adapters.command_runner.spawn(spec, cwd, branch)` + translate each `CommandEvent` → `Action::CommandOutputLine` / `Action::CommandExited` before sending to update-tx
- Remove every `crate::infra::*` import from `src/app/` (43 current references) — replace with `adapters.<port>.<method>(...)` calls in effect_runner
- **Grep guard at end:** `! rg 'crate::infra::' src/app/`  MUST return 0 hits
- **Atomicity:** Every infra call in effect_runner becomes a trait method; compilation + tests verify
- **Risk:** Medium (43 import sites; mechanical mostly)
- **Duration estimate:** Large (2-3 hr)

### Wave D — Recipe consumer + state grouping + UI rewires

**Plan 13-09: F-204 consumer (Recipe dispatches) + F-205 (exhaustive modal arms)**
- Rewrite the 11 inline prereq sites in `src/app/update.rs` to use `Recipe::expand(&DependencyState)`:
  - `CommandRun` — wraps target in `Recipe::Single(spec)` OR `Recipe::SyncThenRun(spec)` if sync needed
  - `SyncBeforeRunAccept` — `Recipe::SyncThenRun`
  - `SyncBeforeMetroAccept` — `Recipe::SyncThenStartMetro`
  - `CleanConfirm` — `Recipe::Clean(opts)`
  - `GitResetHardFetch` dispatch — `Recipe::GitFetchThenReset`
  - `RnReleaseBuild` dispatch — `Recipe::ReleaseBuildAndInstall`
  - `WorktreeSwitchToSelected` — `Recipe::SyncThenStartMetro` (if stale) or direct sequence
- Collapse boolean flags:
  - `pending_metro_run`, `pending_metro_after_sync` → Recipe variant + Effect chain (DELETE flags)
  - `pending_switch_path` → encoded in Recipe::SyncThenStartMetro data
  - `pending_restart` STAYS (metro-lifecycle state, not prereq)
  - `skip_external_metro_check` STAYS (or moves to MetroState sub-struct in 13-10)
- `command_queue` semantic: dispatcher pops + calls `Recipe::expand()` on each, or flattens Recipe at enqueue time — design decision for the planner; recommend flatten-at-enqueue
- Update `dispatch_tests::command_queue` (3 tests): assert on Recipe expansion + queue contents post-flatten
- F-205: replace `_ => {}` arms at app.rs:1140, 1153 (now in update.rs after split) with exhaustive ModalState enumeration
- **Atomicity:** Every inline prereq site migrated in one plan; Rust exhaustiveness catches missed cases
- **Risk:** Medium — behavioral equivalence must hold for the sync-before-run modal flow; 3 dispatch_tests guard the queue observable
- **Duration estimate:** Large (2-3 hr)

**Plan 13-10: F-209 (AppState sub-struct grouping) + F-208 cleanup + F-302 footer + F-303 help-overlay + F-112 tmux.rs delete + F-108 is_inside_tmux move**
- Group AppState's 39 fields into 6-7 sub-structs per F-209:
  - `MetroState { metro, active_worktree_path, skip_external_metro_check, pending_restart, pending_switch_path, pending_metro_after_sync }`
  - `WorktreeBrowserState { worktrees, table_state, selected_worktree_id, fullscreen_panel, worktree_op_in_flight }`
  - `CommandRunnerState { command_queue, output_by_worktree, output_scroll_by_worktree, running_command, command_task }`
  - `ModalStackState { modal, palette_mode, pending_g, pending_claude_open, pending_android_mode, pending_worktree_removal, pending_worktree_add, pending_new_branch_base, pending_new_branch_worktree }` — empties partially post-F-204
  - `JiraState { title_cache, project_prefix }`  (jira_client moved to Adapters per F-202)
  - `AppConfigState { config, repo_root, claude_flags, android_mode }`
  - Keep at AppState root: `focused_panel`, `show_help`, `error_state`, `should_quit`
  - (multiplexer: moves to Adapters)
- Inner fields become `pub(crate)` where safe
- Touch every field-access in `src/app/*.rs` + `src/ui/*.rs` (compiler-driven)
- Rewrite `src/ui/footer.rs::key_hints_for` → `keybindings::footer_hints_for(state)` call (F-302)
- Rewrite `src/ui/help_overlay.rs::render_help` → `keybindings::help_overlay_rows()` call (F-303). Keep the Icons section hand-coded (per audit F-303 recommendation — it's icons, not keybindings)
- Delete `src/infra/tmux.rs` + remove `pub mod tmux;` from `src/infra/mod.rs` (F-112)
- Move `is_inside_tmux` from `infra/jira.rs` → `infra/multiplexer.rs` (F-108)
- Fix `src/infra/mod.rs` doc-claim (F-100) — now accurate after Phase 13
- **Atomicity:** Compiler-driven — field-access churn is automatic via rustc errors
- **Risk:** High invasive (~450 field-access sites) but LOW semantic risk — mechanical rename
- **Duration estimate:** Large (2-3 hr)

### Total count: 10 plans — fits within reasonable phase-13 budget given AUDIT.md scope

| Plan | Wave | Size | Risk | Dependencies |
|------|------|------|------|--------------|
| 13-01 action.rs + ports skeleton + trait relocations | A | Medium | Low | COVER gate green (satisfied) |
| 13-02 REFACTOR-02 is_cancellable() | A | Small | Trivial | 13-01 (action is in domain for any derives) |
| 13-03 Effect + Recipe + Prerequisite + MetroPort + MetroHandle trait | A | Medium | Medium (test helper) | 13-01 |
| 13-04 New ports (WorktreePort, DevicePort, PortProbePort) + adapter shells | B | Medium | Low | 13-01, 13-03 |
| 13-06 F-200 app.rs split (structural lift-and-shift) | C | Large | High (LOC churn) | 13-01, 13-02, 13-03 (types must exist — though consumers come later) |
| 13-07 update() pure + metro helpers → infra + KEYBINDINGS + handle_key reads registry | C | Largest | Highest (behavior) | 13-06, 13-03 (Effect type + MetroPort trait), tests/common update already done in 13-03 |
| 13-08 Adapters injection + CommandRunnerPort + F-202 full hexagonal rewire | C | Large | Medium | 13-07, 13-04, 13-01 |
| 13-09 Recipe consumer + F-205 exhaustive arms | D | Large | Medium | 13-07, 13-03 (Recipe types) |
| 13-10 F-209 state grouping + UI rewires (F-302/F-303) + Minor cleanup | D | Large | Low-Medium | 13-07, 13-08, 13-09 |

**Parallelization opportunities:**
- 13-01, 13-02, 13-03 can run in any order after COVER gate — recommend 13-01 first (it provides `crate::domain::action::Action` path which 13-02's tests may reference)
- 13-04 can run in parallel with 13-02 or 13-03
- Within a wave, plans are sequential in the YOLO CLAUDE.md pattern (one plan at a time per `/gsd:execute-phase`)

**Wave gate pattern:** After each wave, `cargo test && cargo clippy --all-targets -- -D warnings && make cov-check` MUST be green. If `make cov-check` script doesn't exist as a per-row ratchet verifier yet, manually eyeball `make cov-baseline` output against COVERAGE-THRESHOLDS.md.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Type-driven cancellability | Per-variant `matches!(spec, GitResetHard | GitPull | ...)` at every `CommandCancel` call site | `CommandSpec::is_cancellable(&self) -> bool` predicate on the enum (REFACTOR-02) | Audit F-007 + REFACTOR-02 both mandate this. One predicate, one source of truth, enforced by type |
| Recipe/prereq ordering | 11 inline `if spec.needs_metro() && !metro.is_running() { pending_... }` arms | `Recipe::expand(&DependencyState) -> Vec<CommandSpec>` (REFACTOR-03) | Audit F-204 — avoids scattered logic |
| Keybinding mappings across 3 files | Hand-coded tables in footer + help_overlay + handle_key | `KEYBINDINGS: &[KeyBinding]` single source (F-400) | Audit D-14 — drift already observed |
| TEA purity via global channel | `update()` sends Action directly via global static tx | Return `Vec<Effect>` data from update(); effect_runner interprets | F-201 |
| Adapter injection | `AppState { jira_client: Option<Arc<dyn JiraClient>> }` (current) | `Adapters { jira: Option<Arc<dyn JiraPort>>, ... }` struct owned by effect_runner | F-202 |
| Process-group kill | Custom wrapper around `libc::kill` + PID tracking | Existing `.process_group(0)` + `libc::kill(-pgid, SIGTERM)` pattern — already tested by `tests/process_group_kill.rs` | Proven pattern; don't reinvent |
| Async trait boxing | Roll own Box<dyn Future> trait impl | `#[async_trait::async_trait]` — already in Cargo.toml | Consistent with `ProcessClient`, `JiraClient` |
| Runtime arch checks | arch_test_core runtime fitness functions | Static grep guards in Makefile/CI script (listed in §Risk Surface Map) | arch_test_core is AGPL — excluded by REQUIREMENTS.md; grep covers the essential checks cheaply |
| State serialization | Custom serialize for AppState | None needed — no persistence requirement in Phase 13 | AppState lifetime = session lifetime |

**Key insight:** this phase is almost entirely RELOCATION + TYPE-ENCODING. No new algorithms, no new external dependencies, no new I/O. The audit is the design spec; the planner's job is carving it into atomic refactor units that each pass `cargo test + clippy -D warnings`.

## Common Pitfalls

### Pitfall 1: F-200 split breaks `#[cfg(test)] mod dispatch_tests;`
**What goes wrong:** The 17 COVER-03 tests in `src/app/dispatch_tests.rs` are wired via `#[cfg(test)] mod dispatch_tests;` declaration at `src/app.rs:2427-2428`. After F-200 split, `src/app.rs` ceases to exist. If the planner forgets to re-declare the sub-module in the new `src/app/mod.rs` (via `#[cfg(test)] pub mod dispatch_tests;`) the 17 tests silently disappear from the build.
**Why it happens:** Test module declarations are easy to miss during large reorganizations.
**How to avoid:** `src/app/mod.rs` MUST contain `#[cfg(test)] pub mod dispatch_tests;`. Validate with `cargo test --lib dispatch_tests --quiet` — expect 17 passed, not 0 ignored/missing.
**Warning sign:** `cargo test` output drops from 49 passing to <49 passing post-F-200.

### Pitfall 2: `fake_metro_handle` breaks when `MetroHandle` becomes a trait (F-004)
**What goes wrong:** `tests/common/mod.rs::fake_metro_handle(pid, worktree) -> MetroHandle` today constructs a concrete struct with 4 tokio-typed pub fields. If F-004 makes MetroHandle a trait, this helper breaks at compile time.
**Why it happens:** Test helper evolution must stay in lockstep with domain type changes.
**How to avoid:** Plan 13-03 (which introduces the MetroHandle trait) MUST update `tests/common/mod.rs` in the same atomic commit. Propose `pub fn fake_metro_handle(pid: u32, worktree: &str) -> Box<dyn MetroHandle>` returning a `FakeMetroHandle` struct (defined in tests/common/mod.rs OR in `src/domain/metro.rs::testing`).
**Warning sign:** `cargo test --test metro_single_instance` fails with type mismatch.

### Pitfall 3: 5 boolean coordination flags do NOT all die with F-204 Recipe
**What goes wrong:** AUDIT.md F-204 says all 5 flags (`pending_restart`, `pending_switch_path`, `pending_metro_run`, `pending_metro_after_sync`, `skip_external_metro_check`) collapse into Recipe variant data. In practice, `pending_restart` and `skip_external_metro_check` are METRO LIFECYCLE flags (stop-then-start coordination), not prereq-ordering flags. The Recipe type does not naturally carry these.
**Why it happens:** Over-reading the audit; Recipe::SyncThenStartMetro only handles one direction of metro lifecycle.
**How to avoid:** Plan 13-09 planner must enumerate per-flag: which die, which move into `MetroState` sub-struct (F-209), which encode as Effect variant chains. `metro_single_instance.rs` test asserts `state.pending_restart` — that assertion survives (flag is now `state.metro_state.pending_restart` per F-209).
**Warning sign:** `metro_start_while_running_triggers_restart_not_double_spawn` test fails.

### Pitfall 4: Palette `_ => Some(Action::ModalCancel)` context-fallback lost in registry translation
**What goes wrong:** The 5 PaletteMode match arms in current `handle_key` each end with `_ => Some(Action::ModalCancel)` — meaning "any unbound palette key closes the palette." When translating to `KEYBINDINGS` const, a naive impl only registers the explicitly-listed keys → unbound keys fall through to `_ => None` → palette stays open. UX regression.
**Why it happens:** Context-level fallback ≠ per-key entry.
**How to avoid:** After the KEYBINDINGS iteration loop, add `if state.palette_mode.is_some() && matches(key, allowed_escape_keys_for_context) { return Some(Action::ModalCancel); }` OR encode the fallback as per-palette "wildcard" KeyBinding entries. See §Pattern 3 Pitfall.
**Warning sign:** `dispatch_tests::palette_resolution` tests (6 tests covering palette key fallbacks) fail.

### Pitfall 5: Receiver-pin requirement in tests/metro_single_instance.rs
**What goes wrong:** Current tests bind receivers with `_metro_rx` / `_handle_rx` (not `_`) to keep them alive for the test body — `Action::MetroStart` spawns a tokio task that sends on metro_tx; if the receiver is dropped, sends panic with "channel closed" (per 12-RESEARCH.md Pitfall 10). After F-201, `update()` no longer takes channels — update()'s signature becomes `-> Vec<Effect>`. The tests must stop holding receivers AND stop passing channels to update().
**Why it happens:** Test adaptation lag behind refactor.
**How to avoid:** Plan 13-07 updates both `tests/metro_single_instance.rs` call sites: new signature `update(&mut state, Action::MetroStart)` returns `Vec<Effect>`; tests assert on Effect variants (e.g., `assert!(effects.iter().any(|e| matches!(e, Effect::DetectExternalMetro { .. }) || matches!(e, Effect::SpawnMetro { .. })))`). Tests no longer need channels at all.
**Warning sign:** `tests/metro_single_instance.rs` fails to compile after 13-07.

### Pitfall 6: `crate::action::Action` path breakage across file moves
**What goes wrong:** F-002 moves `src/action.rs` → `src/domain/action.rs`. Every `use crate::action::Action` in the codebase must update to `use crate::domain::action::Action`. The test crate uses `use rn_dash::action::Action;` (note: `action`, not `domain::action`) — `src/lib.rs:8` declares `pub mod action;` today. Post-F-002, lib.rs must either (a) re-export via `pub use crate::domain::action;` at module-path `rn_dash::action` (keeps tests working unchanged) OR (b) drop `pub mod action;` and update tests to `use rn_dash::domain::action::Action;`.
**Why it happens:** Breadth-first import rewriting misses the lib.rs re-export question.
**How to avoid:** Plan 13-01 explicitly decides (a) or (b) — recommend (b) for cleanliness (tests updated in same commit; no backcompat re-export clutter). The 2 integration test files that currently use `rn_dash::action::Action` are: `tests/metro_single_instance.rs:13` — update to `rn_dash::domain::action::Action`.
**Warning sign:** Integration tests fail at `use` statement with "unresolved import `rn_dash::action`".

### Pitfall 7: Coverage threshold rows for app.rs are file-based — the split creates NEW rows
**What goes wrong:** `COVERAGE-THRESHOLDS.md` says `src/app.rs >= 10%`. Post-F-200, `src/app.rs` doesn't exist; new rows appear for `src/app/{state,update,effect,effect_runner,handle_key,keybindings,runtime}.rs`. Each new file starts at some baseline that `floor(baseline, 5)` will set a threshold for. But the OLD threshold row for `src/app.rs` is now moot.
**Why it happens:** Per-file ratchet locks files, not code.
**How to avoid:** Plan 13-06 MUST update COVERAGE-THRESHOLDS.md:
1. Delete the `src/app.rs` row (structural change per changelog policy)
2. Run `make cov-baseline` AFTER the split lands
3. Add new rows for each `src/app/*.rs` file with `floor(baseline, 5)` thresholds
4. Append to Changelog: "2026-04-xx | 13 | F-200 split app.rs into app/ module | File-move refactor; old app.rs row obsolete; new per-file rows added per floor-to-5 policy"
5. Verify totals unchanged or IMPROVED (adding structure shouldn't regress overall coverage)
**Warning sign:** `make cov-check` fails citing missing rows.

### Pitfall 8: Hiding tokio from domain — `MetroPort::start` async signature
**What goes wrong:** F-203's `MetroPort::start(worktree: PathBuf, activity_tx) -> anyhow::Result<MetroHandle>` is async. Domain layer (where the port lives) must not import `tokio::sync::mpsc` for the `activity_tx` type. The audit's sketch uses `UnboundedSender<MetroActivity>` which IS a tokio type — reintroduces the leak F-004 resolved.
**Why it happens:** Audit sketch copied verbatim carries tokio types.
**How to avoid:** Define the port in terms of a trait-abstracted sender or a callback: `fn start(&self, worktree: PathBuf, on_activity: Box<dyn Fn(MetroActivity) + Send + Sync>)`. OR re-export `tokio::sync::mpsc::UnboundedSender` through a domain alias — honest but still leaks. Recommend the callback route OR accept the tokio type as a pragmatic exception (same compromise as current `MetroHandle`). **This is an Open Question for planner to decide.** See §Open Questions → Q3.
**Warning sign:** `src/domain/ports/metro_port.rs` contains `use tokio::sync::mpsc`.

### Pitfall 9: F-501 flat-vs-category decision reopened late in phase
**What goes wrong:** Planner starts implementing REFACTOR-02 as flat-enum `is_cancellable()` per Plan 13-02. Mid-phase, planner reads AUDIT-ADDENDUM again, thinks "category split is cleaner for Phase 15 semaphore" — partial refactor branches off. Churn.
**Why it happens:** Addendum's routing table (Phase 13 | Phase 14) framing invites reconsideration.
**How to avoid:** The decision is LOCKED in this research (§Locked Decisions): flat-enum per base AUDIT.md unless `/gsd:discuss-phase 13` explicitly overrides. Planner MUST treat this as a closed decision and not reopen without user intervention.
**Warning sign:** Plan includes `enum Command { Git(GitCmd), ... }` — stop and re-read AUDIT-ADDENDUM routing table.

### Pitfall 10: F-209 AppState sub-structs codify flags that F-204 deleted
**What goes wrong:** F-209 consumer plan (13-10) groups AppState's 39 fields into sub-structs. If done BEFORE 13-09 (F-204 consumer), the `PendingFlags` sub-struct encodes the 5 boolean flags that 13-09 deletes. Then 13-09 has to undo the sub-struct grouping.
**Why it happens:** Sequencing error.
**How to avoid:** Audit's Refactor Sequence item 20 notes: "F-209 — Why this order: without F-204 the 'PendingWork' grouping would codify the flags we're trying to delete." Plan sequence enforces F-204 consumer (13-09) BEFORE F-209 (13-10).
**Warning sign:** 13-10 removes fields that 13-09 already removed — double-delete confusion.

## Code Examples

### Example A — COVER-03 dispatch_tests signature after F-201 consumer

Current (before F-201 consumer):
```rust
// src/app/dispatch_tests.rs
#[tokio::test]
async fn modal_cancel_dismisses_confirm_modal() {
    let (metro_tx, _metro_rx) = tokio::sync::mpsc::unbounded_channel();
    let (handle_tx, _handle_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut state = base_state();
    state.modal = Some(ModalState::Confirm { prompt: "x".into(), pending_command: CommandSpec::GitPull });

    update(&mut state, Action::ModalCancel, &metro_tx, &handle_tx);

    assert!(state.modal.is_none());
}
```

After F-201 consumer:
```rust
// src/app/dispatch_tests.rs
#[test]  // no longer requires tokio runtime (no tokio::spawn in update)
fn modal_cancel_dismisses_confirm_modal() {
    let mut state = base_state();
    state.modal = Some(ModalState::Confirm { prompt: "x".into(), pending_command: CommandSpec::GitPull });

    let effects = update(&mut state, Action::ModalCancel);

    assert!(state.modal.is_none());
    // Most dismissals emit no effects — assert on empty Vec or specific expected variants
    assert!(effects.is_empty(), "ModalCancel of Confirm modal should emit no effects, got: {effects:?}");
}
```

### Example B — Adapters construction in runtime.rs

```rust
// src/app/runtime.rs after F-200 + F-202 land
// Source: AUDIT.md F-202 recommendation adapted

pub async fn run(terminal: ratatui::DefaultTerminal) -> color_eyre::Result<()> {
    let config = crate::infra::config::load_config().ok();
    let jira_title_cache = crate::infra::jira_cache::load_jira_cache().unwrap_or_default();

    let adapters = Adapters {
        command_runner: Arc::new(crate::infra::command_runner::TokioCommandRunner),
        metro: Arc::new(crate::infra::metro::TokioMetroAdapter::new()),
        port_probe: Arc::new(crate::infra::port::LsofPortProbe),
        worktrees: Arc::new(crate::infra::worktrees::GitWorktreeAdapter),
        devices: Arc::new(crate::infra::devices::AdbXcrunDevices),
        jira: config.as_ref().and_then(|c| build_jira(c).map(Arc::from)),
        multiplexer: crate::infra::multiplexer::detect_multiplexer().map(|b| Arc::from(b)),
    };

    let mut state = AppState::default();
    state.config = config;
    state.jira_state.title_cache = jira_title_cache;

    let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut runner = EffectRunner::new(adapters, action_tx.clone());

    let mut event_stream = EventStream::new();
    loop {
        if state.should_quit { break; }

        terminal.draw(|f| crate::ui::view(f, &mut state))?;

        tokio::select! {
            Some(ev_res) = event_stream.next() => {
                let ev = ev_res?;
                if let ratatui::crossterm::event::Event::Key(k) = ev {
                    if let Some(action) = handle_key(&state, k) {
                        let effects = update(&mut state, action);
                        runner.run_effects(effects).await;
                    }
                }
            }
            Some(action) = action_rx.recv() => {
                let effects = update(&mut state, action);
                runner.run_effects(effects).await;
            }
        }
    }
    Ok(())
}
```

### Example C — `CommandRunnerPort` replacing F-101 leak

```rust
// src/domain/ports/command_runner_port.rs (new per F-101)
// Source: AUDIT.md F-101 recommendation verbatim

use crate::domain::command::CommandSpec;
use std::path::PathBuf;
use std::process::ExitStatus;

pub enum CommandEvent {
    OutputLine(String),
    Exited(ExitStatus),
}

#[async_trait::async_trait]
pub trait CommandRunnerPort: Send + Sync {
    /// Spawn the command. Returns a receiver that emits output lines then one Exited event.
    /// Receiver closes after Exited is sent.
    fn spawn(
        &self,
        spec: CommandSpec,
        cwd: PathBuf,
        branch: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<CommandEvent>;
}

// src/infra/command_runner.rs (post-F-101)
pub struct TokioCommandRunner;

impl CommandRunnerPort for TokioCommandRunner {
    fn spawn(
        &self,
        spec: CommandSpec,
        cwd: PathBuf,
        branch: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<CommandEvent> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let argv = build_argv(&spec, &branch);
            // ... same body as today but sends CommandEvent::OutputLine / Exited
            // instead of Action::CommandOutputLine / CommandExited
        });
        rx
    }
}

// src/app/effect_runner.rs translation:
//   Effect::SpawnCommand { spec, cwd, branch } ->
//       let mut rx = self.adapters.command_runner.spawn(spec, cwd, branch);
//       let tx = self.action_tx.clone();
//       tokio::spawn(async move {
//           while let Some(ev) = rx.recv().await {
//               let action = match ev {
//                   CommandEvent::OutputLine(l) => Action::CommandOutputLine(l),
//                   CommandEvent::Exited(_) => Action::CommandExited,
//               };
//               let _ = tx.send(action);
//           }
//       });
```

[CITED: AUDIT.md F-101]

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `update()` calls `tokio::spawn` inline (20 sites) | `update() -> Vec<Effect>` + effect runner | Post-Plan 13-07 | Tests no longer need tokio runtime; effects are loggable data; enables Phase 15 cancellation (can intercept SpawnCommand effects) |
| `use crate::action::Action` in infra/command_runner.rs | `CommandRunnerPort` returns typed `CommandEvent`; app translates at boundary | Post-Plan 13-08 | Infra becomes reusable + Fowler dependency direction restored |
| `AppState { 39 pub fields }` | Sub-structs with `pub(crate)` inner fields | Post-Plan 13-10 | Ousterhout overexposure reduced; refactor risk localized to sub-struct |
| 3 hand-maintained keybinding sites (handle_key + footer + help_overlay) | `KEYBINDINGS: &[KeyBinding]` + 3 consumers | Post-Plan 13-07 + 13-10 | D-14 drift impossible; single source of truth |
| 11 inline prereq sites + 5 boolean flags + command_queue | `Recipe::expand()` + prereq enum | Post-Plan 13-09 | ARCH-05 satisfied; dispatch readable; Phase 14+15 substrate |
| `MetroHandle { pub pid, stdin_tx, stream_task, stdin_task, kill_tx }` (tokio types in domain) | `trait MetroHandle { pid(), send_stdin(), kill(), worktree_id() }` (opaque) | Post-Plan 13-03 + 13-07 | Domain has zero tokio leak; infra adapter holds tokio channels |
| `src/action.rs` at root | `src/domain/action.rs` | Post-Plan 13-01 | Fowler layer boundary correct |
| `src/app.rs` 2,425 LOC god-object | `src/app/` 7 files × ~200-400 LOC | Post-Plan 13-06 | Ousterhout deep-module achievable; Phase 14-16 changes localized |
| `infra/tmux.rs` DEPRECATED dead code | (deleted) | Post-Plan 13-10 | Ousterhout dead-code smell removed |
| Literal `_ => {}` silently drops ModalInputChar in 6 modal types | Exhaustive `ModalState::*` named arms | Post-Plan 13-09 | Rust compiler guards future ModalState additions |

**Deprecated/outdated (Phase 13 REMOVES):**
- `src/action.rs` root file: moves to `src/domain/action.rs`
- `src/app.rs` monolith: splits to `src/app/*.rs`
- `src/infra/tmux.rs`: deleted (replaced by `multiplexer::TmuxAdapter` since v1.0)
- `use crate::action::Action` in `src/infra/*.rs`: forbidden by grep guard
- `crate::infra::*` imports in `src/app/*.rs`: forbidden by grep guard
- `crate::infra::*` imports in `src/ui/*.rs`: forbidden by grep guard (was already only 1 hit — dies with F-107/F-300)
- `_ => {}` silent drops in `ModalInputChar`/`ModalInputBackspace`: forbidden (exhaustive enumeration)
- tokio types in `domain/metro.rs` `pub` fields: forbidden (trait object replacement)
- 5 boolean coordination flags on `AppState` (some): collapse into `Recipe` / `MetroState`

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | 10 plans in 5 waves is the right carving (one plan per atomic refactor) | §Recommended Target Plans | If wrong, planner re-carves. Each plan's end-state is compilable independently, so re-carving is low-cost. [ASSUMED — based on audit Refactor Sequence 23 steps collapsed into 10 compilable units] |
| A2 | F-200 split + F-201 consumer + F-202 consumer should be THREE separate plans, not one | §Risk Surface + §Recommended Target Plans 13-06/13-07/13-08 | If merged, one plan is too large (>5000 LOC churn) and any abort loses everything. If split further (e.g., 5 plans for app-rewiring), per-plan setup overhead dominates. 3-plan carving balances atomicity with reasonable unit size. [ASSUMED] |
| A3 | F-400 KEYBINDINGS placement: app-level (`src/app/keybindings.rs`) per audit's recommendation | §Pattern 3 + §Recommended Target Plans 13-07 | If placement decision reopened, 13-05 ↔ 13-07 ordering changes. Risk: low — audit already settled. [CITED: AUDIT.md F-400] |
| A4 | `Effect` enum lives in `src/app/effect.rs` (app tier) not `src/domain/effect.rs` (domain tier) | §Architectural Responsibility Map | Audit F-201 says "place at `src/app/effect.rs` or `src/domain/effect.rs`". Recommendation: app-level because Effect variants reference `PathBuf` worktree paths which are app-specific runtime values and the effect runner is app-tier. If domain-tier chosen, same implementation, no behavioral difference. [ASSUMED — defers to planner if preference emerges] |
| A5 | `tokio::sync::mpsc::UnboundedSender<MetroActivity>` parameter in `MetroPort::start` is acceptable — a pragmatic tokio-domain exception | §Common Pitfalls → Pitfall 8 | If strict tokio-exclusion wins, `MetroPort::start` signature becomes `Box<dyn Fn(MetroActivity) + Send>` callback — more ceremony but fully hexagonal. Open question for planner (§Open Questions Q3). [ASSUMED — defers to discuss-phase 13] |
| A6 | `Arc<dyn Port>` over `Box<dyn Port>` for ports that may be cloned into spawned tasks | §Standard Stack → Alternatives | If Box chosen, effect_runner needs an additional `Clone` wrapper or per-spawn re-borrow. Both work; Arc is lighter-touch. [CITED: AUDIT.md F-202 uses Arc] |
| A7 | `domain::ports::*` module hierarchy vs flat `domain::*Port` | §Project Structure | Audit consistently uses `domain::ports::*`. Flat works but breaks audit convention. [CITED: AUDIT.md multiple findings] |
| A8 | Remove `jira_client: Option<Arc<dyn JiraClient>>` field from `AppState`; move to `Adapters` struct | §Risk Surface F-202 | If kept on AppState post-F-209, sub-struct groupings get messy. Audit F-202 uses Adapters pattern — consistent with F-202. [CITED: AUDIT.md F-202] |
| A9 | `CommandSpec::is_cancellable()` returns FALSE for exactly 8 git variants (GitResetHard, GitPull, GitPush, GitRebase, GitCheckout, GitCheckoutNew, GitFetch, GitResetHardFetch) and TRUE for the other 15 variants (yarn*, rn*, rm*, adb*, shell) | §Phase Requirements REFACTOR-02 | If the set differs, tests fail. REQUIREMENTS.md line 32 says "git-porcelain variants return false, all other variants return true" — exactly 8 git variants per CommandSpec definition. [VERIFIED: src/domain/command.rs:10-43 count] |
| A10 | `command_queue` gets flatten-at-enqueue semantics post-F-204 (Recipe::expand → append results to queue) | §Pattern 2 Pitfall + §Risk Surface | Alternative: runner invokes Recipe::expand lazily at dequeue. Both work; flatten-at-enqueue preserves existing `command_queue: VecDeque<CommandSpec>` type — less invasive. [ASSUMED — planner calls] |

## Open Questions

1. **F-400 keybinding registry placement: `src/app/keybindings.rs` (app-level) or `src/keybindings.rs` (root)?**
   - What we know: Audit recommends app-level because registry references `AppState` for conditional actions (e.g., R → reload-if-metro-running). Domain placement ruled out (KeyCode is UI concern).
   - What's unclear: Tests currently import `rn_dash::app::AppState` — if keybindings live at `rn_dash::keybindings`, the registry accesses `rn_dash::app::AppState` which creates a root→app dependency. App-level placement sidesteps this.
   - Recommendation: **app-level** per audit. `/gsd:discuss-phase 13` may reopen.
   - Default: `src/app/keybindings.rs`.

2. **`Effect` enum placement: `src/app/effect.rs` or `src/domain/effect.rs`?**
   - What we know: Audit sketch places at `src/app/effect.rs`. Effects include Spawn+CommandSpec+cwd+branch, which is all domain-typed but with runtime values.
   - What's unclear: domain-tier would make Effect reusable outside the app (e.g., by a CLI variant) — currently no need.
   - Recommendation: `src/app/effect.rs` because effects are app-layer sequencing primitives; domain should not know about tokio::spawn timing.
   - Default: `src/app/effect.rs`.

3. **Tokio-type exception in `MetroPort::start(activity_tx: UnboundedSender<MetroActivity>)`?**
   - What we know: Audit F-203 sketch uses tokio channel in the trait signature. This re-leaks what F-004 closes.
   - What's unclear: Strict fix = `Box<dyn Fn(MetroActivity) + Send + Sync>` callback. Pragmatic fix = accept the tokio type; domain gets a second `#[allow(dead_code)] pub use tokio::sync::mpsc::UnboundedSender as ActivityTx;` or similar re-export.
   - Recommendation: **callback-based trait signature** — truly hexagonal-clean. Acceptable complexity (one extra Box::new at construction).
   - **Needs discuss-phase decision** if the planner disagrees.

4. **Persistence port (F-111) — land in Phase 13 or defer?**
   - What we know: Audit marks as Minor with "defer to backlog"; Phase 14+16 may add task-history persistence and is the right trigger.
   - Recommendation: **defer to Phase 16.** Touch 4 small modules in Phase 13 only if a drive-by edit is convenient (e.g., Plan 13-10).
   - Default: defer.

5. **`command_queue` replacement semantics — eager flatten or lazy expand?**
   - What we know: Recipe::expand returns `Vec<CommandSpec>`. The question is whether the dispatcher flattens at enqueue (append Vec to command_queue) or the dispatcher holds a `VecDeque<Recipe>` and expands on pop.
   - Recommendation: **eager flatten** — preserves `command_queue: VecDeque<CommandSpec>` type; smaller change surface.
   - Default: eager flatten.

6. **Minor F-009 (Worktree identity vs enrichment split) — touch in Phase 13?**
   - What we know: Audit says "Do not action. Re-evaluate when Phase 16 begins." Touching it risks destabilizing the WorktreeBrowserState sub-struct in F-209.
   - Recommendation: **defer per audit.**

7. **Minors drive-by window — which ride along in Phase 13?**
   - F-003 (event.rs comment), F-005 (command.rs doc), F-006 (needs_text_input catch-all comment), F-008 (refresh catch-all comment), F-100 (infra/mod.rs doc-claim), F-108 (is_inside_tmux relocation), F-112 (tmux.rs delete), F-206 (fold into F-201), F-207 (fold into F-203), F-210 (no action): all rideable.
   - F-007 (is_cancellable): IS REFACTOR-02 — lands as Plan 13-02.
   - F-009 (Worktree split): defer.
   - F-111 (persistence port): defer.
   - Recommendation: ride F-003/005/006/008/100/108/112 on the nearest plan touching the same file.

## Minor Tagalongs Table

| Minor | Rides with | Rationale |
|-------|------------|-----------|
| F-003 event.rs catch-all comment | 13-10 or standalone | Trivial; fold into any plan touching root area |
| F-005 command.rs variant-count doc | 13-02 (REFACTOR-02 adds to command.rs) | Drive-by in same file |
| F-006 needs_text_input `_ => false` comment | 13-02 | Drive-by in same file |
| F-008 refresh catch-all comment | Skip — refresh.rs must not be touched (100% coverage preserved) | Defer to later phase if at all |
| F-100 infra/mod.rs doc-claim | 13-10 (last plan in phase, all findings resolved) | Doc-claim becomes TRUE only after F-101/F-102/F-104/F-105/F-106/F-110 all land |
| F-108 is_inside_tmux move | 13-10 (last plan; touches infra/jira.rs + infra/multiplexer.rs) | Drive-by during jira.rs cleanup (post-F-107 move of extract_jira_key) |
| F-112 tmux.rs delete | 13-10 | Dead code purge alongside Minor cleanup |
| F-206 recursive update self-dispatch | 13-07 (F-201 consumer) — absorbed naturally | Fixed by construction when update() returns Vec<Effect> |
| F-207 metro_http_post in-app helper | 13-07 (F-203 consumer) — absorbed naturally | Moves with the 7 other metro helpers |
| F-210 config loading inline | No action (audit says "resolved structurally by F-200 + F-202") | N/A |
| F-007 is_cancellable | **13-02 (REFACTOR-02)** | This IS REFACTOR-02 |
| F-009 Worktree split | Defer to Phase 14/16 per audit | Not actioned |
| F-111 persistence port | Defer to Phase 16 per audit | Not actioned |

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cargo | All compilation + test | ✓ | 1.94.1 | — |
| rustc | All compilation | ✓ | 1.94.1 | — |
| cargo-llvm-cov | Coverage ratchet verification | ✓ (per COVER-04 prerequisites) | 0.8.5 | `cargo tarpaulin` (slower, less consistent) |
| rustup llvm-tools-preview component | cargo-llvm-cov dependency | ✓ (per COVER-04 prerequisites) | — | N/A |
| rg (ripgrep) | Grep guards in validation | ✓ | — | `grep -rEn` (slower) |
| git | Source management | ✓ | 2.51.0+ | — |
| make | Makefile cov targets | ✓ | — | Direct `cargo llvm-cov` invocation |
| tmux / zellij | Manual UI smoke test (optional) | tmux ✓ / zellij ✗ | system | N/A — tests pass without either |
| lsof | Phase 12 process_group_kill test | ✓ (macOS built-in) | — | N/A — test is cfg'd to Linux+macOS |
| bash | Validation + Makefile recipes | ✓ | system | — |

**Missing dependencies with no fallback:** None.
**Missing dependencies with fallback:** None required.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust test harness (`cargo test`) + `cargo clippy -D warnings` + `cargo llvm-cov` + grep guards |
| Config file | `Cargo.toml` + `Makefile` (cov targets) |
| Quick run command | `cargo test --lib && cargo test --test metro_single_instance && cargo test --test process_group_kill` (typical < 5s) |
| Full suite command | `cargo test --all-targets && cargo clippy --all-targets -- -D warnings && make cov-check` (< 30s) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| REFACTOR-01 (F-200) | app.rs split preserves all existing TEA behavior | regression | `cargo test --lib` (17 dispatch_tests) + `cargo test --test metro_single_instance` (2 tests) | ✅ |
| REFACTOR-01 (F-201) | update() is pure — no tokio::spawn in update.rs | grep guard | `! rg 'tokio::spawn' src/app/update.rs` | ❌ (add as Plan 13-07 validation step) |
| REFACTOR-01 (F-202) | app/ holds no infra imports | grep guard | `! rg 'crate::infra::' src/app/` | ❌ (add as Plan 13-08 validation step) |
| REFACTOR-01 (F-203) | metro helpers relocated — app/ has no tokio::process imports for metro lifecycle | grep + existing 5 metro tests | `! rg 'tokio::process' src/app/effect_runner.rs` (expect metro via adapter only) + `cargo test --lib domain::metro` (3 tests) + `cargo test --test metro_single_instance` (2 tests) | ✅ (5 tests) |
| REFACTOR-01 (F-101) | command_runner no longer imports Action | grep guard | `! rg 'use crate::(domain::)?action' src/infra/command_runner.rs` | ❌ (add as Plan 13-08 validation step) |
| REFACTOR-01 (F-107 + F-300) | UI no longer imports from infra | grep guard | `! rg 'crate::infra::' src/ui/` | ❌ (add as Plan 13-01 validation step) |
| REFACTOR-01 (F-102/104/105/106/110 trait placement) | all ports in domain::ports | file-exist checks | `test -f src/domain/ports/process_port.rs` (and siblings) | ❌ (add as Plan 13-01/13-04 validation step) |
| REFACTOR-01 (F-204) | Recipe type exists + 11 inline prereq sites eliminated | grep + existing dispatch_tests | `grep -q 'pub enum Recipe' src/domain/pipeline.rs` + `! rg 'pending_metro_run\|pending_metro_after_sync' src/app/` (flags deleted post-13-09) | ❌ |
| REFACTOR-01 (F-208+F-302+F-303 via F-400) | single keybinding registry consumed by 3 sites | grep + existing dispatch_tests | `grep -q 'pub const KEYBINDINGS' src/app/keybindings.rs` + `cargo test --lib dispatch_tests::palette_resolution` | ❌ (add as Plan 13-07 validation step) |
| REFACTOR-01 (F-205) | exhaustive modal arms at 1140, 1153 | rust exhaustiveness | `cargo build --lib` (fails if non-exhaustive ModalState match introduced) | ✅ (implicit) |
| REFACTOR-01 (F-209) | AppState has ≤ 10 top-level pub fields (sub-structs) | awk count | `awk '/^pub struct AppState/,/^}/' src/app/state.rs \| grep -c '^\s\+pub '` — expect ≤ 10 | ❌ |
| REFACTOR-02 | is_cancellable() predicate exists + returns correct values | new unit tests | `cargo test --lib domain::command::tests::is_cancellable` | ❌ (add as Plan 13-02 Wave 0) |
| REFACTOR-03 | Recipe in domain; dispatcher reads from domain | grep + existing metro test | `grep -q 'pub enum Recipe' src/domain/pipeline.rs` + `! rg 'needs_metro()' src/app/update.rs` (inline check gone) | ❌ |
| Coverage ratchet | No per-file threshold regression | `make cov-check` | Human-checked per COVER-04 D-05 | ✅ (Makefile has cov-check target) |
| Metro single-instance invariant | Preserved across all refactor | existing | `cargo test --test metro_single_instance` (2 tests) | ✅ |
| Process-group kill | Preserved across all refactor | existing | `cargo test --test process_group_kill` (cfg'd; 1 test) | ✅ |
| TEA dispatch surface | Preserved across all refactor | existing | `cargo test --lib dispatch_tests` (17 tests) | ✅ |
| Register-twice panic invariant | Preserved | existing | `cargo test --lib domain::metro::tests::register_twice_panics` | ✅ |

### Sampling Rate

- **Per task commit:** `cargo test --lib && cargo clippy -- -D warnings` (< 10s typical). A per-task commit that breaks clippy or tests is a rejection.
- **Per plan end:** `cargo test --all-targets && cargo clippy --all-targets -- -D warnings && make cov-baseline` + human eyeball threshold diff in COVERAGE-THRESHOLDS.md against new baseline.
- **Per wave end:** all of per-plan PLUS grep guards from the table above appropriate to the wave:
  - Wave A end: `! rg 'crate::infra::' src/ui/` (F-300 verify); `! rg 'use crate::(action|domain::action)' src/infra/` if 13-01 landed (action.rs moved)
  - Wave C end: `! rg 'crate::infra::' src/app/`, `! rg 'tokio::spawn' src/app/update.rs`, `test -f src/app/state.rs src/app/update.rs src/app/effect.rs src/app/effect_runner.rs src/app/handle_key.rs src/app/keybindings.rs src/app/runtime.rs`, `! test -f src/app.rs`
  - Wave D end: `grep -q 'pub const KEYBINDINGS' src/app/keybindings.rs`; `! rg 'pending_metro_run\|pending_metro_after_sync' src/app/`; `! test -f src/infra/tmux.rs`
- **Phase gate:** All tests green; clippy clean; all grep guards pass; `make cov-check` green; all 12 Critical+Major findings resolved or explicitly deferred in PHASE-SUMMARY notes.

### Wave 0 Gaps

- [ ] **Validation script** `.planning/phases/13-audit-driven-refactors/13-validate.sh` — codifies all grep guards above as a single command. (Can mirror 11-validate.sh's shape.)
- [ ] **Makefile `cov-check` target** — per D-05 policy is human-checked currently. For Phase 13 a script that diffs `make cov-baseline` output row-by-row against `COVERAGE-THRESHOLDS.md` would automate this. Post-milestone concern per D-05 — may OR may not land in Phase 13. Recommend minimal row-diff jq one-liner in Makefile.
- [ ] **Inline unit tests for REFACTOR-02** — `is_cancellable()` predicate tests in `src/domain/command.rs` — 7 tests: `is_cancellable_git_variants_all_false`, `is_cancellable_yarn_variants_all_true`, `is_cancellable_rn_run_variants_true`, `is_cancellable_clean_variants_true`, `is_cancellable_rm_node_modules_true`, `is_cancellable_shell_true`, `is_cancellable_adb_install_true`. Lands in Plan 13-02.
- [ ] **Inline unit tests for Recipe::expand** — `src/domain/pipeline.rs` — tests per variant: Single, Sequence, Clean (with each CleanOptions combination), SyncThenRun (with + without stale), SyncThenStartMetro, ReleaseBuildAndInstall, GitFetchThenReset. ~8-10 tests. Lands in Plan 13-03.
- [ ] **Update `dispatch_tests` to Vec<Effect> assertions** — all 17 tests in `src/app/dispatch_tests.rs` update their update() call pattern post-F-201 consumer. Lands in Plan 13-07.
- [ ] **Update `tests/common/mod.rs::fake_metro_handle` to Box<dyn MetroHandle>** — lands in Plan 13-03.
- [ ] **Update `tests/metro_single_instance.rs` update() call signature** — lands in Plan 13-07.

*(Existing test infrastructure is sufficient — no new framework needed. All test framework pieces already committed per COVER-00 scaffolding.)*

## Sources

### Primary (HIGH confidence)

- `.planning/phases/11-architecture-audit/AUDIT.md` — source of truth for every F-NNN finding, target shape, and Refactor Sequence [VERIFIED: read all 918 lines]
- `.planning/phases/11-architecture-audit/AUDIT-ADDENDUM.md` — F-500 + F-501 forward-looking findings; Phase 13 scoping decisions [VERIFIED: read all 116 lines]
- `.planning/phases/11-architecture-audit/11-RESEARCH.md` — original Effect/Recipe/KeyBinding sketches; refactor sequencing philosophy [VERIFIED: §"Recommended target shape for D-04", §"Plan Unit Sizing Recommendation", §Pitfall 10]
- `.planning/phases/12-coverage-gate/12-VERIFICATION.md` — which tests exist, what they guard [VERIFIED: read all 140 lines]
- `.planning/phases/12-coverage-gate/BASELINE-COVERAGE.md` + `COVERAGE-THRESHOLDS.md` — per-file ratchet + `floor(baseline, 5)` policy [VERIFIED: read both files]
- `.planning/ROADMAP.md` Phase 13 success criteria [VERIFIED: lines 82-91]
- `.planning/REQUIREMENTS.md` REFACTOR-01..03 [VERIFIED: lines 30-35]
- `.planning/STATE.md` — COVER gate status, phase ordering rules [VERIFIED: YAML + accumulated context]
- `.planning/PROJECT.md` Key Decisions + Constraints [VERIFIED: read all]
- `./CLAUDE.md` — YOLO mode, `--incremental`, branch labels (irrelevant to Phase 13 scope) [VERIFIED]
- `./Cargo.toml` — no new deps needed; version verification [VERIFIED: read all 63 lines]
- `src/app.rs` (2,425 LOC — Module: app/ target of F-200) [VERIFIED: read top 500 lines, grep every Action arm + every tokio::spawn site]
- `src/action.rs` (55 variants) [VERIFIED: read all 151 lines]
- `src/domain/command.rs` (23 CommandSpec variants) [VERIFIED: read all 250 lines]
- `src/domain/metro.rs` (MetroManager + MetroHandle + invariant tests) [VERIFIED: read all 227 lines]
- `src/infra/command_runner.rs` (F-101 Critical target) [VERIFIED: read all 129 lines]
- `src/infra/{mod,port,process,multiplexer}.rs` [VERIFIED: read all]
- `src/ui/{mod,panels,footer,help_overlay}.rs` (F-300/F-302/F-303 targets) [VERIFIED: read all]
- `tests/common/mod.rs` + `tests/metro_single_instance.rs` + `src/app/dispatch_tests.rs` (first 60 lines) [VERIFIED]

### Secondary (MEDIUM confidence)

- [Ratatui TEA concept page](https://ratatui.rs/concepts/application-patterns/the-elm-architecture/) — confirms TEA pattern emphasis on `Message` enum + pure update; does NOT cover Effect-return for side effects (gap in that doc) [VERIFIED: WebFetch]
- [iced-rs crate — TEA in Rust](https://github.com/iced-rs/iced) — prior art for TEA in Rust with Elm-style Message enum [CITED: WebSearch]
- [iced architecture docs](https://book.iced.rs/architecture.html) — confirms `Message` + `update` + `view` + `Command::perform` pattern for async side effects; reinforces Effect-return approach as idiomatic [CITED: WebSearch]
- [Matt Duck book notes — A Philosophy of Software Design](https://www.mattduck.com/2021-04-a-philosophy-of-software-design.html) — Ousterhout red-flag verification (already cited in 11-RESEARCH Secondary) [CITED: 11-RESEARCH.md:791]

### Tertiary (LOW confidence)

- Rust 2024 edition AFIT (async-fn-in-trait) native support — keep `#[async_trait]` for codebase consistency; native AFIT is viable alternative [ASSUMED — training knowledge]
- `Arc<dyn Port>` vs `Box<dyn Port>` vs generics tradeoffs — consensus pattern for hexagonal Rust: Arc for trait objects crossing tasks/threads, Box for single-owner singletons [ASSUMED — based on common Rust patterns not specifically verified for this codebase]

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every dep verified against Cargo.toml; no new deps needed
- Architecture patterns: HIGH — AUDIT.md already specifies every target shape per D-04; this research translates + sequences
- Pitfalls: HIGH — 10 concrete pitfalls verified against actual code + test infrastructure
- Plan carving: MEDIUM-HIGH — 10-plan carving is this research's recommendation; planner may re-carve within the same dependency DAG
- Open questions resolution: MEDIUM — 7 questions explicitly flagged with defaults; `/gsd:discuss-phase 13` may override

**Research date:** 2026-04-24
**Valid until:** 2026-05-24 (30 days — phase is actively in flight; estimate stable assuming no new crate-ecosystem changes to ratatui/tokio/async-trait APIs)

## Sources (final summary)

Sources:
- [AUDIT.md — Phase 11 architecture audit (local file)](file:///Users/cubicme/aljazeera/dashboard/.planning/phases/11-architecture-audit/AUDIT.md)
- [AUDIT-ADDENDUM.md — F-500/F-501 forward-looking findings (local file)](file:///Users/cubicme/aljazeera/dashboard/.planning/phases/11-architecture-audit/AUDIT-ADDENDUM.md)
- [11-RESEARCH.md — Phase 11 research with target shapes (local file)](file:///Users/cubicme/aljazeera/dashboard/.planning/phases/11-architecture-audit/11-RESEARCH.md)
- [12-VERIFICATION.md — Phase 12 coverage gate status (local file)](file:///Users/cubicme/aljazeera/dashboard/.planning/phases/12-coverage-gate/12-VERIFICATION.md)
- [COVERAGE-THRESHOLDS.md — per-file ratchet policy (local file)](file:///Users/cubicme/aljazeera/dashboard/.planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md)
- [The Elm Architecture (TEA) | Ratatui](https://ratatui.rs/concepts/application-patterns/the-elm-architecture/)
- [iced — Rust GUI lib inspired by Elm (GitHub)](https://github.com/iced-rs/iced)
- [iced Architecture docs — Command / Message / update pattern](https://book.iced.rs/architecture.html)
- [A Philosophy of Software Design — book notes](https://www.mattduck.com/2021-04-a-philosophy-of-software-design.html)

## RESEARCH COMPLETE

**Phase:** 13 - Audit-Driven Refactors
**Confidence:** HIGH

### Key Findings

- **AUDIT.md is the design spec.** It supplies D-04 target shapes for every Critical + Major finding. Phase 13 is almost entirely RELOCATION + TYPE-ENCODING — no new algorithms, no new deps, no new I/O. Planner's job is atomicity carving.
- **10 plans in 5 waves** — Wave A (foundational types + trait relocations), Wave B (new port traits), Wave C (app split + TEA purity + hexagonal injection — the hard wave), Wave D (Recipe consumer + state grouping + UI rewires). Each plan's end-state compiles + clippy-clean + tests green. See §Recommended Target Plans.
- **COVER gate is Phase 13's safety net.** 26 tests guard metro single-instance (5), PGID kill (1), TEA dispatch surface (17), register-twice panic (3 inline). Per-file `floor(baseline, 5)` ratchets lock 7 files (refresh=100%, metro=70%, jira=70%, etc.). File moves (app.rs split, jira_key move) will restructure threshold rows — document as structural change in COVERAGE-THRESHOLDS.md changelog.
- **F-500 and F-501 are DEFERRED** per AUDIT-ADDENDUM routing table. Phase 13 uses flat-enum `is_cancellable()` (base audit) not category-split Command. AppState grouping (F-209) preserves `Vec<Worktree>` / `HashMap` topology — Phase 14 adds per-worktree ownership.
- **Highest-risk plans: 13-06 (F-200 app-split, 2500-LOC churn) and 13-07 (F-201 consumer + metro relocation + keybinding registry — every test rewrites its assertion against Vec<Effect>).** Mitigations per §Risk Surface Map + §Common Pitfalls (10 pitfalls with warning signs).

### File Created

`.planning/phases/13-audit-driven-refactors/13-RESEARCH.md`

### Confidence Assessment

| Area | Level | Reason |
|------|-------|--------|
| Standard stack | HIGH | Every dep verified in Cargo.toml; no new deps needed; all ports/traits relocate existing code |
| Architecture patterns | HIGH | AUDIT.md specifies every target shape at D-04 level; research translates + sequences |
| Don't-hand-roll | HIGH | AUDIT.md + REQUIREMENTS.md explicitly list the predicates/types/registries to build |
| Pitfalls | HIGH | 10 pitfalls verified against actual code, test infrastructure, and COVER-04 threshold file |
| Plan carving | MEDIUM-HIGH | 10-plan carving is recommendation; re-carving within dependency DAG is low-cost |
| Validation | HIGH | Grep guards mechanically verifiable; `cargo test + clippy -D warnings + make cov-check` are existing tooling |

### Open Questions (flagged for planner/discuss-phase)

1. F-400 keybinding registry: `src/app/keybindings.rs` (auditor's default) vs root — **discuss-phase may override**
2. F-501 revisit: flat-enum vs category split — **LOCKED to flat-enum per AUDIT-ADDENDUM unless user reopens**
3. MetroPort activity_tx: tokio type in signature vs callback — **pragmatic exception recommended; discuss-phase may tighten**
4. F-111 persistence port: defer to Phase 16 per audit
5. F-009 Worktree split: defer to Phase 16 per audit
6. `command_queue` post-F-204: flatten-at-enqueue recommended
7. Minor tagalongs: see §Minor Tagalongs Table — only F-003/005/006/100/108/112 ride-along; F-007=REFACTOR-02; F-009/F-111 defer

### Ready for Planning

Research complete. Planner can create 10 PLAN.md files across 5 waves per §Recommended Target Plans. First plan (13-01) unblocks Wave A; remaining plans follow the dependency DAG in the Critical & Major Findings Map.
