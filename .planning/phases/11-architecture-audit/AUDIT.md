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
<!-- Plan 11-01 Task 2 appends here. Five files scored; findings start at the next free ID. -->

### File Scores

**File:** `src/domain/mod.rs` (7 LOC)
**Public interface:** four `pub mod` re-exports (`command`, `metro`, `refresh`, `worktree`) + one module-level doc comment
**Verdict:** OK (minimal by design)
**Justification:** Pure re-export hub. The doc comment declares the layer invariant ("Zero dependencies on ratatui, crossterm, or infra") — verified by `rg 'use (ratatui|crossterm|crate::infra)' src/domain/`: **no matches**. The doc itself acknowledges metro.rs's tokio-types carve-out and correctly notes that mod.rs imports nothing from infra, so ARCH-01 is literally respected at the mod.rs level. See the metro.rs finding below for the carve-out's own grading.

**File:** `src/domain/command.rs` (250 LOC)
**Public interface:** `enum CommandSpec` with 23 variants (doc comment claims 17; outdated) + 6 impl methods (`to_argv`, `is_destructive`, `needs_text_input`, `needs_metro`, `needs_device_selection`, `label`) + 3 value types (`CleanOptions`, `ModalState`, `DeviceInfo`). 10 pub items total per `rg '^pub (fn|struct|enum|trait|const)' src/domain/command.rs`.
**Verdict:** Deep
**Justification:** Directly addresses Ousterhout Anti-pattern #1 ("conflating type complexity with shallowness" per RESEARCH §Pitfalls). 23 variants look wide but the *interface* is six small predicates + a single formatter (`to_argv`) + a label — all read-only, pure, no lifecycle. Consumers (`app.rs` / `command_runner.rs`) never pattern-match the enum directly; they call the predicates. The hidden complexity is the variant-to-argv mapping (lines 50-105), which is exactly what Ousterhout calls a deep module: narrow interface, significant implementation. Known gap: absence of `is_cancellable()` predicate (REFACTOR-02 scope — do not propose here; noted so Plan 13/14 planners see the cross-reference).

**File:** `src/domain/metro.rs` (162 LOC)
**Public interface:** 2 data enums (`MetroActivity` 5 variants, `MetroStatus` 4 variants) + 2 structs (`MetroHandle` with 5 pub fields; `MetroManager` with private `handle` and 2 pub status fields) + 7 `MetroManager` methods (`new`, `is_running`, `register`, `clear`, `send_stdin`, `set_starting`, `set_stopping`, `take_handle`) + `Display for MetroActivity`. 4 pub types per `rg '^pub (fn|struct|enum|trait|const)' src/domain/metro.rs`.
**Verdict:** Major (Ousterhout/Hexagonal compromise — narrow interface around `MetroManager` is deep, but `MetroHandle`'s tokio-typed pub fields leak infra into domain)
**Justification:** Two-tier structure. `MetroManager` itself is deep — `Option<MetroHandle>` enforces the single-instance invariant at the type level, methods are small and focused (single-sentence docs). `MetroHandle`'s five pub fields (`pid`, `worktree_id`, `stdin_tx: tokio::sync::mpsc::UnboundedSender`, `stream_task: tokio::task::JoinHandle`, `stdin_task: tokio::task::JoinHandle`, `kill_tx: Option<tokio::sync::oneshot::Sender>`) pull tokio types into `domain/`. The file's 13-line architectural note (lines 1-13) defends the choice ("tokio types used here ... are inert data — they carry no behavior until the infra layer acts on them"). Per Pitfall 2 ("the comment IS the finding"): the documented compromise is graded rather than dismissed. See the metro.rs Major finding below.

**File:** `src/domain/refresh.rs` (248 LOC total, ~70 impl LOC; 178 test LOC per `awk '/^#\[cfg\(test\)\]/{flag=1} flag{c++}'`)
**Public interface:** `struct RefreshSet { worktrees, staleness, jira_titles }` + `RefreshSet::none()` + `RefreshSet::any()` + `fn refresh_needed(cmd: &CommandSpec) -> RefreshSet` (4 items: 1 type + 3 functions)
**Verdict:** Deep — **exemplary deep module; cite as the reference standard for Phase 11+.**
**Justification:** Textbook Ousterhout depth. Four-item interface hides the non-trivial command→refresh mapping behind a single pure function. ~17 inline tests (lines 72-247) document every branch of the mapping as executable specification — callers do not need to know which git operations invalidate staleness; they call `refresh_needed` and trust it. Zero dependencies beyond `CommandSpec`. Zero side effects. Other findings in this audit MAY reference this file's shape as the standard to which refactors should aim (e.g., "collapse this logic into a pure function akin to `refresh_needed`").

**File:** `src/domain/worktree.rs` (78 LOC)
**Public interface:** `struct WorktreeId(pub String)` + `enum WorktreeMetroStatus { Running, Stopped }` + `struct Worktree` (9 pub fields) + `Worktree::display_name()` + `Worktree::preferred_prefix()` (5 pub items per `rg '^pub (fn|struct|enum|trait|const)' src/domain/worktree.rs`)
**Verdict:** OK (borderline — plain data with two small derivations)
**Justification:** Mostly a struct. Interface-to-impl ratio is near 1:1 — all fields are pub; the only hidden logic is the two preferred-prefix fallback ladders (branch → JIRA-key → workspace-dir). Not deep, but not shallow: the fallback ordering is the one place readers benefit from the struct owning the logic rather than scattering `match w.jira_key { ... }` across callers. See the worktree.rs Minor finding below for the `jira_title`/`jira_key` field-placement question.

### Critical

### Major

### [Major] F-004: `domain/metro.rs` `MetroHandle` exposes tokio types via pub fields — hexagonal leak from infra into domain
- **Location:** `src/domain/metro.rs:54-76`
- **Dimension:** Hexagonal | Ousterhout
- **Symptom:** `MetroHandle` declares five `pub` fields of which four carry tokio types directly: `stdin_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>` (line 68), `stream_task: tokio::task::JoinHandle<()>` (line 70), `stdin_task: tokio::task::JoinHandle<()>` (line 72), `kill_tx: Option<tokio::sync::oneshot::Sender<()>>` (line 75). The file's own module doc-comment (lines 1-13) acknowledges the compromise and argues the fields are "inert data — they carry no behavior until the infra layer acts on them." Per RESEARCH Pitfall 2, the architectural note IS the finding — it concedes the violation and asks to be graded.
- **Why's a problem:** Violates Alistair Cockburn's ports-and-adapters discipline — the domain layer's public API references infrastructure types (tokio), so any caller of `domain::metro` transitively depends on tokio. This is the single place in the codebase where the layering claim "`domain/` has zero infra dependencies" (made by `domain/mod.rs` doc) becomes a half-truth. It also limits testability: test doubles for metro lifecycle must fabricate tokio channels/JoinHandles rather than implement a trait.
- **Recommendation:** Introduce an opaque `trait MetroHandle { fn send_stdin(&self, bytes: Vec<u8>) -> anyhow::Result<()>; fn kill(&mut self) -> anyhow::Result<()>; fn pid(&self) -> u32; fn worktree_id(&self) -> &str; }` in `src/domain/metro.rs`, and `move` the tokio-typed implementation to `src/infra/metro.rs` as `struct TokioMetroAdapter { stdin_tx, stream_task, stdin_task, kill_tx, pid, worktree_id } impl MetroHandle for TokioMetroAdapter`. `MetroManager` then stores `Option<Box<dyn MetroHandle>>` (or a generic parameter) instead of the concrete struct. The 4-item `send_stdin` / `kill` / `pid` / `worktree_id` surface matches `refresh.rs`'s shape (narrow public interface over hidden complexity). Ousterhout-wise the verdict promotes from Major-compromise to Deep.
- **Phase 13 task hint:** Extract `trait MetroHandle` in `domain/metro.rs` with four methods (`send_stdin`, `kill`, `pid`, `worktree_id`); `move` the tokio-typed construction and fields into a new `infra/metro.rs::TokioMetroAdapter`; update `MetroManager::register`/`take_handle` signatures to use the trait object; delete the 13-line architectural-note comment (replaced by honest encapsulation). Coordinate with Plan 11-02 infra findings so the new adapter lands alongside any `MetroPort` extraction.

### Minor

### [Minor] F-005: `domain/command.rs` doc comment under-counts CommandSpec variants (17 vs actual 23)
- **Location:** `src/domain/command.rs:7`
- **Dimension:** Ousterhout (documentation drift)
- **Symptom:** Module-level doc-comment on `CommandSpec` says "17 variants total" (line 7). Actual count is **23** (verified by `awk '/^pub enum CommandSpec/,/^}/' src/domain/command.rs | grep -cE '^\s+(Git|Rn|Rm|Yarn|Adb|Shell)'`). Drift accumulated since Phase 05.1 additions (GitFetch, GitResetHardFetch, RnReleaseBuild, AdbInstallApk, ShellCommand, RnRunIosDevice).
- **Why's a problem:** Documentation drift erodes trust in other load-bearing doc-comments in the same file (e.g., `is_destructive`, `needs_metro` predicate semantics). Readers who count variants and find the doc wrong will rightly start mistrusting the rest.
- **Recommendation:** Update the count to 23 and either commit to maintaining it (bad — pure toil) or replace the count with a policy statement: `// Variants grow over time; pattern-match on specific predicates (is_destructive, needs_metro, etc.) instead of counting.` Non-blocking.
- **Phase 13 task hint:** Drive-by fix alongside any other edit to command.rs.

### [Minor] F-006: `domain/command.rs` catch-all in `needs_text_input` masks future variant additions
- **Location:** `src/domain/command.rs:120-129`
- **Dimension:** Catch-All
- **Symptom:** `needs_text_input` uses an explicit four-arm match for git rebase/checkout/checkoutnew/jest, a `ShellCommand` guarded arm, and a final `_ => false`. Cross-referenced for Plan 11-05's full catch-all enumeration.
- **Why's a problem:** Low-severity risk: if a future variant (say, `YarnWorkspace { name: String }`) needs text input, this arm silently defaults to `false` and the variant will dispatch without opening the TextInput modal. The compiler will not help.
- **Recommendation:** Replace `_ => false` with an explicit `_ => false /* All remaining variants take no user-supplied text. Add variants here when that changes. */` comment, OR exhaustively enumerate as `CommandSpec::GitPull | CommandSpec::GitPush | ... => false`. Full enumeration belongs to Plan 11-05.
- **Phase 13 task hint:** Low priority — pair with any other command.rs edit; Plan 11-05's full enumeration will likely cover this.

### [Minor] F-007: `domain/command.rs` missing `is_cancellable()` predicate (known REFACTOR-02 gap — not proposed here)
- **Location:** `src/domain/command.rs:46-178`
- **Dimension:** Ousterhout (interface completeness)
- **Symptom:** Current impl exposes `is_destructive`, `needs_text_input`, `needs_metro`, `needs_device_selection`, `label`. No `is_cancellable()` predicate exists, yet v1.3 Phase 14 explicitly plans "Individual command cancellation for yarn/clean/install, run-android/run-ios, and tests (jest/lint/types). Git operations remain non-cancellable" (per PROJECT.md). Cancellability is a CommandSpec-level attribute that Phase 14 will need.
- **Why's a problem:** Without a domain-level predicate, cancellability decisions will scatter across app.rs / command_runner.rs when Phase 14 lands — exactly the scattered-logic pattern that ARCH-05 flags.
- **Recommendation:** Add `pub fn is_cancellable(&self) -> bool { matches!(self, CommandSpec::YarnInstall | CommandSpec::YarnPodInstall | CommandSpec::RmNodeModules | CommandSpec::RnClean* | CommandSpec::RnRun* | CommandSpec::YarnJest{..} | CommandSpec::YarnLint | CommandSpec::YarnCheckTypes | CommandSpec::RnReleaseBuild | CommandSpec::AdbInstallApk | CommandSpec::ShellCommand{..}) }`. **Do not land in Phase 13 — REFACTOR-02 scope.** Noted here so Plan 14 planners pick it up.
- **Phase 13 task hint:** Do not action. Flag for Plan 14 / REFACTOR-02 traceability.

### [Minor] F-008: `domain/refresh.rs` catch-all is legitimate (reference fall-through)
- **Location:** `src/domain/refresh.rs:66-68`
- **Dimension:** Catch-All
- **Symptom:** `refresh_needed` ends with `_ => RefreshSet::none()`. The handled arms cover every command variant whose completion could change worktree state; `_` absorbs the test/quality commands and shell command (all non-state-changing). Cross-referenced for Plan 11-05.
- **Why's a problem:** Not a problem — the fall-through is correct. Listed as Minor to record the location for Plan 11-05's enumeration and to offer one tiny hardening option below.
- **Recommendation:** Optional: `replace _ => RefreshSet::none()` with an explicit enumeration of test/quality/shell variants, so new CommandSpec variants that need a refresh are caught by the compiler's exhaustiveness check. Not required — the current design is defensible and extensively tested.
- **Phase 13 task hint:** Skip unless paired with a refresh-rule change.

### [Minor] F-009: `domain/worktree.rs` mixes identity fields with enrichment fields on one struct
- **Location:** `src/domain/worktree.rs:22-42`
- **Dimension:** Fowler-4-Layer | Ousterhout
- **Symptom:** `Worktree` struct mixes identity/filesystem fields (`id`, `path`, `branch`, `head_sha`) with state fields (`metro_status`, `stale`, `stale_pods`) and enrichment fields fetched asynchronously (`jira_title: Option<String>`, `jira_key: Option<String>`). The JIRA fields are `Option` precisely because they arrive later (Phase 4 per field comment).
- **Why's a problem:** The struct's identity ("which worktree on disk") is conflated with its enrichment ("what JIRA calls it"). Call sites such as `preferred_prefix` already branch on `jira_key.is_some()`, which hints the enrichment wants its own type. Minor because current callers tolerate the shape; tracked for the per-worktree task model in Phase 16 where a cleaner split would help.
- **Recommendation:** Potentially `move` enrichment fields into a sibling struct, e.g. `pub struct WorktreeEnrichment { pub jira_title: Option<String>, pub jira_key: Option<String>, pub stale: bool, pub stale_pods: bool, pub metro_status: WorktreeMetroStatus }` keyed by `WorktreeId`. Defer decision to Phase 16 — premature if the per-worktree task model lands differently than expected.
- **Phase 13 task hint:** Do not action. Re-evaluate when Phase 16 (per-worktree task model) begins.

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
