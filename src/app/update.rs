//! TEA reducer — `pub fn update` is the ONLY place `AppState` is mutated.
//!
//! Plan 13-07 (F-201 consumer): `update()` is now a PURE function —
//! `pub fn update(state: &mut AppState, action: Action) -> Vec<Effect>`.
//! Every side-effect that used to be an inline async spawn is now an
//! `Effect` variant pushed onto the returned `Vec<Effect>`. The event loop
//! (`src/app/runtime.rs`) calls `runner.run_effects(effects).await` after
//! every invocation.
//!
//! Grep invariants (enforced by `make arch-lint` G-04 / G-05):
//! - no async spawn primitives in this file (G-04)
//! - no HTTP client or process-launch imports anywhere under src/app/ (G-05)

use super::effect::Effect;
use super::state::{
    AppState, ErrorState, FocusedPanel, MAX_COMMAND_LINES, PaletteMode, PendingCachedAndroidRun,
    PendingCachedIosRun, active_output, active_worktree_id,
};
use crate::domain::action::Action;
use crate::domain::command::{
    CleanOptions, CollisionPolicy, CommandSpec, ModalState, RunVariant, android_avd_name,
    android_boot_avd_command,
};
use crate::domain::pipeline::{DependencyState, Recipe};
use crate::domain::ports::device_port::DeviceKind;
use crate::domain::review::{PullRequest, PullRequestFilter};
use crate::domain::task::ExitStatus;
use crate::domain::worktree::{Worktree, WorktreeId, WorktreeMetroStatus};
use crate::domain::worktree_slice::LastRunConfig;
use std::path::{Path, PathBuf};

// Plan 13-09 (F-204 consumer): the 11 inline prereq sites are rewritten to
// build a `Recipe` and call `Recipe::expand(&deps)` — the dispatcher never
// inlines prereq ordering. The boolean coordination flags
// `pending_metro_run`, `pending_metro_after_sync`, and `pending_switch_path`
// are deleted; their semantics are absorbed into the command_queue front-push
// pattern (deferred run waits for metro Ready), `post_drain_action` (post-
// queue-drain action), and direct `active_worktree_path` updates respectively.
//
// Site → Recipe variant mapping:
//   SyncBeforeRunAccept (auto-sync fast path + modal accept) → Recipe::SyncThenRun
//   SyncBeforeMetroAccept (modal accept) + WorktreeSwitch auto-sync → Recipe::SyncThenStartMetro
//   CleanConfirm                                                    → Recipe::Clean
//   RnReleaseBuild dispatch                                         → Recipe::ReleaseBuildAndInstall
//   GitResetHardFetch dispatch                                      → Recipe::GitFetchThenReset
//   needs_metro pre-dispatch (3 sites: CommandRun / CommandExited drain / SyncBeforeRunDecline)
//                                                                   → command_queue.push_front + MetroStart

const DEFAULT_METRO_PORT: u16 = 8081;

fn metro_worktree_id_from_path(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn selected_worktree_path(state: &AppState) -> Option<PathBuf> {
    let idx = state
        .worktree_browser
        .worktree_table_state
        .selected()
        .unwrap_or(0);
    state
        .worktree_browser
        .worktrees
        .get(idx.min(state.worktree_browser.worktrees.len().saturating_sub(1)))
        .map(|wt| wt.path.clone())
}

fn selected_worktree_snapshot(state: &AppState) -> Option<crate::domain::worktree::Worktree> {
    let idx = state
        .worktree_browser
        .worktree_table_state
        .selected()
        .unwrap_or(0);
    state
        .worktree_browser
        .worktrees
        .get(idx.min(state.worktree_browser.worktrees.len().saturating_sub(1)))
        .cloned()
}

fn pull_request_matches_search(pr: &PullRequest, search: &str) -> bool {
    if search.is_empty() {
        return true;
    }
    let needle = search.to_lowercase();
    pr.title.to_lowercase().contains(&needle) || pr.author.to_lowercase().contains(&needle)
}

fn visible_pull_request_count(pull_requests: &[PullRequest], search: &str) -> usize {
    pull_requests
        .iter()
        .filter(|pr| pull_request_matches_search(pr, search))
        .count()
}

fn selected_visible_pull_request(
    pull_requests: &[PullRequest],
    selected: usize,
    search: &str,
) -> Option<PullRequest> {
    pull_requests
        .iter()
        .filter(|pr| pull_request_matches_search(pr, search))
        .nth(selected)
        .cloned()
}

fn default_review_worktree_name(pr: &PullRequest) -> String {
    let sanitized = pr
        .head_ref_name
        .chars()
        .map(|c| match c {
            '/' | '\\' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches(['-', '.', ' ']).to_string();
    if trimmed.is_empty() {
        format!("pr-{}", pr.number)
    } else {
        trimmed
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
}

fn active_worktree_snapshot(
    state: &AppState,
) -> Option<(crate::domain::worktree::WorktreeId, PathBuf)> {
    let idx = state
        .worktree_browser
        .worktree_table_state
        .selected()
        .unwrap_or(0);
    let wt = state
        .worktree_browser
        .worktrees
        .get(idx.min(state.worktree_browser.worktrees.len().saturating_sub(1)))?;
    Some((wt.id.clone(), wt.path.clone()))
}

fn worktree_snapshot_for_id(state: &AppState, worktree_id: &WorktreeId) -> Option<PathBuf> {
    state
        .worktree_browser
        .worktrees
        .iter()
        .find(|wt| &wt.id == worktree_id)
        .map(|wt| wt.path.clone())
}

fn queue_ios_cache_lookup_for_worktree(
    state: &mut AppState,
    effects: &mut Vec<Effect>,
    worktree_id: crate::domain::worktree::WorktreeId,
    worktree_path: PathBuf,
) {
    let slice = state
        .worktrees
        .entry(worktree_id.clone())
        .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
            id: worktree_id.clone(),
            ..Default::default()
        });
    if matches!(
        slice.ios_simulator_cache,
        crate::domain::native_cache::IosSimulatorCacheState::Unknown
    ) {
        slice.ios_simulator_cache = crate::domain::native_cache::IosSimulatorCacheState::Checking;
    }
    effects.push(Effect::LookupIosSimulatorCache {
        worktree_id,
        worktree_path,
    });
}

fn queue_ios_cache_lookups_for_loaded_worktrees(state: &mut AppState, effects: &mut Vec<Effect>) {
    let worktrees = state
        .worktree_browser
        .worktrees
        .iter()
        .map(|wt| (wt.id.clone(), wt.path.clone()))
        .collect::<Vec<_>>();

    for (worktree_id, worktree_path) in worktrees {
        queue_ios_cache_lookup_for_worktree(state, effects, worktree_id, worktree_path);
    }
}

fn queue_android_cache_lookup_for_worktree(
    state: &mut AppState,
    effects: &mut Vec<Effect>,
    worktree_id: crate::domain::worktree::WorktreeId,
    worktree_path: PathBuf,
) {
    let slice = state
        .worktrees
        .entry(worktree_id.clone())
        .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
            id: worktree_id.clone(),
            ..Default::default()
        });
    if matches!(
        slice.android_cache,
        crate::domain::native_cache::AndroidCacheState::Unknown
    ) {
        slice.android_cache = crate::domain::native_cache::AndroidCacheState::Checking;
    }
    effects.push(Effect::LookupAndroidCache {
        worktree_id,
        worktree_path,
    });
}

fn queue_android_cache_lookups_for_loaded_worktrees(
    state: &mut AppState,
    effects: &mut Vec<Effect>,
) {
    let worktrees = state
        .worktree_browser
        .worktrees
        .iter()
        .map(|wt| (wt.id.clone(), wt.path.clone()))
        .collect::<Vec<_>>();

    for (worktree_id, worktree_path) in worktrees {
        queue_android_cache_lookup_for_worktree(state, effects, worktree_id, worktree_path);
    }
}

fn completed_ios_store_request(
    state: &AppState,
    worktree_id: &WorktreeId,
    completed_cmd: &CommandSpec,
    status: &ExitStatus,
) -> Option<Effect> {
    let CommandSpec::UmpRunIos { variant, .. } = completed_cmd else {
        return None;
    };
    if !matches!(status, ExitStatus::Success) {
        return None;
    }
    let worktree_path = state
        .worktree_browser
        .worktrees
        .iter()
        .find(|wt| &wt.id == worktree_id)?
        .path
        .clone();
    Some(Effect::StoreIosSimulatorCache {
        worktree_id: worktree_id.clone(),
        request: crate::domain::native_cache::IosSimulatorCacheStoreRequest {
            worktree_path,
            variant: variant.unwrap_or(RunVariant::Local).label().into(),
        },
    })
}

fn completed_android_store_request(
    state: &AppState,
    worktree_id: &WorktreeId,
    completed_cmd: &CommandSpec,
    status: &ExitStatus,
) -> Option<Effect> {
    let CommandSpec::UmpRunAndroid { variant, .. } = completed_cmd else {
        return None;
    };
    if !matches!(status, ExitStatus::Success) {
        return None;
    }
    let worktree_path = state
        .worktree_browser
        .worktrees
        .iter()
        .find(|wt| &wt.id == worktree_id)?
        .path
        .clone();
    Some(Effect::StoreAndroidCache {
        worktree_id: worktree_id.clone(),
        request: crate::domain::native_cache::AndroidCacheStoreRequest {
            worktree_path,
            variant: variant.unwrap_or(RunVariant::Local).label().into(),
        },
    })
}

fn active_metro_worktree_path(state: &AppState) -> PathBuf {
    state
        .metro_state
        .active_worktree_path
        .clone()
        .or_else(|| selected_worktree_path(state))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn active_metro_worktree_id(state: &AppState) -> String {
    metro_worktree_id_from_path(&active_metro_worktree_path(state))
}

fn slice_id_for_metro_worktree_id(state: &AppState, worktree_id: &str) -> Option<WorktreeId> {
    state
        .worktree_browser
        .worktrees
        .iter()
        .find(|wt| metro_worktree_id_from_path(&wt.path) == worktree_id)
        .map(|wt| wt.id.clone())
        .or_else(|| {
            state
                .worktrees
                .keys()
                .find(|id| id.0 == worktree_id)
                .cloned()
        })
}

fn active_metro_slice_id(state: &AppState) -> WorktreeId {
    if let Some(path) = state.metro_state.active_worktree_path.as_ref() {
        let metro_worktree_id = metro_worktree_id_from_path(path);
        return slice_id_for_metro_worktree_id(state, &metro_worktree_id)
            .unwrap_or(WorktreeId(metro_worktree_id));
    }
    active_worktree_id(state).unwrap_or_else(|| WorktreeId(active_metro_worktree_id(state)))
}

fn active_worktree_has_metro(state: &AppState) -> bool {
    let id = active_metro_slice_id(state);
    state
        .worktrees
        .get(&id)
        .map(|slice| slice.metro.is_running())
        .unwrap_or(false)
}

fn worktree_has_metro_or_reserved(state: &AppState, worktree_id: &WorktreeId) -> bool {
    state
        .worktrees
        .get(worktree_id)
        .map(|slice| slice.metro.is_running() || slice.metro.running_port().is_some())
        .unwrap_or(false)
}

fn worktree_yarn_stale(state: &AppState, worktree_id: &WorktreeId) -> bool {
    state
        .worktree_browser
        .worktrees
        .iter()
        .find(|wt| &wt.id == worktree_id)
        .map(|wt| wt.stale)
        .unwrap_or(false)
}

fn next_available_reserved_metro_port(state: &AppState) -> u16 {
    let mut port = DEFAULT_METRO_PORT;
    loop {
        let already_reserved = state
            .worktrees
            .values()
            .any(|slice| slice.metro.running_port() == Some(port));
        if !already_reserved {
            return port;
        }
        if port == u16::MAX {
            return DEFAULT_METRO_PORT;
        }
        port += 1;
    }
}

fn start_metro_for_worktree(
    state: &mut AppState,
    effects: &mut Vec<Effect>,
    worktree_id: WorktreeId,
) {
    if worktree_has_metro_or_reserved(state, &worktree_id) {
        return;
    }
    let Some(worktree_path) = worktree_snapshot_for_id(state, &worktree_id) else {
        tracing::warn!(
            "start_metro_for_worktree: unknown worktree {:?}, dropping start request",
            worktree_id
        );
        return;
    };

    let port = next_available_reserved_metro_port(state);
    let slice = state
        .worktrees
        .entry(worktree_id.clone())
        .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
            id: worktree_id.clone(),
            ..Default::default()
        });
    slice.metro.reserve_start(port);
    effects.push(Effect::SpawnMetro {
        worktree: worktree_path,
        port,
    });
}

fn ensure_metro_for_worktree(
    state: &mut AppState,
    effects: &mut Vec<Effect>,
    worktree_id: WorktreeId,
) {
    if worktree_has_metro_or_reserved(state, &worktree_id) {
        return;
    }

    let deps = DependencyState::new(worktree_yarn_stale(state, &worktree_id), false, false);
    let mut sequence = Recipe::SyncThenStartMetro.expand(&deps);
    if sequence.is_empty() {
        start_metro_for_worktree(state, effects, worktree_id);
        return;
    }

    let first = sequence.remove(0);
    let slice = state
        .worktrees
        .entry(worktree_id.clone())
        .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
            id: worktree_id.clone(),
            ..Default::default()
        });
    for cmd in sequence {
        slice.queue.push_back(cmd);
    }
    slice.post_drain = Some(Box::new(Action::MetroStartForWorktree {
        worktree_id: worktree_id.clone(),
    }));

    if let Some(eff) = dispatch_command_for_worktree(state, &worktree_id, first) {
        effects.push(eff);
    }
}

/// Directly dispatches a command without going through the pre-processing pipeline.
/// Used by ModalConfirm to run confirmed destructive commands, and internally after
/// text-input and device-picker modals complete.
///
/// Plan 14-09 (D-10, D-20): allocates a fresh TaskId, writes the $ separator into
/// the per-worktree slice, and returns Effect::SpawnTask — the single chokepoint
/// for spawning. Transitional legacy writes removed (Plan 14-09).
///
/// Plan 15-05 / TASK-05: BEFORE allocating a TaskId or writing the `$ argv`
/// line, the function consults the (discriminant, WorktreeId) collision gate
/// (D-05). If `slice.task` holds a running task whose spec discriminant
/// matches the incoming spec, `spec.collision_policy()` decides:
/// - `BlockNew` → return None; slice untouched; no `$ argv` line.
/// - `CancelPrevious` → abort the previous task, clear the slice queue,
///   push `[cancelled by new dispatch]`, then fall through to normal
///   dispatch with the new task_id.
fn dispatch_command(state: &mut AppState, spec: CommandSpec) -> Option<Effect> {
    let Some(wt_id) = active_worktree_id(state) else {
        // No worktrees loaded yet — can't dispatch; log to a fallback message (no per-worktree key)
        tracing::warn!(
            "dispatch_command: no worktree selected, dropping command {:?}",
            spec.label()
        );
        return None;
    };
    dispatch_command_for_worktree(state, &wt_id, spec)
}

fn dispatch_command_for_worktree(
    state: &mut AppState,
    worktree_id: &WorktreeId,
    spec: CommandSpec,
) -> Option<Effect> {
    let Some(wt) = state
        .worktree_browser
        .worktrees
        .iter()
        .find(|wt| &wt.id == worktree_id)
        .cloned()
    else {
        tracing::warn!(
            "dispatch_command_for_worktree: unknown worktree {:?}, dropping command {:?}",
            worktree_id,
            spec.label()
        );
        return None;
    };

    let wt_id = wt.id.clone();

    // Phase 15 / TASK-05 collision gate (D-05, discriminant identity).
    // BEFORE allocating a new TaskId or writing the $ argv line, check whether
    // an existing task on the same worktree shares the spec discriminant.
    // Two-pass approach: immutable borrow to read existing.spec discriminant,
    // then (if CancelPrevious) mutable borrow to take + abort. This avoids the
    // borrow-checker conflict of holding `slice` mutably across the abort call.
    let collision_cancel_previous: bool = if let Some(slice) = state.worktrees.get(&wt_id)
        && let Some(existing) = slice.task.as_ref()
        && std::mem::discriminant(&existing.spec) == std::mem::discriminant(&spec)
    {
        match spec.collision_policy() {
            CollisionPolicy::BlockNew => return None,
            CollisionPolicy::CancelPrevious => true,
        }
    } else {
        false
    };

    if collision_cancel_previous && let Some(slice) = state.worktrees.get_mut(&wt_id) {
        if let Some(record) = slice.task.take() {
            record.handle.abort();
        }
        slice.queue.clear();
        slice.output.push_back("[cancelled by new dispatch]".into());
        while slice.output.len() > MAX_COMMAND_LINES {
            slice.output.pop_front();
        }
    }

    let task_id = crate::domain::task::TaskId::next();
    let argv_line = format!("$ {}", spec.to_argv().join(" "));

    // Slice-local output. Defensive insertion in case the merge_slices pass
    // hasn't run yet (e.g., during early startup before first WorktreesLoaded).
    let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
        crate::domain::worktree_slice::WorktreeSlice {
            id: wt_id.clone(),
            ..Default::default()
        }
    });
    slice.output.push_back(argv_line);
    while slice.output.len() > MAX_COMMAND_LINES {
        slice.output.pop_front();
    }
    slice.output_scroll = 0;

    // D-10 / D-20: the spawn chokepoint.
    // Plan 15-03 / TASK-06: `repo_root` is propagated so effect_runner can
    // key the per-repo-root yarn semaphore. Sourced from AppState (the single
    // source of truth set at startup by main.rs).
    Some(Effect::SpawnTask {
        task_id,
        worktree_id: wt_id,
        spec,
        cwd: wt.path.clone(),
        branch: wt.branch.clone(),
        repo_root: state.app_config.repo_root.clone(),
    })
}

fn is_ump_run(spec: &CommandSpec) -> bool {
    matches!(
        spec,
        CommandSpec::UmpRunAndroid { .. } | CommandSpec::UmpRunIos { .. }
    )
}

fn is_run_command(spec: &CommandSpec) -> bool {
    matches!(
        spec,
        CommandSpec::UmpRunAndroid { .. }
            | CommandSpec::UmpRunIos { .. }
            | CommandSpec::RnReleaseBuild
    )
}

fn is_ios_run_command(spec: &CommandSpec) -> bool {
    matches!(spec, CommandSpec::UmpRunIos { .. })
}

fn command_with_device(spec: CommandSpec, device_id: String) -> CommandSpec {
    match spec {
        CommandSpec::UmpRunAndroid { variant, .. } => {
            CommandSpec::UmpRunAndroid { device_id, variant }
        }
        CommandSpec::UmpRunIos { variant, .. } => CommandSpec::UmpRunIos { device_id, variant },
        other => other,
    }
}

fn command_with_run_variant(spec: CommandSpec, variant: RunVariant) -> CommandSpec {
    match spec {
        CommandSpec::UmpRunAndroid { device_id, .. } => CommandSpec::UmpRunAndroid {
            device_id,
            variant: Some(variant),
        },
        CommandSpec::UmpRunIos { device_id, .. } => CommandSpec::UmpRunIos {
            device_id,
            variant: Some(variant),
        },
        other => other,
    }
}

fn remember_ump_run_config(state: &mut AppState, spec: &CommandSpec, cache_launch_supported: bool) {
    let Some(wt_id) = active_worktree_id(state) else {
        return;
    };

    let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
        crate::domain::worktree_slice::WorktreeSlice {
            id: wt_id,
            ..Default::default()
        }
    });

    match spec {
        CommandSpec::UmpRunAndroid {
            device_id,
            variant: Some(variant),
        } => {
            let cache_launch_supported = cache_launch_supported
                || slice.last_android_run.as_ref().is_some_and(|config| {
                    config.device_id == *device_id
                        && config.variant == *variant
                        && config.cache_launch_supported
                });
            slice.last_android_run = Some(LastRunConfig {
                device_id: device_id.clone(),
                variant: *variant,
                cache_launch_supported,
            });
        }
        CommandSpec::UmpRunIos {
            device_id,
            variant: Some(variant),
        } => {
            let cache_launch_supported = cache_launch_supported
                || slice.last_ios_run.as_ref().is_some_and(|config| {
                    config.device_id == *device_id
                        && config.variant == *variant
                        && config.cache_launch_supported
                });
            slice.last_ios_run = Some(LastRunConfig {
                device_id: device_id.clone(),
                variant: *variant,
                cache_launch_supported,
            });
        }
        _ => {}
    }
}

fn ios_device_supports_cached_launch(device: &crate::domain::command::DeviceInfo) -> bool {
    device.name.ends_with(" (Booted)")
        || device.name.ends_with(" (Shutdown)")
        || device.name.ends_with(" (Creating)")
        || device.name.ends_with(" (Booting)")
        || device.name.ends_with(" (Shutting Down)")
        || device.name.ends_with(" (Unknown)")
}

fn selected_ios_cache_hit_for_variant(
    state: &AppState,
    variant: RunVariant,
) -> Option<crate::domain::native_cache::IosSimulatorCacheHit> {
    let id = active_worktree_id(state)?;
    let selected_cache = &state.worktrees.get(&id)?.ios_simulator_cache;
    if let Some(hit) = selected_cache.hit() {
        return (hit.metadata.variant == variant.label()).then(|| hit.clone());
    }

    let crate::domain::native_cache::IosSimulatorCacheState::Miss { fingerprint } = selected_cache
    else {
        return None;
    };

    state
        .worktrees
        .values()
        .filter_map(|slice| slice.ios_simulator_cache.hit())
        .find(|hit| {
            hit.metadata.fingerprint == *fingerprint && hit.metadata.variant == variant.label()
        })
        .cloned()
}

fn selected_android_cache_hit_for_variant(
    state: &AppState,
    variant: RunVariant,
) -> Option<crate::domain::native_cache::AndroidCacheHit> {
    let id = active_worktree_id(state)?;
    let selected_cache = &state.worktrees.get(&id)?.android_cache;
    if let Some(hit) = selected_cache.hit() {
        return android_cache_variant_matches(&hit.metadata.variant, variant).then(|| hit.clone());
    }

    let crate::domain::native_cache::AndroidCacheState::Miss { fingerprint } = selected_cache
    else {
        return None;
    };

    state
        .worktrees
        .values()
        .filter_map(|slice| slice.android_cache.hit())
        .find(|hit| {
            hit.metadata.fingerprint == *fingerprint
                && android_cache_variant_matches(&hit.metadata.variant, variant)
        })
        .cloned()
}

fn android_cache_variant_matches(metadata_variant: &str, variant: RunVariant) -> bool {
    metadata_variant == variant.label()
        || matches!(
            (variant, metadata_variant),
            (RunVariant::Local, "localDebugOptimized")
                | (RunVariant::Dev, "devDebugOptimized")
                | (RunVariant::Prod, "prodDebug")
        )
}

fn ios_cache_hit_variant(
    cache_hit: &crate::domain::native_cache::IosSimulatorCacheHit,
) -> RunVariant {
    RunVariant::ALL
        .iter()
        .copied()
        .find(|variant| cache_hit.metadata.variant == variant.label())
        .unwrap_or(RunVariant::Local)
}

fn android_cache_hit_variant(
    cache_hit: &crate::domain::native_cache::AndroidCacheHit,
) -> RunVariant {
    RunVariant::ALL
        .iter()
        .copied()
        .find(|variant| android_cache_variant_matches(&cache_hit.metadata.variant, *variant))
        .unwrap_or(RunVariant::Local)
}

fn cached_variants_for_run_picker(
    state: &AppState,
    spec: &CommandSpec,
    cache_launch_supported: bool,
) -> [bool; 3] {
    if !cache_launch_supported {
        return [false; 3];
    }

    let mut cached = [false; 3];
    for (idx, variant) in RunVariant::ALL.iter().enumerate() {
        cached[idx] = match spec {
            CommandSpec::UmpRunIos { .. } => {
                selected_ios_cache_hit_for_variant(state, *variant).is_some()
            }
            CommandSpec::UmpRunAndroid { .. } => {
                selected_android_cache_hit_for_variant(state, *variant).is_some()
            }
            _ => false,
        };
    }
    cached
}

fn refresh_open_run_variant_picker_cache_flags<F>(state: &mut AppState, matches_spec: F)
where
    F: FnOnce(&CommandSpec) -> bool,
{
    let Some(ModalState::RunVariantPicker {
        pending_template,
        cache_launch_supported,
        ..
    }) = state.modal_stack.modal.as_ref()
    else {
        return;
    };
    if !*cache_launch_supported {
        return;
    }

    let spec = pending_template.as_ref().clone();
    if !matches_spec(&spec) {
        return;
    }

    let refreshed = cached_variants_for_run_picker(state, &spec, *cache_launch_supported);
    if let Some(ModalState::RunVariantPicker {
        cached_variants, ..
    }) = state.modal_stack.modal.as_mut()
    {
        *cached_variants = refreshed;
    }
}

fn try_begin_cached_run_for_spec(
    state: &mut AppState,
    effects: &mut Vec<Effect>,
    spec: &CommandSpec,
    cache_launch_supported: bool,
) -> bool {
    if !cache_launch_supported {
        return false;
    }
    let Some((worktree_id, worktree_path)) = active_worktree_snapshot(state) else {
        return false;
    };

    match spec {
        CommandSpec::UmpRunIos {
            device_id,
            variant: Some(variant),
        } => {
            if let Some(cache_hit) = selected_ios_cache_hit_for_variant(state, *variant) {
                begin_cached_ios_launch(
                    state,
                    effects,
                    worktree_id,
                    worktree_path,
                    device_id.clone(),
                    cache_hit,
                );
                true
            } else {
                false
            }
        }
        CommandSpec::UmpRunAndroid {
            device_id,
            variant: Some(variant),
        } => {
            if let Some(cache_hit) = selected_android_cache_hit_for_variant(state, *variant) {
                begin_cached_android_launch(
                    state,
                    effects,
                    worktree_id,
                    worktree_path,
                    device_id.clone(),
                    cache_hit,
                );
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

fn open_run_variant_picker_or_dispatch(
    state: &mut AppState,
    effects: &mut Vec<Effect>,
    spec: CommandSpec,
    boot_android_emulator: bool,
    cache_launch_supported: bool,
) {
    if spec.needs_run_variant_selection() {
        let cached_variants = cached_variants_for_run_picker(state, &spec, cache_launch_supported);
        let selected = cached_variants
            .iter()
            .position(|cached| *cached)
            .unwrap_or(0);
        state.modal_stack.modal = Some(ModalState::RunVariantPicker {
            selected,
            pending_template: Box::new(spec),
            boot_android_emulator,
            cache_launch_supported,
            cached_variants,
        });
        return;
    }

    effects.extend(update(
        state,
        Action::CommandRunWithCache {
            spec,
            cache_launch_supported,
        },
    ));
}

fn cached_ios_launch_request(
    cache_hit: &crate::domain::native_cache::IosSimulatorCacheHit,
    device_id: String,
    metro_port: u16,
) -> crate::domain::native_cache::CachedIosLaunchRequest {
    crate::domain::native_cache::CachedIosLaunchRequest {
        simulator_udid: device_id,
        app_path: cache_hit.artifact_path.clone(),
        bundle_id: cache_hit.metadata.bundle_id.clone(),
        metro_port,
        fingerprint: cache_hit.metadata.fingerprint.clone(),
        variant: ios_cache_hit_variant(cache_hit),
        artifact_digest_algorithm: cache_hit.metadata.artifact_digest_algorithm.clone(),
        artifact_digest: cache_hit.metadata.artifact_digest.clone(),
    }
}

fn cached_android_launch_request(
    cache_hit: &crate::domain::native_cache::AndroidCacheHit,
    device_id: String,
    metro_port: u16,
) -> crate::domain::native_cache::CachedAndroidLaunchRequest {
    crate::domain::native_cache::CachedAndroidLaunchRequest {
        device_id,
        apk_path: cache_hit.artifact_path.clone(),
        application_id: cache_hit.metadata.application_id.clone(),
        metro_port,
        fingerprint: cache_hit.metadata.fingerprint.clone(),
        variant: android_cache_hit_variant(cache_hit),
        artifact_digest_algorithm: cache_hit.metadata.artifact_digest_algorithm.clone(),
        artifact_digest: cache_hit.metadata.artifact_digest.clone(),
    }
}

fn begin_cached_ios_launch(
    state: &mut AppState,
    effects: &mut Vec<Effect>,
    worktree_id: WorktreeId,
    _worktree_path: PathBuf,
    device_id: String,
    cache_hit: crate::domain::native_cache::IosSimulatorCacheHit,
) {
    effects.push(Effect::ScheduleAction(Action::SimulatorUsed(
        device_id.clone(),
    )));

    let launch_port = {
        let slice = state
            .worktrees
            .entry(worktree_id.clone())
            .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
                id: worktree_id.clone(),
                ..Default::default()
            });
        slice.metro.process_port()
    };

    if let Some(port) = launch_port {
        effects.push(Effect::InstallAndLaunchCachedIosSimulator {
            worktree_id,
            request: cached_ios_launch_request(&cache_hit, device_id, port),
        });
        return;
    }

    {
        let slice = state
            .worktrees
            .entry(worktree_id.clone())
            .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
                id: worktree_id.clone(),
                ..Default::default()
            });
        slice.pending_cached_ios_launch =
            Some(crate::domain::native_cache::PendingCachedIosLaunch {
                device_id,
                cache_hit,
            });
    }
    effects.extend(update(state, Action::MetroStartForWorktree { worktree_id }));
}

fn begin_cached_android_launch(
    state: &mut AppState,
    effects: &mut Vec<Effect>,
    worktree_id: WorktreeId,
    _worktree_path: PathBuf,
    device_id: String,
    cache_hit: crate::domain::native_cache::AndroidCacheHit,
) {
    let launch_port = {
        let slice = state
            .worktrees
            .entry(worktree_id.clone())
            .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
                id: worktree_id.clone(),
                ..Default::default()
            });
        slice.metro.process_port()
    };

    if let Some(port) = launch_port {
        effects.push(Effect::InstallAndLaunchCachedAndroid {
            worktree_id,
            request: cached_android_launch_request(&cache_hit, device_id, port),
        });
        return;
    }

    {
        let slice = state
            .worktrees
            .entry(worktree_id.clone())
            .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
                id: worktree_id.clone(),
                ..Default::default()
            });
        slice.pending_cached_android_launch =
            Some(crate::domain::native_cache::PendingCachedAndroidLaunch {
                device_id,
                cache_hit,
            });
    }
    effects.extend(update(state, Action::MetroStartForWorktree { worktree_id }));
}

/// TEA update function — the ONLY place AppState is mutated.
///
/// Post-F-201 (Plan 13-07) signature: pure `(state, action) -> Vec<Effect>`.
/// Every async dispatch / channel send / recursive self-dispatch is
/// expressed as an Effect variant pushed onto the returned vec. The event
/// loop interprets effects via `EffectRunner::run_effects`.
pub fn update(state: &mut AppState, action: Action) -> Vec<Effect> {
    let mut effects: Vec<Effect> = Vec::new();

    // Clear pending_g on any action except SetPendingG
    if !matches!(action, Action::SetPendingG) {
        state.modal_stack.pending_g = false;
    }

    match action {
        // Phase 1 actions
        Action::FocusNext => state.focused_panel = state.focused_panel.next(),
        Action::FocusPrev => state.focused_panel = state.focused_panel.prev(),
        Action::FocusUp => {
            if state.focused_panel == FocusedPanel::CommandOutput
                && let Some(id) = active_worktree_id(state)
                && let Some(slice) = state.worktrees.get_mut(&id)
            {
                slice.output_scroll = slice.output_scroll.saturating_sub(1);
            }
        }
        Action::FocusDown => {
            if state.focused_panel == FocusedPanel::CommandOutput {
                let max = active_output(state).len();
                if let Some(id) = active_worktree_id(state)
                    && let Some(slice) = state.worktrees.get_mut(&id)
                    && slice.output_scroll < max
                {
                    slice.output_scroll += 1;
                }
            }
        }
        Action::FocusLeft => {}
        Action::FocusRight => {}
        Action::Search => {
            // Phase 4+: stub
        }
        Action::ShowHelp => state.show_help = true,
        Action::DismissHelp => state.show_help = false,
        Action::DismissError => state.error_state = None,
        Action::RetryLastCommand => {
            state.error_state = None;
        }
        Action::Quit => state.should_quit = true,

        // --- Metro control actions ---
        Action::MetroStart => {
            state.modal_stack.palette_mode = None;
            let Some(worktree_id) = active_worktree_id(state) else {
                return effects;
            };
            state.metro_state.skip_external_metro_check = false;
            effects.extend(update(state, Action::MetroStartForWorktree { worktree_id }));
        }

        Action::MetroStartForWorktree { worktree_id } => {
            state.modal_stack.palette_mode = None;
            ensure_metro_for_worktree(state, &mut effects, worktree_id);
        }

        Action::MetroStartConfirmed => {
            let worktree_id = active_metro_slice_id(state);
            effects.extend(update(
                state,
                Action::MetroStartConfirmedForWorktree { worktree_id },
            ));
        }

        Action::MetroStartConfirmedForWorktree { worktree_id } => {
            start_metro_for_worktree(state, &mut effects, worktree_id);
        }

        Action::MetroStop => {
            state.modal_stack.palette_mode = None;
            let slice_id = active_metro_slice_id(state);
            if let Some(slice) = state.worktrees.get_mut(&slice_id)
                && let Some(handle) = slice.metro.take_handle()
            {
                slice.metro.set_stopping();
                // Plan 13-03: kill() is the consuming trait method.
                // Post-13-07, the handle is a TokioMetroHandle from TokioMetroAdapter.
                if let Err(e) = handle.kill() {
                    tracing::warn!("metro handle kill failed: {e}");
                }
            }
        }

        Action::MetroSendDebugger => {
            state.modal_stack.palette_mode = None;
            let slice_id = active_metro_slice_id(state);
            if let Some(port) = state
                .worktrees
                .get(&slice_id)
                .and_then(|slice| slice.metro.running_port())
            {
                effects.push(Effect::MetroHttpPost {
                    url: format!("http://localhost:{port}/open-debugger"),
                    body: "{}".to_string(),
                });
            }
        }

        Action::MetroSendReload => {
            state.modal_stack.palette_mode = None;
            let slice_id = active_metro_slice_id(state);
            if let Some(port) = state
                .worktrees
                .get(&slice_id)
                .and_then(|slice| slice.metro.running_port())
            {
                effects.push(Effect::MetroHttpPost {
                    url: format!("http://localhost:{port}/reload"),
                    body: String::new(),
                });
            }
        }

        Action::MetroExited(worktree_id) => {
            // Plan 13-09: pending_metro_run is gone — the deferred run command
            // (if any) sits in command_queue.front() and is drained on the
            // next MetroActivityUpdate(Ready). If metro exited unexpectedly,
            // clear the queue so a stale deferred command doesn't fire on the
            // next successful start.
            if !state.metro_state.pending_restart
                && let Some(slice_id) = state
                    .worktree_browser
                    .worktrees
                    .iter()
                    .find(|wt| metro_worktree_id_from_path(&wt.path) == worktree_id)
                    .map(|wt| wt.id.clone())
                && let Some(slice) = state.worktrees.get_mut(&slice_id)
            {
                slice.queue.clear();
                slice.post_drain = None;
            }
            if let Some(slice_id) = slice_id_for_metro_worktree_id(state, &worktree_id)
                && let Some(slice) = state.worktrees.get_mut(&slice_id)
            {
                let had_pending_cached_launch = slice.pending_cached_ios_launch.take().is_some();
                let had_pending_cached_android_launch =
                    slice.pending_cached_android_launch.take().is_some();
                if had_pending_cached_launch {
                    slice
                        .output
                        .push_back("[cached-ios error] Metro exited before cached launch".into());
                }
                if had_pending_cached_android_launch {
                    slice.output.push_back(
                        "[cached-android error] Metro exited before cached launch".into(),
                    );
                }
                if had_pending_cached_launch || had_pending_cached_android_launch {
                    while slice.output.len() > MAX_COMMAND_LINES {
                        slice.output.pop_front();
                    }
                }
                slice.metro.clear();
            }
            if state.metro_state.pending_restart {
                state.metro_state.pending_restart = false;
                // active_worktree_path is already updated synchronously at the
                // worktree-switch call site (Plan 13-09 — no pending_switch_path).
                state.metro_state.skip_external_metro_check = true;
                let worktree_id = active_metro_slice_id(state);
                effects.extend(update(state, Action::MetroStartForWorktree { worktree_id }));
            }
            // Refresh worktree list so metro status (green bg) updates immediately
            effects.extend(update(state, Action::RefreshWorktrees));
        }

        Action::MetroSpawnFailed {
            worktree_id,
            message,
        } => {
            // Plan 13-09: clear queue + post-drain action; pending flags gone.
            if let Some(slice_id) = state
                .worktree_browser
                .worktrees
                .iter()
                .find(|wt| metro_worktree_id_from_path(&wt.path) == worktree_id)
                .map(|wt| wt.id.clone())
                && let Some(slice) = state.worktrees.get_mut(&slice_id)
            {
                slice.queue.clear();
                slice.post_drain = None;
            }
            if let Some(slice_id) = slice_id_for_metro_worktree_id(state, &worktree_id)
                && let Some(slice) = state.worktrees.get_mut(&slice_id)
            {
                let had_pending_cached_launch = slice.pending_cached_ios_launch.take().is_some();
                let had_pending_cached_android_launch =
                    slice.pending_cached_android_launch.take().is_some();
                slice.metro.clear();
                if had_pending_cached_launch {
                    slice.output.push_back(format!(
                        "[cached-ios error] Metro failed to start: {message}"
                    ));
                }
                if had_pending_cached_android_launch {
                    slice.output.push_back(format!(
                        "[cached-android error] Metro failed to start: {message}"
                    ));
                }
                if had_pending_cached_launch || had_pending_cached_android_launch {
                    while slice.output.len() > MAX_COMMAND_LINES {
                        slice.output.pop_front();
                    }
                }
            }
            state.metro_state.pending_restart = false;
            state.error_state = Some(ErrorState {
                message: format!("Metro failed to start: {message}"),
                can_retry: true,
            });
        }

        Action::MetroActivityUpdate {
            worktree_id,
            activity,
        } => {
            if let Some(slice_id) = slice_id_for_metro_worktree_id(state, &worktree_id)
                && let Some(slice) = state.worktrees.get_mut(&slice_id)
            {
                slice.metro.record_activity(activity.clone());
            }
            // Plan 13-09: pending_metro_run absorbed into command_queue. When
            // metro becomes Ready and nothing is currently running, drain the
            // queue front via CommandRun (preserves the full pipeline:
            // sync-check, device selection, etc.). The push_front-on-defer
            // pattern at the dispatch sites guarantees the awaited spec sits
            // at the head of the queue.
            if matches!(activity, crate::domain::metro::MetroActivity::Ready) {
                // Phase 14 D-13 PRIMARY: drain the queue for the worktree whose
                // Metro instance just became ready.
                let candidate_id = state
                    .worktrees
                    .iter()
                    .find(|(id, s)| {
                        state
                            .worktree_browser
                            .worktrees
                            .iter()
                            .find(|wt| &wt.id == *id)
                            .map(|wt| metro_worktree_id_from_path(&wt.path) == worktree_id)
                            .unwrap_or(false)
                            && s.task.is_none()
                            && s.queue.front().map(|c| c.needs_metro()).unwrap_or(false)
                    })
                    .map(|(id, _)| id.clone());

                if let Some(ref id) = candidate_id
                    && let Some(slice) = state.worktrees.get_mut(id)
                    && let Some(spec) = slice.queue.pop_front()
                    && let Some(effect) = dispatch_command_for_worktree(state, id, spec)
                {
                    effects.push(effect);
                }

                if let Some(slice_id) = slice_id_for_metro_worktree_id(state, &worktree_id) {
                    let launch = state.worktrees.get_mut(&slice_id).and_then(|slice| {
                        let port = slice.metro.running_port()?;
                        slice
                            .pending_cached_ios_launch
                            .take()
                            .map(|pending| (port, pending))
                    });
                    if let Some((port, pending)) = launch {
                        effects.push(Effect::InstallAndLaunchCachedIosSimulator {
                            worktree_id: slice_id,
                            request: cached_ios_launch_request(
                                &pending.cache_hit,
                                pending.device_id,
                                port,
                            ),
                        });
                    }
                }
                if let Some(slice_id) = slice_id_for_metro_worktree_id(state, &worktree_id) {
                    let launch = state.worktrees.get_mut(&slice_id).and_then(|slice| {
                        let port = slice.metro.running_port()?;
                        slice
                            .pending_cached_android_launch
                            .take()
                            .map(|pending| (port, pending))
                    });
                    if let Some((port, pending)) = launch {
                        effects.push(Effect::InstallAndLaunchCachedAndroid {
                            worktree_id: slice_id,
                            request: cached_android_launch_request(
                                &pending.cache_hit,
                                pending.device_id,
                                port,
                            ),
                        });
                    }
                }
            }
        }

        Action::ExternalMetroDetected(info) => {
            state.modal_stack.modal = Some(ModalState::ExternalMetroConflict {
                pid: info.pid,
                working_dir: info.working_dir,
            });
        }

        Action::KillExternalMetro(pid) => {
            state.modal_stack.modal = None;
            // F-201: effect_runner handles the kill + 500ms wait + schedule
            // MetroStartConfirmed follow-up.
            effects.push(Effect::KillProcess { pid });
        }

        // --- Phase 3: Worktree navigation ---
        Action::WorktreeSelectNext => {
            let len = state.worktree_browser.worktrees.len();
            if len > 0 {
                let i = state
                    .worktree_browser
                    .worktree_table_state
                    .selected()
                    .unwrap_or(0);
                let next = if i >= len - 1 { 0 } else { i + 1 };
                state
                    .worktree_browser
                    .worktree_table_state
                    .select(Some(next));
                // Update stable selection id
                state.worktree_browser.selected_worktree_id =
                    Some(state.worktree_browser.worktrees[next].id.clone());
                // Update active worktree for metro
                state.metro_state.active_worktree_path =
                    Some(state.worktree_browser.worktrees[next].path.clone());
            }
        }

        Action::WorktreeSelectPrev => {
            let len = state.worktree_browser.worktrees.len();
            if len > 0 {
                let i = state
                    .worktree_browser
                    .worktree_table_state
                    .selected()
                    .unwrap_or(0);
                let prev = if i == 0 { len - 1 } else { i - 1 };
                state
                    .worktree_browser
                    .worktree_table_state
                    .select(Some(prev));
                // Update stable selection id
                state.worktree_browser.selected_worktree_id =
                    Some(state.worktree_browser.worktrees[prev].id.clone());
                // Update active worktree for metro
                state.metro_state.active_worktree_path =
                    Some(state.worktree_browser.worktrees[prev].path.clone());
            }
        }

        Action::WorktreesLoaded(mut worktrees) => {
            // Re-derive jira_key and re-apply cached JIRA titles using the configured prefix.
            // jira_key is set here (not in list_worktrees) because the prefix comes from config.
            for wt in &mut worktrees {
                if let Some(key) =
                    crate::domain::jira::extract_jira_key(&wt.branch, &state.jira.project_prefix)
                {
                    if let Some(title) = state.jira.title_cache.get(&key) {
                        wt.jira_title = Some(title.clone());
                    }
                    wt.jira_key = Some(key);
                }
            }

            // Derive metro_status from each worktree's slice-owned Metro state.
            for wt in &mut worktrees {
                if state
                    .worktrees
                    .get(&wt.id)
                    .map(|slice| slice.metro.is_running())
                    .unwrap_or(false)
                {
                    wt.metro_status = crate::domain::worktree::WorktreeMetroStatus::Running;
                }
            }

            state.worktree_browser.worktrees = worktrees;

            // Phase 14 / D-17: keep slice map in sync with the loaded worktree set.
            // Surviving ids: slice + queue + task preserved.
            // Removed ids: slice dropped; running task aborted explicitly.
            // New ids: empty default slice inserted.
            let loaded_for_merge: Vec<crate::domain::worktree::Worktree> =
                state.worktree_browser.worktrees.clone();
            crate::app::state::merge_slices(state, &loaded_for_merge);
            queue_ios_cache_lookups_for_loaded_worktrees(state, &mut effects);
            queue_android_cache_lookups_for_loaded_worktrees(state, &mut effects);

            if !state.worktree_browser.worktrees.is_empty() {
                // Re-derive selected index from selected_worktree_id (stable across sorts)
                let selected_idx = state
                    .worktree_browser
                    .selected_worktree_id
                    .as_ref()
                    .and_then(|id| {
                        state
                            .worktree_browser
                            .worktrees
                            .iter()
                            .position(|wt| &wt.id == id)
                    })
                    .unwrap_or(0);
                state
                    .worktree_browser
                    .worktree_table_state
                    .select(Some(selected_idx));
                state.metro_state.active_worktree_path =
                    Some(state.worktree_browser.worktrees[selected_idx].path.clone());
            }

            // Phase 4: fetch titles for uncached branches
            if state.jira.available {
                let keys_to_fetch: Vec<String> = state
                    .worktree_browser
                    .worktrees
                    .iter()
                    .filter_map(|wt| {
                        let key = crate::domain::jira::extract_jira_key(
                            &wt.branch,
                            &state.jira.project_prefix,
                        )?;
                        if state.jira.title_cache.contains_key(&key) {
                            return None;
                        }
                        Some(key)
                    })
                    .collect();

                if !keys_to_fetch.is_empty() {
                    effects.push(Effect::FetchJiraTitles {
                        keys: keys_to_fetch,
                    });
                }
            }
        }

        Action::RefreshWorktrees => {
            if state.worktree_browser.worktree_op_in_flight {
                tracing::debug!("skipping periodic refresh — worktree op in flight");
                return effects;
            }
            effects.push(Effect::ListWorktrees {
                repo_root: state.app_config.repo_root.clone(),
            });
        }

        // --- Phase 3: Command dispatch ---
        Action::CommandRunWithCache {
            spec,
            cache_launch_supported,
        } => {
            state.modal_stack.palette_mode = None;
            remember_ump_run_config(state, &spec, cache_launch_supported);
            if try_begin_cached_run_for_spec(state, &mut effects, &spec, cache_launch_supported) {
                return effects;
            }
            effects.extend(update(state, Action::CommandRun(spec)));
        }

        Action::CommandRun(spec) => {
            // Clear palette mode whenever a command is dispatched
            state.modal_stack.palette_mode = None;
            remember_ump_run_config(state, &spec, false);

            // Get selected worktree (needed for all branches)
            let wt_branch = if !state.worktree_browser.worktrees.is_empty() {
                let idx = state
                    .worktree_browser
                    .worktree_table_state
                    .selected()
                    .unwrap_or(0);
                let idx = idx.min(state.worktree_browser.worktrees.len() - 1);
                Some((
                    state.worktree_browser.worktrees[idx].branch.clone(),
                    state.worktree_browser.worktrees[idx].stale,
                ))
            } else {
                None
            };

            if is_ump_run(&spec) {
                if spec.needs_device_selection() {
                    state.modal_stack.pending_device_command = Some(spec.clone());
                    let kind = if matches!(spec, CommandSpec::UmpRunAndroid { .. }) {
                        crate::domain::ports::device_port::DeviceKind::Android
                    } else {
                        crate::domain::ports::device_port::DeviceKind::Ios
                    };
                    effects.push(Effect::LoadDevices {
                        kind,
                        request_id: None,
                    });
                    return effects;
                }

                if spec.needs_run_variant_selection() {
                    state.modal_stack.modal = Some(ModalState::RunVariantPicker {
                        selected: 0,
                        pending_template: Box::new(spec),
                        boot_android_emulator: false,
                        cache_launch_supported: false,
                        cached_variants: [false; 3],
                    });
                    return effects;
                }
            }

            // Sync-before-run: stale worktree + run command triggers prompt.
            // This MUST run BEFORE the metro start check — yarn install is a
            // dependency of metro, so running metro against stale deps boots it
            // with a broken dependency tree. Also triggers when ONLY pods are
            // stale (yarn fresh, pods out of sync — e.g. after a git checkout
            // that only touched Podfile.lock).
            if let Some((_, yarn_stale)) = &wt_branch
                && is_run_command(&spec)
            {
                let is_ios = is_ios_run_command(&spec);
                let pods_stale = if is_ios {
                    let idx = state
                        .worktree_browser
                        .worktree_table_state
                        .selected()
                        .unwrap_or(0);
                    let wt_path = &state.worktree_browser.worktrees
                        [idx.min(state.worktree_browser.worktrees.len() - 1)]
                    .path;
                    crate::domain::staleness::check_stale_pods(wt_path)
                } else {
                    false
                };

                if *yarn_stale || pods_stale {
                    if state
                        .app_config
                        .config
                        .as_ref()
                        .is_some_and(|c| c.auto_sync)
                    {
                        // Plan 13-09 (F-204 site 1): Recipe::SyncThenRun replaces
                        // the inline yarn/pod sequencing. Auto-sync fast path —
                        // skip the modal, expand the recipe, queue + dispatch.
                        let deps = DependencyState::new(*yarn_stale, pods_stale, is_ios);
                        let mut sequence = Recipe::SyncThenRun(spec).expand(&deps);
                        let first = sequence.remove(0);

                        // D-12: push to slice queue for the originating worktree.
                        let resolved_id = active_worktree_id(state);
                        if let Some(ref wt_id) = resolved_id {
                            let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                                crate::domain::worktree_slice::WorktreeSlice {
                                    id: wt_id.clone(),
                                    ..Default::default()
                                }
                            });
                            for cmd in sequence {
                                slice.queue.push_back(cmd);
                            }
                        }

                        state.modal_stack.palette_mode = None;
                        if let Some(eff) = dispatch_command(state, first) {
                            effects.push(eff);
                        }
                        return effects;
                    }
                    state.modal_stack.modal = Some(ModalState::SyncBeforeRun {
                        run_command: Box::new(spec),
                        needs_yarn: *yarn_stale,
                        needs_pods: pods_stale,
                    });
                    state.modal_stack.palette_mode = None;
                    return effects;
                }
            }

            // Plan 13-09 (F-204 site 2): Metro prerequisite — native run commands
            // need metro running first. The deferred spec is pushed to the
            // FRONT of the command_queue; on `MetroActivityUpdate(Ready)` the
            // queue is drained head-first via CommandRun (preserves the full
            // pipeline). Replaces the deleted `pending_metro_run` field.
            if spec.needs_metro() && !active_worktree_has_metro(state) {
                // D-12 + D-13: push to head of slice queue (push_front) for the originating worktree.
                if let Some(wt_id) = active_worktree_id(state) {
                    let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                        crate::domain::worktree_slice::WorktreeSlice {
                            id: wt_id.clone(),
                            ..Default::default()
                        }
                    });
                    slice.queue.push_front(spec);
                    effects.extend(update(
                        state,
                        Action::MetroStartForWorktree { worktree_id: wt_id },
                    ));
                }
                return effects;
            }

            // Pre-processing pipeline
            if spec.is_destructive() {
                let branch_name = wt_branch
                    .map(|(b, _)| b)
                    .unwrap_or_else(|| "(unknown)".to_string());
                state.modal_stack.modal = Some(ModalState::Confirm {
                    prompt: format!("Run '{}' on {}?", spec.label(), branch_name),
                    pending_command: spec,
                });
                return effects;
            }

            if spec.needs_text_input() {
                let prompt = match &spec {
                    CommandSpec::GitRebase { .. } => "Rebase onto:".to_string(),
                    CommandSpec::GitCheckout { .. } => "Branch to checkout:".to_string(),
                    CommandSpec::GitCheckoutNew { .. } => "New branch name:".to_string(),
                    CommandSpec::YarnJest { .. } => "Jest filter:".to_string(),
                    _ => "Input:".to_string(),
                };
                state.modal_stack.modal = Some(ModalState::TextInput {
                    prompt,
                    buffer: String::new(),
                    pending_template: Box::new(spec),
                });
                return effects;
            }

            if spec.needs_device_selection() {
                state.modal_stack.pending_device_command = Some(spec.clone());
                let kind = if matches!(spec, CommandSpec::UmpRunAndroid { .. }) {
                    crate::domain::ports::device_port::DeviceKind::Android
                } else {
                    crate::domain::ports::device_port::DeviceKind::Ios
                };
                effects.push(Effect::LoadDevices {
                    kind,
                    request_id: None,
                });
                return effects;
            }

            // Plan 13-09 (F-204 site 3): RnReleaseBuild → Recipe::ReleaseBuildAndInstall
            // Android release build: queue adb install to run after assembleRelease completes.
            if matches!(spec, CommandSpec::RnReleaseBuild) {
                let mut sequence = Recipe::ReleaseBuildAndInstall
                    .expand(&DependencyState::new(false, false, false));
                let first = sequence.remove(0);

                // D-12: push to slice queue for the originating worktree.
                let resolved_id = active_worktree_id(state);
                if let Some(ref wt_id) = resolved_id {
                    let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                        crate::domain::worktree_slice::WorktreeSlice {
                            id: wt_id.clone(),
                            ..Default::default()
                        }
                    });
                    for cmd in sequence {
                        slice.queue.push_back(cmd);
                    }
                }

                if let Some(eff) = dispatch_command(state, first) {
                    effects.push(eff);
                }
                return effects;
            }

            // Plan 13-09 (F-204 site 4): GitResetHardFetch → Recipe::GitFetchThenReset
            // Two-step — dispatch fetch, queue reset --hard origin/<branch>.
            if matches!(spec, CommandSpec::GitResetHardFetch) {
                let mut sequence =
                    Recipe::GitFetchThenReset.expand(&DependencyState::new(false, false, false));
                let first = sequence.remove(0);

                // D-12: push to slice queue for the originating worktree.
                let resolved_id = active_worktree_id(state);
                if let Some(ref wt_id) = resolved_id {
                    let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                        crate::domain::worktree_slice::WorktreeSlice {
                            id: wt_id.clone(),
                            ..Default::default()
                        }
                    });
                    for cmd in sequence {
                        slice.queue.push_back(cmd);
                    }
                }

                if let Some(eff) = dispatch_command(state, first) {
                    effects.push(eff);
                }
                return effects;
            }

            // Normal dispatch
            if let Some(eff) = dispatch_command(state, spec) {
                effects.push(eff);
            }
        }

        // --- Phase 3: Command output events ---
        Action::CommandOutputLine { task_id, line } => {
            // D-08: route by task_id. Late stdout from a cancelled task lands
            // here with no matching slice → silently dropped.
            if let Some(slice) = state
                .worktrees
                .values_mut()
                .find(|s| s.task.as_ref().map(|t| t.id) == Some(task_id))
            {
                slice.output.push_back(line);
                if slice.output.len() > MAX_COMMAND_LINES {
                    slice.output.pop_front();
                }
            }
        }

        Action::CommandExited { task_id, status } => {
            // Phase 14 D-11 PRIMARY: find the slice whose task matches task_id.
            let target_id = state
                .worktrees
                .iter()
                .find(|(_, s)| s.task.as_ref().map(|t| t.id) == Some(task_id))
                .map(|(id, _)| id.clone());

            // Take the slice's task (clearing it) and extract the spec for refresh classification.
            let completed_cmd = if let Some(ref id) = target_id {
                state
                    .worktrees
                    .get_mut(id)
                    .and_then(|s| s.task.take())
                    .map(|t| t.spec)
            } else {
                None
            };

            // Refresh staleness BEFORE draining the queue so any queued run command
            // re-entering CommandRun sees up-to-date stale state (prevents re-showing
            // the SyncBeforeRun modal after yarn install just ran).
            if let Some(ref cmd) = completed_cmd {
                let refresh = crate::domain::refresh::refresh_needed(cmd);
                if refresh.worktrees {
                    // Full worktree reload (also re-checks staleness and triggers JIRA fetch
                    // via WorktreesLoaded handler when branch names change)
                    effects.push(Effect::ListWorktrees {
                        repo_root: state.app_config.repo_root.clone(),
                    });
                } else if refresh.staleness {
                    // Staleness refresh: re-check ALL worktrees (cheap I/O, ensures
                    // correct state even if user changed selection during command)
                    for wt in state.worktree_browser.worktrees.iter_mut() {
                        wt.stale = crate::domain::staleness::check_stale(&wt.path);
                        wt.stale_pods = crate::domain::staleness::check_stale_pods(&wt.path);
                    }
                }
            }

            if let (Some(id), Some(cmd)) = (&target_id, &completed_cmd)
                && let Some(effect) = completed_ios_store_request(state, id, cmd, &status)
            {
                effects.push(effect);
            }
            if let (Some(id), Some(cmd)) = (&target_id, &completed_cmd)
                && let Some(effect) = completed_android_store_request(state, id, cmd, &status)
            {
                effects.push(effect);
            }

            // === Slice-local drain (D-11 + D-13) ===
            // Borrow split: extract the next spec and post_drain BEFORE calling
            // dispatch_command (which needs &mut AppState). We use a two-pass
            // approach: peek/pop in one borrow, then dispatch in a fresh borrow.
            enum SliceDrainResult {
                /// Head spec needs to be dispatched.
                Dispatch(WorktreeId, crate::domain::command::CommandSpec),
                /// Head spec needs metro — push back and start metro.
                NeedsMetro(WorktreeId),
                /// Queue is empty — consume post_drain.
                PostDrain(Box<Action>),
                /// Queue is empty, no post_drain.
                Empty,
            }

            let target_has_metro = target_id
                .as_ref()
                .map(|id| worktree_has_metro_or_reserved(state, id))
                .unwrap_or_else(|| active_worktree_has_metro(state));

            let slice_drain = if let Some(ref id) = target_id
                && let Some(slice) = state.worktrees.get_mut(id)
            {
                if let Some(next_spec) = slice.queue.pop_front() {
                    if next_spec.needs_metro() && !target_has_metro {
                        // D-13: metro stays metro-special. Push back to head of slice.
                        slice.queue.push_front(next_spec.clone());
                        SliceDrainResult::NeedsMetro(id.clone())
                    } else {
                        SliceDrainResult::Dispatch(id.clone(), next_spec)
                    }
                } else if let Some(post) = slice.post_drain.take() {
                    // D-14: per-slice post_drain consumed on empty queue.
                    SliceDrainResult::PostDrain(post)
                } else {
                    SliceDrainResult::Empty
                }
            } else {
                SliceDrainResult::Empty
            };

            match slice_drain {
                SliceDrainResult::Dispatch(worktree_id, spec) => {
                    if let Some(eff) = dispatch_command_for_worktree(state, &worktree_id, spec) {
                        effects.push(eff);
                    }
                }
                SliceDrainResult::NeedsMetro(worktree_id) => {
                    effects.extend(update(state, Action::MetroStartForWorktree { worktree_id }));
                }
                SliceDrainResult::PostDrain(post) => {
                    effects.extend(update(state, *post));
                }
                SliceDrainResult::Empty => {}
            }
        }

        Action::CommandOutputClear => {
            if let Some(id) = active_worktree_id(state)
                && let Some(slice) = state.worktrees.get_mut(&id)
            {
                slice.output.clear();
                slice.output_scroll = 0;
            }
        }

        Action::CommandCancel => {
            let active_id = active_worktree_id(state);

            // D-03: opaque abort via TaskHandle trait for the active worktree.
            // Phase 15 / TASK-04 (15-RESEARCH §Pitfall 5): is_cancellable() guard
            // prevents SIGTERM at running git rebase/push (data-integrity risk).
            // Take-then-maybe-reinsert pattern: take() requires &mut on slice.task,
            // we then inspect the OWNED record and either abort + clear OR put it back.
            if let Some(ref id) = active_id
                && let Some(slice) = state.worktrees.get_mut(id)
                && let Some(record) = slice.task.take()
            {
                if record.spec.is_cancellable() {
                    record.handle.abort();
                    slice.queue.clear();
                    slice.post_drain = None;
                    slice.output.push_back("[cancelled]".into());
                    if slice.output.len() > MAX_COMMAND_LINES {
                        slice.output.pop_front();
                    }
                } else {
                    // Non-cancellable variants (all 8 git porcelain commands):
                    // CommandCancel is a no-op. Re-insert the record so the
                    // running task continues to completion. Queue and output
                    // are also untouched.
                    slice.task = Some(record);
                }
            }
        }

        // --- Phase 5.1: Command queue actions ---
        Action::CommandQueuePush(spec) => {
            // D-12: push to slice queue for the originating worktree.
            if let Some(wt_id) = active_worktree_id(state) {
                let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                    crate::domain::worktree_slice::WorktreeSlice {
                        id: wt_id.clone(),
                        ..Default::default()
                    }
                });
                slice.queue.push_back(spec);
            }
        }

        Action::CommandQueueClear => {
            for slice in state.worktrees.values_mut() {
                slice.queue.clear();
            }
        }

        // --- Phase 3: Modal actions ---
        Action::ModalConfirm => {
            // Check for pending worktree removal BEFORE falling through to normal confirm
            if let Some((wt_id, wt_path, _branch)) =
                state.modal_stack.pending_worktree_removal.take()
            {
                state.modal_stack.modal = None;

                // Stop metro if it's running on the worktree being removed
                if state
                    .worktrees
                    .get(&wt_id)
                    .map(|slice| slice.metro.is_running())
                    .unwrap_or(false)
                {
                    state.metro_state.active_worktree_path = Some(wt_path.clone());
                    effects.extend(update(state, Action::MetroStop));
                }

                // Immediately remove from worktree list for instant visual feedback
                state.worktree_browser.worktrees.retain(|wt| wt.id != wt_id);
                if state.worktree_browser.worktrees.is_empty() {
                    state.worktree_browser.worktree_table_state.select(None);
                    state.worktree_browser.selected_worktree_id = None;
                    state.metro_state.active_worktree_path = None;
                } else {
                    let idx = state
                        .worktree_browser
                        .worktree_table_state
                        .selected()
                        .unwrap_or(0)
                        .min(state.worktree_browser.worktrees.len() - 1);
                    state
                        .worktree_browser
                        .worktree_table_state
                        .select(Some(idx));
                    state.worktree_browser.selected_worktree_id =
                        Some(state.worktree_browser.worktrees[idx].id.clone());
                    state.metro_state.active_worktree_path =
                        Some(state.worktree_browser.worktrees[idx].path.clone());
                }

                // Emit the async removal effect
                state.worktree_browser.worktree_op_in_flight = true;
                effects.push(Effect::RemoveWorktree {
                    repo_root: state.app_config.repo_root.clone(),
                    path: wt_path,
                });
                return effects;
            }

            if let Some(ModalState::Confirm {
                pending_command, ..
            }) = state.modal_stack.modal.take()
            {
                // Dispatch directly — skip pre-processing (already confirmed)
                if let Some(eff) = dispatch_command(state, pending_command) {
                    effects.push(eff);
                }
            }
        }

        Action::ModalCancel => {
            state.modal_stack.modal = None;
            state.modal_stack.palette_mode = None;
            if state.modal_stack.pending_cached_ios_run.take().is_some() {
                state.modal_stack.pending_device_command = None;
            }
            if state
                .modal_stack
                .pending_cached_android_run
                .take()
                .is_some()
            {
                state.modal_stack.pending_device_command = None;
            }
            state.modal_stack.pending_worktree_removal = None; // discard any pending removal on cancel
            state.modal_stack.pending_worktree_add = false; // discard any pending add on cancel
            state.modal_stack.pending_new_branch_base = None; // discard new-branch base on cancel
            state.modal_stack.pending_new_branch_worktree = false;
            state.modal_stack.pending_review_checkout = None;
        }

        Action::ModalInputChar(c) => {
            // Plan 13-09 (F-205): exhaustive ModalState enumeration. Rust's
            // exhaustiveness checker now guards future ModalState additions —
            // a new variant will trigger a compile error here until explicitly
            // handled.
            match state.modal_stack.modal.as_mut() {
                Some(ModalState::TextInput { buffer, .. }) => {
                    buffer.push(c);
                }
                Some(ModalState::DevicePicker {
                    filter, selected, ..
                }) => {
                    filter.push(c);
                    *selected = 0; // reset selection when filter changes
                }
                // Intentionally ignore char input — these modals do not accept
                // arbitrary characters as text. (BranchPicker filter input
                // arrives as Action::BranchPickerFilter, not ModalInputChar.)
                Some(ModalState::Confirm { .. })
                | Some(ModalState::RunVariantPicker { .. })
                | Some(ModalState::CleanToggle { .. })
                | Some(ModalState::SyncBeforeRun { .. })
                | Some(ModalState::SyncBeforeMetro { .. })
                | Some(ModalState::ExternalMetroConflict { .. })
                | Some(ModalState::BranchPicker { .. })
                | Some(ModalState::PullRequestPicker { .. })
                | Some(ModalState::Info { .. })
                | None => { /* no-op */ }
            }
        }

        Action::ModalInputBackspace => {
            // Plan 13-09 (F-205): exhaustive ModalState enumeration — same
            // rationale as ModalInputChar above.
            match state.modal_stack.modal.as_mut() {
                Some(ModalState::TextInput { buffer, .. }) => {
                    buffer.pop();
                }
                Some(ModalState::DevicePicker {
                    filter, selected, ..
                }) => {
                    filter.pop();
                    *selected = 0; // reset selection when filter changes
                }
                // Intentionally ignore backspace — these modals do not have
                // editable text buffers. (BranchPicker filter editing arrives
                // as Action::BranchPickerBackspace.)
                Some(ModalState::Confirm { .. })
                | Some(ModalState::RunVariantPicker { .. })
                | Some(ModalState::CleanToggle { .. })
                | Some(ModalState::SyncBeforeRun { .. })
                | Some(ModalState::SyncBeforeMetro { .. })
                | Some(ModalState::ExternalMetroConflict { .. })
                | Some(ModalState::BranchPicker { .. })
                | Some(ModalState::PullRequestPicker { .. })
                | Some(ModalState::Info { .. })
                | None => { /* no-op */ }
            }
        }

        Action::ModalInputSubmit => {
            if let Some(modal) = state.modal_stack.modal.take() {
                match modal {
                    ModalState::TextInput {
                        buffer,
                        pending_template,
                        ..
                    } => {
                        if let Some(pr) = state.modal_stack.pending_review_checkout.take() {
                            let worktree_name = buffer.trim().to_string();
                            if worktree_name.is_empty() {
                                return effects;
                            }
                            state.worktree_browser.worktree_op_in_flight = true;
                            effects.push(Effect::AddReviewWorktree {
                                repo_root: state.app_config.repo_root.clone(),
                                pr,
                                worktree_name,
                            });
                        } else if state.modal_stack.pending_new_branch_worktree {
                            state.modal_stack.pending_new_branch_worktree = false;
                            let new_branch_name = buffer.trim().to_string();
                            let base_branch = state.modal_stack.pending_new_branch_base.take();
                            if new_branch_name.is_empty() {
                                return effects;
                            }
                            let base_branch = match base_branch {
                                Some(b) => b,
                                None => return effects,
                            };
                            state.worktree_browser.worktree_op_in_flight = true;
                            effects.push(Effect::AddWorktreeNewBranch {
                                repo_root: state.app_config.repo_root.clone(),
                                new: new_branch_name,
                                base: base_branch,
                            });
                        } else if state.modal_stack.pending_worktree_add {
                            state.modal_stack.pending_worktree_add = false;
                            let branch_name = buffer.trim().to_string();
                            if branch_name.is_empty() {
                                return effects;
                            }
                            state.worktree_browser.worktree_op_in_flight = true;
                            effects.push(Effect::AddWorktree {
                                repo_root: state.app_config.repo_root.clone(),
                                branch: branch_name,
                            });
                        } else {
                            // Build the real CommandSpec by filling in the text
                            let real_spec = match *pending_template {
                                CommandSpec::GitRebase { .. } => {
                                    CommandSpec::GitRebase { target: buffer }
                                }
                                CommandSpec::GitCheckout { .. } => {
                                    CommandSpec::GitCheckout { branch: buffer }
                                }
                                CommandSpec::GitCheckoutNew { .. } => {
                                    CommandSpec::GitCheckoutNew { branch: buffer }
                                }
                                CommandSpec::YarnJest { .. } => {
                                    CommandSpec::YarnJest { filter: buffer }
                                }
                                CommandSpec::ShellCommand { .. } => {
                                    CommandSpec::ShellCommand { command: buffer }
                                }
                                other => other,
                            };
                            if let Some(eff) = dispatch_command(state, real_spec) {
                                effects.push(eff);
                            }
                        }
                    }
                    other => {
                        // Restore modal if wrong type (shouldn't happen)
                        state.modal_stack.modal = Some(other);
                    }
                }
            }
        }

        Action::ModalDeviceNext => {
            if let Some(ModalState::DevicePicker {
                ref devices,
                ref mut selected,
                ref filter,
                ..
            }) = state.modal_stack.modal
            {
                let count = if filter.is_empty() {
                    devices.len()
                } else {
                    let lower = filter.to_lowercase();
                    devices
                        .iter()
                        .filter(|d| d.name.to_lowercase().contains(&lower))
                        .count()
                };
                if count > 0 {
                    *selected = if *selected >= count - 1 {
                        0
                    } else {
                        *selected + 1
                    };
                }
            }
        }

        Action::ModalDevicePrev => {
            if let Some(ModalState::DevicePicker {
                ref devices,
                ref mut selected,
                ref filter,
                ..
            }) = state.modal_stack.modal
            {
                let count = if filter.is_empty() {
                    devices.len()
                } else {
                    let lower = filter.to_lowercase();
                    devices
                        .iter()
                        .filter(|d| d.name.to_lowercase().contains(&lower))
                        .count()
                };
                if count > 0 {
                    *selected = if *selected == 0 {
                        count - 1
                    } else {
                        *selected - 1
                    };
                }
            }
        }

        Action::ModalDeviceConfirm => {
            if let Some(ModalState::DevicePicker {
                devices,
                selected,
                pending_template,
                filter,
            }) = state.modal_stack.modal.take()
            {
                // Apply filter to get the actual visible list (mirrors render logic)
                let filtered: Vec<&crate::domain::command::DeviceInfo> = if filter.is_empty() {
                    devices.iter().collect()
                } else {
                    let lower = filter.to_lowercase();
                    devices
                        .iter()
                        .filter(|d| d.name.to_lowercase().contains(&lower))
                        .collect()
                };
                if let Some(device) = filtered.get(selected) {
                    let device_id = device.id.clone();
                    let is_ios = matches!(pending_template.as_ref(), CommandSpec::UmpRunIos { .. });
                    let is_available_emulator = android_avd_name(&device_id).is_some();

                    if let Some(cache_hit) = state.modal_stack.pending_cached_ios_run.take() {
                        state.modal_stack.pending_device_command = None;
                        begin_cached_ios_launch(
                            state,
                            &mut effects,
                            cache_hit.worktree_id,
                            cache_hit.worktree_path,
                            device_id,
                            cache_hit.cache_hit,
                        );
                        return effects;
                    }

                    if let Some(cache_hit) = state.modal_stack.pending_cached_android_run.take() {
                        state.modal_stack.pending_device_command = None;
                        begin_cached_android_launch(
                            state,
                            &mut effects,
                            cache_hit.worktree_id,
                            cache_hit.worktree_path,
                            device_id,
                            cache_hit.cache_hit,
                        );
                        return effects;
                    }

                    if is_available_emulator
                        && matches!(pending_template.as_ref(), CommandSpec::UmpRunAndroid { .. })
                    {
                        let real_spec = command_with_device(*pending_template, device_id);
                        open_run_variant_picker_or_dispatch(
                            state,
                            &mut effects,
                            real_spec,
                            true,
                            false,
                        );
                        return effects;
                    }

                    let cache_launch_supported = match pending_template.as_ref() {
                        CommandSpec::UmpRunAndroid { .. } => true,
                        CommandSpec::UmpRunIos { .. } => ios_device_supports_cached_launch(device),
                        _ => false,
                    };
                    let real_spec = command_with_device(*pending_template, device_id.clone());
                    // Record iOS simulator usage for sort-by-recent
                    if is_ios && cache_launch_supported {
                        effects.push(Effect::ScheduleAction(Action::SimulatorUsed(device_id)));
                    }
                    open_run_variant_picker_or_dispatch(
                        state,
                        &mut effects,
                        real_spec,
                        false,
                        cache_launch_supported,
                    );
                }
            }
        }

        Action::ModalRunVariantNext => {
            if let Some(ModalState::RunVariantPicker {
                ref mut selected, ..
            }) = state.modal_stack.modal
            {
                *selected = (*selected + 1) % RunVariant::ALL.len();
            }
        }

        Action::ModalRunVariantPrev => {
            if let Some(ModalState::RunVariantPicker {
                ref mut selected, ..
            }) = state.modal_stack.modal
            {
                *selected = if *selected == 0 {
                    RunVariant::ALL.len() - 1
                } else {
                    *selected - 1
                };
            }
        }

        Action::ModalRunVariantConfirm => {
            if let Some(ModalState::RunVariantPicker {
                selected,
                pending_template,
                boot_android_emulator,
                cache_launch_supported,
                ..
            }) = state.modal_stack.modal.take()
            {
                let variant = RunVariant::ALL[selected.min(RunVariant::ALL.len() - 1)];
                let real_spec = command_with_run_variant(*pending_template, variant);

                if boot_android_emulator
                    && let CommandSpec::UmpRunAndroid { device_id, .. } = &real_spec
                    && let Some(avd_name) = android_avd_name(device_id)
                {
                    remember_ump_run_config(state, &real_spec, cache_launch_supported);
                    let boot = CommandSpec::ShellCommand {
                        command: android_boot_avd_command(avd_name),
                    };
                    if let Some(eff) = dispatch_command(state, boot) {
                        effects.push(eff);
                    }
                    if let Some(wt_id) = active_worktree_id(state) {
                        let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                            crate::domain::worktree_slice::WorktreeSlice {
                                id: wt_id,
                                ..Default::default()
                            }
                        });
                        slice.queue.push_back(real_spec);
                    }
                    return effects;
                }

                effects.extend(update(
                    state,
                    Action::CommandRunWithCache {
                        spec: real_spec,
                        cache_launch_supported,
                    },
                ));
            }
        }

        // --- Phase 3: Device enumeration (async callback) ---
        Action::DevicesEnumerated {
            kind,
            request_id,
            devices,
        } => {
            if let Some(pending_run) = state.modal_stack.pending_cached_ios_run.as_ref()
                && (kind != DeviceKind::Ios || request_id != Some(pending_run.device_request_id))
            {
                return effects;
            }

            if let Some(pending_run) = state.modal_stack.pending_cached_ios_run.take() {
                state.modal_stack.pending_device_command = None;
                match devices.len() {
                    0 => {
                        let slice = state
                            .worktrees
                            .entry(pending_run.worktree_id.clone())
                            .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
                                id: pending_run.worktree_id,
                                ..Default::default()
                            });
                        slice
                            .output
                            .push_back("[error] no iOS simulators found for cached run".into());
                        while slice.output.len() > MAX_COMMAND_LINES {
                            slice.output.pop_front();
                        }
                    }
                    1 => {
                        begin_cached_ios_launch(
                            state,
                            &mut effects,
                            pending_run.worktree_id,
                            pending_run.worktree_path,
                            devices[0].id.clone(),
                            pending_run.cache_hit,
                        );
                    }
                    _ => {
                        let mut sorted_devices = devices;
                        let history = &state.app_config.sim_history;
                        sorted_devices.sort_by_key(|d| {
                            history
                                .iter()
                                .position(|h| h == &d.id)
                                .unwrap_or(usize::MAX)
                        });
                        state.modal_stack.pending_cached_ios_run = Some(pending_run);
                        state.modal_stack.modal = Some(ModalState::DevicePicker {
                            devices: sorted_devices,
                            selected: 0,
                            pending_template: Box::new(CommandSpec::UmpRunIos {
                                device_id: String::new(),
                                variant: Some(RunVariant::Local),
                            }),
                            filter: String::new(),
                        });
                    }
                }
                return effects;
            }

            if let Some(pending_run) = state.modal_stack.pending_cached_android_run.as_ref()
                && (kind != DeviceKind::Android
                    || request_id != Some(pending_run.device_request_id))
            {
                return effects;
            }

            if let Some(pending_run) = state.modal_stack.pending_cached_android_run.take() {
                state.modal_stack.pending_device_command = None;
                match devices.len() {
                    0 => {
                        let slice = state
                            .worktrees
                            .entry(pending_run.worktree_id.clone())
                            .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
                                id: pending_run.worktree_id,
                                ..Default::default()
                            });
                        slice
                            .output
                            .push_back("[error] no Android devices found for cached run".into());
                        while slice.output.len() > MAX_COMMAND_LINES {
                            slice.output.pop_front();
                        }
                    }
                    1 => {
                        begin_cached_android_launch(
                            state,
                            &mut effects,
                            pending_run.worktree_id,
                            pending_run.worktree_path,
                            devices[0].id.clone(),
                            pending_run.cache_hit,
                        );
                    }
                    _ => {
                        state.modal_stack.pending_cached_android_run = Some(pending_run);
                        state.modal_stack.modal = Some(ModalState::DevicePicker {
                            devices,
                            selected: 0,
                            pending_template: Box::new(CommandSpec::UmpRunAndroid {
                                device_id: String::new(),
                                variant: Some(RunVariant::Local),
                            }),
                            filter: String::new(),
                        });
                    }
                }
                return effects;
            }

            if let Some(spec) = state.modal_stack.pending_device_command.take() {
                match devices.len() {
                    0 => {
                        if let Some(id) = active_worktree_id(state)
                            && let Some(slice) = state.worktrees.get_mut(&id)
                        {
                            slice.output.push_back("[error] no devices found".into());
                            if slice.output.len() > MAX_COMMAND_LINES {
                                slice.output.pop_front();
                            }
                        }
                    }
                    1 => {
                        // Only one device — skip picker
                        let is_available_emulator = android_avd_name(&devices[0].id).is_some();

                        // Available emulator: boot it, then run via shell command
                        if is_available_emulator {
                            if matches!(spec, CommandSpec::UmpRunAndroid { .. }) {
                                let real_spec = command_with_device(spec, devices[0].id.clone());
                                open_run_variant_picker_or_dispatch(
                                    state,
                                    &mut effects,
                                    real_spec,
                                    true,
                                    false,
                                );
                                return effects;
                            }
                        } else {
                            let cache_launch_supported = match &spec {
                                CommandSpec::UmpRunAndroid { .. } => true,
                                CommandSpec::UmpRunIos { .. } => {
                                    ios_device_supports_cached_launch(&devices[0])
                                }
                                _ => false,
                            };
                            let real_spec = command_with_device(spec, devices[0].id.clone());
                            open_run_variant_picker_or_dispatch(
                                state,
                                &mut effects,
                                real_spec,
                                false,
                                cache_launch_supported,
                            );
                        }
                    }
                    _ => {
                        // Multiple devices — show picker
                        // Sort iOS simulators by last-used from sim_history
                        let mut sorted_devices = devices;
                        if matches!(spec, CommandSpec::UmpRunIos { .. }) {
                            // Plan 13-08: sim_history is loaded into AppState at startup
                            // (src/main.rs) so update() never crosses the infra boundary.
                            let history = &state.app_config.sim_history;
                            sorted_devices.sort_by_key(|d| {
                                history
                                    .iter()
                                    .position(|h| h == &d.id)
                                    .unwrap_or(usize::MAX)
                            });
                        }
                        state.modal_stack.modal = Some(ModalState::DevicePicker {
                            devices: sorted_devices,
                            selected: 0,
                            pending_template: Box::new(spec),
                            filter: String::new(),
                        });
                    }
                }
            }
        }

        // --- Phase 3: Palette mode activation ---
        Action::EnterGitPalette => {
            state.modal_stack.palette_mode = Some(PaletteMode::Git);
        }

        // --- Phase 5: Worktree switching and Claude Code ---
        Action::WorktreeSwitchToSelected => {
            let selected_idx = state
                .worktree_browser
                .worktree_table_state
                .selected()
                .unwrap_or(0);
            // Capture target path NOW — navigation may change active_worktree_path later
            let target_path = state
                .worktree_browser
                .worktrees
                .get(selected_idx)
                .map(|wt| wt.path.clone());
            let target_id = state
                .worktree_browser
                .worktrees
                .get(selected_idx)
                .map(|wt| wt.id.clone());

            // Stale dependency check — metro only needs yarn, not pods
            if let Some(wt) = state.worktree_browser.worktrees.get(selected_idx)
                && wt.stale
            {
                if state
                    .app_config
                    .config
                    .as_ref()
                    .is_some_and(|c| c.auto_sync)
                {
                    // Plan 13-09 (F-204 site 10a): auto-sync fast path uses
                    // Recipe::SyncThenStartMetro. Set active_worktree_path
                    // synchronously (replaces pending_switch_path).
                    let Some(wt_id) = target_id.clone() else {
                        return effects;
                    };
                    if let Some(path) = target_path {
                        state.metro_state.active_worktree_path = Some(path);
                    }
                    let deps = DependencyState::new(true, false, false);
                    let mut sequence = Recipe::SyncThenStartMetro.expand(&deps);
                    if sequence.is_empty() {
                        effects.extend(update(
                            state,
                            Action::MetroStartForWorktree { worktree_id: wt_id },
                        ));
                        return effects;
                    }
                    let first = sequence.remove(0);

                    // D-12 + D-14: push to slice queue; set per-slice post_drain to MetroStart.
                    let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                        crate::domain::worktree_slice::WorktreeSlice {
                            id: wt_id.clone(),
                            ..Default::default()
                        }
                    });
                    for cmd in sequence {
                        slice.queue.push_back(cmd);
                    }
                    slice.post_drain = Some(Box::new(Action::MetroStartForWorktree {
                        worktree_id: wt_id.clone(),
                    }));

                    if let Some(eff) = dispatch_command_for_worktree(state, &wt_id, first) {
                        effects.push(eff);
                    }
                    return effects;
                }
                // Plan 13-09 (F-204 site 10b): set active_worktree_path NOW so the
                // SyncBeforeMetro modal accept/decline paths don't need a stash.
                if let Some(path) = target_path {
                    state.metro_state.active_worktree_path = Some(path);
                }
                state.modal_stack.modal = Some(ModalState::SyncBeforeMetro {
                    needs_yarn: true,
                    needs_pods: false,
                });
                return effects;
            }

            // Original logic — only reached when deps are fresh.
            // Plan 13-09: set active_worktree_path immediately; pending_restart
            // alone signals the MetroExited handler to dispatch MetroStart.
            if let Some(path) = target_path {
                state.metro_state.active_worktree_path = Some(path);
            }
            if let Some(worktree_id) = target_id
                && !worktree_has_metro_or_reserved(state, &worktree_id)
            {
                effects.extend(update(state, Action::MetroStartForWorktree { worktree_id }));
            }
        }

        Action::OpenClaudeCode => {
            state.modal_stack.palette_mode = None;
            if !state.app_config.multiplexer_available {
                state.error_state = Some(ErrorState {
                    message:
                        "Cannot open Claude Code: not inside a tmux, zellij, or Ghostty session"
                            .into(),
                    can_retry: false,
                });
                return effects;
            }
            let Some(wt) = selected_worktree_snapshot(state) else {
                return effects;
            };
            let path = wt.path.clone();
            let name = format!("{}-claude", wt.preferred_prefix());
            let flags = state.app_config.claude_flags.clone();
            let command = if flags.is_empty() {
                "claude".to_string()
            } else {
                format!("claude {flags}")
            };
            effects.push(Effect::OpenInMultiplexer {
                worktree: path,
                name,
                command,
            });
        }

        Action::OpenShellTab => {
            state.modal_stack.palette_mode = None;
            if !state.app_config.multiplexer_available {
                state.error_state = Some(ErrorState {
                    message: "Cannot open shell tab: not inside a tmux, zellij, or Ghostty session"
                        .into(),
                    can_retry: false,
                });
                return effects;
            }
            let Some(wt) = selected_worktree_snapshot(state) else {
                return effects;
            };
            let path = wt.path.clone();
            let name = format!("{}-shell", wt.preferred_prefix());
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
            effects.push(Effect::OpenInMultiplexer {
                worktree: path,
                name,
                command: shell,
            });
        }
        Action::OpenEditor => {
            state.modal_stack.palette_mode = None;
            let editor = state.app_config.editor.trim().to_string();
            if editor.is_empty() {
                state.error_state = Some(ErrorState {
                    message: "Cannot open editor: configure the editor setting first".into(),
                    can_retry: false,
                });
                return effects;
            }

            let Some(wt) = selected_worktree_snapshot(state) else {
                return effects;
            };

            if state.app_config.editor_in_terminal {
                if !state.app_config.multiplexer_available {
                    state.error_state = Some(ErrorState {
                        message:
                            "Cannot open editor: not inside a tmux, zellij, or Ghostty session"
                                .into(),
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

        // --- Phase 4: JIRA title fetch results ---
        Action::JiraTitlesFetched(titles) => {
            // Update in-memory cache
            for (key, title) in &titles {
                state.jira.title_cache.insert(key.clone(), title.clone());
            }
            // Persist cache to disk via effect_runner (F-201)
            effects.push(Effect::SaveJiraCache(state.jira.title_cache.clone()));
            // Apply titles to currently loaded worktrees
            for wt in &mut state.worktree_browser.worktrees {
                if let Some(key) =
                    crate::domain::jira::extract_jira_key(&wt.branch, &state.jira.project_prefix)
                    && let Some(title) = state.jira.title_cache.get(&key)
                {
                    wt.jira_title = Some(title.clone());
                }
            }
        }

        // --- Phase 5.1: New submenu and action stubs ---
        Action::EnterAndroidPalette => {
            state.modal_stack.palette_mode = Some(PaletteMode::Android);
            if let Some((worktree_id, worktree_path)) = active_worktree_snapshot(state) {
                queue_android_cache_lookup_for_worktree(
                    state,
                    &mut effects,
                    worktree_id,
                    worktree_path,
                );
            }
        }
        Action::EnterIosPalette => {
            state.modal_stack.palette_mode = Some(PaletteMode::Ios);
            if let Some((worktree_id, worktree_path)) = active_worktree_snapshot(state) {
                queue_ios_cache_lookup_for_worktree(
                    state,
                    &mut effects,
                    worktree_id,
                    worktree_path,
                );
            }
        }
        Action::EnterYarnPalette => {
            state.modal_stack.palette_mode = Some(PaletteMode::Yarn);
        }
        Action::EnterWorktreePalette => {
            state.modal_stack.palette_mode = Some(PaletteMode::Worktree);
        }
        Action::EnterOpenPalette => {
            state.modal_stack.palette_mode = Some(PaletteMode::Open);
        }
        Action::OpenCleanMenu => {
            state.modal_stack.palette_mode = None;
            state.modal_stack.modal = Some(ModalState::CleanToggle {
                options: CleanOptions::default(),
            });
        }
        Action::CleanToggleNodeModules => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal_stack.modal {
                options.node_modules = !options.node_modules;
            }
        }
        Action::CleanTogglePods => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal_stack.modal {
                options.pods = !options.pods;
            }
        }
        Action::CleanToggleAndroid => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal_stack.modal {
                options.android = !options.android;
            }
        }
        Action::CleanToggleSyncAfter => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal_stack.modal {
                options.sync_after = !options.sync_after;
            }
        }
        Action::CleanConfirm => {
            if let Some(ModalState::CleanToggle { options }) = state.modal_stack.modal.take() {
                state.modal_stack.palette_mode = None;

                // Plan 13-09 (F-204 site 6): Recipe::Clean replaces inline
                // option-to-command sequencing. Order is enforced by
                // Recipe::expand: react-native clean first, node_modules last,
                // sync_after appended (preserves the comment's hard ordering
                // rule).
                let mut cmds =
                    Recipe::Clean(options).expand(&DependencyState::new(false, false, false));

                if cmds.is_empty() {
                    return effects;
                }

                // Dispatch first, queue rest
                let first = cmds.remove(0);

                // D-12: push to slice queue for the originating worktree.
                let resolved_id = active_worktree_id(state);
                if let Some(ref wt_id) = resolved_id {
                    let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                        crate::domain::worktree_slice::WorktreeSlice {
                            id: wt_id.clone(),
                            ..Default::default()
                        }
                    });
                    for cmd in cmds {
                        slice.queue.push_back(cmd);
                    }
                }

                if let Some(eff) = dispatch_command(state, first) {
                    effects.push(eff);
                }
            }
        }
        Action::ToggleFullscreen => {
            if state.worktree_browser.fullscreen_panel.is_some() {
                state.worktree_browser.fullscreen_panel = None;
                state.focused_panel = state.focused_panel.next();
            } else {
                // Only CommandOutput can be fullscreened
                if state.focused_panel == FocusedPanel::CommandOutput {
                    state.worktree_browser.fullscreen_panel = Some(state.focused_panel);
                }
            }
        }
        Action::StartShellCommand => {
            state.modal_stack.modal = Some(ModalState::TextInput {
                prompt: "Shell command:".to_string(),
                buffer: String::new(),
                pending_template: Box::new(CommandSpec::ShellCommand {
                    command: String::new(),
                }),
            });
        }
        Action::SimulatorUsed(udid) => {
            // Fire-and-forget write to sim history via effect_runner
            effects.push(Effect::RecordSimUsed(udid));
        }
        Action::IosSimulatorCacheLookupFinished {
            worktree_id,
            result,
        } => {
            match result {
                Ok(crate::domain::native_cache::IosSimulatorCacheLookup::Hit(hit)) => {
                    let hit = *hit;
                    let fingerprint = hit.metadata.fingerprint.clone();
                    for (id, slice) in state.worktrees.iter_mut() {
                        let same_fingerprint_miss = matches!(
                            &slice.ios_simulator_cache,
                            crate::domain::native_cache::IosSimulatorCacheState::Miss {
                                fingerprint: miss_fingerprint
                            } if miss_fingerprint == &fingerprint
                        );
                        if id == &worktree_id || same_fingerprint_miss {
                            slice.ios_simulator_cache =
                                crate::domain::native_cache::IosSimulatorCacheState::Hit(Box::new(
                                    hit.clone(),
                                ));
                        }
                    }
                }
                Ok(crate::domain::native_cache::IosSimulatorCacheLookup::Miss { fingerprint }) => {
                    if let Some(slice) = state.worktrees.get_mut(&worktree_id) {
                        let existing_hit_for_same_fingerprint = slice
                            .ios_simulator_cache
                            .hit()
                            .is_some_and(|hit| hit.metadata.fingerprint == fingerprint);
                        if !existing_hit_for_same_fingerprint {
                            slice.ios_simulator_cache =
                                crate::domain::native_cache::IosSimulatorCacheState::Miss {
                                    fingerprint,
                                };
                        }
                    }
                }
                Err(message) => {
                    if let Some(slice) = state.worktrees.get_mut(&worktree_id) {
                        slice.ios_simulator_cache =
                            crate::domain::native_cache::IosSimulatorCacheState::Error(message);
                    }
                }
            }
            refresh_open_run_variant_picker_cache_flags(state, |spec| {
                matches!(spec, CommandSpec::UmpRunIos { .. })
            });
        }
        Action::AndroidCacheLookupFinished {
            worktree_id,
            result,
        } => {
            match result {
                Ok(crate::domain::native_cache::AndroidCacheLookup::Hit(hit)) => {
                    let hit = *hit;
                    let fingerprint = hit.metadata.fingerprint.clone();
                    for (id, slice) in state.worktrees.iter_mut() {
                        let same_fingerprint_miss = matches!(
                            &slice.android_cache,
                            crate::domain::native_cache::AndroidCacheState::Miss {
                                fingerprint: miss_fingerprint
                            } if miss_fingerprint == &fingerprint
                        );
                        if id == &worktree_id || same_fingerprint_miss {
                            slice.android_cache =
                                crate::domain::native_cache::AndroidCacheState::Hit(Box::new(
                                    hit.clone(),
                                ));
                        }
                    }
                }
                Ok(crate::domain::native_cache::AndroidCacheLookup::Miss { fingerprint }) => {
                    if let Some(slice) = state.worktrees.get_mut(&worktree_id) {
                        let existing_hit_for_same_fingerprint = slice
                            .android_cache
                            .hit()
                            .is_some_and(|hit| hit.metadata.fingerprint == fingerprint);
                        if !existing_hit_for_same_fingerprint {
                            slice.android_cache =
                                crate::domain::native_cache::AndroidCacheState::Miss { fingerprint };
                        }
                    }
                }
                Err(message) => {
                    if let Some(slice) = state.worktrees.get_mut(&worktree_id) {
                        if slice.android_cache.hit().is_none() {
                            slice.android_cache =
                                crate::domain::native_cache::AndroidCacheState::Error(
                                    message.clone(),
                                );
                        }
                        slice
                            .output
                            .push_back(format!("[cached-android error] {message}"));
                        while slice.output.len() > MAX_COMMAND_LINES {
                            slice.output.pop_front();
                        }
                        slice.output_scroll = 0;
                    }
                }
            }
            refresh_open_run_variant_picker_cache_flags(state, |spec| {
                matches!(spec, CommandSpec::UmpRunAndroid { .. })
            });
        }
        Action::CachedIosRun(cache_hit) => {
            state.modal_stack.modal = None;
            state.modal_stack.palette_mode = None;
            state.modal_stack.pending_cached_ios_run = None;
            state.modal_stack.pending_device_command = None;
            let Some((worktree_id, worktree_path)) = active_worktree_snapshot(state) else {
                return effects;
            };
            state.modal_stack.next_device_request_id += 1;
            let device_request_id = state.modal_stack.next_device_request_id;
            state.modal_stack.pending_cached_ios_run = Some(PendingCachedIosRun {
                worktree_id,
                worktree_path,
                cache_hit,
                device_request_id,
            });
            state.modal_stack.pending_device_command = Some(CommandSpec::UmpRunIos {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            });
            effects.push(Effect::LoadDevices {
                kind: DeviceKind::Ios,
                request_id: Some(device_request_id),
            });
        }
        Action::CachedAndroidRun(cache_hit) => {
            state.modal_stack.modal = None;
            state.modal_stack.palette_mode = None;
            state.modal_stack.pending_cached_android_run = None;
            state.modal_stack.pending_device_command = None;
            let Some((worktree_id, worktree_path)) = active_worktree_snapshot(state) else {
                return effects;
            };
            state.modal_stack.next_device_request_id += 1;
            let device_request_id = state.modal_stack.next_device_request_id;
            state.modal_stack.pending_cached_android_run = Some(PendingCachedAndroidRun {
                worktree_id,
                worktree_path,
                cache_hit,
                device_request_id,
            });
            state.modal_stack.pending_device_command = Some(CommandSpec::UmpRunAndroid {
                device_id: String::new(),
                variant: Some(RunVariant::Local),
            });
            effects.push(Effect::LoadDevices {
                kind: DeviceKind::Android,
                request_id: Some(device_request_id),
            });
        }
        Action::CachedIosLaunchFinished {
            worktree_id,
            result,
        } => {
            let mut fallback_spec = None;
            if let Some(slice) = state.worktrees.get_mut(&worktree_id) {
                match result {
                    crate::domain::native_cache::CachedIosLaunchResult::Success(lines) => {
                        slice
                            .output
                            .push_back("[cached-ios] installed and launched cached app".into());
                        for line in lines {
                            slice.output.push_back(format!("[cached-ios] {line}"));
                        }
                    }
                    crate::domain::native_cache::CachedIosLaunchResult::Failure(message) => {
                        slice
                            .output
                            .push_back(format!("[cached-ios error] {message}"));
                    }
                    crate::domain::native_cache::CachedIosLaunchResult::InvalidArtifact {
                        message,
                        device_id,
                        variant,
                    } => {
                        slice
                            .output
                            .push_back(format!("[cached-ios stale] {message}"));
                        slice.ios_simulator_cache =
                            crate::domain::native_cache::IosSimulatorCacheState::Error(
                                message.clone(),
                            );
                        fallback_spec = Some(CommandSpec::UmpRunIos {
                            device_id,
                            variant: Some(variant),
                        });
                    }
                }
                while slice.output.len() > MAX_COMMAND_LINES {
                    slice.output.pop_front();
                }
                slice.output_scroll = 0;
            }
            if let Some(spec) = fallback_spec {
                remember_ump_run_config(state, &spec, false);
                if let Some(effect) = dispatch_command_for_worktree(state, &worktree_id, spec) {
                    effects.push(effect);
                }
            }
        }
        Action::CachedAndroidLaunchFinished {
            worktree_id,
            result,
        } => {
            let mut fallback_spec = None;
            if let Some(slice) = state.worktrees.get_mut(&worktree_id) {
                match result {
                    crate::domain::native_cache::CachedAndroidLaunchResult::Success(lines) => {
                        slice
                            .output
                            .push_back("[cached-android] installed and launched cached app".into());
                        for line in lines {
                            slice.output.push_back(format!("[cached-android] {line}"));
                        }
                    }
                    crate::domain::native_cache::CachedAndroidLaunchResult::Failure(message) => {
                        slice
                            .output
                            .push_back(format!("[cached-android error] {message}"));
                    }
                    crate::domain::native_cache::CachedAndroidLaunchResult::InvalidArtifact {
                        message,
                        device_id,
                        variant,
                    } => {
                        slice
                            .output
                            .push_back(format!("[cached-android stale] {message}"));
                        slice.android_cache =
                            crate::domain::native_cache::AndroidCacheState::Error(message.clone());
                        fallback_spec = Some(CommandSpec::UmpRunAndroid {
                            device_id,
                            variant: Some(variant),
                        });
                    }
                }
                while slice.output.len() > MAX_COMMAND_LINES {
                    slice.output.pop_front();
                }
                slice.output_scroll = 0;
            }
            if let Some(spec) = fallback_spec {
                remember_ump_run_config(state, &spec, false);
                if let Some(effect) = dispatch_command_for_worktree(state, &worktree_id, spec) {
                    effects.push(effect);
                }
            }
        }
        Action::SyncBeforeRunAccept => {
            if let Some(ModalState::SyncBeforeRun {
                run_command,
                needs_yarn,
                needs_pods,
            }) = state.modal_stack.modal.take()
            {
                // Plan 13-09 (F-204 site 7): Recipe::SyncThenRun expansion.
                // The modal already encodes the staleness decision in
                // (needs_yarn, needs_pods); rebuild a DependencyState that
                // reproduces the same expansion. The is_ios_target flag is
                // derived from needs_pods being meaningful — only iOS run
                // commands ever set needs_pods=true at the modal-construction
                // site (CommandRun stale check).
                let deps = DependencyState::new(needs_yarn, needs_pods, needs_pods);
                let mut sequence = Recipe::SyncThenRun(*run_command).expand(&deps);

                // Guaranteed non-empty: we only get here from the modal which only
                // appears when needs_yarn || needs_pods, so sequence has ≥2 elements.
                let first = sequence.remove(0);

                // D-12: push to slice queue for the originating worktree.
                let resolved_id = active_worktree_id(state);
                if let Some(ref wt_id) = resolved_id {
                    let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                        crate::domain::worktree_slice::WorktreeSlice {
                            id: wt_id.clone(),
                            ..Default::default()
                        }
                    });
                    for cmd in sequence {
                        slice.queue.push_back(cmd);
                    }
                }

                if let Some(eff) = dispatch_command(state, first) {
                    effects.push(eff);
                }
            }
        }
        Action::SyncBeforeRunDecline => {
            if let Some(ModalState::SyncBeforeRun { run_command, .. }) =
                state.modal_stack.modal.take()
            {
                // Plan 13-09 (F-204 site 8): skip sync. Metro may still need to
                // start — push the spec onto command_queue front and trigger
                // MetroStart (replaces `pending_metro_run`). The stale check
                // won't re-trigger because the user just declined it.
                let spec = *run_command;
                if spec.needs_metro() && !active_worktree_has_metro(state) {
                    // D-12 + D-13: push to head of slice queue for the originating worktree.
                    if let Some(wt_id) = active_worktree_id(state) {
                        let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                            crate::domain::worktree_slice::WorktreeSlice {
                                id: wt_id.clone(),
                                ..Default::default()
                            }
                        });
                        slice.queue.push_front(spec);
                        effects.extend(update(
                            state,
                            Action::MetroStartForWorktree { worktree_id: wt_id },
                        ));
                    }
                } else if let Some(eff) = dispatch_command(state, spec) {
                    effects.push(eff);
                }
            }
        }

        Action::SyncBeforeMetroAccept => {
            if let Some(ModalState::SyncBeforeMetro {
                needs_yarn,
                needs_pods,
            }) = state.modal_stack.modal.take()
            {
                let worktree_id = active_metro_slice_id(state);
                // Plan 13-09: pending_switch_path deleted. The active_worktree_path
                // was already updated synchronously at the WorktreeSwitchToSelected
                // call site when the SyncBeforeMetro modal was constructed.

                // Stop metro if running (no auto-restart — sync must finish first)
                if active_worktree_has_metro(state) {
                    state.metro_state.pending_restart = false;
                    effects.extend(update(state, Action::MetroStop));
                }

                // Plan 13-09 (F-204 site 9): Recipe::SyncThenStartMetro expansion.
                // is_ios_target is irrelevant for SyncThenStartMetro (pods always
                // included when stale per the metro-platform-agnostic rule).
                let deps = DependencyState::new(needs_yarn, needs_pods, false);
                let sequence = Recipe::SyncThenStartMetro.expand(&deps);

                let mut sequence = sequence;

                if sequence.is_empty() {
                    // No sync needed — start metro directly.
                    effects.extend(update(
                        state,
                        Action::MetroStartForWorktree {
                            worktree_id: worktree_id.clone(),
                        },
                    ));
                    return effects;
                }

                let first = sequence.remove(0);

                // D-12 + D-14: push to slice queue; set per-slice post_drain to MetroStart.
                let slice = state
                    .worktrees
                    .entry(worktree_id.clone())
                    .or_insert_with(|| crate::domain::worktree_slice::WorktreeSlice {
                        id: worktree_id.clone(),
                        ..Default::default()
                    });
                for cmd in sequence {
                    slice.queue.push_back(cmd);
                }
                slice.post_drain = Some(Box::new(Action::MetroStartForWorktree {
                    worktree_id: worktree_id.clone(),
                }));

                if let Some(eff) = dispatch_command_for_worktree(state, &worktree_id, first) {
                    effects.push(eff);
                }
            }
        }

        Action::SyncBeforeMetroDecline => {
            if let Some(ModalState::SyncBeforeMetro { .. }) = state.modal_stack.modal.take() {
                // Plan 13-09: active_worktree_path is already set at the
                // WorktreeSwitchToSelected call site (no pending_switch_path).
                let worktree_id = active_metro_slice_id(state);
                effects.extend(update(state, Action::MetroStartForWorktree { worktree_id }));
            }
        }

        // --- Phase 5.2: Universal scroll ---
        Action::ScrollToTop => {
            if state.focused_panel == FocusedPanel::CommandOutput
                && let Some(id) = active_worktree_id(state)
                && let Some(slice) = state.worktrees.get_mut(&id)
            {
                slice.output_scroll = 0;
            }
        }

        Action::ScrollToBottom => {
            if state.focused_panel == FocusedPanel::CommandOutput
                && let Some(id) = active_worktree_id(state)
                && let Some(slice) = state.worktrees.get_mut(&id)
            {
                slice.output_scroll = slice.output.len();
            }
        }

        Action::SetPendingG => {
            state.modal_stack.pending_g = true;
        }

        Action::CommandOutputScrollUp => {
            if let Some(id) = active_worktree_id(state)
                && let Some(slice) = state.worktrees.get_mut(&id)
            {
                slice.output_scroll = slice.output_scroll.saturating_sub(1);
            }
        }

        Action::CommandOutputScrollDown => {
            let max = active_output(state).len();
            if let Some(id) = active_worktree_id(state)
                && let Some(slice) = state.worktrees.get_mut(&id)
                && slice.output_scroll < max
            {
                slice.output_scroll += 1;
            }
        }

        // --- Quick-2: Worktree removal ---
        Action::WorktreeRemove => {
            if state.worktree_browser.worktrees.is_empty() {
                return effects;
            }
            let idx = state
                .worktree_browser
                .worktree_table_state
                .selected()
                .unwrap_or(0)
                .min(state.worktree_browser.worktrees.len() - 1);
            let wt = state.worktree_browser.worktrees[idx].clone();

            // Guard: cannot remove the main worktree (its path equals repo_root)
            if wt.path == state.app_config.repo_root {
                state.error_state = Some(ErrorState {
                    message: "Cannot remove the main worktree".into(),
                    can_retry: false,
                });
                state.modal_stack.palette_mode = None;
                return effects;
            }

            // Store removal target so ModalConfirm knows what to do
            state.modal_stack.pending_worktree_removal =
                Some((wt.id.clone(), wt.path.clone(), wt.branch.clone()));

            // Build confirm prompt — mention metro if it will be stopped
            let metro_note = if state
                .worktrees
                .get(&wt.id)
                .map(|slice| slice.metro.is_running())
                .unwrap_or(false)
            {
                " (metro will be stopped)"
            } else {
                ""
            };
            let prompt = format!(
                "Remove worktree '{}' and delete directory?{}",
                wt.branch, metro_note
            );

            // Use a sentinel CommandSpec for the confirm modal — the actual removal
            // logic is in ModalConfirm when pending_worktree_removal is Some.
            state.modal_stack.modal = Some(ModalState::Confirm {
                prompt,
                pending_command: crate::domain::command::CommandSpec::GitPull, // sentinel
            });
            state.modal_stack.palette_mode = None;
        }

        Action::WorktreeRemoved(path_str) => {
            tracing::info!("worktree removed: {}", path_str);
            state.worktree_browser.worktree_op_in_flight = false;
            // Refresh the worktree list to reflect the removal
            effects.push(Effect::ListWorktrees {
                repo_root: state.app_config.repo_root.clone(),
            });
        }

        Action::WorktreeRemoveFailed(err) => {
            state.worktree_browser.worktree_op_in_flight = false;
            state.error_state = Some(ErrorState {
                message: format!("Failed to remove worktree: {err}"),
                can_retry: false,
            });
            // Re-add the worktree to the UI since git removal failed
            effects.extend(update(state, Action::RefreshWorktrees));
        }

        // --- Quick-260403-dmz: Worktree creation ---
        Action::WorktreeAdd => {
            state.modal_stack.palette_mode = None;
            state.modal_stack.pending_worktree_add = true;
            state.modal_stack.modal = Some(ModalState::TextInput {
                prompt: "New worktree branch name:".to_string(),
                buffer: String::new(),
                pending_template: Box::new(crate::domain::command::CommandSpec::GitPull), // sentinel — not used
            });
        }

        Action::WorktreeAdded(path_str) => {
            tracing::info!("worktree added: {}", path_str);
            state.worktree_browser.worktree_op_in_flight = false;
            effects.push(Effect::ListWorktrees {
                repo_root: state.app_config.repo_root.clone(),
            });
        }

        Action::WorktreeAddFailed(err) => {
            state.worktree_browser.worktree_op_in_flight = false;
            state.error_state = Some(ErrorState {
                message: format!("Failed to create worktree: {err}"),
                can_retry: false,
            });
        }

        // Phase 08-02: New-branch worktree creation flow
        Action::WorktreeAddNewBranch => {
            state.modal_stack.palette_mode = None;
            effects.push(Effect::ListRemoteBranches {
                repo_root: state.app_config.repo_root.clone(),
            });
        }

        Action::BranchesLoaded(branches) => {
            state.modal_stack.modal = Some(ModalState::BranchPicker {
                branches,
                selected: 0,
                filter: String::new(),
            });
        }

        Action::BranchPickerNext => {
            if let Some(ModalState::BranchPicker {
                ref branches,
                ref mut selected,
                ref filter,
            }) = state.modal_stack.modal
            {
                let count = if filter.is_empty() {
                    branches.len()
                } else {
                    let lower = filter.to_lowercase();
                    branches
                        .iter()
                        .filter(|b| b.to_lowercase().contains(&lower))
                        .count()
                };
                if count > 0 {
                    *selected = if *selected >= count - 1 {
                        0
                    } else {
                        *selected + 1
                    };
                }
            }
        }

        Action::BranchPickerPrev => {
            if let Some(ModalState::BranchPicker {
                ref branches,
                ref mut selected,
                ref filter,
            }) = state.modal_stack.modal
            {
                let count = if filter.is_empty() {
                    branches.len()
                } else {
                    let lower = filter.to_lowercase();
                    branches
                        .iter()
                        .filter(|b| b.to_lowercase().contains(&lower))
                        .count()
                };
                if count > 0 {
                    *selected = if *selected == 0 {
                        count - 1
                    } else {
                        *selected - 1
                    };
                }
            }
        }

        Action::BranchPickerFilter(c) => {
            if let Some(ModalState::BranchPicker {
                ref mut filter,
                ref mut selected,
                ..
            }) = state.modal_stack.modal
            {
                filter.push(c);
                *selected = 0;
            }
        }

        Action::BranchPickerBackspace => {
            if let Some(ModalState::BranchPicker {
                ref mut filter,
                ref mut selected,
                ..
            }) = state.modal_stack.modal
            {
                filter.pop();
                *selected = 0;
            }
        }

        Action::BranchPickerConfirm => {
            if let Some(ModalState::BranchPicker {
                branches,
                selected,
                filter,
            }) = state.modal_stack.modal.take()
            {
                // Apply filter to get visible list
                let filtered: Vec<&String> = if filter.is_empty() {
                    branches.iter().collect()
                } else {
                    let lower = filter.to_lowercase();
                    branches
                        .iter()
                        .filter(|b| b.to_lowercase().contains(&lower))
                        .collect()
                };
                if let Some(base_branch) = filtered.get(selected) {
                    state.modal_stack.pending_new_branch_base = Some((*base_branch).clone());
                    state.modal_stack.pending_new_branch_worktree = true;
                    state.modal_stack.modal = Some(ModalState::TextInput {
                        prompt: "New branch name:".to_string(),
                        buffer: String::new(),
                        pending_template: Box::new(CommandSpec::GitPull), // sentinel — not used
                    });
                }
            }
        }

        Action::WorktreeNewBranchCreated(path_str) => {
            tracing::info!("worktree with new branch created: {}", path_str);
            state.worktree_browser.worktree_op_in_flight = false;
            effects.push(Effect::ListWorktrees {
                repo_root: state.app_config.repo_root.clone(),
            });
        }

        Action::WorktreeNewBranchFailed(err) => {
            state.worktree_browser.worktree_op_in_flight = false;
            state.error_state = Some(ErrorState {
                message: format!("Failed to create worktree with new branch: {err}"),
                can_retry: false,
            });
        }

        // Quick-260622-bvn: PR review worktree flow
        Action::ReviewOpen => {
            state.modal_stack.palette_mode = None;
            state.modal_stack.pending_review_checkout = None;
            state.modal_stack.modal = Some(ModalState::PullRequestPicker {
                pull_requests: Vec::new(),
                selected: 0,
                search: String::new(),
                filter: PullRequestFilter::All,
            });
            effects.push(Effect::ListPullRequests {
                repo_root: state.app_config.repo_root.clone(),
                filter: PullRequestFilter::All,
            });
        }

        Action::PullRequestsLoaded(pull_requests) => {
            if let Some(ModalState::PullRequestPicker {
                pull_requests: existing,
                selected,
                ..
            }) = state.modal_stack.modal.as_mut()
            {
                *existing = pull_requests;
                *selected = 0;
            }
        }

        Action::PullRequestsLoadFailed(err) => {
            state.error_state = Some(ErrorState {
                message: format!("Failed to load pull requests: {err}"),
                can_retry: false,
            });
        }

        Action::PullRequestPickerNext => {
            if let Some(ModalState::PullRequestPicker {
                pull_requests,
                selected,
                search,
                ..
            }) = state.modal_stack.modal.as_mut()
            {
                let count = visible_pull_request_count(pull_requests, search);
                if count > 0 {
                    *selected = if *selected >= count - 1 {
                        0
                    } else {
                        *selected + 1
                    };
                }
            }
        }

        Action::PullRequestPickerPrev => {
            if let Some(ModalState::PullRequestPicker {
                pull_requests,
                selected,
                search,
                ..
            }) = state.modal_stack.modal.as_mut()
            {
                let count = visible_pull_request_count(pull_requests, search);
                if count > 0 {
                    *selected = if *selected == 0 {
                        count - 1
                    } else {
                        *selected - 1
                    };
                }
            }
        }

        Action::PullRequestPickerFilter(c) => {
            if let Some(ModalState::PullRequestPicker {
                search, selected, ..
            }) = state.modal_stack.modal.as_mut()
            {
                search.push(c);
                *selected = 0;
            }
        }

        Action::PullRequestPickerBackspace => {
            if let Some(ModalState::PullRequestPicker {
                search, selected, ..
            }) = state.modal_stack.modal.as_mut()
            {
                search.pop();
                *selected = 0;
            }
        }

        Action::PullRequestPickerNextFilter => {
            if let Some(ModalState::PullRequestPicker {
                pull_requests,
                selected,
                search,
                filter,
            }) = state.modal_stack.modal.as_mut()
            {
                *filter = filter.next();
                pull_requests.clear();
                search.clear();
                *selected = 0;
                effects.push(Effect::ListPullRequests {
                    repo_root: state.app_config.repo_root.clone(),
                    filter: *filter,
                });
            }
        }

        Action::PullRequestPickerConfirm => {
            if let Some(ModalState::PullRequestPicker {
                pull_requests,
                selected,
                search,
                filter,
            }) = state.modal_stack.modal.take()
            {
                let Some(pr) = selected_visible_pull_request(&pull_requests, selected, &search)
                else {
                    state.modal_stack.modal = Some(ModalState::PullRequestPicker {
                        pull_requests,
                        selected,
                        search,
                        filter,
                    });
                    return effects;
                };

                if let Some(existing) = state
                    .worktree_browser
                    .worktrees
                    .iter()
                    .find(|wt| wt.branch == pr.head_ref_name)
                {
                    state.modal_stack.modal = Some(ModalState::Info {
                        message: format!(
                            "Branch '{}' is already checked out at {}",
                            pr.head_ref_name,
                            existing.path.display()
                        ),
                    });
                    state.modal_stack.pending_review_checkout = None;
                    return effects;
                }

                let default_name = default_review_worktree_name(&pr);
                state.modal_stack.pending_review_checkout = Some(pr);
                state.modal_stack.modal = Some(ModalState::TextInput {
                    prompt: "Review worktree name:".to_string(),
                    buffer: default_name,
                    pending_template: Box::new(CommandSpec::GitPull),
                });
            }
        }

        Action::ReviewWorktreeCreated {
            branch,
            path,
            head_sha,
        } => {
            state.worktree_browser.worktree_op_in_flight = false;
            let id = WorktreeId(path.to_string_lossy().to_string());
            let worktree = Worktree {
                id: id.clone(),
                path: path.clone(),
                branch: branch.clone(),
                head_sha,
                metro_status: WorktreeMetroStatus::Stopped,
                jira_title: None,
                stale: true,
                stale_pods: false,
                jira_key: crate::domain::jira::extract_jira_key(
                    &branch,
                    &state.jira.project_prefix,
                ),
            };

            if let Some(existing) = state
                .worktree_browser
                .worktrees
                .iter_mut()
                .find(|existing| existing.id == id)
            {
                *existing = worktree;
            } else {
                state.worktree_browser.worktrees.push(worktree);
            }
            let selected = state
                .worktree_browser
                .worktrees
                .iter()
                .position(|wt| wt.id == id)
                .unwrap_or(0);
            state
                .worktree_browser
                .worktree_table_state
                .select(Some(selected));
            state.worktree_browser.selected_worktree_id = Some(id.clone());
            state.worktrees.entry(id.clone()).or_insert_with(|| {
                crate::domain::worktree_slice::WorktreeSlice {
                    id: id.clone(),
                    ..Default::default()
                }
            });

            if let Some(effect) =
                dispatch_command_for_worktree(state, &id, CommandSpec::YarnInstall)
            {
                effects.push(effect);
            }
            effects.push(Effect::ListWorktrees {
                repo_root: state.app_config.repo_root.clone(),
            });
        }

        Action::ReviewWorktreeCreateFailed(err) => {
            state.worktree_browser.worktree_op_in_flight = false;
            state.error_state = Some(ErrorState {
                message: format!("Failed to create review worktree: {err}"),
                can_retry: false,
            });
        }
    }

    effects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::native_cache::{
        CachedIosLaunchResult, IosSimulatorCacheHit, IosSimulatorCacheMetadata,
        IosSimulatorCacheState, PendingCachedIosLaunch,
    };

    fn cache_hit() -> IosSimulatorCacheHit {
        IosSimulatorCacheHit {
            metadata: IosSimulatorCacheMetadata {
                platform: "ios-simulator".into(),
                fingerprint: "fingerprint-a".into(),
                bundle_id: "com.aljazeera.test".into(),
                variant: "Debug".into(),
                created_at: "2026-06-01T00:00:00Z".into(),
                source_worktree: "wt-a".into(),
                artifact_kind: "app-bundle".into(),
                storage_mode: "copy".into(),
                source_artifact_path: PathBuf::from("/tmp/wt-a/app.app"),
                artifact_digest_algorithm: "sha256".into(),
                artifact_digest: "digest".into(),
            },
            artifact_path: PathBuf::from("/tmp/cached.app"),
        }
    }

    #[test]
    fn cached_ios_launch_finished_appends_output_without_mutating_cache_state() {
        let worktree_id = WorktreeId("wt-a".into());
        let existing_hit = cache_hit();
        let pending = PendingCachedIosLaunch {
            device_id: "SIM-1".into(),
            cache_hit: existing_hit.clone(),
        };
        let mut state = AppState {
            error_state: Some(ErrorState {
                message: "existing error".into(),
                can_retry: true,
            }),
            ..Default::default()
        };
        state.worktrees.insert(
            worktree_id.clone(),
            crate::domain::worktree_slice::WorktreeSlice {
                id: worktree_id.clone(),
                ios_simulator_cache: IosSimulatorCacheState::Checking,
                pending_cached_ios_launch: Some(pending.clone()),
                output_scroll: 7,
                ..Default::default()
            },
        );

        let effects = update(
            &mut state,
            Action::CachedIosLaunchFinished {
                worktree_id: worktree_id.clone(),
                result: CachedIosLaunchResult::Failure("launch failed".into()),
            },
        );

        assert!(effects.is_empty());
        let slice = state
            .worktrees
            .get(&worktree_id)
            .expect("slice should remain");
        assert_eq!(slice.ios_simulator_cache, IosSimulatorCacheState::Checking);
        assert_eq!(slice.pending_cached_ios_launch, Some(pending.clone()));
        assert!(
            slice
                .output
                .iter()
                .any(|line| line == "[cached-ios error] launch failed")
        );
        assert_eq!(slice.output_scroll, 0);
        let error_state = state
            .error_state
            .as_ref()
            .expect("error state should remain");
        assert_eq!(error_state.message, "existing error");
        assert!(error_state.can_retry);
    }
}
