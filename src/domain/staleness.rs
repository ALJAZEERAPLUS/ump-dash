//! Worktree staleness checks — pure file-system inspection.
//!
//! These helpers are reads of file metadata + small file contents
//! (Podfile.lock). They contain no side effects beyond `std::fs` reads, no
//! async I/O, and no external process spawns. The domain layer can call them
//! directly without crossing a port boundary.
//!
//! Plan 13-08: extracted from `crate::infra::worktrees` so `src/app/update.rs`
//! can invoke staleness checks inside the pure reducer without importing
//! `crate::infra::*` (G-01 hexagonal boundary).
//!
//! `crate::infra::worktrees::check_stale` and `check_stale_pods` continue to
//! exist as thin re-exports for any non-app callers that still reference them.

#![allow(dead_code)]

use std::path::Path;

/// Returns `true` when the yarn install state is older than the lock files
/// (or no install sentinel is found and `node_modules` is missing).
///
/// Sentinel preference:
/// 1. `.yarn/install-state.gz` — Yarn Berry (v2/v3/v4) — most reliable.
/// 2. `node_modules/.yarn-integrity` — Yarn classic v1 fallback.
///
/// When neither sentinel is found AND `node_modules` does not exist → stale.
/// When `node_modules` exists but no sentinel is found → not stale (benefit
/// of the doubt — common with non-yarn package managers).
/// When a sentinel IS found, staleness = sentinel mtime < max(package.json,
/// yarn.lock) mtime.
pub fn check_stale(worktree_path: &Path) -> bool {
    let berry_state = worktree_path.join(".yarn").join("install-state.gz");
    let yarn_integrity = worktree_path.join("node_modules").join(".yarn-integrity");

    let sentinel_mtime = std::fs::metadata(&berry_state)
        .and_then(|m| m.modified())
        .ok()
        .or_else(|| {
            std::fs::metadata(&yarn_integrity)
                .and_then(|m| m.modified())
                .ok()
        });

    let sentinel_mtime = match sentinel_mtime {
        Some(t) => t,
        None => {
            let node_modules = worktree_path.join("node_modules");
            if !node_modules.exists() {
                return true;
            }
            return false;
        }
    };

    let mut max_lock_mtime: Option<std::time::SystemTime> = None;
    for lock_file in &["package.json", "yarn.lock"] {
        let lock_path = worktree_path.join(lock_file);
        if let Ok(mtime) = std::fs::metadata(&lock_path).and_then(|m| m.modified()) {
            max_lock_mtime = Some(match max_lock_mtime {
                Some(current) => current.max(mtime),
                None => mtime,
            });
        }
    }

    match max_lock_mtime {
        Some(lock_mtime) => sentinel_mtime < lock_mtime,
        None => false,
    }
}

/// Returns `true` when pods are out of sync — same check CocoaPods' build
/// phase uses: compare `ios/Podfile.lock` contents against
/// `ios/Pods/Manifest.lock`. If they differ (or `Manifest.lock` is missing)
/// pods need `pod install`.
pub fn check_stale_pods(worktree_path: &Path) -> bool {
    let podfile_lock = worktree_path.join("ios").join("Podfile.lock");
    let manifest_lock = worktree_path.join("ios").join("Pods").join("Manifest.lock");

    let lock_bytes = match std::fs::read(&podfile_lock) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let manifest_bytes = match std::fs::read(&manifest_lock) {
        Ok(b) => b,
        Err(_) => return true,
    };

    lock_bytes != manifest_bytes
}
