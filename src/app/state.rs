//! AppState — the single source of truth for the TUI.
//!
//! Plan 13-10 (F-209) regrouped AppState's ~30 pub fields into 6 sub-structs
//! by domain concern: MetroState, WorktreeBrowserState, CommandRunnerState,
//! ModalStackState, JiraState, AppConfigState. The 4 cross-cutting fields
//! (focused_panel, show_help, error_state, should_quit) and the MetroManager
//! itself stay at the root — MetroManager keeps its `state.metro.is_running()`
//! call site clear (avoiding a `state.metro_state.metro.is_running()` clash).
//!
//! See AUDIT.md F-209 (lines 545-576) and 13-PATTERNS.md:741-793 for the
//! design rationale.

/// Maximum number of command output lines retained in memory.
pub(crate) const MAX_COMMAND_LINES: usize = 1000;

/// Which panel currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FocusedPanel {
    #[default]
    WorktreeTable,
    CommandOutput,
}

impl FocusedPanel {
    pub fn next(self) -> Self {
        match self {
            Self::WorktreeTable => Self::CommandOutput,
            Self::CommandOutput => Self::WorktreeTable,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Self::WorktreeTable => Self::CommandOutput,
            Self::CommandOutput => Self::WorktreeTable,
        }
    }
}

/// Error state shown in the error overlay. Phase 2+ will set this from real command failures.
#[derive(Debug, Clone)]
pub struct ErrorState {
    pub message: String,
    pub can_retry: bool,
}

/// Which submenu the command palette is in (Phase 05.1 expanded scheme).
#[derive(Debug, Clone, PartialEq)]
pub enum PaletteMode {
    /// 'a' — Android submenu
    Android,
    /// 'i' — iOS submenu
    Ios,
    /// 'y' — Yarn palette (install, clean, test, lint)
    Yarn,
    /// 'g' — Git submenu
    Git,
    /// 'w' — Worktree palette (create, remove, create-with-new-branch)
    Worktree,
}

// ---------------------------------------------------------------------------
// Sub-structs (Plan 13-10 / F-209). Each groups a cohesive concern.
// MetroManager itself stays at the AppState root to keep `state.metro.is_running()`
// readable (see header doc).
// ---------------------------------------------------------------------------

/// Metro lifecycle coordination flags. The MetroManager itself lives at
/// AppState root (avoid `state.metro_state.metro.is_running()` clash).
#[derive(Debug, Default)]
pub struct MetroState {
    /// Active worktree path (updated from WorktreesLoaded + WorktreeSelectNext/Prev).
    pub active_worktree_path: Option<std::path::PathBuf>,

    /// True when worktree-switch triggers a stop-first-then-start sequence.
    /// When MetroExited fires and this is true, a new MetroStart is auto-dispatched.
    /// Plan 13-09 Pitfall 3 survivor — metro-lifecycle, not prereq ordering.
    pub pending_restart: bool,

    /// Skip external metro detection when restarting our own metro (worktree switch).
    /// Set true in MetroExited when pending_restart was true; consumed (reset) in MetroStart.
    /// Plan 13-09 Pitfall 3 survivor.
    pub skip_external_metro_check: bool,
}

/// Worktree browser / picker state.
#[derive(Debug)]
pub struct WorktreeBrowserState {
    pub worktrees: Vec<crate::domain::worktree::Worktree>,
    pub worktree_table_state: ratatui::widgets::TableState,
    pub selected_worktree_id: Option<crate::domain::worktree::WorktreeId>,
    pub fullscreen_panel: Option<FocusedPanel>,
    /// Guard against periodic refresh during worktree mutations.
    pub worktree_op_in_flight: bool,
}

impl Default for WorktreeBrowserState {
    fn default() -> Self {
        let mut worktree_table_state = ratatui::widgets::TableState::default();
        worktree_table_state.select(Some(0));
        Self {
            worktrees: Vec::new(),
            worktree_table_state,
            selected_worktree_id: None,
            fullscreen_panel: None,
            worktree_op_in_flight: false,
        }
    }
}

/// Command execution state — FIFO queue + per-worktree output + handle.
#[derive(Debug, Default)]
pub struct CommandRunnerState {
    /// Command queue — FIFO, drained on CommandExited.
    pub command_queue: std::collections::VecDeque<crate::domain::command::CommandSpec>,

    /// Per-worktree output persistence.
    pub command_output_by_worktree: std::collections::HashMap<
        crate::domain::worktree::WorktreeId,
        std::collections::VecDeque<String>,
    >,
    pub command_output_scroll_by_worktree:
        std::collections::HashMap<crate::domain::worktree::WorktreeId, usize>,

    /// Currently running command and its task handle.
    pub running_command: Option<crate::domain::command::CommandSpec>,
    pub command_task: Option<tokio::task::JoinHandle<()>>,

    /// Plan 13-09: post-queue-drain Action slot. Generalizes the older
    /// sync-then-metro coordination bool — the sync-then-metro flow stores
    /// `Some(Action::MetroStart)` here; arbitrary future post-drain actions
    /// can reuse the same mechanism without growing AppState.
    ///
    /// Consumed in CommandExited's empty-queue branch. Cleared in CommandCancel
    /// and MetroSpawnFailed.
    pub post_drain_action: Option<Box<crate::domain::action::Action>>,
}

/// Modal + palette + pending coordinator state. Single bag for one-modal-at-a-time
/// dispatch and the various pending_* fields that survive between palette → modal
/// → submit handoffs.
#[derive(Debug, Default)]
pub struct ModalStackState {
    /// Modal state — only one modal active at a time.
    pub modal: Option<crate::domain::command::ModalState>,

    /// Command palette mode — Some when user pressed 'a'/'i'/'y'/'g'/'w' in WorktreeList.
    pub palette_mode: Option<PaletteMode>,

    /// First 'g' press sets this true; second 'g' triggers ScrollToTop. Cleared on any other action.
    pub pending_g: bool,

    /// Pending device command — stored while async device enumeration is in flight.
    pub pending_device_command: Option<crate::domain::command::CommandSpec>,

    /// Pending claude open — stores worktree dir name while TextInput modal is open for tab suffix.
    pub pending_claude_open: Option<String>,

    /// Pending android mode change — set by StartSetAndroidMode, consumed by ModalInputSubmit.
    pub pending_android_mode: bool,

    /// Worktree removal — set when w>d is pressed, consumed by ModalConfirm.
    pub pending_worktree_removal:
        Option<(crate::domain::worktree::WorktreeId, std::path::PathBuf, String)>,

    /// Worktree creation — set when w>w is pressed, consumed by ModalInputSubmit.
    pub pending_worktree_add: bool,

    /// Selected base branch for the new-branch worktree flow
    /// (set by BranchPickerConfirm, consumed by ModalInputSubmit).
    pub pending_new_branch_base: Option<String>,

    /// True when the pending TextInput modal is for a new-branch worktree (not a regular worktree add).
    pub pending_new_branch_worktree: bool,
}

/// JIRA-related state (cache + config-derived availability + project prefix).
#[derive(Debug)]
pub struct JiraState {
    /// PROJ-XXXX -> title cache (persisted via Effect::SaveJiraCache).
    pub title_cache: std::collections::HashMap<String, String>,

    /// True when the `Adapters.jira` port is available (config loaded with token).
    pub available: bool,

    /// JIRA project key prefix used in branch names (e.g., "UMP" for UMP-1234).
    pub project_prefix: String,
}

impl Default for JiraState {
    fn default() -> Self {
        Self {
            title_cache: std::collections::HashMap::new(),
            available: false,
            project_prefix: "UMP".to_string(),
        }
    }
}

/// App-level config + immutable-per-session state.
#[derive(Debug)]
pub struct AppConfigState {
    /// Loaded dashboard config — kept for runtime access to claude_flags and
    /// other settings. Type lives in `crate::domain::dash_config::DashConfig`
    /// (Plan 13-08 moved it from infra so AppState stays infra-free).
    pub config: Option<crate::domain::dash_config::DashConfig>,

    /// Repo root — worktrees are listed relative to this path.
    pub repo_root: std::path::PathBuf,

    /// Claude Code launch flags loaded from config (e.g. "--dangerously-skip-permissions").
    pub claude_flags: String,

    /// Persisted Android run mode (e.g. "debugOptimized"). None while not yet loaded.
    pub android_mode: Option<String>,

    /// Loaded simulator UDID history (most-recent first). Used by update() to
    /// sort iOS picker entries without crossing the infra boundary.
    pub sim_history: Vec<String>,

    /// True when a terminal multiplexer (tmux or zellij) is detected at startup.
    /// Plan 13-08 replaced the `multiplexer: Option<Box<dyn MultiplexerPort>>` field —
    /// the port now lives in `Adapters` (constructed in `src/main.rs`).
    pub multiplexer_available: bool,
}

impl Default for AppConfigState {
    fn default() -> Self {
        Self {
            config: None,
            repo_root: std::env::current_dir().unwrap_or_default(),
            claude_flags: "--dangerously-skip-permissions".to_string(),
            android_mode: Some("debugOptimized".to_string()),
            sim_history: Vec::new(),
            multiplexer_available: false,
        }
    }
}

/// Application state — the single source of truth. All mutations happen in update().
///
/// Plan 13-10 (F-209): regrouped from ~30 flat pub fields into 4 cross-cutting
/// roots + MetroManager + 6 domain sub-structs.
#[derive(Debug, Default)]
pub struct AppState {
    // --- Cross-cutting / top-level UI concerns ---
    pub focused_panel: FocusedPanel,
    pub show_help: bool,
    pub error_state: Option<ErrorState>,
    pub should_quit: bool,

    /// MetroManager kept at root (avoid name clash inside MetroState — keeps
    /// `state.metro.is_running()` readable).
    pub metro: crate::domain::metro::MetroManager,

    // --- Domain sub-structs ---
    pub metro_state: MetroState,
    pub worktree_browser: WorktreeBrowserState,
    pub command_runner: CommandRunnerState,
    pub modal_stack: ModalStackState,
    pub jira: JiraState,
    pub app_config: AppConfigState,
}

// ---------------------------------------------------------------------------
// Per-worktree output accessor helpers (used by panels.rs)
// ---------------------------------------------------------------------------

/// Returns the WorktreeId for the currently selected worktree, or None if list is empty.
pub fn active_worktree_id(state: &AppState) -> Option<crate::domain::worktree::WorktreeId> {
    if state.worktree_browser.worktrees.is_empty() {
        return None;
    }
    let idx = state
        .worktree_browser
        .worktree_table_state
        .selected()
        .unwrap_or(0)
        .min(state.worktree_browser.worktrees.len() - 1);
    Some(state.worktree_browser.worktrees[idx].id.clone())
}

/// Returns a reference to the active worktree's command output deque (empty if none selected).
pub fn active_output(state: &AppState) -> &std::collections::VecDeque<String> {
    static EMPTY: std::sync::LazyLock<std::collections::VecDeque<String>> =
        std::sync::LazyLock::new(std::collections::VecDeque::new);
    if let Some(id) = active_worktree_id(state) {
        state
            .command_runner
            .command_output_by_worktree
            .get(&id)
            .unwrap_or(&EMPTY)
    } else {
        &EMPTY
    }
}

/// Returns the scroll offset for the active worktree's command output (0 if none selected).
pub fn active_output_scroll(state: &AppState) -> usize {
    active_worktree_id(state)
        .and_then(|id| {
            state
                .command_runner
                .command_output_scroll_by_worktree
                .get(&id)
                .copied()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod merge_slices_tests {
    use super::*;
    use crate::domain::worktree::{Worktree, WorktreeId, WorktreeMetroStatus};
    use crate::domain::worktree_slice::WorktreeSlice;
    use std::path::PathBuf;

    fn wt(id: &str) -> Worktree {
        Worktree {
            id: WorktreeId(id.into()),
            path: PathBuf::from(format!("/tmp/{id}")),
            branch: "main".into(),
            head_sha: "0000000".into(),
            metro_status: WorktreeMetroStatus::Stopped,
            jira_title: None,
            stale: false,
            stale_pods: false,
            jira_key: None,
        }
    }

    #[test]
    fn merge_inserts_default_slices_for_new_worktrees() {
        let mut state = AppState::default();
        let loaded = vec![wt("wt-1"), wt("wt-2")];
        merge_slices(&mut state, &loaded);
        assert_eq!(state.worktrees.len(), 2);
        assert!(state.worktrees.contains_key(&WorktreeId("wt-1".into())));
        assert!(state.worktrees.contains_key(&WorktreeId("wt-2".into())));
    }

    #[test]
    fn merge_preserves_surviving_slice_state() {
        let mut state = AppState::default();
        // Seed slice with a queued command.
        state.worktrees.insert(
            WorktreeId("wt-1".into()),
            WorktreeSlice {
                id: WorktreeId("wt-1".into()),
                queue: {
                    let mut q = std::collections::VecDeque::new();
                    q.push_back(crate::domain::command::CommandSpec::YarnInstall);
                    q
                },
                ..Default::default()
            },
        );
        // Refresh with the same id present.
        merge_slices(&mut state, &[wt("wt-1")]);
        let slice = state.worktrees.get(&WorktreeId("wt-1".into())).unwrap();
        assert_eq!(slice.queue.len(), 1);
    }

    #[test]
    fn merge_drops_slice_for_removed_worktree() {
        let mut state = AppState::default();
        state.worktrees.insert(
            WorktreeId("wt-gone".into()),
            WorktreeSlice {
                id: WorktreeId("wt-gone".into()),
                ..Default::default()
            },
        );
        merge_slices(&mut state, &[wt("wt-survivor")]);
        assert!(!state.worktrees.contains_key(&WorktreeId("wt-gone".into())));
        assert!(state.worktrees.contains_key(&WorktreeId("wt-survivor".into())));
    }

    #[test]
    fn merge_short_circuits_when_loaded_set_equals_current_set() {
        // Q4: when the id sets match, surviving slice state stays untouched
        // even if internal state is structurally suspicious.
        let mut state = AppState::default();
        let mut q = std::collections::VecDeque::new();
        q.push_back(crate::domain::command::CommandSpec::YarnInstall);
        state.worktrees.insert(
            WorktreeId("wt-1".into()),
            WorktreeSlice { id: WorktreeId("wt-1".into()), queue: q, ..Default::default() },
        );
        merge_slices(&mut state, &[wt("wt-1")]);
        assert_eq!(state.worktrees.get(&WorktreeId("wt-1".into())).unwrap().queue.len(), 1);
    }
}
