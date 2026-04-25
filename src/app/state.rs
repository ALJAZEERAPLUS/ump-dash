//! AppState — the single source of truth for the TUI.
//!
//! This file hosts the core data types (AppState, FocusedPanel, PaletteMode,
//! ErrorState) and the three per-worktree-output accessor helpers used by
//! `ui/panels.rs`. Plan 13-10 will group `AppState` fields into domain sub-
//! structs (metro_state, worktree_state, modal_state, jira_state); for now
//! everything remains flat per the verbatim lift-and-shift of Plan 13-06.

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

/// Application state — the single source of truth. All mutations happen in update().
///
/// Plan 13-08: AppState no longer holds infra adapter trait objects.
/// `jira_client` and `multiplexer` fields were deleted — those ports now
/// live in the `Adapters` struct (constructed in `src/main.rs`, owned by
/// `EffectRunner`). update() reads `jira_available` / `multiplexer_available`
/// booleans to decide whether to push corresponding effects.
#[derive(Debug)]
pub struct AppState {
    // Phase 1 fields
    pub focused_panel: FocusedPanel,
    pub show_help: bool,
    pub error_state: Option<ErrorState>,
    pub should_quit: bool,

    // Metro state — single-instance enforced by MetroManager's Option<MetroHandle>
    pub metro: crate::domain::metro::MetroManager,

    // Active worktree (updated from WorktreesLoaded + WorktreeSelectNext/Prev)
    pub active_worktree_path: Option<std::path::PathBuf>,

    // Set to true when worktree-switch triggers a stop-first-then-start sequence.
    // When MetroExited fires and this is true, a new MetroStart is auto-dispatched.
    pub pending_restart: bool,

    // Phase 5: captured target worktree path during worktree switch (consumed by MetroExited)
    pub pending_switch_path: Option<std::path::PathBuf>,

    // --- Phase 3 fields ---

    // Worktree browser
    pub worktrees: Vec<crate::domain::worktree::Worktree>,
    pub worktree_table_state: ratatui::widgets::TableState,
    pub selected_worktree_id: Option<crate::domain::worktree::WorktreeId>,
    pub fullscreen_panel: Option<FocusedPanel>,

    // Command queue — FIFO, drained on CommandExited
    pub command_queue: std::collections::VecDeque<crate::domain::command::CommandSpec>,

    // Per-worktree output persistence
    pub command_output_by_worktree: std::collections::HashMap<crate::domain::worktree::WorktreeId, std::collections::VecDeque<String>>,
    pub command_output_scroll_by_worktree: std::collections::HashMap<crate::domain::worktree::WorktreeId, usize>,

    // Currently running command and its task handle
    pub running_command: Option<crate::domain::command::CommandSpec>,
    pub command_task: Option<tokio::task::JoinHandle<()>>,

    // Modal state — only one modal active at a time
    pub modal: Option<crate::domain::command::ModalState>,

    // Repo root — worktrees are listed relative to this path
    pub repo_root: std::path::PathBuf,

    // Command palette mode — Some when user pressed 'g' or 'c' in WorktreeList
    pub palette_mode: Option<PaletteMode>,

    // Pending device command — stored while async device enumeration is in flight
    pub pending_device_command: Option<crate::domain::command::CommandSpec>,

    // Pending claude open — stores worktree dir name while TextInput modal is open for tab suffix
    pub pending_claude_open: Option<String>,

    // Pending android mode change — set by StartSetAndroidMode, consumed by ModalInputSubmit
    pub pending_android_mode: bool,

    // --- Phase 4 fields ---
    pub jira_title_cache: std::collections::HashMap<String, String>,  // PROJ-XXXX -> title
    /// True when the `Adapters.jira` port is available (config loaded with
    /// token). Plan 13-08: replaces the `jira_client: Option<Arc<dyn JiraPort>>`
    /// field — the port now lives in `Adapters` (constructed in `src/main.rs`).
    /// update() only needs the availability bit to decide whether to push
    /// `Effect::FetchJiraTitles`.
    pub jira_available: bool,
    /// JIRA project key prefix used in branch names (e.g., "UMP" for UMP-1234).
    pub jira_project_prefix: String,

    // --- Phase 5.2 fields ---
    /// First 'g' press sets this true; second 'g' triggers ScrollToTop. Cleared on any other action.
    pub pending_g: bool,

    // --- Phase 5.1 fields ---
    /// True when a terminal multiplexer (tmux or zellij) is detected at startup.
    /// Plan 13-08: replaces the `multiplexer: Option<Box<dyn MultiplexerPort>>`
    /// field — the port now lives in `Adapters` (constructed in `src/main.rs`).
    /// update() reads this bit to decide whether to surface a "not in
    /// tmux/zellij" error or push `Effect::OpenInMultiplexer`.
    pub multiplexer_available: bool,
    /// Claude Code launch flags loaded from config (e.g. "--dangerously-skip-permissions").
    pub claude_flags: String,
    /// Loaded dashboard config — kept for runtime access to claude_flags and
    /// other settings. The type is `crate::domain::dash_config::DashConfig`
    /// (Plan 13-08 moved it from infra so AppState stays infra-free).
    pub config: Option<crate::domain::dash_config::DashConfig>,
    /// Loaded simulator UDID history (most-recent first). Plan 13-08: read at
    /// startup in `src/main.rs` and supplied to `AppState` so `update()` can
    /// sort iOS pickers without crossing the infra boundary.
    pub sim_history: Vec<String>,

    // Quick-2: Worktree removal — set when g>D is pressed, consumed by ModalConfirm
    pub pending_worktree_removal: Option<(crate::domain::worktree::WorktreeId, std::path::PathBuf, String)>,

    // Quick-260331-cw5: Android run mode — persisted preference (e.g. "debugOptimized")
    pub android_mode: Option<String>,

    // Quick-260403-dmz: Worktree creation — set when g>W is pressed, consumed by ModalInputSubmit
    pub pending_worktree_add: bool,

    // Phase 08-02: New-branch worktree creation flow
    /// Selected base branch for the new-branch worktree flow (set by BranchPickerConfirm, consumed by ModalInputSubmit).
    pub pending_new_branch_base: Option<String>,
    /// True when the pending TextInput modal is for a new-branch worktree (not a regular worktree add).
    pub pending_new_branch_worktree: bool,

    // Quick-260405-ijq: RN run command waiting for metro to become Ready before dispatch.
    pub pending_metro_run: Option<crate::domain::command::CommandSpec>,

    // Phase 08-04: skip external metro detection when restarting our own metro (worktree switch).
    // Set true in MetroExited when pending_restart was true; consumed (reset) in MetroStart.
    pub skip_external_metro_check: bool,

    // Quick-260407-cq5: Guard against periodic refresh during worktree mutations.
    pub worktree_op_in_flight: bool,

    // Quick-260410-mu7: metro start pending after sync commands drain
    pub pending_metro_after_sync: bool,
}

impl Default for AppState {
    fn default() -> Self {
        let mut worktree_table_state = ratatui::widgets::TableState::default();
        worktree_table_state.select(Some(0));
        Self {
            focused_panel: FocusedPanel::default(),
            show_help: false,
            error_state: None,
            should_quit: false,
            metro: crate::domain::metro::MetroManager::new(),
            active_worktree_path: None,
            pending_restart: false,
            pending_switch_path: None,
            // Phase 3
            worktrees: Vec::new(),
            worktree_table_state,
            selected_worktree_id: None,
            fullscreen_panel: None,
            command_queue: std::collections::VecDeque::new(),
            command_output_by_worktree: std::collections::HashMap::new(),
            command_output_scroll_by_worktree: std::collections::HashMap::new(),
            running_command: None,
            command_task: None,
            modal: None,
            repo_root: std::env::current_dir().unwrap_or_default(),
            palette_mode: None,
            pending_device_command: None,
            pending_claude_open: None,
            pending_android_mode: false,
            // Phase 5.2
            pending_g: false,
            // Phase 4
            jira_title_cache: std::collections::HashMap::new(),
            jira_available: false,  // populated by main.rs from Adapters.jira.is_some()
            jira_project_prefix: "UMP".to_string(),
            // Phase 5.1
            multiplexer_available: false,  // populated by main.rs from Adapters.multiplexer.is_some()
            claude_flags: "--dangerously-skip-permissions".to_string(),
            config: None,
            sim_history: Vec::new(),
            // Quick-2
            pending_worktree_removal: None,
            // Quick-260331-cw5: default Android run mode; main.rs may override
            // with the persisted value loaded via infra::android_prefs.
            // Plan 13-08: removed the inline `infra::*` call so
            // AppState's Default impl is infra-free (G-01).
            android_mode: Some("debugOptimized".to_string()),
            // Quick-260403-dmz
            pending_worktree_add: false,
            // Quick-260405-ijq
            pending_metro_run: None,
            // Phase 08-02
            pending_new_branch_base: None,
            pending_new_branch_worktree: false,
            // Phase 08-04
            skip_external_metro_check: false,
            // Quick-260407-cq5
            worktree_op_in_flight: false,
            // Quick-260410-mu7
            pending_metro_after_sync: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Per-worktree output accessor helpers (used by panels.rs)
// ---------------------------------------------------------------------------

/// Returns the WorktreeId for the currently selected worktree, or None if list is empty.
pub fn active_worktree_id(state: &AppState) -> Option<crate::domain::worktree::WorktreeId> {
    if state.worktrees.is_empty() {
        return None;
    }
    let idx = state.worktree_table_state.selected().unwrap_or(0)
        .min(state.worktrees.len() - 1);
    Some(state.worktrees[idx].id.clone())
}

/// Returns a reference to the active worktree's command output deque (empty if none selected).
pub fn active_output(state: &AppState) -> &std::collections::VecDeque<String> {
    static EMPTY: std::sync::LazyLock<std::collections::VecDeque<String>> =
        std::sync::LazyLock::new(std::collections::VecDeque::new);
    if let Some(id) = active_worktree_id(state) {
        state.command_output_by_worktree.get(&id).unwrap_or(&EMPTY)
    } else {
        &EMPTY
    }
}

/// Returns the scroll offset for the active worktree's command output (0 if none selected).
pub fn active_output_scroll(state: &AppState) -> usize {
    active_worktree_id(state)
        .and_then(|id| state.command_output_scroll_by_worktree.get(&id).copied())
        .unwrap_or(0)
}
