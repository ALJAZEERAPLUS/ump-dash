# Architecture Audit — Phase 11

> Phase 11 deliverable. Read-only audit of the codebase against four lenses:
> Ousterhout deep-module/narrow-interface, Fowler 4-layer model,
> hexagonal ports-and-adapters discipline, and two completeness sweeps
> (catch-all match arms, misplaced prerequisite/ordering logic).
>
> Severity calibration is **Aggressive** per CONTEXT.md D-01:
> - **Critical:** cross-layer leak, god-object behavior, TEA impurity in `update()`,
>   shallow modules on the v1.3 critical path.
> - **Major:** clear lens violation with refactor cost <1 day; misplaced
>   prerequisite logic; catch-all `_ => {}` arms with reachable variants.
> - **Minor:** cosmetic, naming, small extractions, documentation gaps.
>
> Phase 13 will resolve every Critical and Major finding (REFACTOR-01).
>
> Findings use sequential IDs F-NNN. ID ranges per plan:
> 11-01 domain: F-001..F-099 · 11-02 infra: F-100..F-199 ·
> 11-03 app: F-200..F-299 · 11-04 ui: F-300..F-399 · 11-05 cross: F-400..F-499.

## Module: root/
<!-- Coverage: src/main.rs, src/tui.rs, src/event.rs, src/action.rs (per Pitfall 9 — root files in scope) -->
<!-- Plan 11-01 scope: main.rs, tui.rs, event.rs, action.rs. -->
<!-- src/app.rs is covered by Plan 11-03; this section carries only a placeholder Verdict line for it so `--module root` coverage passes. -->

### File Scores

**File:** `src/main.rs` (40 LOC)
**Public interface:** `async fn main() -> color_eyre::Result<()>` (0 `pub` items — `main` is special; all items are private `mod` declarations + `#[tokio::main]` entry)
**Verdict:** Deep (for its role)
**Justification:** 40 lines of pure boot sequencing (color_eyre → panic hook → logging → ratatui init → `app::run` → restore) with a numbered comment contract enforcing the ordering; no domain/UI knowledge leaks in; wiring cost is hidden behind six labelled steps.

**File:** `src/tui.rs` (38 LOC)
**Public interface:** `setup_logging() -> color_eyre::Result<WorkerGuard>` (1 pub fn)
**Verdict:** OK (deep enough for a logging helper)
**Justification:** Single public function whose implementation covers directory creation, daily rolling appender, non-blocking writer, ANSI-off filter, and env-filter chaining — caller only needs to hold the returned `WorkerGuard` for the program lifetime. No ratatui or domain imports; strictly infra.

**File:** `src/event.rs` (22 LOC)
**Public interface:** `enum Event { Key, Resize, Tick }` + `from_crossterm(CrosstermEvent) -> Option<Event>` (1 pub enum + 1 pub fn)
**Verdict:** OK
**Justification:** Minimal wrapper whose purpose (per its doc comment) is to decouple the rest of the codebase from `crossterm::event::Event`. Interface is narrow (3 variants) and deliberate; implementation is a single `match` (see the event.rs catch-all finding below for the fall-through cross-ref).

**File:** `src/action.rs` (151 LOC)
**Public interface:** `enum Action` (1 pub enum, ~55 variants)
**Verdict:** Deep (by Ousterhout Anti-pattern #1 — large variant count is not shallowness; dispatch in `update()` is the hidden complexity)
**Justification:** Single enum whose variants are the full TEA action grammar. Each variant is either user input, background-task outcome, or modal transition — the complexity is in `update()`'s match, not in the type. See the action.rs placement finding below: the *file's placement at src/* (root) rather than `src/domain/` is the architectural concern, not the type itself.

**File:** `src/app.rs` — **placeholder; full audit in Plan 11-03.**
**Verdict:** Reserved (scored in Plan 11-03 — app/ module section)
**Justification:** app.rs audit (2,425 LOC, god-object candidate per D-03) is Plan 11-03's scope; this line exists so `--module root` coverage passes without stealing Plan 11-03's score.

### Critical

### Major

### [Major] F-002: `action.rs` belongs in `domain/`, not at repo root
- **Location:** `src/action.rs:1-151`
- **Dimension:** Fowler-4-Layer | Hexagonal
- **Symptom:** `Action` is the TEA intent type — the central domain concept that `update()` dispatches on. It lives at `src/action.rs` (root) and is imported by both `app.rs:2` (expected) and `infra/command_runner.rs:12` (not expected — infra should not know domain's action grammar directly). The root placement reads as "this type is cross-cutting" but actually the type *is* domain vocabulary (Per RESEARCH Open Question 2 and §Codebase Inventory).
- **Why's a problem:** Violates the Fowler 4-layer boundary — the Domain layer owns its intent grammar; keeping it at root makes `mod domain` look smaller than it is and lets infra import the domain grammar through a path (`crate::action::Action`) that hides the dependency direction. Hexagonal-wise, this is the upstream enabling condition for the command_runner → action coupling captured separately in Plan 11-02.
- **Recommendation:** `move src/action.rs → src/domain/action.rs`; add `pub mod action;` to `src/domain/mod.rs`; update the two importers (`src/app.rs:2` and `src/infra/command_runner.rs:12`) to `use crate::domain::action::Action`. The command_runner import should additionally die as part of Plan 11-02's infra → domain port refactor (command_runner returns typed `CommandEvent` values, leaving `Action` translation to app.rs). No behavioral change — pure file move + import rewrite.
- **Phase 13 task hint:** Move `action.rs` into `domain/`, update the two import sites; coordinate with Plan 11-02's command_runner refactor so the infra import disappears rather than being merely rewritten.

### Minor

### [Minor] F-003: `event.rs` catch-all drops Mouse/Paste/FocusGained/FocusLost (legitimate fall-through)
- **Location:** `src/event.rs:15-22`
- **Dimension:** Catch-All
- **Symptom:** `from_crossterm` has `_ => None` at line 20, silently dropping `CrosstermEvent::Mouse`, `Paste`, `FocusGained`, `FocusLost` (four variants the rn-dash UI deliberately does not consume). The doc comment (line 14) documents the intent ("event types we don't handle").
- **Why's a problem:** Acceptable fall-through — the drop is deliberate and documented — but it's a silent filter at the boundary between crossterm and the rest of the app. If rn-dash ever wants to support mouse selection or IME paste, this is the gate that must open. Graded Minor because it's documented and currently correct; the full enumeration belongs to Plan 11-05 cross-cutting.
- **Recommendation:** Keep the `_ => None` arm (behavior is correct and intentional) but enumerate the dropped variants explicitly in a comment, e.g. `_ /* Mouse, Paste, FocusGained, FocusLost */ => None`, so future readers do not have to cross-reference crossterm's `Event` definition to know what is dropped. Non-blocking. Cross-referenced by Plan 11-05's full catch-all enumeration.
- **Phase 13 task hint:** Low-priority cleanup — expand the fall-through with an inline comment listing dropped variants. Can piggyback on any other edit to this file.


## Module: domain/
<!-- Coverage: src/domain/mod.rs, command.rs, metro.rs, refresh.rs, worktree.rs -->
<!-- Wave 1 Plan 11-01 appends here -->

### Critical
### Major
### Minor

## Module: infra/
<!-- Coverage: src/infra/{mod,port,process,worktrees,command_runner,devices,config,jira,jira_cache,multiplexer,sim_history,android_prefs,tmux}.rs -->
<!-- Wave 1 Plan 11-02 appends here -->

### Critical
### Major
### Minor

## Module: app/
<!-- Coverage: src/app.rs (the single 2,425-LOC file) -->
<!-- Wave 1 Plan 11-03 appends here, INCLUDING D-04 target shapes for Criticals -->

### Critical
### Major
### Minor

## Module: ui/
<!-- Coverage: src/ui/{mod,panels,footer,help_overlay,error_overlay,modals,theme}.rs -->
<!-- Wave 1 Plan 11-04 appends here, plus initial keybinding evidence (D-14) -->

### Critical
### Major
### Minor

## Cross-Cutting Findings

### Catch-all match arms (ARCH-04)
<!-- Wave 2 Plan 11-05 enumerates every `_ => {}` and `_ =>` arm here -->

### Misplaced prerequisite/ordering logic (ARCH-05)
<!-- Wave 2 Plan 11-05 enumerates the prerequisite locations from RESEARCH §Prerequisite/Ordering Logic Detection -->

### Hexagonal port violations (cross-module — ARCH-03)
<!-- Wave 2 Plan 11-05 captures cross-module hexagonal findings not already attached to a single per-module section -->

### Keybinding source-of-truth (D-14)
<!-- Wave 2 Plan 11-05 finalizes the D-14 finding, referencing handle_key + footer.rs + help_overlay.rs -->

## Refactor Sequence

<!-- Wave 2 Plan 11-05 lists every Critical and Major F-NNN here in dependency order, per D-09 -->
