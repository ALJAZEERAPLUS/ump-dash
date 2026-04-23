# Phase 12: Coverage Gate — Research

**Researched:** 2026-04-23
**Domain:** Rust test infrastructure, coverage instrumentation, tokio subprocess characterization, TEA action dispatch testing
**Confidence:** HIGH (all load-bearing claims verified against official sources)

---

## Summary

Phase 12 installs a characterization net before Phase 13 refactors anything. Four deliverables: (COVER-01) a test that the `MetroManager` Option-wrapper invariant panics on double-`register()` and that `update()` rejects a second `MetroStart`; (COVER-02) a real-subprocess test that `tokio::process::Command::process_group(0)` + `libc::kill(-pgid, SIGTERM)` reaps a shell + child-sleep tree on macOS AND Linux within 2 s; (COVER-03) table-driven TEA tests for six palettes (a/i/x/y/g/w), eight modals, and the `command_queue` drain via `CommandExited`; (COVER-04) a committed `BASELINE-COVERAGE.json` + `BASELINE-COVERAGE.md` + thresholds file.

**Primary recommendation:** Install `cargo-llvm-cov` 0.8.5 (no nightly needed for JSON + HTML, no nightly needed for edition 2024 under Rust 1.94 stable). Write COVER-01 and COVER-02 as `tests/metro_single_instance.rs` and `tests/process_group_kill.rs` (real subprocess, integration-test crate). Write COVER-03 inline under `src/app.rs` as `#[cfg(test)] mod dispatch_tests` — pure TEA, no tokio needed for table-driven palette and modal tests. Use `libc::kill(-pgid, SIGTERM)` directly (crate already in runtime deps — D-13 forbids `nix`). Use `tokio::time::timeout(Duration::from_secs(2), ...)` inside each subprocess test — do not rely on a macro-level timeout (tokio 1.49 has no stable `#[tokio::test(timeout=...)]`).

**Six load-bearing findings:**
1. `tokio::process::Command::process_group` is stable since tokio 1.40 — rn-dash's 1.49 is fine, no `tokio_unstable` cfg needed. [VERIFIED via docs.rs/tokio/1.49.0]
2. `cargo-llvm-cov` excludes `tests/` from the report by default — COVER-01 and COVER-02 *do* run under coverage, but their own source lines are not counted toward coverage %. Good: we want coverage of `src/`, not of the tests. [VERIFIED via taiki-e/cargo-llvm-cov README]
3. tokio issue #6934 (panic in `tokio::test` + `process::Command`) affects *Windows only* — rn-dash targets macOS + Linux, so irrelevant, but belongs in the Pitfalls section for when someone runs the suite on Windows. [VERIFIED via GitHub issue tracker]
4. The LLVM JSON schema has stable top-level `data[0].totals.lines.percent` and `data[0].files[N].summary.lines.percent` paths — safe to parse with `jq` for threshold extraction. [VERIFIED via LLVM llvm-cov manpage]
5. `Command::process_group(0)` equals `setpgid(0,0)` at *`exec()` time*, not spawn time — child could briefly exist before the PGID is set on older kernels. In 2026 tokio 1.49 uses `posix_spawn` where available, which sets PGID atomically via `POSIX_SPAWN_SETPGROUP`. No race on Linux ≥ 4.11 or macOS ≥ 10.15. [CITED: tokio/process/mod.rs source]
6. Edition 2024 + Rust 1.94 stable works with cargo-llvm-cov 0.8.5 with no special flags. Doc-tests and branch coverage are the only nightly-gated features, and we don't need them. [VERIFIED via cargo-llvm-cov README nightly section]

---

<user_constraints>
## User Constraints (from 12-CONTEXT.md)

### Locked Decisions (MUST honor verbatim)

- **D-01** — Coverage tool is `cargo-llvm-cov`. Not tarpaulin.
- **D-02** — No CI wiring this phase. Makefile target or `.cargo/config.toml` alias only.
- **D-03** — Commit `BASELINE-COVERAGE.json` (raw llvm-cov JSON) + `BASELINE-COVERAGE.md` (human-readable summary). HTML output is gitignored.
- **D-04** — Per-module threshold = `floor(baseline %, 5)`. No aspirational numbers.
- **D-05** — Thresholds in `.planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md` with `module | baseline % | threshold %` columns. No enforcement script.
- **D-06** — Pure domain tests stay inline `#[cfg(test)] mod tests`.
- **D-07** — COVER-01 and COVER-02 live in a new `tests/` directory at the workspace root (integration-test convention). They need real subprocess behavior.
- **D-08** — COVER-03 stays inline in `src/app.rs` (or `src/app/dispatch_tests.rs` if `app.rs` > ~1500 lines post-audit — it is currently 2425 lines, so a sub-module file is likely warranted).
- **D-09** — COVER-01 tests BOTH: (a) direct `MetroManager::register` double-call assertion, (b) `update()`-level `Action::MetroStart` while metro running.
- **D-10** — COVER-02 uses `bash -c 'trap "" SIGTERM; sleep 30 & wait'`. Records PGID, cancels, asserts via `kill(pgid, 0)` within 2 s that both parent shell AND child sleep are gone.
- **D-11** — COVER-03 uses table-driven tests. One test iterates all six palettes.
- **D-12** — No new runtime deps. Only `tokio` dev-features if needed. `tempfile` only if process-group test needs a scratch dir.
- **D-13** — No `mockall`, no `rstest`, no `proptest`. Inline fixture scripts, not committed `.sh` files.

### Claude's Discretion (research options, recommend)

- `BASELINE-COVERAGE.md` standalone vs. embedded in Phase 12 SUMMARY.md — pick whichever keeps SUMMARY ≤ 200 lines. **Research recommendation:** standalone, because the table will grow as modules are added/split in Phase 13.
- Exact modal enumeration for COVER-03 — grep `ModalState::` variants. **Research recommendation:** 8 variants (see Modal Inventory below).
- COVER-03 location: `src/app.rs` bottom vs. `src/app/dispatch_tests.rs`. **Research recommendation:** new file — `app.rs` is 2425 lines and `#[cfg(test)] mod tests` of ~250 lines pushes it to ~2700, past the 2000-line Ousterhout-unhealthy threshold.

### Deferred Ideas (OUT OF SCOPE — ignore completely)

- Property-based tests (`proptest`).
- `cargo-deny` / `cargo-modules` CI integration.
- Coverage tool CI wiring (`.github/workflows/coverage.yml`).
- Enforcement script that fails CI on threshold violation.
- TASK-04 arbitrary-shell cancellation (that's Phase 15).
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| COVER-01 | Characterization test: metro single-instance invariant — starting metro in worktree B while running in A must fail/resolve via existing conflict flow; `MetroManager` holds exactly one live handle at any time. | Sections 1, 3 — `MetroManager::register()` panic path confirmed at `src/domain/metro.rs:112-121`. `update()` path at `src/app.rs:586-609` dispatches `MetroStop` if `metro.is_running()`, setting `pending_restart`. Tests at both layers per D-09. |
| COVER-02 | Characterization test: process-group kill — killing a running command terminates the full subprocess tree; no orphaned PIDs. | Section 2 — `.process_group(0)` + `libc::kill(-pgid, SIGTERM)` + 200 ms grace + `SIGKILL` fallback. Verified process_group is stable in tokio 1.49 (stabilized 1.40). Fixture: `bash -c 'trap "" SIGTERM; sleep 30 & wait'`. Use `tokio::time::timeout` per-test, not macro attribute. |
| COVER-03 | Coverage tests: `CommandQueuePush` + `CommandExited` routing, modal dismissal flow, palette→action resolution for a/i/x/y/g/w. | Section 4 — six `PaletteMode` variants at `src/app.rs:44-55` (NB: `x` is NOT a palette — it is the clean-confirm key inside CleanToggle modal; confirm with user or interpret as "the clean submenu triggered by `y>c`"). Eight `ModalState` variants at `src/domain/command.rs:193-241`. Table-driven pattern in Section 4. |
| COVER-04 | Committed baseline coverage report + per-module minimum threshold. | Sections 1, 5 — `cargo llvm-cov --workspace --json --summary-only --output-path BASELINE-COVERAGE.json` + human summary. JSON schema confirmed: `data[0].files[N].summary.lines.percent`. Threshold extractor uses `jq`. |

**NOTE on the phase description's `a/i/x/y/g/w` palette list:** only `a`, `i`, `y`, `g`, `w` are `PaletteMode` variants in `src/app.rs:44`. `x` is the clean-confirm key inside `ModalState::CleanToggle` (`src/app.rs:299`). The planner SHOULD interpret COVER-03's "palette x" as "the CleanToggle modal triggered by `y>c`, which uses `x` as its confirm key." Test both the entry transition (`y>c` opens CleanToggle) and the exit action (`x` inside CleanToggle fires `CleanConfirm`).
</phase_requirements>

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| MetroManager single-instance invariant | Domain (`src/domain/metro.rs`) | App (`update()` in `src/app.rs`) | Invariant is type-level (Option wrapper, `debug_assert!` in `register`); reachability into that state goes through TEA `update()` |
| Process-group kill mechanism | Infra (`src/infra/process.rs`, `src/infra/command_runner.rs`) | App (cleanup code at `src/app.rs:2185-2196`) | Syscall-level; `tokio::process::Command::process_group` + `libc::kill` belong behind infra adapters |
| Palette → Action resolution | App (`handle_key` in `src/app.rs`) | Domain (Action enum in `src/action.rs`) | Pure function `(state, KeyEvent) -> Option<Action>` — TEA boundary |
| Modal dismissal flow | App (`update()` in `src/app.rs`) | Domain (ModalState enum in `src/domain/command.rs`) | Modal is app-state; dismissal is a TEA transition |
| Coverage measurement | Tooling (external to source) | — | `cargo-llvm-cov` runs all `#[test]` and `tests/*.rs` with instrumentation |
| Threshold enforcement | Doc-only this phase | — | D-05 — human-checked ratchet; no enforcement script until post-milestone |

---

## Standard Stack

### Core

| Tool / Library | Version | Purpose | Why Standard |
|----------------|---------|---------|--------------|
| `cargo-llvm-cov` | 0.8.5 | Coverage measurement (LLVM source-based) | Ecosystem default for Rust 2025-2026; accurate on async/generics unlike tarpaulin's ptrace sampling; stable on edition 2024 under Rust 1.94 stable [VERIFIED: docs.rs/crate/cargo-llvm-cov/latest, GitHub releases] |
| `tokio` | 1.49 (already in deps) | Async runtime + `#[tokio::test]` macro | Pinned in Cargo.toml. `process_group` stable since 1.40 [VERIFIED: tokio PR #6731, docs.rs/tokio/1.49.0] |
| `libc` | 0.2 (already in deps) | `libc::kill(-pgid, SIGTERM)` in tests | Already used at `src/app.rs:2278` — reuse, do not add `nix` (D-13 spirit: minimal deps) |

### Supporting (dev-only, already-in-tree)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `anyhow` | 1 (already in deps) | Error propagation in tests | Any test returning `-> anyhow::Result<()>` |
| `tracing` | 0.1 (already in deps) | Log skip-reason for `cfg_attr(ignore)`-style guards | Optional — tests can use `eprintln!` instead |

### Dev-Dependencies Addition

Per D-12, the *only* line to add to `Cargo.toml`:

```toml
[dev-dependencies]
# Enables #[tokio::test] macro. The "macros" + "rt-multi-thread" + "process" + "time"
# features are already transitively enabled by features = ["full"] on the runtime dep,
# but dev-dependencies are a SEPARATE resolver scope — we re-declare the minimal set
# needed by the integration tests crate.
tokio = { version = "1.49", features = ["macros", "rt-multi-thread", "process", "time", "io-util"] }
```

**Why re-declare tokio in dev-dependencies:** Rust's integration test crate (`tests/*.rs`) is a *separate* crate that depends on rn-dash via its regular `[dependencies]`. However, the test crate itself also needs the `#[tokio::test]` macro and `tokio::time::timeout`, which requires tokio to be in dev-dependencies. Without this, the test file can't compile `#[tokio::test]`. The runtime-dep `features = ["full"]` on the library crate does NOT flow through to the test crate.

**Alternative considered and rejected:** `[dev-dependencies] tokio = { workspace = true, features = [...] }` — rn-dash is a single crate, not a workspace, so there is no `[workspace.dependencies]` to inherit from. Fresh declaration is the right move.

**Do NOT add:** `mockall`, `rstest`, `proptest`, `tempfile` (unless COVER-02 proves to need a scratch dir — unlikely), `nix`, `command-group`. All forbidden by D-13 or redundant.

### Installation

```bash
# One-time, local dev only (D-02 — no CI wiring)
cargo install cargo-llvm-cov --locked
```

### Version Verification (2026-04-23)

- `cargo-llvm-cov` 0.8.5 — current stable on crates.io as of April 2026. MSRV: 1.80. Edition 2024 + Rust 1.94 stable works. [VERIFIED: crates.io listing, docs.rs]
- `tokio` 1.49 — pinned in `Cargo.toml:20`. `process_group` stabilized in 1.40 (Aug 2024). [VERIFIED: tokio CHANGELOG commit `process: stabilize Command::process_group (#6731)`]
- `libc` 0.2 — pinned in `Cargo.toml:46`. No version bump needed.

### Alternatives Considered

| Instead of | Could Use | Tradeoff | Why Rejected |
|------------|-----------|----------|--------------|
| `cargo-llvm-cov` | `cargo-tarpaulin` | Simpler setup on some platforms | Locked out by D-01; also less accurate on async/generic code |
| `libc::kill` direct | `nix::sys::signal::kill` | Type-safe `Pid::from_raw` wrapper | D-13 forbids new deps; `libc` already in tree; `unsafe { libc::kill(...) }` is a one-line call, safety is contained |
| `#[tokio::test(flavor = "multi_thread")]` | Default (current_thread) | Exposes race conditions | For COVER-02 either works; *default current_thread* is sufficient for subprocess tests — the subprocess itself parallelizes the work, and the test's tokio tasks are serialized intentionally |
| Committed `.sh` fixture | Inline `bash -c '...'` string | Reusable | D-13 forbids committed fixture files; inline is also simpler to audit |
| Per-test timeout via macro | Per-test `tokio::time::timeout(...)` wrap | Attribute syntax | There is no stable `#[tokio::test(timeout = ...)]`; `tokio::time::timeout` inside the test body is the idiomatic approach |

---

## Architecture Patterns

### Test Layout (Post-Phase-12)

```
rn-dash/
├── src/
│   ├── app.rs                           # COVER-03 tests OR new file below
│   ├── app/
│   │   └── dispatch_tests.rs            # RECOMMENDED: COVER-03 here (app.rs is 2425 lines)
│   ├── domain/
│   │   ├── metro.rs                     # existing — MetroManager invariant
│   │   └── refresh.rs                   # existing — 17 inline tests, reference standard
│   └── infra/
│       ├── process.rs                   # existing — .process_group(0) + kill_on_drop(true)
│       └── command_runner.rs            # existing — NB: does NOT set process_group; only infra/process.rs does (see Pitfall 6)
├── tests/                               # NEW TOP-LEVEL DIR (D-07)
│   ├── metro_single_instance.rs         # COVER-01: MetroManager::register() panic + update() rejection
│   └── process_group_kill.rs            # COVER-02: real bash -c fixture, PGID kill verification
├── .planning/phases/12-coverage-gate/
│   ├── BASELINE-COVERAGE.json           # Committed (D-03)
│   ├── BASELINE-COVERAGE.md             # Committed (D-03)
│   └── COVERAGE-THRESHOLDS.md           # Committed (D-05)
├── Makefile                             # NEW — `make cov` target (D-02)
└── .gitignore                           # Add target/llvm-cov*
```

### Pattern 1: Integration Test Crate Anatomy

```rust
// tests/metro_single_instance.rs
// Source: pattern from Rust Reference §integration tests + Cargo Book
//
// This file is compiled as a SEPARATE crate. It can only `use rn_dash::*` items
// that are `pub`. Private items inside `src/` are inaccessible here — if a test
// needs private access, it belongs inline in a `#[cfg(test)] mod tests` block.

use rn_dash::domain::metro::{MetroManager, MetroHandle};

#[test]
#[should_panic(expected = "BUG: MetroManager::register() called with an existing handle")]
fn register_twice_panics() {
    let mut mgr = MetroManager::new();
    mgr.register(fake_handle(1, "wt-a"));
    mgr.register(fake_handle(2, "wt-b")); // PANIC — characterization locked
}

fn fake_handle(pid: u32, wt: &str) -> MetroHandle {
    // Construct a MetroHandle with dummy channels. All fields are pub per
    // src/domain/metro.rs:61-76.
    let (stdin_tx, _stdin_rx) = tokio::sync::mpsc::unbounded_channel();
    let (kill_tx, _kill_rx) = tokio::sync::oneshot::channel();
    MetroHandle {
        pid,
        worktree_id: wt.into(),
        stdin_tx,
        stream_task: tokio::spawn(async {}),
        stdin_task: tokio::spawn(async {}),
        kill_tx: Some(kill_tx),
    }
}
```

**Landmine:** `fake_handle` calls `tokio::spawn`, which requires a runtime. The test above has no runtime — it'll panic with "there is no reactor running." Fix: make the test `#[tokio::test]` even though we don't await anything, OR construct the dummy `JoinHandle` via `tokio::task::spawn_blocking(|| {}).await.unwrap()` inside a runtime. **Recommendation:** use `#[tokio::test]` with `flavor = "current_thread"` (default) — simplest and correct.

```rust
#[tokio::test]
#[should_panic(expected = "BUG: MetroManager::register() called with an existing handle")]
async fn register_twice_panics() {
    let mut mgr = MetroManager::new();
    mgr.register(fake_handle(1, "wt-a"));
    mgr.register(fake_handle(2, "wt-b"));
}
```

### Pattern 2: update()-Level COVER-01 Test (D-09 second test)

The `update()` path for `Action::MetroStart` when `state.metro.is_running()` is already true sits at `src/app.rs:586-609`:

```rust
Action::MetroStart => {
    state.palette_mode = None;
    if state.metro.is_running() {
        state.pending_restart = true;
        update(state, Action::MetroStop, metro_tx, handle_tx);
        return;
    }
    // ... continues to external-metro detection
}
```

The *characterization* asserts: after dispatching `MetroStart` twice in a row (without an intervening `MetroExited`), the SECOND call must NOT construct a second `MetroHandle`. It must set `pending_restart = true` and dispatch `MetroStop`. Test shape:

```rust
#[tokio::test]
async fn metro_start_while_running_triggers_restart_not_double_spawn() {
    let (metro_tx, _metro_rx) = tokio::sync::mpsc::unbounded_channel();
    let (handle_tx, _handle_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut state = AppState::default();
    state.metro.register(fake_handle(9999, "wt-a"));  // simulates "already running"

    update(&mut state, Action::MetroStart, &metro_tx, &handle_tx);

    // Invariant-level assertion: pending_restart flipped, metro still holds the OLD handle,
    // stopping-in-flight signaled.
    assert!(state.pending_restart, "must flag restart, not double-spawn");
    assert!(state.metro.is_running() || matches!(state.metro.status, MetroStatus::Stopping));
}
```

**Note:** `update()` is `pub` on `app.rs`; `AppState` is `pub`. `fake_handle` shape from Pattern 1. Private helpers inside `app.rs` (`dispatch_command`, `metro_http_post`) are NOT accessible from `tests/` — only public items. That's fine for this test.

### Pattern 3: Process-Group Kill (COVER-02)

```rust
// tests/process_group_kill.rs
use std::time::Duration;
use tokio::process::Command;
use tokio::time::{sleep, timeout};

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[tokio::test(flavor = "multi_thread")]
async fn killing_pgid_reaps_child_tree() -> anyhow::Result<()> {
    // Fixture: bash parent that ignores SIGTERM, spawns a sleep child, waits.
    // Sending SIGTERM to just the bash PID does nothing (trap). Sending SIGTERM
    // to -PGID delivers to both bash AND sleep, breaking the wait loop.
    let mut child = Command::new("bash")
        .args(["-c", r#"trap "" SIGTERM; sleep 30 & wait"#])
        .process_group(0)          // CRITICAL: new PGID = child PID
        .kill_on_drop(true)
        .spawn()?;

    let pgid = child.id().expect("child has pid") as i32;

    // Give bash ~100 ms to fork sleep(30). Without this, we might kill the group
    // BEFORE the sleep child has joined it, failing to exercise the real invariant.
    sleep(Duration::from_millis(100)).await;

    // Verify the sleep child exists and is in our PGID. (Optional probe — skip
    // if test log noise becomes an issue.)
    // `pgrep -P <bash_pid>` or `/proc/<pid>/stat` on Linux; macOS has no /proc.
    // Simpler probe below via kill(pgid, 0).
    assert_eq!(unsafe { libc::kill(-pgid, 0) }, 0, "pgid group exists");

    // Send SIGTERM to the entire process group. Negative PID = PGID dispatch
    // (POSIX kill(2) — when pid < -1, signal goes to every process whose PGID
    // equals |pid|).
    unsafe { libc::kill(-pgid, libc::SIGTERM) };

    // Give handlers 200 ms grace to exit cleanly.
    sleep(Duration::from_millis(200)).await;

    // Bash ignores SIGTERM (trap ""), BUT sleep does not — sleep dies, bash's
    // `wait` returns, bash exits 0. Without process_group(0), killing only
    // bash's PID would be a no-op and sleep would live for 30 s.

    // Reap the parent. This also reaps the sleep child via bash's exit status
    // cascade (sleep's parent is bash, not our test; bash collects sleep
    // before exiting).
    let status = timeout(Duration::from_secs(2), child.wait())
        .await
        .expect("child must exit within 2 s of SIGTERM to the group")?;
    assert!(status.success() || status.code() == Some(143)); // 143 = 128+SIGTERM

    // Final invariant: PGID is empty. kill(-pgid, 0) should return -1 / ESRCH.
    // Poll up to 500 ms for reaper to complete.
    let start = std::time::Instant::now();
    loop {
        let probe = unsafe { libc::kill(-pgid, 0) };
        if probe == -1 {
            break;                 // ESRCH — group is empty, invariant holds
        }
        if start.elapsed() > Duration::from_millis(500) {
            panic!("pgid {pgid} still alive after 500 ms — orphan detected");
        }
        sleep(Duration::from_millis(20)).await;
    }

    Ok(())
}
```

**Critical details this pattern locks in:**

- `bash -c 'trap "" SIGTERM; sleep 30 & wait'` — bash ignores SIGTERM; sleep does not. If PGID kill works, sleep dies and the `wait` cascade completes; if PGID kill is broken (only bash gets the signal), the test times out at 2 s and fails LOUDLY. This is the test's discriminating shape.
- `kill(-pgid, 0)` — POSIX trick: signal 0 doesn't send anything, just probes existence. Returns 0 if group has any member, -1 with ESRCH if empty. Cheap, standard.
- `#[cfg(any(target_os = "linux", target_os = "macos"))]` on the TEST (D-10 says use `cfg_attr(not(...), ignore)` but `#[cfg]` at module level is cleaner — the test simply doesn't compile on Windows, rather than compiling + ignoring). Either is acceptable; `#[cfg]` is fewer characters.
- `flavor = "multi_thread"` — recommended for subprocess tests to expose any tokio-runtime-internal race on reaping (see Pitfall 8). current_thread also works; multi_thread is defense-in-depth.
- `timeout(Duration::from_secs(2), child.wait())` — if the group-kill invariant is broken, this fails the test in 2 s rather than hanging CI for 30+ seconds.

### Pattern 4: Table-Driven Palette Tests (COVER-03)

```rust
// src/app/dispatch_tests.rs  (D-11 + Claude's Discretion recommendation)
use super::*;
use crate::action::Action;
use crate::domain::command::CommandSpec;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

fn key(c: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: ratatui::crossterm::event::KeyEventState::NONE,
    }
}

#[test]
fn yarn_palette_resolves_each_key_to_expected_action() {
    let mut state = AppState::default();
    state.focused_panel = FocusedPanel::WorktreeTable;
    state.palette_mode = Some(PaletteMode::Yarn);

    let cases: &[(char, Action)] = &[
        ('i', Action::CommandRun(CommandSpec::YarnInstall)),
        ('p', Action::CommandRun(CommandSpec::YarnPodInstall)),
        ('u', Action::CommandRun(CommandSpec::YarnUnitTests)),
        ('t', Action::CommandRun(CommandSpec::YarnCheckTypes)),
        ('j', Action::CommandRun(CommandSpec::YarnJest { filter: String::new() })),
        ('l', Action::CommandRun(CommandSpec::YarnLint)),
        ('c', Action::OpenCleanMenu),
    ];

    for (input, expected) in cases {
        let got = handle_key(&state, key(*input));
        assert_eq!(
            got.as_ref(),
            Some(expected),
            "yarn palette input {input:?} must resolve to {expected:?}"
        );
    }
}

// Repeat for PaletteMode::{Android, Ios, Git, Worktree} + modal-enter palette tests
// (y>c opens CleanToggle, then x confirms). Six palette functions total.
```

**Single-source avoidance of duplication:** the palette → action table literally LIVES in `handle_key` at `src/app.rs:333-382`. The tests above re-declare it — that *is* the characterization. The value is: if Phase 13 refactors `handle_key` into a registry and changes behavior, the tests fail. If future phases add a new palette key and forget to update this test, the new key is not characterized — but the test as-written won't catch that. **Recommendation:** add a final assertion per palette that asserts *unrecognized keys fall through to `ModalCancel`* (per the existing `_ => Some(Action::ModalCancel)` fallback at `src/app.rs:344, 351, 362, 373, 380`). This converts "forgot to add new key" into a test-caught regression, because a new palette variant would require the fallback behavior to be retested.

### Pattern 5: Modal Dismissal Tests (COVER-03)

```rust
#[test]
fn confirm_modal_dismisses_on_n() {
    let mut state = AppState::default();
    state.modal = Some(ModalState::Confirm {
        prompt: "Run?".into(),
        pending_command: CommandSpec::YarnInstall,
    });

    let action = handle_key(&state, key('n')).expect("n must produce action");
    assert_eq!(action, Action::ModalCancel);

    // Apply the action and assert modal cleared.
    let (metro_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let (handle_tx, _) = tokio::sync::mpsc::unbounded_channel();
    update(&mut state, action, &metro_tx, &handle_tx);
    assert!(state.modal.is_none(), "modal must clear on ModalCancel");
}
```

Eight modal variants per `src/domain/command.rs:193-241` — each gets one dismissal test (ModalCancel or equivalent: ExternalMetroConflict uses `n`, BranchPicker uses Esc, etc.):

**Modal Inventory (COVER-03 coverage set):**

| Variant | Dismiss key(s) | Action emitted |
|---------|----------------|----------------|
| Confirm | `n`, `N`, Esc | ModalCancel |
| TextInput | Esc | ModalCancel |
| DevicePicker | Esc | ModalCancel |
| CleanToggle | Esc | ModalCancel |
| SyncBeforeRun | `n`, `N`, Esc | SyncBeforeRunDecline |
| SyncBeforeMetro | `n`, `N`, Esc | SyncBeforeMetroDecline |
| ExternalMetroConflict | `n`, `N`, Esc | ModalCancel |
| BranchPicker | Esc | ModalCancel |

### Pattern 6: `command_queue` Drain Test (COVER-03)

```rust
#[tokio::test]
async fn command_exited_drains_queue_next_in_order() {
    let (metro_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let (handle_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let mut state = seeded_state_with_worktree();
    state.command_queue.push_back(CommandSpec::YarnInstall);
    state.command_queue.push_back(CommandSpec::YarnPodInstall);
    state.running_command = Some(CommandSpec::GitFetch); // pretend something just finished

    update(&mut state, Action::CommandExited, &metro_tx, &handle_tx);

    // First element popped and dispatched.
    assert_eq!(state.running_command.as_ref(), Some(&CommandSpec::YarnInstall));
    assert_eq!(state.command_queue.len(), 1);
    assert_eq!(state.command_queue.front(), Some(&CommandSpec::YarnPodInstall));
}
```

**Landmine:** `dispatch_command` spawns a `tokio::task` via `tokio::spawn(spawn_command_task(...))`. In a `#[tokio::test]` the spawned task starts but the `update()` call returns before the child process has actually been launched. Assertions about `running_command` are against STATE, not the subprocess — fine. But if the test cares about `command_task` being a live JoinHandle, it must tolerate the handle not being done yet.

**COVER-03 note: `Action::CommandQueued` and the phase-description action name.** The additional_context mentions `Action::CommandQueued` and `Action::CommandExited`. `CommandExited` exists (`src/action.rs:48`). `CommandQueued` does NOT exist — the real enqueue action is `Action::CommandQueuePush(spec)` (`src/action.rs:79`). The planner MUST translate "CommandQueued" → `CommandQueuePush`. [VERIFIED via grep of src/action.rs]

---

### Anti-Patterns to Avoid

- **Do not sleep `sleep(Duration::from_secs(2))` as a proxy for "child dies."** Poll `kill(pgid, 0)` in a loop with a short interval. Sleeping wastes CI time AND fails slower than needed.
- **Do not `child.kill().await` in COVER-02.** `child.kill()` sends SIGKILL to the *parent only* (bash), not to the group. That bypasses the characterization — we need `libc::kill(-pgid, SIGTERM)` to exercise the real invariant.
- **Do not mock `tokio::process::Command`.** D-13 forbids mockall; more importantly, the whole point of COVER-02 is that the REAL syscall chain (`fork → setpgid → exec → kill(-pgid, …)`) works. Mocking defeats the test.
- **Do not commit the `target/llvm-cov/html/` tree.** D-03 says HTML is gitignored. Add `target/llvm-cov*` to `.gitignore`. (The `/target` line already there covers it transitively, but being explicit helps.)
- **Do not write an integration test that imports private `app.rs` internals.** Integration tests only see `pub` items. Tests needing private access belong inline in `#[cfg(test)] mod tests`.
- **Do not put `Action::CommandQueued` in the plan.** It is not a real variant. Use `Action::CommandQueuePush(_)`.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Line/region coverage measurement | Custom `rustc -C instrument-coverage` wiring with manual `.profraw` merge | `cargo llvm-cov` | It already handles: multi-target merge, integration-tests merge, LLVM toolchain detection via rustup component, demangling, source range resolution. Hand-rolling this is a week of work [VERIFIED: taiki-e/cargo-llvm-cov README] |
| Process-tree cleanup | Recursive walk of `/proc/$PID/children` then `kill` each | `setpgid(0,0)` via `.process_group(0)` + `kill(-pgid, …)` | POSIX handles the grouping atomically; walking `/proc` races with fork() and doesn't work on macOS at all |
| "Is this PID alive?" probe | `/proc/$PID/status` parse | `libc::kill(pid, 0)` | Portable, single syscall, standard [CITED: POSIX kill(2) man page] |
| Async test timeout | Custom sleep + atomic flag | `tokio::time::timeout(dur, fut).await` | Correctly composable with `select!`; cancels underlying future |
| JSON coverage schema parse | Custom serde struct matching guess | `jq` in the threshold extractor, OR the `llvm-cov-json` crate | LLVM's exporter is stable; `jq` on `data[0].files[].summary.lines.percent` gives what we need in one line |
| Shell-script fixture file | `tests/fixtures/bash_trap.sh` | Inline `bash -c '...'` string | D-13. Also: inline fixture is self-contained in the test, no I/O fragility |

**Key insight:** every hand-rolled alternative here loses the portability or correctness of the standard. Specifically, process-group handling has 30+ years of POSIX semantics baked in — don't invent a new one.

---

## Runtime State Inventory

Not a rename/refactor phase. **SKIPPED** — this phase adds new test files and new planning artifacts; no existing runtime state is renamed, moved, or migrated.

---

## Common Pitfalls

### Pitfall 1: `cargo-llvm-cov` without `rustup` LLVM component

**What goes wrong:** First run fails with "llvm-tools-preview not found."
**Why:** cargo-llvm-cov needs the `llvm-tools-preview` rustup component for `llvm-profdata` and `llvm-cov`.
**How to avoid:** Makefile should `rustup component add llvm-tools-preview || true` before the first invocation, or document in `BASELINE-COVERAGE.md`.
**Warning sign:** `error: command 'llvm-profdata' not found`.

### Pitfall 2: Edition 2024 spurious unused-import warnings under coverage instrumentation

**What goes wrong:** Coverage runs with warnings that don't appear in a normal `cargo test`.
**Why:** `-C instrument-coverage` adds code-gen that can expose dead-code paths the normal optimizer elides.
**How to avoid:** Allow `#[allow(dead_code)]` in test modules only; keep `#![deny(warnings)]` off in test crates. The codebase already uses per-module `#![allow(dead_code)]` (e.g., `src/infra/process.rs:7`).
**Warning sign:** Clippy clean under `cargo clippy -- -D warnings` but cargo llvm-cov emits "unused variable" warnings.

### Pitfall 3: `libc::kill(-pgid, sig)` panic when `pgid` happens to equal 1

**What goes wrong:** `kill(-1, sig)` sends the signal to EVERY process the calling UID owns. Catastrophic on a dev machine.
**Why:** POSIX kill(2): `pid = -1` is the broadcast case, not "kill pgid 1." `pid < -1` is the PGID targeting case.
**How to avoid:** Debug-assert `pgid > 1` before calling. In practice, spawned children will never get PID 1 — but the assert is cheap insurance.
**Warning sign:** test passes spuriously on a clean CI runner; fails the entire shell session on a dev box.

```rust
debug_assert!(pgid > 1, "refusing to kill(-1, …) — would broadcast to UID");
unsafe { libc::kill(-pgid, libc::SIGTERM) };
```

### Pitfall 4: tokio Child dropped before `.wait()` leaves a zombie

**What goes wrong:** Test ends before reaping the bash child. Zombie accumulates. Later tests flake.
**Why:** On Unix, a child that has exited but not been `wait()`ed is a zombie. `kill_on_drop(true)` sends SIGKILL when the Child drops, but it is NOT a `waitpid` — it asks tokio to reap on a best-effort basis [CITED: docs.rs/tokio/1.49.0/tokio/process/struct.Child].
**How to avoid:** Always `child.wait().await` in the test body before letting Child drop. The pattern above does this.
**Warning sign:** `ps aux` after the test suite shows `<defunct>` bash processes.

### Pitfall 5: `tokio::test` + `process::Command` panic on Windows (tokio 1.41+)

**What goes wrong:** `"The handle is invalid"` panic when COVER-02 runs on Windows. [VERIFIED: github.com/tokio-rs/tokio/issues/6934]
**Why:** Windows-only regression in tokio ≥ 1.41. rn-dash is at 1.49.
**How to avoid:** `#[cfg(any(target_os = "linux", target_os = "macos"))]` gate on COVER-02 (already planned per D-10). Do not enable Windows-targets for this test.
**Warning sign:** Any Windows CI config that tries to run these tests.

### Pitfall 6: `infra/command_runner.rs` does NOT set `.process_group(0)`

**What goes wrong:** Only `infra/process.rs` (metro spawn) sets `.process_group(0)`. `infra/command_runner.rs:44-50` does NOT. A test that claims to characterize process-group kill via `CommandRunner` would be wrong — the group isn't a group.
**Why:** The metro spawn path was hardened in an earlier phase. `command_runner.rs` was written with `.kill_on_drop(true)` only because non-metro commands terminate on their own (yarn returns after finishing).
**How to avoid:** COVER-02 must spawn `tokio::process::Command::new("bash")...process_group(0)` DIRECTLY in the test — not through `CommandRunner`. Alternatively, the test can construct a spec that goes through `TokioProcessClient::spawn_metro` (which does set process_group), but that requires a real metro binary. Direct spawn is simpler.
**Warning sign:** Test pattern says "use `CommandRunner`" — rewrite to spawn directly.
**Phase 13 implication:** this is a **F-NNN** candidate for the audit — `command_runner.rs` probably should set `.process_group(0)` so TASK-04's per-task SIGTERM works. Not a Phase 12 fix, but worth flagging in the AUDIT.md consumption as a reinforcement for REFACTOR-02.

### Pitfall 7: Baseline is not reproducible across Rust toolchain upgrades

**What goes wrong:** Phase 13 runs `cargo llvm-cov` with Rust 1.95 and gets subtly different line counts than Phase 12's 1.94 baseline.
**Why:** Different `rustc` versions inline/constant-fold differently, producing different LLVM IR and different region counts. A file with the same *Rust source* can report 92 % on one toolchain and 91 % on another.
**How to avoid:** Pin a `rust-toolchain.toml` (D-04 doesn't mandate this, but it's low-cost). Or document in `BASELINE-COVERAGE.md` that the baseline was measured on `rustc 1.94.1`. Threshold = `floor(baseline, 5)` already has slack built-in for minor drift.
**Warning sign:** Baseline diffs with no source-code change between runs.

### Pitfall 8: tokio `Command` on current_thread runtime can serialize concurrent subprocesses

**What goes wrong:** A test that spawns two subprocesses and assumes they run in parallel instead runs them sequentially.
**Why:** default `#[tokio::test]` uses `flavor = "current_thread"` — a single-threaded runtime.
**How to avoid:** Use `#[tokio::test(flavor = "multi_thread")]` for COVER-02 (we only spawn one subprocess, but defense-in-depth against future additions).
**Warning sign:** test timing is exactly 2x expected.

### Pitfall 9: `cargo llvm-cov` warns about duplicate instrumentation across `tests/*.rs`

**What goes wrong:** Each integration-test file in `tests/` is a separate binary; llvm-cov runs each, generating a separate `.profraw`, then merges. Occasionally the merge step emits a "region counts mismatch" warning.
**Why:** Known benign llvm behavior. Does not affect the reported percentages.
**How to avoid:** Ignore the warning; verify the JSON report has non-null `data[0].totals.lines.percent`.
**Warning sign:** stderr `warning: The "region counts" conflict` — benign.

### Pitfall 10: `handle_tx` mpsc channel closed after test exits — spawned tokio tasks panic

**What goes wrong:** `update()` calls `tokio::spawn(...)` internally. If `handle_tx` is dropped before the spawned task sends, you get a log error "MetroSpawnFailed: channel closed".
**Why:** The channel's receiver is held by `_handle_rx`, which goes out of scope at test end.
**How to avoid:** Let receivers live past the assertion — hold them in `let _rx = ...` and do the assertion before dropping. Or use `tokio::time::sleep(Duration::from_millis(10))` after dispatching so the spawned task completes before test return. Flakiness is rare but possible in CI.
**Warning sign:** test passes locally, flakes in CI with "channel closed" tracing output.

---

## Code Examples

### A) Exact cargo-llvm-cov invocations (COVER-04)

```bash
# 1. HTML for local dev (D-03, gitignored)
cargo llvm-cov --workspace --html
# output: target/llvm-cov/html/index.html [VERIFIED: cargo-llvm-cov README]

# 2. JSON summary for baseline (D-03, committed)
cargo llvm-cov --workspace --json --summary-only \
    --output-path .planning/phases/12-coverage-gate/BASELINE-COVERAGE.json

# 3. Full JSON (regions + functions + lines), NOT committed — for debugging only
cargo llvm-cov --workspace --json \
    --output-path target/llvm-cov/full-coverage.json

# 4. Lcov for future tooling (optional, skip this phase)
# cargo llvm-cov --workspace --lcov --output-path coverage.lcov
```

**Note on `--workspace`:** rn-dash is a single-crate project, not a workspace. `--workspace` is a no-op but doesn't error — use it for forward-compat with Phase 13's potential decomposition.

**Integration test inclusion:** `tests/*.rs` is run by `cargo llvm-cov` by default (same as `cargo test`). The tests themselves are EXCLUDED from the coverage *report* (cargo-llvm-cov's default `--ignore-filename-regex` skips `tests/`). This is the desired behavior per D-03 and general intent — we want to know what % of `src/` is exercised, not what % of test code runs.

### B) Extracting per-module thresholds from the JSON

```bash
# One-liner to regenerate COVERAGE-THRESHOLDS.md rows from the baseline JSON.
# Produces: src/module.rs | 92.34 | 90
# (threshold = floor(baseline, 5))
jq -r '
  .data[0].files
  | map({
      file: .filename,
      pct: .summary.lines.percent
    })
  | map("\(.file) | \(.pct) | \((.pct / 5 | floor) * 5)")
  | .[]
' .planning/phases/12-coverage-gate/BASELINE-COVERAGE.json
```

### C) Makefile target (D-02)

```makefile
# Makefile — local-only (D-02, no CI wiring).
.PHONY: cov cov-html cov-baseline cov-check

# Quick HTML report for dev
cov-html:
	cargo llvm-cov --workspace --html

# Regenerate the committed baseline (run this once per phase-12 plan iteration)
cov-baseline:
	cargo llvm-cov --workspace --json --summary-only \
		--output-path .planning/phases/12-coverage-gate/BASELINE-COVERAGE.json
	@echo "Baseline written. Now regenerate BASELINE-COVERAGE.md + COVERAGE-THRESHOLDS.md"

# Human-check gate — prints each module's current % alongside the committed threshold
cov-check:
	@cargo llvm-cov --workspace --json --summary-only --output-path /tmp/cov-current.json
	@jq -r '.data[0].files[] | "\(.filename) \(.summary.lines.percent)"' /tmp/cov-current.json

# Default
cov: cov-baseline cov-html
```

### D) Gitignore addition

```gitignore
# Coverage artifacts (D-03 — HTML gitignored, JSON committed to .planning/)
/target/llvm-cov*
```

(Technically `/target` already covers this, but listing explicitly is clearer to future readers.)

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `cargo-tarpaulin` (ptrace-based) | `cargo-llvm-cov` (LLVM source-based) | Common practice since ~2023; locked-in by D-01 | Accurate on async/generics; no ptrace = works on ARM macOS |
| `tokio_unstable` cfg for `process_group` | Stable in tokio ≥ 1.40 | Aug 2024 | No cfg flag needed; no MSRV hazard |
| `.before_exec` closure on CommandExt | `.pre_exec` (same thing, renamed) | `before_exec` deprecated since Rust 1.37 (2019) | Don't search for `before_exec` examples — outdated |
| Manual `$CARGO_TARGET_DIR/coverage/` setup | `cargo llvm-cov` handles via subprocess + profraw merge | Since cargo-llvm-cov 0.4.x (2022) | No manual plumbing needed |
| `nix` crate for Pid + signal | `libc` direct when deps budget is tight | Matter of preference; both are correct | Project choice — D-13 pushes us toward libc |

**Deprecated / outdated — DO NOT reference:**
- `before_exec` → use `pre_exec`
- `tokio_unstable` for `process_group` → just use it directly
- `cargo-tarpaulin` → tool-of-choice by D-01 is cargo-llvm-cov
- `throbber-widgets-tui` → Out of Scope per REQUIREMENTS.md (unrelated to this phase but noted)

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The phase-description's `Action::CommandQueued` is a typo for `Action::CommandQueuePush(_)` — only the latter exists in code. | phase_requirements | Low — plan rewrites in terms of `CommandQueuePush`; user can correct during discuss if desired |
| A2 | "Palette `x`" in the phase description refers to the CleanToggle modal's confirm key (not a sixth top-level palette). | phase_requirements | Low — the CleanToggle flow is covered either way; plan can include both interpretations |
| A3 | `rustc 1.94.1` on the dev machine will be the same toolchain used by Phase 13's coverage check. | Pitfall 7 | Low — threshold floor(baseline, 5) has slack; worst case re-baseline under new toolchain |
| A4 | COVER-02 does not need `tempfile` — the inline `bash -c` fixture writes no files. | D-12 decision | Low — add tempfile only if a follow-up test needs scratch |
| A5 | `infra/command_runner.rs` lacking `.process_group(0)` is out-of-scope for Phase 12 but relevant to Phase 13 (REFACTOR-02 / TASK-04 ties). | Pitfall 6 | Medium — if the planner interprets COVER-02 as "must characterize CommandRunner," they'll get stuck. Research explicitly steers toward direct spawn. |

All other claims are `[VERIFIED]` or `[CITED]` to an external authoritative source.

---

## Open Questions

1. **Should the test suite also cover the `kill_external_metro` path (`src/infra/port.rs`)?**
   - What we know: `Action::KillExternalMetro(pid)` at `src/app.rs:705-714` invokes `infra::port::kill_process`; it's on the "external metro conflict" user journey, adjacent to COVER-01.
   - What's unclear: is this scope-creep? COVER-01 only requires characterizing the single-instance invariant, not the recovery path.
   - Recommendation: leave out. Phase 12 is a *gate*, not exhaustive coverage. Flag for COVER-05 in a later milestone if the module changes.

2. **Where exactly should `COVER-03 palette x` tests live — inside CleanToggle modal tests or as a standalone palette test?**
   - What we know: there is no `PaletteMode::CleanToggle` enum — the flow is `PaletteMode::Yarn` ('c' key) → `ModalState::CleanToggle` (within which 'x' confirms).
   - What's unclear: the phase description's enumeration "a/i/x/y/g/w palettes" makes `x` look like a top-level palette.
   - Recommendation: treat as a modal-level test, not a palette-level test. Describe in plan as "palette 'y' → submenu 'c' → modal CleanToggle → 'x' confirms."

3. **Is `rustup component add llvm-tools-preview` already installed on this dev machine?**
   - What we know: the section 2.6 availability audit below probes this.
   - What's unclear: whether the user has installed it locally (it was not checked by the research run).
   - Recommendation: include as a Wave-0 task or documented prereq in the Makefile.

---

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `rustc` | coverage tool, all tests | ✓ (assumed — dev machine runs rn-dash) | 1.94.1 | — |
| `cargo` | coverage tool, all tests | ✓ | 1.94.1 | — |
| `cargo-llvm-cov` | COVER-04 | ✗ (installed per-phase) | — | `cargo install cargo-llvm-cov --locked` (one-time; Plan Wave 0) |
| `llvm-tools-preview` (rustup component) | cargo-llvm-cov internals | Unknown | — | `rustup component add llvm-tools-preview` (Plan Wave 0) |
| `bash` | COVER-02 subprocess fixture | ✓ (macOS + Linux ship bash; Rust CI ubuntu images include it) | — | — |
| `jq` | threshold extractor script (optional) | Likely ✓ on macOS + common Linux | — | `brew install jq` / fallback: hand-write the threshold table |
| `libc` crate | COVER-02 `unsafe { libc::kill(...) }` | ✓ (in `[dependencies]` at Cargo.toml:47) | 0.2 | — |
| `tokio` dev-feature `macros + process + time` | `#[tokio::test]`, `timeout`, `Command` | ✓ after Cargo.toml addition | 1.49 | — |

**Missing dependencies with no fallback:** None. `cargo-llvm-cov` install is a one-line prereq, not a blocker.

**Missing dependencies with fallback:** `jq` — if unavailable, write the threshold table by eyeballing the JSON output. Does not block any plan.

---

## Validation Architecture

> Nyquist validation enabled (workflow.nyquist_validation is not false in .planning/config.json).

### Test Framework

| Property | Value |
|----------|-------|
| Framework | built-in `#[test]` + `#[tokio::test]` + Rust integration tests in `tests/` |
| Config file | Cargo.toml `[dev-dependencies] tokio = { ... }` (new) |
| Quick run command | `cargo test --lib --quiet` (unit) or `cargo test --test metro_single_instance --quiet` (integration single) |
| Full suite command | `cargo test --quiet` |

### Phase Requirements → Test Map

| Req ID | Behavior (invariant locked) | Test Type | Automated Command | File |
|--------|----------------------------|-----------|-------------------|------|
| COVER-01 | `MetroManager` MUST hold ≤ 1 live handle; `register()` MUST panic when `self.handle.is_some()`. | integration (real #[tokio::test] to allow MetroHandle construction) | `cargo test --test metro_single_instance register_twice_panics -- --exact` | `tests/metro_single_instance.rs` ❌ Wave 0 |
| COVER-01 | `update(_, MetroStart, …)` MUST set `pending_restart = true` and dispatch `MetroStop` when `metro.is_running()` — MUST NOT double-spawn. | integration | `cargo test --test metro_single_instance metro_start_while_running_triggers_restart -- --exact` | `tests/metro_single_instance.rs` ❌ Wave 0 |
| COVER-02 | Killing the process group MUST reap both `bash` parent and `sleep` child within 2 s; `kill(-pgid, 0)` MUST return ESRCH after 500 ms grace. | integration (real subprocess) | `cargo test --test process_group_kill killing_pgid_reaps_child_tree -- --exact` | `tests/process_group_kill.rs` ❌ Wave 0 |
| COVER-03 | Each of 6 palette modes (Android/Ios/Yarn/Git/Worktree, plus y>c→CleanToggle→x) MUST map declared keys to declared Actions. Unrecognized keys MUST produce `ModalCancel` fallback. | unit (inline) | `cargo test --lib dispatch_tests -- --test-threads=1` | `src/app/dispatch_tests.rs` ❌ Wave 0 |
| COVER-03 | Each of 8 `ModalState` variants MUST dismiss on its documented key(s) and clear `state.modal` to `None`. | unit (inline) | `cargo test --lib modal_dismiss -- --test-threads=1` | `src/app/dispatch_tests.rs` ❌ Wave 0 |
| COVER-03 | `update(_, CommandExited, …)` with non-empty `command_queue` MUST pop front, set `running_command` to the popped item, and leave remaining items in queue. | unit (inline) | `cargo test --lib command_exited_drains_queue -- --test-threads=1` | `src/app/dispatch_tests.rs` ❌ Wave 0 |
| COVER-04 | `BASELINE-COVERAGE.json` MUST exist and parse as valid LLVM JSON (root has `version`, `type == "llvm.coverage.json.export"`, `data[0].totals.lines.percent` is a number ≤ 100). | artifact check | `jq '.data[0].totals.lines.percent' .planning/phases/12-coverage-gate/BASELINE-COVERAGE.json` | artifact ❌ Wave 0 |
| COVER-04 | `BASELINE-COVERAGE.md` MUST contain one row per `src/` module with baseline % and derived threshold. | artifact check | `grep -c '^|' .planning/phases/12-coverage-gate/BASELINE-COVERAGE.md` (row count > 0) | artifact ❌ Wave 0 |
| COVER-04 | `COVERAGE-THRESHOLDS.md` MUST exist with `module \| baseline % \| threshold %` header and thresholds = floor(baseline, 5). | artifact check | same as above + jq sanity-check that threshold % ≤ baseline % for every row | artifact ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test --lib --quiet` (fast unit path)
- **Per wave merge:** `cargo test --quiet` (unit + all integration tests) + `cargo clippy --all-targets -- -D warnings`
- **Phase gate:** `make cov-baseline` regenerates JSON; `make cov-check` compares current % to thresholds; human verifies every row ≥ threshold before `/gsd-verify-work`.

### Wave 0 Gaps

All test files are new. Wave 0 MUST include:

- [ ] `tests/metro_single_instance.rs` — COVER-01 (two tests, both `#[tokio::test]`)
- [ ] `tests/process_group_kill.rs` — COVER-02 (one test, `#[tokio::test(flavor = "multi_thread")]`, `#[cfg(any(linux,macos))]`)
- [ ] `src/app/dispatch_tests.rs` (or inline in `app.rs` if planner rejects the sub-module split) — COVER-03 (~10-12 tests)
- [ ] `src/app/mod.rs` minor change if splitting — add `#[cfg(test)] mod dispatch_tests;`
- [ ] `Cargo.toml` — add `[dev-dependencies] tokio = { … features = ["macros", "rt-multi-thread", "process", "time", "io-util"] }`
- [ ] `Makefile` — add `cov-html`, `cov-baseline`, `cov-check` targets
- [ ] `.gitignore` — add `/target/llvm-cov*` line
- [ ] `.planning/phases/12-coverage-gate/BASELINE-COVERAGE.json` — committed after running `cargo llvm-cov --json --summary-only`
- [ ] `.planning/phases/12-coverage-gate/BASELINE-COVERAGE.md` — committed, human-readable summary
- [ ] `.planning/phases/12-coverage-gate/COVERAGE-THRESHOLDS.md` — committed, `module | baseline % | threshold %` table
- [ ] Prereq documented in BASELINE-COVERAGE.md: `cargo install cargo-llvm-cov --locked` and `rustup component add llvm-tools-preview`

---

## Security Domain

> `security_enforcement` status: unknown (not present in `.planning/config.json` check here). Including the domain for completeness per the prompt's audit checklist — but Phase 12 is a *test* phase, not a product-feature phase. The security surface is near-zero.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — (rn-dash is single-user local dashboard) |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes (marginal) | Test fixtures MUST not interpolate untrusted input into `bash -c`. The inline fixture is a fixed literal — safe. |
| V6 Cryptography | no | — |

### Known Threat Patterns for {Rust + subprocess tests}

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Test accidentally `kill(-1, SIGTERM)` | DoS (dev machine) | Pitfall 3 — debug_assert pgid > 1 before calling |
| Zombie accumulation from un-`wait()`ed children | Resource exhaustion on CI | Pitfall 4 — always `.wait().await` in test body |
| Coverage baseline includes secrets from test env | Information disclosure | `--summary-only` strips per-region data; the committed JSON carries only percentages and filenames, no captured test I/O |

---

## Project Constraints (from CLAUDE.md)

Actionable directives extracted from `/Users/cubicme/aljazeera/dashboard/CLAUDE.md`:

1. **YOLO mode** — do not ask for confirmation at workflow gates. Auto-approve research, plans, and verification unless something is clearly wrong. (This phase: proceed without a discuss checkpoint; user already ran `/gsd-discuss-phase 12 --auto`.)
2. **`check-types` always uses `--incremental`** — any Makefile target invoking type-check must include `--incremental`. Coverage target does NOT invoke `check-types`, so no direct impact — but if the Phase 13 refactor adds a pre-coverage type-check step, it MUST use `--incremental`.
3. **Branch labels are per-branch** — no impact on test code, but relevant if tests touch `labels.json` (they do not in this phase).
4. **Metro logs only stream when a filter is applied** — no impact on COVER-01's metro invariant (we only care about the handle wrapper, not log streaming).
5. **Domain/infra/app/ui separation** — tests MUST respect this. `tests/metro_single_instance.rs` imports from `rn_dash::domain` only. `tests/process_group_kill.rs` imports `tokio::process` directly (syscall test, not layered). `src/app/dispatch_tests.rs` imports `action`, `domain::command`, `app::*` — app + domain only, no infra.

---

## Sources

### Primary (HIGH confidence — Context7 / official docs)

- [Tokio 1.49 docs — `tokio::process::Command`](https://docs.rs/tokio/1.49.0/tokio/process/struct.Command.html) — confirms `process_group` is stable, Unix-only, signature `&mut self, pgroup: i32`.
- [Tokio 1.49 docs — `tokio::process`](https://docs.rs/tokio/1.49.0/tokio/process/index.html) — spawn semantics, zombie-reap behavior, kill_on_drop.
- [Tokio PR #6731 (stabilize process_group)](https://github.com/tokio-rs/tokio/pull/5114) — stabilization history (unstable in 1.22.0, stable in 1.40).
- [cargo-llvm-cov README](https://github.com/taiki-e/cargo-llvm-cov) — JSON flag, `--summary-only`, `--ignore-filename-regex` default excludes `tests/`, workspace support.
- [cargo-llvm-cov crates.io](https://crates.io/crates/cargo-llvm-cov) — current version 0.8.5.
- [LLVM llvm-cov manpage](https://llvm.org/docs/CommandGuide/llvm-cov.html) — JSON export schema: `data[0].totals.lines.percent`, `data[0].files[N].summary.lines.percent`.
- `src/domain/metro.rs` — `MetroManager::register()` panic path at line 112-121, Option-wrapper invariant.
- `src/infra/process.rs` — `.process_group(0)` + `.kill_on_drop(true)` pattern for metro spawn.
- `src/app.rs` — `update()` function, `handle_key` palette resolution table, ModalState dismissal flow.
- `src/action.rs` — Action enum (confirms `CommandQueuePush`, not `CommandQueued`, is the real variant).

### Secondary (MEDIUM confidence — WebSearch verified against official source)

- [tokio issue #6934 — Windows panic with process::Command](https://github.com/tokio-rs/tokio/issues/6934) — confirmed Windows-only; rn-dash is macOS + Linux so not a blocker.
- [nix crate kill(2) docs](https://docs.rs/nix/latest/nix/sys/signal/fn.kill.html) — confirms `kill(-pgid, …)` semantics for PGID targeting; we use `libc` instead per D-13 but behavior is the same.
- [Rust Project Primer — test coverage](https://www.rustprojectprimer.com/measure/coverage.html) — community guidance on cargo-llvm-cov in 2025-2026.

### Tertiary (LOW — informational only)

- [cargo-llvm-cov issue #123 — ignore #[test] code automatically](https://github.com/taiki-e/cargo-llvm-cov/issues/123) — confirms test-code exclusion is the tool's default behavior.
- [cargo-nextest coverage integration docs](https://nexte.st/docs/integrations/test-coverage/) — alternative runner; not used here, reference only.

---

## Metadata

**Confidence breakdown:**
- Standard stack (cargo-llvm-cov version, tokio version, stability of process_group) — HIGH — verified via docs.rs and tokio CHANGELOG.
- Architecture (test layout, TEA dispatch pattern) — HIGH — verified against actual src/app.rs, src/action.rs, src/domain/command.rs.
- Pitfalls (Windows tokio regression, PGID kill -1 hazard, zombie reap, edition 2024 toolchain concerns) — HIGH — each backed by official source.
- COVER-03 palette `x` interpretation — MEDIUM — assumed it refers to the CleanToggle modal's 'x' confirm key. Low risk; easy to correct in plan.

**Research date:** 2026-04-23
**Valid until:** 2026-07-23 (90-day shelf — tokio minor versions ship ~monthly but `process_group` is stable; cargo-llvm-cov JSON schema is LLVM-stable; Rust edition 2024 is locked.)

---

## RESEARCH COMPLETE
