# Configurable Editor Launch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `o>e` to open the selected worktree in a configurable editor, defaulting to terminal Vim while supporting external GUI launch such as `emacsclient -c -n`.

**Architecture:** Keep editor launch as pure intent in `app::update`: terminal editors reuse `Effect::OpenInMultiplexer`, external editors use a new effect interpreted through a domain-owned `ExternalCommandPort`. Concrete shell execution lives in `infra`, injected through `app::Adapters` from `main.rs`.

**Tech Stack:** Rust 2024, Ratatui/crossterm key handling, serde/toml config, tokio effect runner, existing hexagonal ports.

---

## File Map

- Modify `src/domain/dash_config.rs`: add `editor`, `editor_in_terminal`, default helpers, and config tests.
- Modify `src/app/state.rs`: add app config fields with defaults.
- Modify `src/main.rs`: copy config values into `AppConfigState`; inject external command adapter.
- Modify `src/domain/action.rs`: add `OpenEditor` and `OpenEditorFailed`.
- Modify `src/app/keybindings.rs`: bind `o>e` to `OpenEditor`.
- Modify `src/app/effect.rs`: add `OpenExternalEditor`.
- Create `src/domain/ports/external_command_port.rs`: domain-owned port for shell command launch.
- Modify `src/domain/ports/mod.rs`: export the new port.
- Create `src/infra/external_command.rs`: run shell commands with `/bin/sh -lc`.
- Modify `src/infra/mod.rs`: export the new adapter module.
- Modify `src/app/adapters.rs`: inject `external_command`.
- Modify `src/app/effect_runner.rs`: interpret `OpenExternalEditor`, send `OpenEditorFailed` on failure.
- Modify `src/app/update.rs`: construct editor effects and errors.
- Modify `src/app/dispatch_tests.rs`: add keybinding/update tests.
- Modify `README.md` and `config.example.toml`: document config and keybinding.

## Task 1: Config Fields

**Files:**
- Modify: `src/domain/dash_config.rs`
- Modify: `src/app/state.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write failing config tests**

Add these tests inside `src/domain/dash_config.rs`'s existing `tests` module:

```rust
#[test]
fn editor_defaults_to_terminal_vim() {
    let config = parse_config("");

    assert_eq!(config.editor, "vim");
    assert!(config.editor_in_terminal);
}

#[test]
fn editor_config_overrides_defaults() {
    let config = parse_config(
        r#"
editor = "emacsclient -c -n"
editor_in_terminal = false
"#,
    );

    assert_eq!(config.editor, "emacsclient -c -n");
    assert!(!config.editor_in_terminal);
}
```

- [ ] **Step 2: Run the failing tests**

Run: `cargo test domain::dash_config::tests::editor_`

Expected: compile failure because `DashConfig` has no `editor` or `editor_in_terminal` fields.

- [ ] **Step 3: Implement config fields**

Add helper functions in `src/domain/dash_config.rs`:

```rust
fn default_editor() -> String {
    "vim".to_string()
}

fn default_editor_in_terminal() -> bool {
    true
}
```

Add fields to `DashConfig` near `claude_flags`:

```rust
/// Editor command prefix used by the Open editor action.
#[serde(default = "default_editor")]
pub editor: String,

/// When true, editor opens in a terminal surface. When false, editor launches
/// as an external command with the absolute worktree path appended.
#[serde(default = "default_editor_in_terminal")]
pub editor_in_terminal: bool,
```

Add fields to `AppConfigState` in `src/app/state.rs`:

```rust
/// Editor command prefix used by o>e.
pub editor: String,

/// True when the configured editor should run inside a terminal surface.
pub editor_in_terminal: bool,
```

Set defaults:

```rust
editor: "vim".to_string(),
editor_in_terminal: true,
```

Copy config in `build_state()` in `src/main.rs`:

```rust
state.app_config.editor = cfg.editor.clone();
state.app_config.editor_in_terminal = cfg.editor_in_terminal;
```

- [ ] **Step 4: Run config tests**

Run: `cargo test domain::dash_config::tests::editor_`

Expected: PASS.

## Task 2: Action, Keybinding, and Effect Grammar

**Files:**
- Modify: `src/domain/action.rs`
- Modify: `src/app/keybindings.rs`
- Modify: `src/app/effect.rs`
- Modify: `src/app/dispatch_tests.rs`

- [ ] **Step 1: Write failing keybinding test**

In `src/app/dispatch_tests.rs`, update `open_palette_resolves_lowercase_keys`:

```rust
assert_eq!(handle_key(&state, key('e')), Some(Action::OpenEditor));
assert_eq!(handle_key(&state, key('E')), Some(Action::ModalCancel));
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test app::dispatch_tests::keybindings::open_palette_resolves_lowercase_keys`

Expected: compile failure because `Action::OpenEditor` does not exist.

- [ ] **Step 3: Add action variants**

Add to `src/domain/action.rs` near `OpenShellTab`:

```rust
OpenEditor,                 // o>e on worktree — open configured editor
OpenEditorFailed(String),   // background: external editor launch failed
```

- [ ] **Step 4: Add effect variant**

Add to `src/app/effect.rs` after `OpenInMultiplexer`:

```rust
OpenExternalEditor {
    command: String,
},
```

Update the `effect_has_at_least_fifteen_variants` exhaustive match:

```rust
Effect::OpenExternalEditor { .. } => 22,
```

- [ ] **Step 5: Add keybinding**

Add to `src/app/keybindings.rs` in the Open palette block:

```rust
KeyBinding {
    key: KeyCode::Char('e'),
    label: "e",
    short_desc: "editor",
    long_desc: "Open configured editor at worktree",
    context: BindingContext::Palette(PaletteMode::Open),
    action: |_| Some(Action::OpenEditor),
    visible: |_| true,
},
```

- [ ] **Step 6: Run the keybinding test**

Run: `cargo test app::dispatch_tests::keybindings::open_palette_resolves_lowercase_keys`

Expected: PASS.

## Task 3: External Command Port and Runner Effect

**Files:**
- Create: `src/domain/ports/external_command_port.rs`
- Modify: `src/domain/ports/mod.rs`
- Create: `src/infra/external_command.rs`
- Modify: `src/infra/mod.rs`
- Modify: `src/app/adapters.rs`
- Modify: `src/app/effect_runner.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create domain port**

Create `src/domain/ports/external_command_port.rs`:

```rust
//! ExternalCommandPort — domain-owned shell command launcher boundary.

#![allow(dead_code)]

/// Runs a shell command outside the dashboard task system.
pub trait ExternalCommandPort: Send + Sync + std::fmt::Debug {
    fn run_shell_command(&self, command: &str) -> anyhow::Result<()>;
}
```

Export it in `src/domain/ports/mod.rs`:

```rust
pub mod external_command_port;
```

- [ ] **Step 2: Create infra adapter**

Create `src/infra/external_command.rs`:

```rust
//! External command adapter for fire-and-forget GUI editor launches.

use crate::domain::ports::external_command_port::ExternalCommandPort;

#[derive(Debug)]
pub struct ShellExternalCommand;

impl ExternalCommandPort for ShellExternalCommand {
    fn run_shell_command(&self, command: &str) -> anyhow::Result<()> {
        let status = std::process::Command::new("/bin/sh")
            .args(["-lc", command])
            .status()?;
        if !status.success() {
            anyhow::bail!("external command failed: exit code {:?}", status.code());
        }
        Ok(())
    }
}
```

Export it in `src/infra/mod.rs`:

```rust
pub mod external_command;
```

- [ ] **Step 3: Inject adapter**

In `src/app/adapters.rs`, import and add a field:

```rust
use crate::domain::ports::external_command_port::ExternalCommandPort;

pub external_command: Arc<dyn ExternalCommandPort>,
```

In `src/main.rs` adapters construction:

```rust
external_command: Arc::new(ump_dash::infra::external_command::ShellExternalCommand),
```

In `src/app/effect_runner.rs` tests, add a fake:

```rust
#[derive(Debug)]
struct NoopExternalCommand;

impl crate::domain::ports::external_command_port::ExternalCommandPort for NoopExternalCommand {
    fn run_shell_command(&self, _command: &str) -> anyhow::Result<()> {
        Ok(())
    }
}
```

Add it to the test `Adapters` literal:

```rust
external_command: Arc::new(NoopExternalCommand),
```

- [ ] **Step 4: Interpret external editor effect**

In `src/app/effect_runner.rs`, add to the effect coverage comment:

```rust
//!   OpenExternalEditor { command }                 → adapters.external_command.run_shell_command(...)
```

Add match arm near `OpenInMultiplexer`:

```rust
Effect::OpenExternalEditor { command } => {
    let external_command = self.adapters.external_command.clone();
    let tx = self.action_tx.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = external_command.run_shell_command(&command) {
            let _ = tx.send(Action::OpenEditorFailed(e.to_string()));
        }
    });
}
```

- [ ] **Step 5: Run compile check**

Run: `cargo test --lib app::effect::tests::effect_has_at_least_fifteen_variants`

Expected: PASS.

## Task 4: Editor Update Logic

**Files:**
- Modify: `src/app/update.rs`
- Modify: `src/app/dispatch_tests.rs`

- [ ] **Step 1: Write failing update tests**

Add to `mod claude_tab` or rename it to `mod open_palette_dispatch` in `src/app/dispatch_tests.rs`:

```rust
#[test]
fn open_editor_terminal_mode_opens_configured_editor_in_multiplexer() {
    let mut state = base_state();
    state.app_config.multiplexer_available = true;
    state.app_config.editor = "vim".into();
    state.app_config.editor_in_terminal = true;
    seed_one_worktree_id(&mut state, "ump-dash");

    let effects = update(&mut state, Action::OpenEditor);

    match &effects[..] {
        [Effect::OpenInMultiplexer { worktree, name, command }] => {
            assert_eq!(worktree, &std::path::PathBuf::from("/tmp/ump-dash"));
            assert_eq!(name, "main-editor");
            assert_eq!(command, "vim .");
        }
        other => panic!("expected OpenInMultiplexer effect, got {other:?}"),
    }
}

#[test]
fn open_editor_terminal_mode_requires_multiplexer() {
    let mut state = base_state();
    state.app_config.multiplexer_available = false;
    state.app_config.editor = "vim".into();
    state.app_config.editor_in_terminal = true;
    seed_one_worktree_id(&mut state, "ump-dash");

    let effects = update(&mut state, Action::OpenEditor);

    assert!(effects.is_empty());
    assert_eq!(
        state.error_state.as_ref().map(|e| e.message.as_str()),
        Some("Cannot open editor: not inside a tmux, zellij, or Ghostty session")
    );
}

#[test]
fn open_editor_external_mode_emits_quoted_external_command() {
    let mut state = base_state();
    state.app_config.editor = "emacsclient -c -n".into();
    state.app_config.editor_in_terminal = false;
    seed_one_worktree_id(&mut state, "ump dash");

    let effects = update(&mut state, Action::OpenEditor);

    match &effects[..] {
        [Effect::OpenExternalEditor { command }] => {
            assert_eq!(command, "emacsclient -c -n '/tmp/ump dash'");
        }
        other => panic!("expected OpenExternalEditor effect, got {other:?}"),
    }
}

#[test]
fn open_editor_empty_config_shows_error() {
    let mut state = base_state();
    state.app_config.editor = "   ".into();
    state.app_config.editor_in_terminal = false;
    seed_one_worktree_id(&mut state, "ump-dash");

    let effects = update(&mut state, Action::OpenEditor);

    assert!(effects.is_empty());
    assert_eq!(
        state.error_state.as_ref().map(|e| e.message.as_str()),
        Some("Cannot open editor: configure the editor setting first")
    );
}
```

- [ ] **Step 2: Run failing tests**

Run: `cargo test app::dispatch_tests::claude_tab::open_editor`

Expected: compile failure or failing tests because `Action::OpenEditor` handling is not implemented.

- [ ] **Step 3: Add helper functions**

In `src/app/update.rs`, add near `selected_worktree_path`:

```rust
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
}

fn selected_worktree_snapshot(state: &AppState) -> Option<crate::domain::worktree::Worktree> {
    if state.worktree_browser.worktrees.is_empty() {
        return None;
    }
    let idx = state
        .worktree_browser
        .worktree_table_state
        .selected()
        .unwrap_or(0)
        .min(state.worktree_browser.worktrees.len() - 1);
    Some(state.worktree_browser.worktrees[idx].clone())
}
```

Use `selected_worktree_snapshot()` in the existing `OpenClaudeCode` and `OpenShellTab` arms to remove duplicated selection code.

- [ ] **Step 4: Implement `OpenEditor` and failure action**

In `src/app/update.rs`, add a match arm near the existing Open palette arms:

```rust
Action::OpenEditor => {
    let editor = state.app_config.editor.trim().to_string();
    if editor.is_empty() {
        state.error_state = Some(ErrorState {
            message: "Cannot open editor: configure the editor setting first".into(),
            can_retry: false,
        });
        return effects;
    }

    let wt = match selected_worktree_snapshot(state) {
        Some(wt) => wt,
        None => return effects,
    };

    if state.app_config.editor_in_terminal {
        if !state.app_config.multiplexer_available {
            state.error_state = Some(ErrorState {
                message: "Cannot open editor: not inside a tmux, zellij, or Ghostty session".into(),
                can_retry: false,
            });
            return effects;
        }
        effects.push(Effect::OpenInMultiplexer {
            worktree: wt.path.clone(),
            name: format!("{}-editor", wt.preferred_prefix()),
            command: format!("{editor} ."),
        });
    } else {
        let quoted_path = shell_quote(&wt.path.to_string_lossy());
        effects.push(Effect::OpenExternalEditor {
            command: format!("{editor} {quoted_path}"),
        });
    }
}

Action::OpenEditorFailed(message) => {
    state.error_state = Some(ErrorState {
        message: format!("Cannot open editor: {message}"),
        can_retry: false,
    });
}
```

- [ ] **Step 5: Run update tests**

Run: `cargo test app::dispatch_tests::claude_tab::open_editor`

Expected: PASS.

## Task 5: Documentation

**Files:**
- Modify: `config.example.toml`
- Modify: `README.md`

- [ ] **Step 1: Update config example**

Add after `claude_flags` in `config.example.toml`:

```toml
# Editor opened from the Open palette with o>e.
# Default: "vim"
# editor = "vim"

# When true, o>e opens a terminal surface at the selected worktree and runs
# "<editor> .". When false, ump-dash runs "<editor> <absolute-worktree-path>"
# as an external command. For GUI Emacs, use:
# editor = "emacsclient -c -n"
# editor_in_terminal = false
# Default: true
# editor_in_terminal = true
```

- [ ] **Step 2: Update README**

In the config reference table, add:

```markdown
| `editor` | string | `"vim"` | Any shell command prefix | Editor command used by `o>e`; the selected worktree target is appended automatically. |
| `editor_in_terminal` | boolean | `true` | `true`, `false` | When true, open a terminal surface and run `<editor> .`; when false, run `<editor> <absolute worktree path>` as an external command. |
```

Update keybinding row:

```markdown
| o | Open palette (`c` Claude Code, `e` editor, `t` shell tab, `j` Metro debugger) |
```

- [ ] **Step 3: Check docs diff**

Run: `git diff -- README.md config.example.toml`

Expected: docs mention both config fields and `o>e`.

## Task 6: Verification and Commit

**Files:**
- All modified files

- [ ] **Step 1: Run targeted tests**

Run:

```bash
cargo test domain::dash_config::tests::editor_
cargo test app::dispatch_tests::keybindings::open_palette_resolves_lowercase_keys
cargo test app::dispatch_tests::claude_tab::open_editor
cargo test app::effect::tests::effect_has_at_least_fifteen_variants
```

Expected: PASS.

- [ ] **Step 2: Run full verification**

Run:

```bash
cargo test
make arch-lint
```

Expected: PASS.

- [ ] **Step 3: Commit implementation**

Run:

```bash
git status --short
git add src/domain/dash_config.rs src/app/state.rs src/main.rs src/domain/action.rs src/app/keybindings.rs src/app/effect.rs src/domain/ports/external_command_port.rs src/domain/ports/mod.rs src/infra/external_command.rs src/infra/mod.rs src/app/adapters.rs src/app/effect_runner.rs src/app/update.rs src/app/dispatch_tests.rs README.md config.example.toml docs/superpowers/plans/2026-06-18-configurable-editor-launch.md
git commit -m "feat: add configurable editor launch"
```

Expected: clean commit with tests passing.

