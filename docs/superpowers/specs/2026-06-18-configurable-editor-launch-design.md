# Configurable Editor Launch

## Goal

Add an `o>e` action that opens the selected worktree in the user's configured editor.

The first supported configuration must cover the current Emacs GUI workflow while keeping the default conservative for terminal users:

```toml
editor = "vim"
editor_in_terminal = true
```

For Emacs GUI, the expected config is:

```toml
editor = "emacsclient -c -n"
editor_in_terminal = false
```

## User Experience

From the worktree table:

1. Press `o` to open the existing Open palette.
2. Press `e` to open the selected worktree in the configured editor.

Terminal mode opens a new tmux, zellij, or Ghostty surface at the selected worktree and runs the configured editor against `.`. External mode launches the configured editor command directly and appends the absolute selected worktree path.

Examples:

| Config | Behavior |
| --- | --- |
| `editor = "vim"`, `editor_in_terminal = true` | `OpenInMultiplexer` at worktree, command `vim .` |
| `editor = "nvim"`, `editor_in_terminal = true` | `OpenInMultiplexer` at worktree, command `nvim .` |
| `editor = "emacsclient -c -n"`, `editor_in_terminal = false` | external command `emacsclient -c -n <worktree>` |
| `editor = "code -n"`, `editor_in_terminal = false` | external command `code -n <worktree>` |
| `editor = "cursor -n"`, `editor_in_terminal = false` | external command `cursor -n <worktree>` |

## Configuration

Add fields to `DashConfig`:

- `editor: String`, default `"vim"`.
- `editor_in_terminal: bool`, default `true`.

Update `AppConfigState` so `update()` can read these values without reaching into infra. `build_state()` should copy configured values into app state.

`config.example.toml` and the README config table must document both fields.

## Architecture

The feature must preserve the repo's TEA and layer boundaries:

- `domain` owns the user action and config data.
- `app::handle_key` maps `o>e` to `Action::OpenEditor` through `KEYBINDINGS`.
- `app::update` selects the active worktree, builds the editor launch effect, and performs no I/O.
- `app::effect_runner` executes the effect asynchronously.
- `infra` owns concrete process spawning for external GUI editor commands.
- `ui` only renders keybinding hints and help rows generated from keybinding metadata.

Terminal editor launch should reuse the existing `Effect::OpenInMultiplexer` path. External editor launch should use a new effect so it does not require tmux, zellij, or Ghostty.

## Command Construction

`editor` is treated as a shell command prefix.

Terminal mode:

```text
<editor> .
```

The multiplexer opens with the selected worktree as cwd, so `.` is the selected worktree.

External mode:

```text
<editor> <quoted absolute worktree path>
```

The implementation must quote the appended path for the shell. This keeps editor flags natural while supporting paths with spaces.

Empty or whitespace-only editor strings should not spawn a process. They should surface a dashboard error telling the user to configure `editor`.

## Error Handling

Terminal mode requires a detected multiplexer. If none is available, show:

```text
Cannot open editor: not inside a tmux, zellij, or Ghostty session
```

External mode does not require a multiplexer. If process spawning or command execution fails, the runner should dispatch an error action so the dashboard can show the failure rather than logging only.

## Tests

Add focused coverage for:

- Config defaults: `editor == "vim"` and `editor_in_terminal == true`.
- Config parsing copies custom editor fields into app state.
- `o>e` maps to `Action::OpenEditor`.
- Terminal editor mode emits `Effect::OpenInMultiplexer` with command `vim .`.
- Terminal editor mode without a multiplexer shows the editor-specific missing mux error.
- External editor mode emits the new external launch effect with a shell command that includes the quoted selected worktree path.
- Empty editor config surfaces a dashboard error and emits no effect.

Run:

```bash
cargo test
make arch-lint
```

