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

### File Scores

**File:** `src/infra/mod.rs` (15 LOC)
**Public interface:** 12 `pub mod` re-exports (port, process, worktrees, command_runner, devices, config, jira, jira_cache, tmux, multiplexer, sim_history, android_prefs) + module doc-comment
**Verdict:** OK (minimal re-export hub)
**Justification:** Pure re-export hub. Doc-comment (line 2) claims "All concrete implementations are behind trait boundaries (ARCH-02)" — aspirational: only 3 of 12 modules expose a trait (`process::ProcessClient`, `multiplexer::Multiplexer`, `jira::JiraClient`). See the mod.rs Minor finding below.

**File:** `src/infra/port.rs` (66 LOC)
**Public interface:** `pub fn port_is_free(u16) -> bool` + `pub struct ExternalMetroInfo { pid: u32, working_dir: String }` + `pub async fn detect_external_metro(u16) -> Option<ExternalMetroInfo>` + `pub async fn kill_process(u32) -> anyhow::Result<()>` (4 pub items)
**Verdict:** OK (shallow-ish — small interface over small impl; hexagonal port candidate)
**Justification:** Three free functions directly wrap OS calls (`TcpListener::bind`, `lsof`, `kill`). No trait. Called directly from `app.rs` (per RESEARCH §Direct infra↔app coupling). Literally named after hexagonal ports yet isn't one — see the port.rs Major finding.

**File:** `src/infra/process.rs` (51 LOC)
**Public interface:** `#[async_trait] pub trait ProcessClient { async fn spawn_metro(&self, PathBuf) -> Result<Child> }` + `pub struct TokioProcessClient` (2 pub items)
**Verdict:** OK — positive example of a trait boundary, but trait placement is wrong per strict hexagonal rule.
**Justification:** Cleanest single-method trait in the codebase. Caller depends on `ProcessClient`, not on `tokio::process`. However both the trait AND its impl live in `infra/`, which per Cockburn's ports-and-adapters rule makes this interface segregation, not a hexagonal port. Strict grading produces a Major finding below.

**File:** `src/infra/worktrees.rs` (348 LOC)
**Public interface:** 8 pub fns — `parse_worktree_porcelain`, `check_stale`, `check_stale_pods` (pure), `remove_worktree`, `add_worktree`, `add_worktree_new_branch`, `list_remote_branches`, `list_worktrees` (async/I/O). No trait.
**Verdict:** OK (deep on the parser, shallow on the I/O surface — mixed responsibilities)
**Justification:** Pure parsers are deep — non-trivial sentinel logic hidden behind a single function, unit-tested without git. Six async git-invoking functions are thin wrappers over `tokio::process::Command` with no abstraction over the tool. No trait; callers import the free functions directly. Hexagonal port candidate — see the worktrees.rs Major finding.

**File:** `src/infra/command_runner.rs` (129 LOC)
**Public interface:** `pub async fn spawn_command_task(CommandSpec, PathBuf, String, UnboundedSender<Action>) -> JoinHandle<()>` (1 pub fn; `stream_command_output`, `build_argv` are private)
**Verdict:** Shallow — clearest layer violation in the codebase. Single public fn imports `crate::action::Action` at line 12 and sends TEA actions over a caller-provided channel.
**Justification:** Interface surface is one function. Implementation pipes stdout/stderr lines as `Action::CommandOutputLine` and final exit as `Action::CommandExited`. The module cannot be reused outside this codebase without dragging `Action` along. Lines 99 and 105 contain legitimate `_ => { stdout_done = true; }` / `_ => { stderr_done = true; }` catch-alls (next_line returned Err → stream done; full enumeration in Plan 11-05). See the command_runner.rs Critical finding below.

**File:** `src/infra/devices.rs` (273 LOC)
**Public interface:** 4 pure parsers (`parse_adb_devices`, `parse_xcrun_simctl`, `parse_xctrace_devices`, `parse_avd_list`) + 3 async runners (`list_android_devices`, `list_ios_simulators`, `list_ios_physical_devices`) — 7 pub fns.
**Verdict:** OK — parsers are deep (well-tested, hide parsing complexity); runners are thin shells over `tokio::process::Command`. No trait.
**Justification:** Parsers take raw command output and return `Vec<DeviceInfo>` — pure/testable shape. Runners bundle command-invocation + parsing. Callers invoke the async fns directly. Hexagonal port candidate — see the devices.rs Major finding.

**File:** `src/infra/jira.rs` (175 LOC)
**Public interface:** `#[async_trait] pub trait JiraClient: Send + Sync + Debug { async fn fetch_title(&self, &str) -> Option<String> }` + `pub struct HttpJiraClient` + `pub fn extract_jira_key(&str, &str) -> Option<String>` (pure, 6 inline tests) + `pub fn is_inside_tmux() -> bool` (pure).
**Verdict:** OK on the trait boundary; two misplaced helpers (`extract_jira_key`, `is_inside_tmux`).
**Justification:** `JiraClient` + `HttpJiraClient` mirror `ProcessClient` — clean single-method trait, swappable. Same hexagonal grading question as process.rs (trait in infra rather than domain — Major below). `extract_jira_key` is pure and called from `ui/panels.rs:71` (UI→infra leak for domain logic — Major). `is_inside_tmux` is a multiplexer concern misplaced in jira.rs (Minor).

**File:** `src/infra/config.rs` (138 LOC)
**Public interface:** `pub fn config_dir() -> PathBuf` + `pub struct DashConfig` (9 pub fields: `jira_base_url`, `jira_email`, `jira_token`, `auth_mode`, `claude_flags`, `repo_root`, `jira_project_prefix`, `app_title`, `auto_sync`) + `impl DashConfig::repo_root_path()` + `pub fn load_config()` + `pub fn save_config()` — 4 pub items + 1 pub method.
**Verdict:** OK (appropriately-sized config module)
**Justification:** `DashConfig` is a serde-derived data struct; `load_config` / `save_config` are thin TOML I/O. Importers: `src/app.rs` + `src/infra/jira.rs` (cross-infra coupling for credentials — normal) + sibling persistence modules (`jira_cache`, `sim_history`, `android_prefs` for `config_dir()`). Verified `rg 'crate::infra::config' src/domain/` = no matches — no domain→infra leak. Credentials stored at `~/.config/rn-dash/config.toml` with chmod 0600 on Unix (security-correct). No finding attached — see F-111 below for the persistence-pattern cross-file observation.

**File:** `src/infra/jira_cache.rs` (45 LOC)
**Public interface:** `pub fn cache_path() -> PathBuf` + `pub fn load_jira_cache() -> Result<HashMap<String, String>>` + `pub fn save_jira_cache(&HashMap) -> Result<()>` (3 pub fns)
**Verdict:** OK (tiny persistence helper; appropriate shape for caller count)
**Justification:** Single-caller (app.rs) flat-JSON cache of JIRA title lookups. No credentials → no 0600 needed. Fits the accessor-proliferation pattern across four small persistence modules (see F-111). No per-file finding — folded into F-111.

**File:** `src/infra/multiplexer.rs` (85 LOC)
**Public interface:** `pub trait Multiplexer: Send + Sync + Debug { fn new_window(&self, &Path, &str, &str) -> Result<()>; fn is_available(&self) -> bool; }` + `pub struct TmuxAdapter` + `pub struct ZellijAdapter` + `pub fn detect_multiplexer() -> Option<Box<dyn Multiplexer>>` (4 pub items).
**Verdict:** OK — positive example cited in PROJECT.md as "✓ Good — clean trait boundary"; however trait placement is wrong per strict hexagonal rule (same critique as process.rs / jira.rs).
**Justification:** Two-method trait with two adapters + auto-detect. Reference standard for `Phase 13`'s adapter pattern. Strict-grading note: trait lives in infra, not domain — same hexagonal misplacement as `ProcessClient` (F-103) and `JiraClient` (F-106). See the multiplexer.rs Major finding below (F-110).

**File:** `src/infra/sim_history.rs` (32 LOC)
**Public interface:** `pub fn load_sim_history() -> Vec<String>` + `pub fn record_sim_used(&str) -> Result<()>` (2 pub fns; `sim_history_path` is private)
**Verdict:** OK (small, purpose-built)
**Justification:** JSON array of recently-used simulator UDIDs, push-front + dedup + truncate-to-20. Matches accessor-proliferation pattern (see F-111). No per-file finding.

**File:** `src/infra/android_prefs.rs` (27 LOC)
**Public interface:** `pub fn load_android_mode() -> Option<String>` + `pub fn save_android_mode(&str) -> Result<()>` (2 pub fns; `android_prefs_path` private)
**Verdict:** OK (smallest persistence helper; shape is right-sized for its caller)
**Justification:** Single-key JSON file (`{"mode": "debugOptimized"}`). Matches accessor-proliferation pattern — same shape as `sim_history.rs`, `jira_cache.rs`, and (in part) `config.rs`. See F-111 below for the cross-file cohesion observation.

**File:** `src/infra/tmux.rs` (29 LOC, DEPRECATED)
**Public interface:** `pub fn open_claude_in_worktree(&Path, &str) -> Result<()>` (1 pub fn, marked `#[allow(dead_code)]`)
**Verdict:** Shallow (dead-code / conjoined-method — replaced by multiplexer abstraction)
**Justification:** File's own doc-comment at line 4 reads "DEPRECATED: Use multiplexer::TmuxAdapter::new_window() instead." The function duplicates TmuxAdapter::new_window with a `-d` flag variant (no focus switch). Retained only because app.rs's OpenClaudeCode action hasn't been rewired. See the tmux.rs Minor finding below (F-112).

### Critical

### [Critical] F-101: `command_runner.rs` imports `crate::action::Action` — Data Source layer knows Service-layer messaging type
- **Location:** `src/infra/command_runner.rs:12` (`use crate::action::Action;`); `src/infra/command_runner.rs:26-70` (`spawn_command_task` signature takes `UnboundedSender<Action>`); lines 38, 54, 68, 98, 104 send `Action::CommandOutputLine(...)` / `Action::CommandExited`.
- **Dimension:** Hexagonal | Fowler-4-Layer
- **Symptom:** Infrastructure module imports and sends `Action::CommandOutputLine` and `Action::CommandExited` directly via the channel supplied by app.rs. The function signature itself bakes the TEA vocabulary into the infra API (`UnboundedSender<Action>`).
- **Why it's a problem:** Reverses the dependency direction — Data Source layer knows the Service layer's messaging vocabulary. `command_runner` cannot be reused without dragging `Action` along; any change to `Action`'s variant set forces a recompile of infra. Combined with F-002 (action.rs placement), this is the single most load-bearing layer violation in the codebase.
- **Recommendation:** Extract `domain::ports::CommandRunnerPort` that returns a typed event stream; app.rs translates events into `Action` at the boundary. Concrete target shape:
  ```rust
  // src/domain/ports/command_runner_port.rs (new file)
  pub enum CommandEvent { OutputLine(String), Exited(std::process::ExitStatus) }
  pub trait CommandRunnerPort: Send + Sync {
      fn spawn(&self, spec: CommandSpec, cwd: PathBuf, branch: String)
          -> tokio::sync::mpsc::UnboundedReceiver<CommandEvent>;
  }
  ```
  `move` the spawn-and-stream body from `infra/command_runner.rs` into a `TokioCommandRunner` adapter implementing the trait; delete the `use crate::action::Action` import; let app.rs's effect runner translate `CommandEvent::OutputLine` → `Action::CommandOutputLine` and `CommandEvent::Exited` → `Action::CommandExited`. Coordinate with F-002 — once command_runner no longer imports `Action`, the F-002 move becomes a single-importer change (only `app.rs` references `Action`).
- **Phase 13 task hint:** Define `domain::ports::CommandRunnerPort` + `CommandEvent` enum; move `command_runner.rs` body into `infra::command_runner::TokioCommandRunner` implementing the trait; rewire app.rs to translate events into Actions at the boundary.

### Major

### [Major] F-102: `infra/port.rs` exposes three free functions for an external port probe — no hexagonal port trait
- **Location:** `src/infra/port.rs:12-66` (functions `port_is_free`, `detect_external_metro`, `kill_process`); callers in `src/app.rs` (per RESEARCH §Direct infra↔app coupling)
- **Dimension:** Hexagonal
- **Symptom:** The module wraps three OS calls (`TcpListener::bind`, `lsof`, `kill`) as free pub fns. app.rs imports them directly (`use crate::infra::port::{detect_external_metro, kill_process, ...}`). No trait abstracts the probe, so the domain layer cannot express "detect an external metro" without depending on the concrete implementation.
- **Why it's a problem:** The file is literally named after hexagonal ports yet is not one. Consumer depends on the concrete module, not on an abstraction — no way to inject a fake for testing "external metro conflict" flows in `app.rs` without also faking `lsof`/`kill`. Strict hexagonal grading makes this Major.
- **Recommendation:** Extract `trait domain::ports::PortProbePort { fn is_free(&self, port: u16) -> bool; async fn detect_external(&self, port: u16) -> Option<ExternalProcessInfo>; async fn kill(&self, pid: u32) -> anyhow::Result<()>; }` in a new `src/domain/ports/port_probe_port.rs`. `move` the free-function bodies behind an `infra::port::LsofPortProbe` adapter implementing the trait. `ExternalMetroInfo` becomes `ExternalProcessInfo` (more general — no mention of metro) in domain. app.rs holds the trait object at startup injection.
- **Phase 13 task hint:** Define `domain::ports::PortProbePort` trait + `ExternalProcessInfo` struct; move port.rs contents behind an `LsofPortProbe` adapter; update app.rs to consume the trait instead of free functions.

### [Major] F-103: `ProcessClient` trait belongs in `domain/`, not `infra/` — strict hexagonal grading
- **Location:** `src/infra/process.rs:16-26` (trait definition at infra path); `src/infra/process.rs:29-51` (impl in same file)
- **Dimension:** Hexagonal
- **Symptom:** `pub trait ProcessClient` and its sole implementation `TokioProcessClient` both live in `src/infra/process.rs`. Per Cockburn's ports-and-adapters rule this is interface segregation, not a port — trait-and-impl in the same layer, domain has no knowledge of the abstraction. `domain/metro.rs::MetroManager` has no reference to `ProcessClient`; the trait serves only app.rs.
- **Why it's a problem:** Domain layer cannot express "spawn a metro process" without crossing into infra. Test doubles for metro lifecycle must live in infra crate even though they have no business there. Same critique applies to `Multiplexer` (F-110) and `JiraClient` (F-106) — all three infra traits share this wrong-layer placement. Graded Major (not Critical) because the refactor cost is small and the current shape does reduce coupling to `tokio::process` at the call site.
- **Recommendation:** `move` the `trait ProcessClient` declaration from `src/infra/process.rs` to `src/domain/ports/process_port.rs` (new file); keep the `TokioProcessClient` impl in `src/infra/process.rs` and change it to `impl crate::domain::ports::ProcessClient for TokioProcessClient`. `AppState` holds `Arc<dyn ProcessClient>` injected at startup from `main.rs`. No behavior change — pure file move + import rewrite across three sites (`app.rs`, `infra/process.rs`, new domain module). Alternative (downgrade to Minor): accept the current shape as "pragmatic interface segregation adequate for this codebase's scale" — document the trade-off in domain/mod.rs and close the finding without a refactor. This auditor recommends the strict move for consistency with F-101's CommandRunnerPort placement.
- **Phase 13 task hint:** Move `trait ProcessClient` from `infra/process.rs` to `domain/ports/process_port.rs`; update impl line in infra; update the import site in app.rs.

### [Major] F-104: `infra/worktrees.rs` exposes 8 free functions for git worktree operations — no hexagonal port
- **Location:** `src/infra/worktrees.rs:23-89` (pure parser), `:101-185` (pure staleness checks), `:196-348` (6 async git-invoking fns: `remove_worktree`, `add_worktree`, `list_remote_branches`, `add_worktree_new_branch`, `list_worktrees`)
- **Dimension:** Hexagonal | Fowler-4-Layer
- **Symptom:** 8 free functions in a single 348-LOC file. The six I/O functions each shell out to `git` via `tokio::process::Command`. No trait abstracts the git backend. app.rs calls them directly.
- **Why it's a problem:** Domain layer has no "worktree repository" abstraction — the concept of "list/add/remove a worktree" is defined only by whatever git's CLI accepts. Swapping to a different VCS or injecting a fake for app.rs integration tests requires stubbing out `tokio::process::Command` globally, not a clean trait substitution.
- **Recommendation:** Extract `trait domain::ports::WorktreePort { async fn list(&self) -> anyhow::Result<Vec<Worktree>>; async fn add(&self, branch: &str) -> anyhow::Result<PathBuf>; async fn remove(&self, path: &Path) -> anyhow::Result<()>; async fn add_with_new_branch(&self, new: &str, base: &str) -> anyhow::Result<PathBuf>; async fn list_remote_branches(&self) -> anyhow::Result<Vec<String>>; }`. `move` the six async fns behind a `infra::worktrees::GitWorktreeAdapter` impl. Keep `parse_worktree_porcelain`, `check_stale`, `check_stale_pods` as free module-private helpers (pure parsers/checks, not behavior). app.rs injects `Arc<dyn WorktreePort>` at startup.
- **Phase 13 task hint:** Define `domain::ports::WorktreePort`; move the six git-invoking fns into `infra::worktrees::GitWorktreeAdapter`; keep the three pure helpers private to infra; rewire app.rs.

### [Major] F-105: `infra/devices.rs` exposes pure parsers + async runners — no hexagonal port for device enumeration
- **Location:** `src/infra/devices.rs:32-203` (4 pure parsers); `:208-273` (3 async runners `list_android_devices`, `list_ios_simulators`, `list_ios_physical_devices`)
- **Dimension:** Hexagonal
- **Symptom:** Three async enumeration fns each shell out to `adb`/`xcrun` and delegate parsing to pure helpers. No trait. app.rs calls the async fns directly.
- **Why it's a problem:** Same failure mode as F-102 and F-104 — the concept "list devices" is concretely coupled to `adb`/`xcrun`. Cannot inject a stub-device set for app.rs flows without process-level faking.
- **Recommendation:** Extract `trait domain::ports::DevicePort { async fn list_android(&self) -> anyhow::Result<Vec<DeviceInfo>>; async fn list_ios_simulators(&self) -> anyhow::Result<Vec<DeviceInfo>>; async fn list_ios_physical(&self) -> anyhow::Result<Vec<DeviceInfo>>; }`. `move` the three async runners behind an `infra::devices::AdbXcrunDevices` adapter impl. Keep the four parsers as free module-private helpers (pure, already unit-tested).
- **Phase 13 task hint:** Define `domain::ports::DevicePort`; move async runners into `AdbXcrunDevices` adapter; keep parsers private to infra; update app.rs import.

### [Major] F-106: `JiraClient` trait belongs in `domain/`, not `infra/` (same shape as F-103)
- **Location:** `src/infra/jira.rs:22-30` (trait), `:33-88` (`HttpJiraClient` impl)
- **Dimension:** Hexagonal
- **Symptom:** `trait JiraClient` and `HttpJiraClient` both in `infra/jira.rs`. Same pattern as `ProcessClient` (F-103) and `Multiplexer` (F-110 below) — wrong layer for a hexagonal port.
- **Why it's a problem:** Ticket-title fetching is a domain concept (enriches `Worktree`). Domain code today cannot reference the trait — any domain-side test that needs a fake JIRA client must reach into infra.
- **Recommendation:** `move` the `trait JiraClient` declaration from `src/infra/jira.rs` to `src/domain/ports/jira_port.rs`; keep `HttpJiraClient` in infra, changing its `impl JiraClient` to reference the domain path. AppState holds `Arc<dyn domain::ports::JiraClient>`. Same strict-vs-pragmatic trade-off as F-103 — recommend the strict move for consistency; downgrade to Minor only if F-103 is also downgraded.
- **Phase 13 task hint:** Move `trait JiraClient` from `infra/jira.rs` to `domain/ports/jira_port.rs`; update impl site; update AppState field type.

### [Major] F-107: `extract_jira_key` is pure domain logic called from `ui/panels.rs:71` — UI→infra leak
- **Location:** `src/infra/jira.rs:103-120` (pure function, no I/O); called from `src/ui/panels.rs:71` (per RESEARCH §Direct infra↔app coupling)
- **Dimension:** Hexagonal | Fowler-4-Layer
- **Symptom:** `extract_jira_key(branch: &str, project_prefix: &str) -> Option<String>` is a pure string parser (6 inline unit tests at lines 130-175). It lives in infra but is called from ui/panels.rs — Presentation reaches directly into Data Source for logic that is purely domain (string manipulation on branch names).
- **Why it's a problem:** ui/mod.rs's doc-comment claims "Imports: domain types and ratatui ONLY. Never imports infra directly" — panels.rs:71 violates this claim. The function itself has no infra concern (no HTTP, no I/O, no tokio) — its placement in infra is incidental to `HttpJiraClient` happening to share the file.
- **Recommendation:** `move` `extract_jira_key` (and its 6 inline tests) from `src/infra/jira.rs` to `src/domain/jira.rs` (new file) or `src/domain/worktree.rs` as a free function. Update the two import sites (`src/ui/panels.rs:71` and any app.rs use). Infra keeps only `JiraClient` + `HttpJiraClient`. The UI→infra leak dies.
- **Phase 13 task hint:** Move `extract_jira_key` and its tests from `infra/jira.rs` to a new `domain/jira.rs` (or `domain/worktree.rs`); update panels.rs and app.rs import paths.

### [Major] F-110: `Multiplexer` trait belongs in `domain/`, not `infra/` (same shape as F-103, F-106)
- **Location:** `src/infra/multiplexer.rs:10-17` (trait); `:19-37` (`TmuxAdapter` impl); `:39-73` (`ZellijAdapter` impl); `:77-85` (`detect_multiplexer` factory)
- **Dimension:** Hexagonal
- **Symptom:** `pub trait Multiplexer` plus both adapter impls live in `src/infra/multiplexer.rs`. PROJECT.md Key Decisions table cites this as "✓ Good — clean trait boundary" — and it is good interface segregation. But per Cockburn's strict hexagonal rule (same as F-103 `ProcessClient`, F-106 `JiraClient`), the trait belongs in `domain/` so the domain/app layers depend on an abstraction defined by the domain, not imported from infra.
- **Why it's a problem:** Three infra traits (`ProcessClient`, `JiraClient`, `Multiplexer`) all share wrong-layer placement. A new Phase 13 contributor reading `domain/` cannot see what external collaborators the application depends on — they must read all of `infra/` to discover the abstractions. Also prevents any future domain-layer helper from accepting a `&dyn Multiplexer` parameter without a domain→infra upward import. Graded Major for consistency with F-103 and F-106; auditor recommends downgrading to Minor only if ALL three are downgraded together (uniform treatment — either the strict rule applies to this codebase or it doesn't).
- **Recommendation:** `move` the `pub trait Multiplexer` declaration from `src/infra/multiplexer.rs` to `src/domain/ports/multiplexer_port.rs` (new file); keep `TmuxAdapter`, `ZellijAdapter`, and `detect_multiplexer` in infra, changing their `impl Multiplexer` lines to reference the new domain path. AppState holds `Option<Box<dyn domain::ports::Multiplexer>>`. No behavior change — the trait itself already has exactly the right shape (2 methods, Send+Sync+Debug bound).
- **Phase 13 task hint:** Move `trait Multiplexer` from `infra/multiplexer.rs` to `domain/ports/multiplexer_port.rs`; update impl sites in infra; update AppState field.

### Minor

### [Minor] F-100: `infra/mod.rs` doc-claim "All concrete implementations are behind trait boundaries (ARCH-02)" is not enforced
- **Location:** `src/infra/mod.rs:1-2`
- **Dimension:** Ousterhout (documentation-bleed / aspirational invariant)
- **Symptom:** The module-level doc-comment states "All concrete implementations are behind trait boundaries (ARCH-02)." Per this plan's audit, only 3 of 12 modules expose a trait (`process::ProcessClient`, `multiplexer::Multiplexer`, `jira::JiraClient`); the other 9 (`port`, `worktrees`, `command_runner`, `devices`, `config`, `jira_cache`, `sim_history`, `android_prefs`, `tmux`) expose free functions directly.
- **Why it's a problem:** The doc-comment is load-bearing (an invariant claim about layer discipline) yet false by inspection. Readers who trust it will assume port discipline holds when it doesn't — exactly the misplaced-trust pattern that F-005's CommandSpec miscount embodies on a smaller scale. Minor because the fix is a single-line doc revision; promoted from trivial because the claim directly advertises ARCH-02 compliance.
- **Recommendation:** Either (a) revise the doc to be honest about the current state — "Most I/O-bearing modules expose a trait; the remainder (port, worktrees, devices, command_runner, persistence helpers) are candidates for trait extraction" — or (b) land the per-module hexagonal findings above (F-102/F-104/F-105/F-106 plus the new `CommandRunnerPort` from F-101) and then the doc-claim becomes true. Option (b) is Phase 13's likely path; option (a) is the immediate safety-valve fix if the refactor slips. No file `move`/`trait` keyword required (Minor) — the concrete keyword budget is covered by the per-module hexagonal findings this doc-claim points to.
- **Phase 13 task hint:** Drive-by — once the per-module hexagonal extractions land, rewrite `infra/mod.rs` doc-comment to reflect reality; if any module remains trait-less, name it explicitly.

### [Minor] F-108: `is_inside_tmux` lives in `infra/jira.rs` but is a multiplexer concern
- **Location:** `src/infra/jira.rs:122-128`
- **Dimension:** Ousterhout (misplaced utility)
- **Symptom:** `pub fn is_inside_tmux() -> bool { std::env::var("TMUX").is_ok() }` sits at the bottom of `infra/jira.rs` next to `HttpJiraClient` and `extract_jira_key`. It has no relationship to JIRA — the same helper already exists de-facto inside `infra/multiplexer.rs::TmuxAdapter::is_available` (line 34-36).
- **Why it's a problem:** Low-severity readability hit. A reader looking for "tmux detection" searches `multiplexer.rs`/`tmux.rs` first and misses this duplicate. Minor — no behavior risk.
- **Recommendation:** `move` `is_inside_tmux` from `src/infra/jira.rs` to `src/infra/multiplexer.rs` (or delete it entirely and have callers use `TmuxAdapter::new().is_available()` directly). Update any existing callers (grep shows none outside tests — function may be unused production code).
- **Phase 13 task hint:** Drive-by — move `is_inside_tmux` to `multiplexer.rs` or delete it if unused.

### [Minor] F-111: persistence-accessor proliferation across four small infra modules — cohesion miss
- **Location:** `src/infra/sim_history.rs:12-31` (`load_sim_history` / `record_sim_used`); `src/infra/android_prefs.rs:12-26` (`load_android_mode` / `save_android_mode`); `src/infra/jira_cache.rs:24-44` (`load_jira_cache` / `save_jira_cache`); `src/infra/config.rs:99-138` (`load_config` / `save_config`)
- **Dimension:** Ousterhout (accessor proliferation) | Fowler-4-Layer
- **Symptom:** Four modules in `src/infra/` each expose a `pub fn load_X / save_X` pair over a single JSON-or-TOML file in `~/.config/rn-dash/`. None shares structure with the others; each repeats the same pattern of "check file exists → deserialize → on error return empty/None". Identified as accessor-proliferation in RESEARCH §Anti-patterns (lines 200-203).
- **Why it's a problem:** Not broken — each module works — just a cohesion miss. The four modules collectively define "application persistence" but have no shared boundary, so any policy change (e.g. encrypt credentials, swap to sqlite, rotate on every save) must be repeated four times. Minor because the current shape satisfies its callers and the cost of consolidation is not justified by present needs.
- **Recommendation:** Consider either (a) a single `trait domain::ports::PersistencePort { fn load<T: DeserializeOwned>(&self, key: &str) -> anyhow::Result<Option<T>>; fn save<T: Serialize>(&self, key: &str, value: &T) -> anyhow::Result<()>; }` with one infra adapter reading/writing the four known files; or (b) a generic `Repository<T>` helper in infra that each module delegates to. `trait`-based option (a) is the hexagonal-consistent choice. DO NOT action in Phase 13 unless a second persistence need arises — per D-02, Minor findings may be deferred with rationale. Phase 16 (per-worktree tasks) may add persistence for task history and is the right trigger to consolidate.
- **Phase 13 task hint:** Do not action — defer until a new persistence concern lands. When it does, extract `trait PersistencePort` and migrate all four modules behind a single adapter.

### [Minor] F-112: `infra/tmux.rs` is DEPRECATED per its own doc-comment — delete in Phase 13 (NOT Phase 11)
- **Location:** `src/infra/tmux.rs:1-29` (the entire file); re-exported via `src/infra/mod.rs:12` (`pub mod tmux;`)
- **Dimension:** Ousterhout (conjoined-method / dead-code) | Fowler-4-Layer
- **Symptom:** File's own doc-comment at `src/infra/tmux.rs:4` reads "DEPRECATED: Use multiplexer::TmuxAdapter::new_window() instead." The sole public function `open_claude_in_worktree` is marked `#[allow(dead_code)]` (line 14). The comment promises removal once "app.rs OpenClaudeCode action is rewired in Plan 05" — that rewiring has happened (the multiplexer abstraction is live per PROJECT.md v1.0), but the file remains.
- **Why it's a problem:** Dead code erodes trust in the codebase — readers encountering `pub mod tmux;` in `infra/mod.rs` must investigate whether it's still in use. Per RESEARCH Open Question 4, deletion is Phase 13's job (Phase 11 is read-only audit).
- **Recommendation:** `move` the file (figurative delete): remove `src/infra/tmux.rs` entirely; `move` the `pub mod tmux;` line out of `src/infra/mod.rs` (i.e., delete line 12 of mod.rs). Any remaining importers (expected: none — the function is marked `#[allow(dead_code)]`) should be rewired to `infra::multiplexer::TmuxAdapter::new_window()` with a `-d` flag variant if no-focus-switch behavior is still needed. Verify with `rg 'crate::infra::tmux|infra::tmux::' src/` returning no matches before deletion. Explicitly deferred to Phase 13 per D-11 (Phase 11 is audit-only — no src/ modifications allowed).
- **Phase 13 task hint:** Delete `src/infra/tmux.rs`; remove `pub mod tmux;` from `src/infra/mod.rs`; confirm no remaining importers via ripgrep.

## Module: app/
<!-- Coverage: src/app.rs (the single 2,425-LOC file) -->
<!-- Wave 1 Plan 11-03 appends here, INCLUDING D-04 target shapes for Criticals -->

### File Scores

**File:** `src/app.rs` (2,425 LOC — 41% of the codebase)
**Public interface:** `enum FocusedPanel` (+2 methods) + `struct ErrorState` (2 pub fields) + `enum PaletteMode` (5 variants) + `struct AppState` (**39 pub fields**) + `fn active_worktree_id` + `fn active_output` + `fn active_output_scroll` + `fn handle_key` + `fn update` + `async fn run`. 10 pub items + 39 pub struct fields; additionally 7 private async helpers (`spawn_metro_task`, `metro_process_task`, `parse_metro_line`, `extract_percent`, `drain_metro_output`, `stdin_writer`, `metro_http_post`) and `fn dispatch_command` live in the same file.
**Verdict:** **Shallow / God-object** (Critical per D-03 Aggressive rubric)
**Justification:** A single 2,425-LOC file whose `AppState` struct exposes 39 pub fields and whose `update()` function spans lines 538-2061 (~1,520 lines) — the file owns event loop + key dispatch + state mutation + metro lifecycle + command dispatch + modal flow + async I/O runners + log parsing + HTTP POST (≥9 responsibilities, all unrelated). This fails Ousterhout's "deep module" criterion at every joint: the interface is wide (50-field AppState + 5 mega-functions), and functionality is sprawling rather than hidden behind a narrow API. See F-200, F-201, F-202, F-203 below for the Critical findings this score drives.

### Critical

### [Critical] F-200: `app.rs` is a 2,425-LOC god-object with ≥9 unrelated responsibilities (D-03)
- **Location:** `src/app.rs:1-2425`
- **Dimension:** Ousterhout | Fowler-4-Layer
- **Symptom:** A single source file owns all of: event loop (`run()` — lines 2065-2202), key dispatch (`handle_key()` — 260-478), state mutation (`update()` — 538-2061), metro lifecycle (`spawn_metro_task`/`metro_process_task` — 2209-2295), command dispatch (`dispatch_command()` — 485-536), modal flow (ModalInputChar/Backspace/Submit arms + 8 ModalState variants scattered across update()), async I/O runners (7 private async helpers at 2209-2425), log parsing (`parse_metro_line` + `extract_percent` — 2300-2355), and HTTP POST (`metro_http_post` — 2411-2425). `AppState` has **39 pub fields** (verified via `awk '/^pub struct AppState/,/^}/' | grep -c '^\s\+pub '`), every one reachable by every consumer.
- **Why it's a problem:** Fails Ousterhout's deep-module principle at every joint — the interface (39-field struct + 4 mega-functions) is wide; functionality is sprawling rather than hidden behind a narrow API. Per CONTEXT.md D-03 Aggressive calibration, a file handling >5 unrelated responsibilities is Critical. Every reader must load 2,425 lines into working memory to safely modify any single arm; every refactor touches files nothing else uses the structure of. Phase 13 cannot stage refactors atomically against this shape — each of F-201/F-202/F-203 requires a split first.
- **Recommendation:** Per D-04, design the target shape. Concrete proposal — **move** `src/app.rs` into `src/app/` submodule:
  ```
  src/app/
  ├── mod.rs            — re-exports: pub use state::*; pub use runtime::run; (preserve public API)
  ├── state.rs          — struct AppState (with fields grouped into sub-structs per F-209), Default impl,
  │                       pub fn active_worktree_id / active_output / active_output_scroll
  ├── update.rs         — pub fn update(state: AppState, action: Action) -> (AppState, Vec<Effect>)
  │                       (pure — no tokio::spawn; returns effects, see F-201)
  ├── effect_runner.rs  — pub struct EffectRunner { adapters: Adapters, tx: UnboundedSender<Action> }
  │                       impl { pub fn run_effects(&self, effects: Vec<Effect>); }
  │                       — owns the tokio::spawn calls; translates Effect variants into adapter calls
  ├── handle_key.rs     — pub fn handle_key(state: &AppState, key: KeyEvent) -> Option<Action>
  │                       (later: reads from KEYBINDINGS registry per F-208)
  └── runtime.rs        — pub async fn run(terminal) — the event loop; wires up channels,
                          calls handle_key → update → effect_runner.run_effects; holds Adapters.
  ```
  The 7 async metro helpers **move** to `src/infra/metro.rs` (see F-203). The direct `crate::infra::*` imports **move** behind trait objects on an `Adapters` struct (see F-202). Recommendation MUST contain `move` (and does: move app.rs into app/ submodule).
- **Phase 13 task hint:** Split `src/app.rs` into `src/app/{mod.rs,state.rs,update.rs,effect_runner.rs,handle_key.rs,runtime.rs}` preserving public API at module root; stage this split **first** in Phase 13's refactor sequence because F-201/F-202/F-203 all require it.

### [Critical] F-201: `update()` directly invokes `tokio::spawn` 20 times — TEA purity violation (D-03)
- **Location:** `src/app.rs:538-2061` — the `update()` function body; specifically the 20 in-function side-effect call sites: `tokio::spawn` at lines **524** (inside `dispatch_command`), **602, 619, 636, 649, 708, 794, 816, 929, 992, 1101, 1186, 1205, 1862, 1902, 1928, 2041** (17 direct), plus `tokio::task::spawn_blocking` at **1236, 1548, 1678** (3 more).
- **Dimension:** Ousterhout | Fowler-4-Layer
- **Symptom:** `update(state: &mut AppState, action: Action, metro_tx, handle_tx)` directly invokes `tokio::spawn` 17 times and `tokio::task::spawn_blocking` 3 times inline. State mutation is interleaved with task spawning — e.g. `Action::MetroStart` mutates `state.pending_restart`, then spawns external-metro detection; `Action::CommandRun` mutates `state.command_queue`, then spawns device enumeration; `Action::WorktreeRemoveConfirmed` mutates `state.pending_worktree_removal`, then spawns `remove_worktree`. The function is impure (performs I/O dispatch) and cannot be tested without a tokio runtime; effects cannot be replayed, intercepted, logged, or instrumented.
- **Why it's a problem:** Violates The Elm Architecture's pure-update guarantee that CONTEXT.md D-03 evaluates `update()` at face value against. Every update-level test requires a tokio runtime; every effect is fire-and-forget (no way to know when the side effect completed for deterministic testing); no central place to log/intercept effects for debugging; no way to dry-run a state transition. Per D-03 Aggressive calibration, "`update()` performs I/O or holds mutable side effects" — flag Critical.
- **Recommendation:** Per D-04, design the `Effect` enum. Concrete proposal (place at `src/app/effect.rs` or `src/domain/effect.rs`):
  ```rust
  pub enum Effect {
      // Metro lifecycle
      DetectExternalMetro { port: u16 },                       // replaces tokio::spawn at 602
      SpawnMetro { worktree: PathBuf },                        // replaces 619
      MetroHttpPost { url: String, body: String },             // replaces 636, 649 (debugger, reload)
      KillProcess { pid: u32 },                                // replaces 709 (external metro kill)

      // Commands
      SpawnCommand { spec: CommandSpec, cwd: PathBuf,
                     branch: String },                         // replaces 524 (dispatch_command)
      LoadDevices { kind: DeviceKind },                        // replaces 929 (android/iOS list)

      // Worktrees
      ListWorktrees,                                           // replaces 817, 993, 1863, 1903, 2042, 2107
      RemoveWorktree { path: PathBuf },                        // replaces 1101
      AddWorktree { branch: String },                          // replaces 1205
      AddWorktreeNewBranch { new: String, base: String },      // replaces 1186
      ListRemoteBranches,                                      // replaces 1928

      // Persistence
      SaveJiraCache(HashMap<String, String>),                  // replaces 1564 (inline, not spawned)
      SaveAndroidMode(String),                                 // replaces 1170, 1339, 1362, 1392, 1413
      RecordSimUsed(String),                                   // replaces 1678

      // External processes
      OpenInMultiplexer { worktree: PathBuf, name: String,
                          command: String },                   // replaces 1236, 1548

      // JIRA
      FetchJiraTitles { keys: Vec<String> },                   // replaces 708, 794
  }
  pub fn update(state: AppState, action: Action) -> (AppState, Vec<Effect>);
  ```
  The new `effect_runner.rs` from F-200 consumes `Vec<Effect>` and translates each variant into the actual `tokio::spawn` / `spawn_blocking` / direct-call. `update()` becomes pure — testable without a tokio runtime; every effect is a recorded data value; effects can be logged, replayed, or intercepted at the runner boundary. Recommendation MUST contain `enum` (and does: `pub enum Effect`).
- **Phase 13 task hint:** Define `pub enum Effect` (15+ variants) in `src/app/effect.rs`; refactor `update()` signature to `(AppState, Action) -> (AppState, Vec<Effect>)`; implement `effect_runner.rs::EffectRunner::run_effects(Vec<Effect>)` that consumes the effects and performs the `tokio::spawn` calls previously inline in update(). Depends on F-200 (the split must land first).

### [Critical] F-202: `app.rs` depends on concrete `crate::infra::*` modules instead of domain ports — hexagonal dependency inversion violation
- **Location:** `src/app.rs` — 43 direct `crate::infra::*` references (verified via `grep -cE 'crate::infra::' src/app.rs`), including `infra::port::{detect_external_metro, kill_process, port_is_free}` (lines 603, 709, 2122, 2283), `infra::worktrees::{list_worktrees, check_stale_pods, remove_worktree, add_worktree, add_worktree_new_branch, list_remote_branches}` (817, 855, 993, 1002, 1003, 1102, 1187, 1206, 1863, 1903, 1929, 2042, 2107), `infra::devices::{list_android_devices, list_ios_simulators}` (931, 933), `infra::jira::{extract_jira_key, HttpJiraClient}` (748, 785, 1569, 2090), `infra::multiplexer::detect_multiplexer` (1237, 1549, 2079), `infra::config::load_config` (2082), `infra::command_runner::spawn_command_task` (525), `infra::android_prefs::{load_android_mode, save_android_mode}` (207, 1170, 1339, 1362, 1392, 1413), `infra::sim_history::{load_sim_history, record_sim_used}` (1423, 1679), `infra::jira_cache::{load_jira_cache, save_jira_cache}` (1564, 2100), `infra::process::{ProcessClient, TokioProcessClient}` (2214, 2215).
- **Dimension:** Hexagonal | Fowler-4-Layer
- **Symptom:** Service-layer code (app.rs `update()` / `run()`) reaches into Data Source (infra) modules directly. `AppState` holds a concrete `Option<crate::infra::config::DashConfig>` field (line 134), an `Option<std::sync::Arc<dyn crate::infra::jira::JiraClient>>` (120), an `Option<Box<dyn crate::infra::multiplexer::Multiplexer>>` (130). There is no injection point: `run()` constructs concrete adapters inline (`HttpJiraClient::new`, `detect_multiplexer()`, `TokioProcessClient`) rather than receiving them.
- **Why it's a problem:** Inverts Cockburn's hexagonal dependency rule — the app layer should depend on domain-defined traits and receive adapter implementations injected at startup. Currently app.rs transitively pulls in tokio::process, reqwest, lsof-invocation, and every other infra concern. Any app-layer test must fake every infra module separately (no single boundary to swap); any swap of an adapter (e.g. alternative JIRA backend, fake device enumerator for a demo mode) requires edits throughout app.rs, not a single `main.rs` wiring change. This is the same dependency-inversion failure already graded Critical for `command_runner.rs` in Plan 11-02 (F-101), now seen from the consumer side.
- **Recommendation:** Define domain ports for every external dependency app.rs touches — cross-referenced to Plan 11-02's port-extraction findings (F-102 PortProbePort, F-103 ProcessClient, F-104 WorktreePort, F-105 DevicePort, F-106 JiraClient, F-110 Multiplexer, plus F-101 CommandRunnerPort and a new PersistencePort for the four small persistence modules). Introduce an `Adapters` struct that owns all trait objects:
  ```rust
  // src/app/adapters.rs (new)
  pub struct Adapters {
      pub command_runner: Arc<dyn CommandRunnerPort>,
      pub metro: Arc<dyn MetroPort>,                   // see F-203
      pub port_probe: Arc<dyn PortProbePort>,          // Plan 11-02 F-102
      pub worktrees: Arc<dyn WorktreePort>,            // Plan 11-02 F-104
      pub devices: Arc<dyn DevicePort>,                // Plan 11-02 F-105
      pub jira: Option<Arc<dyn JiraPort>>,             // Plan 11-02 F-106
      pub multiplexer: Option<Arc<dyn Multiplexer>>,   // Plan 11-02 F-110
      pub persistence: Arc<dyn PersistencePort>,       // Plan 11-02 F-111
  }
  ```
  `run()` (in `app/runtime.rs` after the F-200 split) constructs the concrete adapters and builds the `Adapters` struct at startup. `update()` + `effect_runner.rs` call methods through the trait objects only — zero `crate::infra::*` imports remain in `src/app/`. Recommendation MUST contain `trait` (and does: every port above is a `trait`).
- **Phase 13 task hint:** After F-200 split and after Plan 11-02 port extractions land, create `src/app/adapters.rs` with the `Adapters` struct; update `run()` in `src/app/runtime.rs` to construct concrete impls once and hold the struct; remove every `crate::infra::*` reference from `src/app/` (verify with `rg 'crate::infra::' src/app/` = 0 matches).

### [Critical] F-203: Async metro helpers (7 functions, 218 LOC) are Data Source code colocated with Service code
- **Location:** `src/app.rs:2209-2425` — `spawn_metro_task` (2209-2256), `metro_process_task` (2259-2295), `parse_metro_line` (2300-2332), `extract_percent` (2336-2355), `drain_metro_output` (2358-2395), `stdin_writer` (2398-2409), `metro_http_post` (2411-2425).
- **Dimension:** Fowler-4-Layer | Hexagonal
- **Symptom:** 218 lines of pure-infra code — tokio process spawning (`ProcessClient::spawn_metro`), raw byte-stream parsing (`BufReader::lines`, SIGKILL via `libc::kill(-PGID, SIGKILL)`), HTTP POST (`reqwest::Client`) — live in the same file as the application Service layer. `spawn_metro_task` imports `crate::infra::process::{ProcessClient, TokioProcessClient}` inline (lines 2214-2215); `metro_process_task` uses `libc::kill` + `tokio::process::Child` directly; `metro_http_post` uses `reqwest` directly. No domain-level metro port exists; update() spawns these helpers directly (lines 619, 636, 649).
- **Why it's a problem:** Mixes Data Source with Service. Forces app.rs to import `tokio::process`, `reqwest`, `libc`, and raw byte-stream parsing into the same file that orchestrates TEA state transitions. The `domain::metro::MetroManager` (which currently holds tokio-typed fields via `MetroHandle` per Plan 11-01 F-004) has no adapter to delegate to — the "adapter" is scattered across 7 free functions in app.rs. Combined with Plan 11-01 F-004 (MetroHandle tokio leak), this is the load-bearing reason metro lifecycle currently spans 3 layers with no clean boundary.
- **Recommendation:** `move` `src/app.rs:2209-2425` into a new file `src/infra/metro.rs` containing a `TokioMetroAdapter` struct implementing a new `trait MetroPort` defined in `src/domain/ports/metro_port.rs`:
  ```rust
  // src/domain/ports/metro_port.rs (new)
  pub struct MetroHandle { /* opaque — replaces Plan 11-01 F-004 tokio-leaking struct */ }
  pub enum MetroActivity { /* re-export or relocate from domain/metro.rs */ }

  pub trait MetroPort: Send + Sync {
      // Starts metro in the worktree; returns when spawn completes; streams activity via tx.
      async fn start(&self, worktree: PathBuf,
                     activity_tx: UnboundedSender<MetroActivity>) -> anyhow::Result<MetroHandle>;
      // Writes a byte buffer to metro's stdin.
      fn send_stdin(&self, handle: &MetroHandle, bytes: Vec<u8>) -> anyhow::Result<()>;
      // Kills the metro process group and waits for port 8081 to free.
      async fn kill(&self, handle: MetroHandle);
      // Sends a control HTTP POST to metro (reload, open-debugger).
      async fn http_post(&self, path: &str, body: &str) -> anyhow::Result<()>;
  }
  ```
  `infra::metro::TokioMetroAdapter` implements the trait by absorbing `spawn_metro_task`, `metro_process_task`, `drain_metro_output`, `stdin_writer`, `metro_http_post`. Pure helpers `parse_metro_line` and `extract_percent` stay with the adapter as private module fns (pure parsers, no I/O — same pattern as Plan 11-02's recommendation for `infra/devices.rs` parsers staying module-private). `app.rs` receives `Arc<dyn MetroPort>` via the `Adapters` struct from F-202. Recommendation MUST contain both `move` and `trait` (and does: move ... to src/infra/metro.rs implementing trait MetroPort).
- **Phase 13 task hint:** Create `src/domain/ports/metro_port.rs` defining `trait MetroPort` (4 methods) + opaque `MetroHandle` + `MetroActivity` enum; create `src/infra/metro.rs::TokioMetroAdapter` implementing the trait by moving the 7 helpers from `app.rs:2209-2425`; keep `parse_metro_line` / `extract_percent` as private helpers in the adapter module; rewire app.rs `update()` to call `adapters.metro.start/send_stdin/kill/http_post` through the trait object. Depends on F-200 (the split) and coordinates with Plan 11-01 F-004 (the trait here replaces the tokio-leaking MetroHandle struct).

### Major

### [Major] F-204: Inline prerequisite/ordering logic scattered across 11 sites in `update()` (ARCH-05)
- **Location:** `src/app.rs:843-887` (sync-before-run modal flow; pod staleness inline at 852-858), `:890` (`spec.needs_metro() && !state.metro.is_running()` — metro-before-RN-run), `:949-953` (RnReleaseBuild → queue AdbInstallApk pipeline), `:956-960` (GitResetHardFetch → queue GitResetHard pipeline), `:1014` (second `needs_metro` check in CommandExited drain), `:1463-1499` (sync-before-metro on worktree switch; auto_sync fast-path at 1467-1478), `:1622-1635` (CleanConfirm multi-step sequence assembly — cocoapods, android, node_modules, sync_after), `:1684-1705` (SyncBeforeRunAccept sequence), `:1713` (third `needs_metro` check in SyncBeforeRunDecline), `:1722-1753` (SyncBeforeMetroAccept sequence), `:657-674` (MetroExited auto-restart via pending_restart flag; skip_external_metro_check gating at 669), `:594-599` (MetroStart skip_external_metro_check consumption). Plus **five boolean flag fields** coordinating multi-step flows across **45 in-file references** (verified via `grep -cE 'pending_restart|pending_switch_path|pending_metro_run|pending_metro_after_sync|skip_external_metro_check' src/app.rs`): `pending_restart`, `pending_switch_path`, `pending_metro_run`, `pending_metro_after_sync`, `skip_external_metro_check`. Plus `state.command_queue: VecDeque<CommandSpec>` (line 90) as a sixth ad-hoc sequencing mechanism.
- **Dimension:** Prerequisite-Placement | Fowler-4-Layer
- **Symptom:** 11 distinct locations in `update()` encode command prerequisite/ordering rules inline (sync-before-run, sync-before-metro, metro-before-run, GitFetch-then-GitResetHard, RnReleaseBuild-then-AdbInstall, CleanConfirm multi-command sequencing, auto-restart after MetroStop, skip-external-detection-during-restart). Six ad-hoc coordination mechanisms (5 boolean flags + VecDeque) implement what is logically a single domain pipeline type. Rule knowledge (which commands depend on which others, in what order) is scattered across `update()` arms — not encoded as domain data.
- **Why it's a problem:** Domain orchestration logic (command dependency graph) lives in the Service layer (app.rs::update) instead of the Domain layer. Cannot test prerequisite rules without app/runtime context. Every new RN-like command that needs metro-first must touch the SyncBeforeRun arm, the MetroStart arm, the SyncBeforeRunDecline arm, and the needs_metro check inline — four edits for one rule. This is the single clearest ARCH-05 violation in the codebase; per REQUIREMENTS.md REFACTOR-03 ("introduce a domain-level command-prerequisite representation"), Phase 13 must resolve it.
- **Recommendation:** Introduce domain types encoding prerequisites and recipes. Per RESEARCH §"Recommended target shape for D-04":
  ```rust
  // src/domain/prerequisite.rs (new)
  pub enum Prerequisite {
      MetroRunning,
      DependenciesFresh { yarn: bool, pods: bool },
  }
  impl CommandSpec {
      pub fn prerequisites(&self) -> Vec<Prerequisite>;  // replaces needs_metro inline checks
  }

  // src/domain/recipe.rs (new)
  pub enum Recipe {
      Single(CommandSpec),
      Sequence(Vec<CommandSpec>),
      Clean(CleanOptions),
      SyncThenRun(CommandSpec),     // replaces SyncBeforeRun flow
      SyncThenStartMetro,           // replaces SyncBeforeMetro flow
      ReleaseBuildAndInstall,       // replaces RnReleaseBuild → AdbInstall
      GitFetchThenReset,            // replaces GitResetHardFetch → GitResetHard
  }
  impl Recipe {
      pub fn expand(&self, state: &DependencyState) -> Vec<CommandSpec>;
  }
  ```
  Dispatcher reads from `Recipe::expand` instead of inline conditionals across the 11 sites. The five coordinating boolean flags collapse into the Recipe variant's data. Phase 13 picks either (a) a full prerequisite-graph model or (b) this Recipe enum per REFACTOR-03's latitude. Recommendation MUST contain `enum` (and does: `pub enum Prerequisite`, `pub enum Recipe`).
- **Phase 13 task hint:** Implement REFACTOR-03 by introducing `Prerequisite` + `Recipe` domain types per the sketch above; replace the 11 inline ordering sites in `update()` with `Recipe::expand()` consumers; collapse `pending_restart` / `pending_switch_path` / `pending_metro_run` / `pending_metro_after_sync` / `skip_external_metro_check` into Recipe variant data. Depends on F-200 (update() must be pure before this is tractable).
- **Cross-reference to Plan 11-05:** Plan 11-05 enumerates each of these 11 locations row-by-row with every variant touched in its `## Cross-Cutting Findings > Misplaced prerequisite/ordering logic (ARCH-05)` section.

### [Major] F-205: Catch-all match arms in `app.rs` drop inputs without exhaustive coverage (ARCH-04)
- **Location:** `src/app.rs:441` (handle_key WorktreeTable focused — `_ => {}`), `:461` (handle_key CommandOutput focused — `_ => {}`), `:1140` (ModalInputChar fall-through — `_ => {}`), `:1153` (ModalInputBackspace fall-through — `_ => {}`), `:2153` (run() event-loop Mouse/Paste/Focus fall-through — `_ => {}`). Plus **16 `_ => None` arms** in handle_key modal dispatchers (lines 274, 281, 292, 301, 306, 311, 316, 325, 344, 351, 362, 373, 380, 389, 397, 476) and the `_ => Some(Action::ModalCancel)` arms (344, 351, 362, 373, 380). Plus broader arms at 915 (`_ => "Input:".to_string()` in TextInput prompt builder) and 1418 (`_ => { ... multi-line modal DevicePicker creation }`).
- **Dimension:** Catch-All
- **Symptom:** 5 literal `_ => {}` arms + 16 `_ => None` / `_ => Some(ModalCancel)` arms + 2 wider-body catch-alls. The `Action` enum (src/action.rs) has ~55 variants; the `ModalState` enum has 8 variants (`Confirm`, `TextInput`, `DevicePicker`, `CleanToggle`, `SyncBeforeRun`, `SyncBeforeMetro`, `ExternalMetroConflict`, `BranchPicker`). The two modal-input catch-alls at 1140 and 1153 silently drop typing events for five of those eight ModalState variants — a future modal type that should accept character input would be silently ignored because the compiler does not force the author to consider the new variant.
- **Why it's a problem:** Per RESEARCH §"Why this matters more than it seems" — Action enum has ~55 variants and several update() arms have implicit assumptions about which ModalState variant is active when a key arrives. Future variant additions can be silently swallowed. Handle_key's 16 `_ => None` arms collectively document nothing about WHICH keys are intentionally unhandled versus accidentally missed. Of the 5 `_ => {}` literals, lines 1140 and 1153 are the most load-bearing because they gate user input based on a non-exhaustive ModalState match.
- **Recommendation:** For ModalInputChar / ModalInputBackspace catch-alls at 1140/1153 — `replace _ => {}` with **explicit named arms covering each ModalState variant** (`Some(ModalState::Confirm {..}) | Some(ModalState::CleanToggle {..}) | Some(ModalState::SyncBeforeRun {..}) | Some(ModalState::SyncBeforeMetro {..}) | Some(ModalState::ExternalMetroConflict {..}) | Some(ModalState::BranchPicker {..}) | None => { /* intentionally ignore — modal does not accept char input */ }`). Rust's exhaustiveness check then guards future ModalState additions. For the 5 literal `_ => {}` arms at 441/461/2153 and the 16 `_ => None` in handle_key, document the propagation policy in a doc-comment on `handle_key` itself ("unhandled keys return None so callers can compose key dispatchers"). Recommendation MUST contain `replace _ =>` (and does: `replace _ => {} with explicit named arms`).
- **Phase 13 task hint:** `replace _ =>` literal arms at `src/app.rs:1140` and `:1153` with exhaustive ModalState enumeration; for handle_key `_ => None` arms, add a doc-comment on handle_key documenting the "return None for anything unhandled" policy; for `:441`, `:461`, `:2153` event-loop arms, add `// intentionally unhandled` comments with the event categories (Mouse/Paste/Focus).
- **Cross-reference to Plan 11-05:** Plan 11-05 enumerates each catch-all arm row-by-row with full variant breakdown in its `## Cross-Cutting Findings > Catch-all match arms (ARCH-04)` section.

### [Major] F-208: `handle_key` is one of three keybinding definition sites — no single source of truth (D-14)
- **Location:** `src/app.rs:260-478` — the entire `handle_key` function body, which encodes the canonical `(KeyCode → Action)` mapping (e.g. `Char('q') => Some(Action::Quit)`, `Char('?') | F(1) => Some(Action::ShowHelp)`, `Char('m') => Some(Action::MetroStart)` in various contexts). The other two sites are `src/ui/footer.rs::key_hints_for` (encodes the same keybindings as footer hint strings — key labels + short descriptions) and `src/ui/help_overlay.rs::render_help` (encodes the same keybindings as a help-table of key labels + long descriptions). Three sources of truth that must agree manually.
- **Dimension:** Ousterhout (overexposure / knowledge duplication) | Hexagonal
- **Symptom:** The keybinding knowledge — which key triggers which Action, in which context, with what description — is duplicated across three files in three different encodings: `handle_key` knows `KeyCode → Action`; `footer.rs::key_hints_for` knows key label + short description; `help_overlay.rs::render_help` knows key label + long description. Adding a new keybinding requires three coordinated edits in three different styles. Per RESEARCH §"Drift evidence" (confirmed during context-gathering): the Yarn-palette `c` key currently has three slightly different descriptions across the three sites.
- **Why it's a problem:** D-14 CONTEXT.md directive (folded from the 2026-03-11 keybindings todo) mandates the auditor explicitly evaluate this for source-of-truth violations. The violation is confirmed: three sites encode the same information redundantly, with observed drift. Future keybinding additions will drift further. New team members must read all three files to understand which keys are actually bound. The single-source keybinding registry requested by the original todo is concretely justified by this F-208 finding.
- **Recommendation:** Introduce a single keybinding registry. Concrete sketch:
  ```rust
  // src/keybindings.rs (root) or src/app/keybindings.rs (after F-200 split)
  pub enum BindingContext {
      Always, NormalMode, WorktreeTable, CommandOutput,
      Modal(ModalKind), Palette(PaletteMode),
  }
  pub struct KeyBinding {
      pub key: KeyCode,
      pub label: &'static str,       // e.g. "c", "?", "F1"
      pub short_desc: &'static str,  // footer hint text
      pub long_desc: &'static str,   // help overlay description
      pub context: BindingContext,
      pub action: fn(&AppState) -> Option<Action>,
  }
  pub const KEYBINDINGS: &[KeyBinding] = &[ /* ~60 entries covering every key currently in handle_key */ ];

  pub fn handle_key(state: &AppState, key: KeyEvent) -> Option<Action>;
  pub fn footer_hints_for(state: &AppState) -> Vec<(&'static str, &'static str)>;
  pub fn help_overlay_rows() -> Vec<(&'static str, &'static str)>;
  ```
  All three call sites (`handle_key`, `footer.rs::key_hints_for`, `help_overlay.rs::render_help`) read from `KEYBINDINGS` — drift becomes impossible. Recommendation MUST contain `struct` (and does: `pub struct KeyBinding`) and `move` (the three current encoding sites' data **move** into the single KEYBINDINGS table).
- **Phase 13 task hint:** Create `src/app/keybindings.rs` (after F-200 split) containing the `KeyBinding` struct + `KEYBINDINGS` registry with ~60 entries; refactor `handle_key` to scan the registry filtered by `BindingContext`; refactor `footer.rs::key_hints_for` and `help_overlay.rs::render_help` to project rows from the same registry. The three sites now all read from one source of truth. Depends on F-200 (so new registry can live at `src/app/keybindings.rs`).
- **Cross-reference to Plan 11-04 and Plan 11-05:** Plan 11-04 captures the `footer.rs` and `help_overlay.rs` definition sites; Plan 11-05 finalizes the unified D-14 finding in `## Cross-Cutting Findings > Keybinding source-of-truth (D-14)` with the full cross-file evidence and the unified recommendation.

### [Major] F-209: `AppState` exposes 39 pub fields — Ousterhout "Overexposure" anti-pattern
- **Location:** `src/app.rs:61-163` — the `AppState` struct definition. Verified field count: 39 pub fields (via `awk '/^pub struct AppState/,/^}/' src/app.rs | grep -c '^\s\+pub '`).
- **Dimension:** Ousterhout (overexposure red flag)
- **Symptom:** Every field of AppState is `pub`, including 5 coordinating boolean flags (pending_restart, pending_switch_path, pending_metro_run, pending_metro_after_sync, skip_external_metro_check), 3 pending-operation slots (pending_device_command, pending_claude_open, pending_android_mode, pending_worktree_removal, pending_worktree_add, pending_new_branch_base, pending_new_branch_worktree — actually 7), metro-related fields (metro, active_worktree_path, skip_external_metro_check), worktree browser state (worktrees, worktree_table_state, selected_worktree_id, fullscreen_panel), command state (command_queue, command_output_by_worktree, command_output_scroll_by_worktree, running_command, command_task), modal state (modal), configuration (repo_root, palette_mode, config, jira_title_cache, jira_client, jira_project_prefix, multiplexer, claude_flags, android_mode), and first-press tracking (pending_g). Every consumer of AppState can read every field.
- **Why it's a problem:** Per Ousterhout's "Overexposure" red flag — "interface forces the caller to learn implementation internals." `update()` arms that touch only metro state still see all 39 fields; `handle_key` that only needs modal + palette_mode + pending_g + focused_panel + metro.is_running() sees everything. Any refactoring of AppState breaks every reader because the struct shape is the public API. The 7 `pending_*` fields are a particularly clear code smell: they implement an ad-hoc coordination protocol that would be better encoded as the Recipe variants in F-204.
- **Recommendation:** Group fields into sub-structs with narrower public API. Concrete proposal:
  ```rust
  // src/app/state.rs (after F-200 split)
  pub struct AppState {
      pub focused_panel: FocusedPanel,
      pub show_help: bool,
      pub error_state: Option<ErrorState>,
      pub should_quit: bool,
      pub metro_state: MetroState,
      pub worktree_browser: WorktreeBrowserState,
      pub command_runner: CommandRunnerState,
      pub modal_stack: ModalStackState,
      pub pending: PendingFlags,     // shrinks to 0 after F-204 Recipe lands
      pub config: AppConfigState,
  }
  pub struct MetroState { pub metro: MetroManager, pub active_worktree_path: Option<PathBuf> }
  pub struct WorktreeBrowserState { pub worktrees: Vec<Worktree>, pub table_state: TableState,
                                    pub selected_worktree_id: Option<WorktreeId>,
                                    pub fullscreen_panel: Option<FocusedPanel>, ... }
  pub struct CommandRunnerState { pub command_queue: VecDeque<CommandSpec>,
                                   pub output_by_worktree: HashMap<...>,
                                   pub running_command: Option<CommandSpec>, ... }
  // ... etc
  ```
  Sub-structs are pub; inner fields become `pub(crate)` (visible within the `app/` module only, not to external consumers). Readers of `AppState` declare intent via which sub-struct they touch; refactoring `CommandRunnerState` cannot break `modals.rs`. Recommendation MUST contain `struct` (and does: multiple `pub struct` sub-types).
- **Phase 13 task hint:** Group AppState's 39 pub fields into 6-7 sub-structs (`MetroState`, `WorktreeBrowserState`, `CommandRunnerState`, `ModalStackState`, `PendingFlags`, `AppConfigState`); make sub-structs pub but narrow inner fields to `pub(crate)`. Stage after F-200 (fields move with the split) and coordinate with F-204 (the `PendingFlags` sub-struct empties out once Recipe lands).

### Minor

### [Minor] F-206: Recursive `update()` self-dispatch calls scattered across 7 arms — temporal decomposition
- **Location:** `src/app.rs:590` (MetroStart calls `update(state, Action::MetroStop, ...)`), `:670` (MetroExited calls `update(state, Action::MetroStart, ...)`), `:673` (MetroExited calls `update(state, Action::RefreshWorktrees, ...)`), `:893` (CommandRun calls `update(state, Action::MetroStart, ...)`), `:1474` (WorktreeSwitchToSelected calls `update(state, Action::MetroStop, ...)`), `:1491` (same arm, different branch), `:1497` (same arm), `:1715` (SyncBeforeRunDecline calls `update(state, Action::MetroStart, ...)`), `:1732` (SyncBeforeMetroAccept calls `update(state, Action::MetroStop, ...)`), `:2140` (runtime periodic-refresh calls `update(state, Action::RefreshWorktrees, ...)`), `:2149` (handle_key result dispatches via update), `:2157` (metro_rx.recv dispatches via update), `:2161` (handle_rx.recv dispatches via update), `:2170` (drain loop dispatches via update).
- **Dimension:** Ousterhout (temporal decomposition) | Fowler-4-Layer
- **Symptom:** `update()` recursively calls itself at least 7 distinct call sites within its own body to chain sequential actions (MetroStart → MetroStop → MetroExited → MetroStart again). This is temporal decomposition — the arm breaks are dictated by execution order ("first stop metro, then start metro, then refresh worktrees") rather than responsibility. The recursion is necessary today because update() is the only place that knows how to fan out effects; once F-201 lands, these recursive calls become `vec![Effect::..., Effect::...]` returns instead.
- **Why it's a problem:** Temporal decomposition is one of Ousterhout's named anti-patterns. Readers tracing a single user action (e.g. "press Enter on worktree row") must follow the recursion three hops deep (WorktreeSwitchToSelected → MetroStop → MetroExited → MetroStart → MetroStartConfirmed) to see the full effect chain. Minor because the fix falls out naturally from F-201 (the Effect enum makes the chain declarative rather than recursive — each arm returns `vec![Effect::StopMetro, Effect::ScheduleAction(Action::MetroStart, after_delay)]`).
- **Recommendation:** No direct action. Resolution follows from F-201 — once `update()` returns `(AppState, Vec<Effect>)`, the 7+ recursive self-dispatch sites collapse into effect returns. The `ScheduleAction { action, after }` variant in F-201's Effect enum handles the "after metro exits, dispatch MetroStart" case. Optional `enum Effect::Chain(Vec<Effect>)` variant can model explicit ordering if needed.
- **Phase 13 task hint:** Do not action independently — fix is absorbed by F-201 implementation. Verify during F-201 rollout that every recursive `update(...)` call becomes an `Effect::` return.

### [Minor] F-207: Metro debugger/reload/port-kill logic owns a private in-file `metro_http_post` instead of going through a port
- **Location:** `src/app.rs:636-643` (MetroSendDebugger — `metro_http_post("http://localhost:8081/open-debugger", "{}")`), `:649-654` (MetroSendReload — `metro_http_post("http://localhost:8081/reload", "")`), `:2411-2425` (`metro_http_post` definition).
- **Dimension:** Hexagonal | Ousterhout (colocated-concern)
- **Symptom:** Two Action arms call `metro_http_post` — a private async fn in the same file that uses `reqwest::Client` directly — to drive metro's HTTP control endpoints. The URL is hard-coded to `http://localhost:8081/`. Combined with F-203, this is the HTTP-POST leaf of the metro-helpers-colocated-with-service finding, called out separately because it's the specific case app.rs imports `reqwest` for.
- **Why it's a problem:** Metro control HTTP is a domain-level concept ("tell metro to reload", "tell metro to open debugger") that the hexagonal MetroPort should own — but it's currently a private helper in app.rs. Consumers of MetroSendDebugger / MetroSendReload see `tokio::spawn(metro_http_post(...))` at the call site, leaking the HTTP mechanism through the Service layer.
- **Recommendation:** Absorbed by F-203 — the `MetroPort::http_post(path, body)` method in the proposed trait owns this. `metro_http_post` `move`s into `infra/metro.rs::TokioMetroAdapter`; app.rs calls `adapters.metro.http_post("/open-debugger", "{}")`.
- **Phase 13 task hint:** Do not action independently — resolved by F-203 implementation.

### [Minor] F-210: Config loading inlined at startup in `run()` — not a real problem, listed for completeness
- **Location:** `src/app.rs:2079-2100` — multiplexer detection, config load, jira_client construction, jira_title_cache load all inline in `run()`.
- **Dimension:** Ousterhout (acceptable shape; listed per D-13 completeness)
- **Symptom:** 22 lines of startup wiring live inline at the top of `run()` — config deserialization, config→state field extraction (claude_flags, jira_project_prefix, repo_root), JIRA client construction, cache hydration. Not broken; just one of the few legitimate uses of direct `crate::infra::*` access in app.rs (because this is where the ports-injection decision happens — F-202 uses this same code path as the injection point).
- **Why it's a problem:** Not a problem today. Listed for completeness because D-13 asks for every Ousterhout-score block to note structural observations, and this is the startup-wiring section that F-202's `Adapters` construction will occupy after Phase 13.
- **Recommendation:** Leave as-is for Phase 11. After F-200/F-202 land in Phase 13, this code moves into `src/app/runtime.rs::run` as the Adapters-construction block (documented at that file's head).
- **Phase 13 task hint:** Do not action. Resolved structurally by F-200 + F-202.

## Module: ui/
<!-- Coverage: src/ui/{mod,panels,footer,help_overlay,error_overlay,modals,theme}.rs -->
<!-- Wave 1 Plan 11-04 appends here, plus initial keybinding evidence (D-14) -->

### File Scores

**File:** `src/ui/panels.rs` (267 LOC)
**Public interface:** 3 pub fns — `render_title_bar`, `render_worktree_table`, `render_command_output` (per `rg '^pub (fn|struct|enum|trait|const)' src/ui/panels.rs`)
**Verdict:** OK on rendering; contains the single UI→infra hexagonal leak in the codebase.
**Justification:** Three render functions that take `&AppState` and draw ratatui widgets — narrow interface relative to the 267 LOC of table/detail-row/scrollbar logic. The load-bearing concern is the import at line 71: `crate::infra::jira::extract_jira_key` — a pure domain helper misplaced in infra, reached into directly by the Presentation layer. See F-300 below; this is the other side of Plan 11-02 F-107 (same function, viewed from the UI side).

**File:** `src/ui/footer.rs` (161 LOC)
**Public interface:** 1 pub fn — `render_footer` (`key_hints_for` is private)
**Verdict:** Shallow — narrow interface but the implementation is a giant hand-coded keybinding table duplicating data owned by `app.rs::handle_key`.
**Justification:** One pub render fn dispatches to `key_hints_for` (lines 29-161) — 5 palette hint tables (lines 40-79), 8 modal hint tables (lines 84-118), and panel hint tables (lines 124-153). ~70 `(key, label)` tuples total, all plain strings with no reference to `Action` or any canonical binding type. This is D-14 keybinding definition site #2; handle_key is site #1 (Plan 11-03 F-208), help_overlay is site #3 (F-303 below). See F-302 for the UI-side D-14 evidence.

**File:** `src/ui/help_overlay.rs` (137 LOC)
**Public interface:** 1 pub fn — `render_help` (`centered_rect` is private)
**Verdict:** Shallow — narrow interface but the implementation is a ~55-row hand-coded `Vec<Row>` duplicating keybinding strings from footer + handle_key.
**Justification:** One pub render fn. Lines 17-112 hand-code the help table with section headers (Navigation, Worktree Table, Android a>, iOS i>, Yarn y>, Git g>, Worktree w>, Output Pane, Icons) and `Row::new(vec!["key", "description"])` rows — no iteration, no data source beyond the literal strings. This is D-14 keybinding definition site #3. See F-303 for the finding; Plan 11-05 finalizes the unified cross-cutting D-14 finding referencing all three sites.

**File:** `src/ui/modals.rs` (376 LOC)
**Public interface:** 1 pub fn — `render_modal` (8 private per-modal renderers + private `centered_rect`)
**Verdict:** Deep — single dispatch entry point hiding 8 modal renderers behind one function.
**Justification:** Textbook Ousterhout depth applied to UI. One pub fn, one match on `ModalState`, eight private renderers (`render_confirm_modal`, `render_text_input_modal`, `render_device_picker_modal`, `render_clean_modal`, `render_sync_prompt`, `render_sync_before_metro`, `render_external_metro_modal`, `render_branch_picker_modal`). Imports are strictly `ratatui::*` + `crate::domain::command::ModalState` + (via full-path) `crate::domain::command::{DeviceInfo, CleanOptions, CommandSpec}`. Verified `rg 'crate::app|crate::infra' src/ui/modals.rs` → no matches. Narrow interface, deep functionality, clean layer discipline. No finding required.

### Critical

### Major

### [Major] F-300: `ui/panels.rs:71` calls `crate::infra::jira::extract_jira_key` directly — UI→infra hexagonal leak
- **Location:** `src/ui/panels.rs:71` (call site inside `render_worktree_table`); transitive import via full-path at the call (no `use crate::infra::jira;` at file top — the `use crate::{app::..., domain::worktree::..., ui::theme}` block at lines 12-16 is clean, so the only infra reach is the fully-qualified call at line 71)
- **Dimension:** Hexagonal | Fowler-4-Layer
- **Symptom:** Presentation layer calls `crate::infra::jira::extract_jira_key(branch, &state.jira_project_prefix)` to derive a display string. `extract_jira_key` is a pure function (no I/O, six inline tests in `infra/jira.rs`) — so the runtime is benign — but the dependency direction is wrong: Presentation should not reach Data Source.
- **Why's a problem:** Per `ui/mod.rs:2` doc-claim ("Imports: domain types and ratatui ONLY. Never imports infra directly."), this is a self-acknowledged contract violation. Per hexagonal: Presentation → Data Source is the one edge the architecture forbids. Per Fowler 4-layer: pure business-vocabulary helpers (parse a branch name → ticket key) live in Domain, not Data Source.
- **Recommendation:** Coordinate with Plan 11-02 F-107 (same function, infra side): `move extract_jira_key from src/infra/jira.rs to src/domain/worktree.rs` (or a new `src/domain/jira.rs`); update `src/ui/panels.rs:71` to import from domain instead (`use crate::domain::worktree::extract_jira_key;`). No behavior change — pure function move + import rewrite. This is the UI-side of a symmetric two-line fix.
- **Phase 13 task hint:** When implementing Plan 11-02 F-107's move of `extract_jira_key` to domain, also update the import at `src/ui/panels.rs:71` to reference the new domain path; verify afterwards with `! rg 'crate::infra' src/ui/`.

### [Major] F-301: `ui/mod.rs` doc-claim contradicts actual imports — self-documented but unenforced layer contract
- **Location:** `src/ui/mod.rs:1-2` (doc-comment: "UI layer — ratatui widgets, rendering, layout. Imports: domain types and ratatui ONLY. Never imports infra directly.") vs `src/ui/panels.rs:71` (calls `crate::infra::jira::extract_jira_key`)
- **Dimension:** Hexagonal | Fowler-4-Layer
- **Symptom:** `ui/mod.rs` documents the layer's import contract in a module-level doc-comment. `rg 'crate::infra' src/ui/` returns a single hit at `panels.rs:71` that violates it. The contract is declared but not enforced (no CI check, no arch_test fitness function — arch_test_core is out of scope per REQUIREMENTS.md).
- **Why's a problem:** Self-documented contracts that aren't enforced create false confidence — readers of `ui/mod.rs` assume the layer is clean, and reviewers of new ui code anchor on the existing comment rather than regrepping. Per Pitfall 2 (§Common Pitfalls): "the comment IS the finding" when the trade-off is documented but drift occurs.
- **Recommendation:** Preferred path: fix the code, not the doc — land F-300 (`move extract_jira_key from src/infra/jira.rs to src/domain/worktree.rs` + rewrite the `ui/panels.rs:71` import) so the doc-claim holds. After the move, add a verifying grep to any CI/local-lint step: `! rg 'crate::infra' src/ui/` must pass. Do NOT revise the doc to accept the violation — that weakens the layer boundary.
- **Phase 13 task hint:** After moving `extract_jira_key` per F-300 / Plan 11-02 F-107, verify `ui/mod.rs` doc-claim holds via `! rg 'crate::infra' src/ui/`; add the grep to the project's pre-commit or CI lint step if a cheap one exists.

### [Major] F-302: `ui/footer.rs::key_hints_for` is D-14 keybinding definition site #2 — duplicates handle_key + help_overlay as plain strings
- **Location:** `src/ui/footer.rs:29-161` (private `key_hints_for` function). Palette hint tables: lines 40-79 (Android, iOS, Yarn, Git, Worktree). Modal hint tables: lines 84-118 (Confirm, TextInput, DevicePicker, CleanToggle, SyncBeforeRun, SyncBeforeMetro, ExternalMetroConflict, BranchPicker). Panel hint tables: lines 124-153 (WorktreeTable, CommandOutput).
- **Dimension:** Ousterhout (knowledge duplication) | D-14
- **Symptom:** `key_hints_for` encodes ~70 `(key, label)` tuples covering the same keybinding space as `app.rs::handle_key` (lines 260-478) and `ui/help_overlay.rs::render_help` (lines 17-112) — but as plain strings with no reference to the `Action` enum. Drift evidence: the Yarn palette entry at `footer.rs:60` reads `("c", "clean…")`; `help_overlay.rs:72` reads `"c" → "Clean… (select targets: pods, android, node_modules)"`; `app.rs:360` reads `Char('c') => Some(Action::OpenCleanMenu)` — three sources, three different descriptions. Also: WorktreeTable `R` is conditionally `MetroSendReload | RefreshWorktrees` in handle_key (lines 421-427), but `footer.rs:135` only writes `"reload"` — drops the conditional.
- **Why's a problem:** D-14 mandates a single source of truth for keybindings. Three hand-maintained sites guarantee drift over time — evidence confirms it already has. Adding a new palette key requires three edits in three different styles; missing one produces a silent UI/help divergence.
- **Recommendation:** Cross-reference Plan 11-03 F-208 (`handle_key` side of this finding) and its concrete `struct KeyBinding` + `KEYBINDINGS` registry sketch. Concrete fix: `move footer key-hint generation to read from a single src/keybindings.rs registry (the struct from F-208)`; `replace the 5 palette hint tables + 8 modal hint tables + 2 panel hint tables with a single iteration over KEYBINDINGS filtered by current-context (PaletteMode / ModalState / FocusedPanel)`. Recommendation directly aligned with Plan 11-05's unified D-14 finalization.
- **Phase 13 task hint:** After Phase 13 implements the `KeyBinding` / `KEYBINDINGS` registry per F-208, refactor `footer.rs::key_hints_for` to derive hints by iterating `KEYBINDINGS` filtered by current `AppState` context (palette / modal / panel). Delete all 15 inline hint tables once the registry is the sole source.
- **Cross-reference:** See Plan 11-05 §Cross-Cutting Findings → Keybinding source-of-truth (D-14) for the unified finding listing all three definition sites — `handle_key` (F-208), `footer.rs` (F-302, this finding), `help_overlay.rs` (F-303 below) — and the consolidated registry design.

### [Major] F-303: `ui/help_overlay.rs::render_help` is D-14 keybinding definition site #3 — hand-coded `Vec<Row>` duplicating handle_key + footer
- **Location:** `src/ui/help_overlay.rs:17-112` (`keybindings: Vec<Row>` inside `render_help`). Sections: Navigation (lines 19-24), Worktree Table (28-42), Android a> (46-51), iOS i> (55-60), Yarn y> (64-73), Git g> (77-86), Worktree w> (90-95), Output Pane (99-104), Icons (108-111). ~55 `Row::new(vec!["key", "description"])` rows total.
- **Dimension:** Ousterhout (knowledge duplication) | D-14
- **Symptom:** `render_help` hand-codes a ratatui `Vec<Row>` with keybindings duplicated from `app.rs::handle_key` + `ui/footer.rs::key_hints_for` as plain strings. Drift evidence: WorktreeTable `R` is correctly captured here as "Reload metro (when running) / Refresh list" (line 40), while footer (line 135) only writes "reload" — the two UI sites disagree even though both describe the same key. Yarn `c` at line 72 reads "Clean… (select targets: pods, android, node_modules)" vs footer's "clean…" (line 60).
- **Why's a problem:** Same as F-302 — D-14 single-source-of-truth violation. This is the longest and most detailed of the three sites, making it the most expensive to keep in sync by hand. Help-overlay drift is user-visible (users press `?` to see the authoritative mapping) in a way footer drift is not.
- **Recommendation:** Cross-reference Plan 11-03 F-208's `KeyBinding` registry. Concrete fix: `move help-overlay row generation to read from src/keybindings.rs::help_overlay_rows() — a helper that groups KEYBINDINGS by section and emits Vec<Row> with section headers`. Replace lines 17-112's hand-coded `Vec<Row>` with `let keybindings = keybindings::help_overlay_rows();`. Section headers derived from `BindingContext::section()` on the registry entries. Recommendation aligns with Plan 11-05's unified D-14 finalization.
- **Phase 13 task hint:** After Phase 13 implements the `KeyBinding` / `KEYBINDINGS` registry per F-208, refactor `help_overlay.rs::render_help` to call `keybindings::help_overlay_rows()` instead of hand-coding the `Vec<Row>`. Keep the Icons section as a separate hand-coded block (it documents display semantics, not key bindings).
- **Cross-reference:** See Plan 11-05 §Cross-Cutting Findings → Keybinding source-of-truth (D-14) for the unified finding. The three definition sites `handle_key` (F-208) + `footer.rs` (F-302) + `help_overlay.rs` (F-303, this finding) collapse into a single consumer of the `KEYBINDINGS` registry under the Phase 13 refactor.

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
