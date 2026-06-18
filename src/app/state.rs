//! AppState — the single source of truth for the TUI.
//!
//! Plan 13-10 (F-209) regrouped AppState's ~30 pub fields into 6 sub-structs
//! by domain concern: MetroState, WorktreeBrowserState, ModalStackState,
//! JiraState, AppConfigState. The 4 cross-cutting fields
//! (focused_panel, show_help, error_state, should_quit) stay at the root.
//! Metro runtime state lives in each per-worktree `WorktreeSlice`.
//!
//! Plan 14-09: `CommandRunnerState` struct deleted — all 5 of its fields have
//! been migrated to per-worktree `WorktreeSlice` entries in `state.worktrees`.
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
    /// 'o' — Open palette (terminal surfaces and Metro debugger)
    Open,
}

// ---------------------------------------------------------------------------
// Sub-structs (Plan 13-10 / F-209). Each groups a cohesive concern.
// Per-worktree Metro runtime state lives in WorktreeSlice.
// ---------------------------------------------------------------------------

/// Metro lifecycle coordination flags that are not owned by a specific slice.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCachedIosRun {
    pub worktree_id: crate::domain::worktree::WorktreeId,
    pub worktree_path: std::path::PathBuf,
    pub cache_hit: crate::domain::native_cache::IosSimulatorCacheHit,
    pub device_request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCachedAndroidRun {
    pub worktree_id: crate::domain::worktree::WorktreeId,
    pub worktree_path: std::path::PathBuf,
    pub cache_hit: crate::domain::native_cache::AndroidCacheHit,
    pub device_request_id: u64,
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

    /// Pending cached iOS run — stored while async simulator enumeration is in flight.
    pub pending_cached_ios_run: Option<PendingCachedIosRun>,

    /// Pending cached Android run — stored while async target enumeration is in flight.
    pub pending_cached_android_run: Option<PendingCachedAndroidRun>,

    /// Monotonic request id used to ignore stale cached iOS device enumeration callbacks.
    pub next_device_request_id: u64,

    /// Worktree removal — set when w>d is pressed, consumed by ModalConfirm.
    pub pending_worktree_removal: Option<(
        crate::domain::worktree::WorktreeId,
        std::path::PathBuf,
        String,
    )>,

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

    /// Loaded simulator UDID history (most-recent first). Used by update() to
    /// sort iOS picker entries without crossing the infra boundary.
    pub sim_history: Vec<String>,

    /// True when a supported terminal surface (tmux, zellij, or Ghostty) is detected at startup.
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
            sim_history: Vec::new(),
            multiplexer_available: false,
        }
    }
}

/// Application state — the single source of truth. All mutations happen in update().
///
/// Plan 13-10 (F-209): regrouped from ~30 flat pub fields into 4 cross-cutting
/// roots + 5 domain sub-structs (Plan 14-09 removed
/// `CommandRunnerState`; its fields migrated to per-worktree `WorktreeSlice`).
#[derive(Debug, Default)]
pub struct AppState {
    // --- Cross-cutting / top-level UI concerns ---
    pub focused_panel: FocusedPanel,
    pub show_help: bool,
    pub error_state: Option<ErrorState>,
    pub should_quit: bool,

    // --- Domain sub-structs ---
    pub metro_state: MetroState,
    pub worktree_browser: WorktreeBrowserState,
    pub modal_stack: ModalStackState,
    pub jira: JiraState,
    pub app_config: AppConfigState,

    /// Phase 14 / D-16: per-worktree task slice map at AppState root.
    /// Keyed by `WorktreeId`. Replaces (incrementally — Plan 14-09 finishes the
    /// migration) the 4 global fields on `CommandRunnerState`.
    pub worktrees: std::collections::HashMap<
        crate::domain::worktree::WorktreeId,
        crate::domain::worktree_slice::WorktreeSlice,
    >,
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
///
/// Plan 14-09: reads from `state.worktrees` slice (was `CommandRunnerState.command_output_by_worktree`).
pub fn active_output(state: &AppState) -> &std::collections::VecDeque<String> {
    static EMPTY: std::sync::LazyLock<std::collections::VecDeque<String>> =
        std::sync::LazyLock::new(std::collections::VecDeque::new);
    if let Some(id) = active_worktree_id(state) {
        state
            .worktrees
            .get(&id)
            .map(|s| &s.output)
            .unwrap_or(&EMPTY)
    } else {
        &EMPTY
    }
}

/// Returns the scroll offset for the active worktree's command output (0 if none selected).
///
/// Plan 14-09: reads from `state.worktrees` slice (was `CommandRunnerState.command_output_scroll_by_worktree`).
pub fn active_output_scroll(state: &AppState) -> usize {
    active_worktree_id(state)
        .and_then(|id| state.worktrees.get(&id).map(|s| s.output_scroll))
        .unwrap_or(0)
}

/// Phase 14 / D-07: convenient lookup of the running task in a worktree's slice.
/// Returns `None` if no slice exists for `id`, or if the slice has no current task.
pub fn task_for_worktree<'a>(
    state: &'a AppState,
    id: &crate::domain::worktree::WorktreeId,
) -> Option<&'a crate::domain::task::TaskRecord> {
    state.worktrees.get(id).and_then(|s| s.task.as_ref())
}

/// Phase 14 / D-17: merge `loaded` worktrees into `state.worktrees`.
///
/// - Surviving ids: existing slice kept (preserves task + queue + output).
/// - Removed ids: slice dropped; if it had a running task, `handle.abort()` is
///   called explicitly (Phase 14 contract — `Box<dyn TaskHandle>::Drop` is
///   not specified to abort; Phase 15 widens this).
/// - New ids: default slice inserted with the worktree's id.
///
/// Q4 short-circuit (RESEARCH lines 752-755): when the loaded set equals the
/// current set, the function does ONE HashSet build + ONE comparison and
/// returns without iterating. Cost is O(n) HashSet build vs. O(n²) naive.
pub fn merge_slices(state: &mut AppState, loaded: &[crate::domain::worktree::Worktree]) {
    let loaded_ids: std::collections::HashSet<_> = loaded.iter().map(|w| w.id.clone()).collect();
    let current_ids: std::collections::HashSet<_> = state.worktrees.keys().cloned().collect();
    if loaded_ids == current_ids {
        // Q4: identity refresh — no-op.
        return;
    }

    // Drop slices for worktrees that disappeared.
    state.worktrees.retain(|id, slice| {
        if !loaded_ids.contains(id) {
            // Phase 14 contract: explicit abort. Phase 15 widens to
            // SIGTERM/SIGKILL via TaskHandle::abort.
            if let Some(record) = slice.task.take() {
                record.handle.abort();
            }
            false
        } else {
            true
        }
    });

    // Insert default slices for new worktrees.
    for wt in loaded {
        state.worktrees.entry(wt.id.clone()).or_insert_with(|| {
            crate::domain::worktree_slice::WorktreeSlice {
                id: wt.id.clone(),
                ..Default::default()
            }
        });
    }
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
        assert!(
            state
                .worktrees
                .contains_key(&WorktreeId("wt-survivor".into()))
        );
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
            WorktreeSlice {
                id: WorktreeId("wt-1".into()),
                queue: q,
                ..Default::default()
            },
        );
        merge_slices(&mut state, &[wt("wt-1")]);
        assert_eq!(
            state
                .worktrees
                .get(&WorktreeId("wt-1".into()))
                .unwrap()
                .queue
                .len(),
            1
        );
    }
}
