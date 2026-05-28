# rn-dash

Terminal dashboard for managing React Native worktrees, metro, and git.

Built with [Ratatui](https://ratatui.rs) in Rust.

## Features

- Browse and switch between git worktrees
- Start, stop, and reload Metro bundler with one key
- Run React Native commands (iOS/Android) with device picker
- Yarn and pod-install via command palette
- JIRA ticket title integration (auto-fetches from branch names)
- Open Claude Code in tmux/zellij splits
- Context-sensitive keybindings with dynamic hints

## Installation / Build

**Prerequisites:** Rust toolchain — install from [rustup.rs](https://rustup.rs)

```bash
git clone https://github.com/cubicme/rn-dash.git
cd rn-dash
cargo build --release
# Binary at target/release/rn-dash
```

Optionally copy the binary to a directory on your PATH:

```bash
cp target/release/rn-dash ~/.local/bin/
```

**macOS Gatekeeper:** If downloading a prebuilt binary from GitHub Releases, macOS may block it. Clear the quarantine flag:

```bash
xattr -cr /path/to/rn-dash
```

## Configuration

Config file location: `~/.config/rn-dash/config.toml`

Copy the example and fill in your values:

```bash
cp config.example.toml ~/.config/rn-dash/config.toml
```

The file is stored with `0600` permissions because it contains JIRA credentials.

### Config reference

| Field | Type | Default | Accepted values | Description |
|-------|------|---------|-----------------|-------------|
| `repo_root` | string | launch directory | Any path string; absolute or `~/` recommended | React Native monorepo root. |
| `jira_base_url` | string | — (required in config file) | Any JIRA base URL, e.g. `https://your-org.atlassian.net` | Base URL for your JIRA instance. |
| `jira_email` | string | — | Any email string, or omit | JIRA account email. Required for Cloud auth mode. |
| `jira_token` | string | — (required in config file) | Any API token or PAT string | JIRA API token (Cloud) or Personal Access Token (Data Center). |
| `auth_mode` | string | `"cloud"` | `"cloud"`, `"datacenter"` | JIRA authentication mode. |
| `jira_project_prefix` | string | `"UMP"` | Any exact branch ticket prefix, e.g. `"PROJ"` | Project key used to extract tickets like `PROJ-1234` from branch names. |
| `app_title` | string | `"RN Dash"` | Any string | Title shown in the dashboard header. |
| `claude_flags` | string | `"--dangerously-skip-permissions"` | Any string | Flags passed when launching Claude Code. |
| `auto_sync` | boolean | `false` | `true`, `false` | Automatically accept sync-before-run and sync-before-metro prompts. |
| `spinner_style` | string | `"circles"` | `"circles"`, `"braille"`, `"dots"` | Spinner glyph set for live task indicators. |
| `columns` | array of strings | `["status", "branch", "ticket", "dir", "task"]` | `"status"`, `"branch"`, `"ticket"`, `"dir"`, `"task"` | Worktree table columns in display order. Omit a value to hide that column. |

### Invalid config values

Malformed TOML, missing required fields in an existing config file, wrong types
such as `auto_sync = "yes"`, a non-array `columns` value, an unknown column
name such as `"owner"`, or a duplicate column name make the config fail to
load. rn-dash logs a warning and runs as if no config file was present for that
launch.

Unknown keys are ignored. Unknown `spinner_style` strings fall back to
`"circles"`. Unknown `auth_mode` strings are not rejected, but JIRA requests use
Cloud-style Basic Auth unless the value is exactly `"datacenter"`.

The `columns` array may be omitted; the default order is used. An empty array is
valid and hides every worktree table column.

See `config.example.toml` for an annotated template.

## Usage

Launch from a directory inside your monorepo, or anywhere if `repo_root` is set in config:

```bash
rn-dash
# or from source:
cargo run
```

### Keybindings

| Key | Action |
|-----|--------|
| j / k or arrows | Navigate worktree list |
| Enter | Start metro / run on device |
| Esc | Stop metro |
| y | Open yarn palette |
| w | Open worktree palette |
| c | Open Claude Code |
| R | Reload metro (when running) |
| ? | Toggle help overlay |
| q | Quit |

## License

[MIT](LICENSE)
