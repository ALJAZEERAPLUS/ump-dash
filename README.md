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

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `repo_root` | string | — (required) | Absolute path to your React Native monorepo root. Supports `~/`. |
| `jira_base_url` | string | — | Base URL for your JIRA instance, e.g. `https://your-org.atlassian.net`. |
| `jira_email` | string | — | JIRA account email. Required for Cloud auth mode. |
| `jira_token` | string | — | JIRA API token (Cloud) or Personal Access Token (Data Center). |
| `auth_mode` | string | `"cloud"` | Authentication mode: `"cloud"` (Basic Auth) or `"datacenter"` (Bearer PAT). |
| `jira_project_prefix` | string | `"UMP"` | JIRA project key prefix used in branch names, e.g. `PROJ` for `PROJ-1234`. |
| `app_title` | string | `"RN Dash"` | Title shown in the dashboard header. |
| `claude_flags` | string | `"--dangerously-skip-permissions"` | Flags passed when launching Claude Code. |
| `[columns].status` | integer | `4` | Worktree table status icon column width. |
| `[columns].branch` | integer | `20` | Worktree table branch column width. |
| `[columns].ticket` | integer | `20` | Minimum width for the flexible ticket/title column. |
| `[columns].dir` | integer | `16` | Worktree table directory column width. |
| `[columns].task` | integer | `20` | Worktree table task column width. |

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
