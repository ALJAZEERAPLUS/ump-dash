//! Worktree domain types.
//!
//! A `Worktree` represents one git worktree in the UMP repository. The struct
//! holds all display-relevant state so the UI layer can render the worktree
//! list without making any I/O calls itself.

/// Unique identifier for a worktree. Newtype around String to prevent accidental mixing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct WorktreeId(pub String);

/// Metro bundler status for a given worktree.
#[derive(Debug, Clone, PartialEq)]
pub enum WorktreeMetroStatus {
    Running,
    Stopped,
}

/// Fully-populated Worktree struct. All fields needed for list rendering and
/// command dispatch are present here. Populated by the infrastructure layer
/// via `git worktree list --porcelain` + optional JIRA lookups.
#[derive(Debug, Clone, PartialEq)]
pub struct Worktree {
    /// Stable identifier derived from the worktree path.
    pub id: WorktreeId,
    /// Absolute path to the worktree root.
    pub path: std::path::PathBuf,
    /// Current branch name (e.g. "feature/UMP-1234-login").
    pub branch: String,
    /// Short SHA of HEAD commit (first 7 chars).
    pub head_sha: String,
    /// Whether the Metro bundler is currently running in this worktree.
    pub metro_status: WorktreeMetroStatus,
    /// JIRA ticket title fetched in Phase 4 (None until then).
    pub jira_title: Option<String>,
    /// True when `node_modules` mtime is older than `package.json` or `yarn.lock`.
    pub stale: bool,
    /// True when `ios/Pods` is missing or older than `ios/Podfile.lock`.
    pub stale_pods: bool,
    /// Extracted JIRA ticket key (e.g. "UMP-1234") from the branch name.
    /// Set during WorktreesLoaded via domain::jira::extract_jira_key().
    pub jira_key: Option<String>,
}

impl Worktree {
    /// Returns the worktree directory name as the display name for this worktree.
    /// Falls back to `"worktree"` when the path has no file-name component.
    ///
    /// Used for single-string contexts such as modal titles and status messages.
    /// The worktree list widget accesses fields directly for layout control.
    #[allow(dead_code)]
    pub fn display_name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("worktree")
    }

    /// Returns the worktree directory name as the single source of truth for naming.
    /// Falls back to `"worktree"` when the path has no file-name component.
    ///
    /// Used as the prefix for tab names: Claude tab, shell tab, editor tab, etc.
    /// e.g. `<dir>-claude`, `<dir>-shell`, `<dir>-editor`.
    pub fn preferred_prefix(&self) -> String {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("worktree")
            .to_string()
    }
}
