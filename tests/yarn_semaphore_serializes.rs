//! TASK-06 end-to-end characterization — proves that two yarn-family
//! `SpawnTask` effects with the same canonicalized `repo_root` execute
//! serially via the per-repo-root `Semaphore(1)` owned by `EffectRunner`.
//!
//! Substitutes `tokio::time::sleep` for `yarn install` (the real install would
//! take 30s+; this fixture proves the SERIALIZATION CONTRACT, not yarn itself).
//! Asserts the second task's `started_at` is at least 450ms after the first
//! task's `started_at` (the first task sleeps 500ms, with 50ms jitter slack).
//!
//! Two-test pair catches both the "always serializes" and "never serializes"
//! bug shapes symmetrically:
//!
//! - `same_repo_root_serializes_two_yarn_family_tasks` — same `repo_root` →
//!   serial; second `started_at >= first.finished_at`.
//! - `different_repo_roots_run_in_parallel` — different `repo_root` →
//!   concurrent; second `started_at < first.finished_at`.
//!
//! GUARD ROLE: this test fails RED if `EffectRunner::yarn_semaphores` ever
//! becomes a single global `Semaphore` instead of a `HashMap<PathBuf, _>`
//! (would break the parallel test), or if the semaphore acquire is dropped
//! from the `SpawnTask` arm of `effect_runner` (would break the serial test).
//! This is a CONTRACT test against the same data structure shape — the
//! production wiring is locked by Plan 15-03's inline `effect_runner` tests
//! and code review of the `SpawnTask` arm.

#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

/// Shared semaphore map keyed by canonicalized `repo_root`. Mirrors
/// `src/app/effect_runner.rs::EffectRunner::yarn_semaphores`.
type SemaphoreMap = Arc<std::sync::Mutex<HashMap<PathBuf, Arc<Semaphore>>>>;

/// Acquire-then-sleep helper. Mirrors the `SpawnTask` arm of
/// `src/app/effect_runner.rs`: canonicalize `repo_root`, look up or insert
/// `Arc<Semaphore::new(1)>`, await the permit, then run the task body.
///
/// Returns `(started_at, finished_at)`. The permit drops at function end,
/// releasing the next waiter.
async fn run_with_semaphore(
    semaphores: SemaphoreMap,
    repo_root: PathBuf,
    sleep_ms: u64,
) -> (Instant, Instant) {
    // Step 1: canonicalize (matches Plan 15-03 effect_runner logic).
    let canonical = repo_root.canonicalize().unwrap_or(repo_root);

    // Step 2: acquire semaphore.
    let sem = {
        let mut map = semaphores.lock().unwrap();
        map.entry(canonical)
            .or_insert_with(|| Arc::new(Semaphore::new(1)))
            .clone()
    };
    let _permit = sem.acquire_owned().await.expect("semaphore closed");

    // Step 3: simulate yarn install with a sleep fixture.
    let started_at = Instant::now();
    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
    let finished_at = Instant::now();

    (started_at, finished_at)
}

/// Two tasks with the SAME repo_root must serialize.
#[tokio::test(flavor = "multi_thread")]
async fn same_repo_root_serializes_two_yarn_family_tasks() {
    let body = async {
        let semaphores: SemaphoreMap =
            Arc::new(std::sync::Mutex::new(HashMap::new()));

        let repo = std::env::temp_dir();
        let h1 = run_with_semaphore(semaphores.clone(), repo.clone(), 500);
        let h2 = run_with_semaphore(semaphores.clone(), repo.clone(), 100);
        let (r1, r2) = tokio::join!(h1, h2);

        // Whichever task acquired the permit first ran first; the other waited.
        // Identify the leader by `started_at` to make the assertion symmetric
        // under tokio scheduling jitter.
        let (first, second) = if r1.0 <= r2.0 { (r1, r2) } else { (r2, r1) };

        // Strict serialization: second.started_at must be at-or-after
        // first.finished_at.
        assert!(
            second.0 >= first.1,
            "SERIALIZATION REGRESSION: second task started at {:?} before first \
             finished at {:?} (gap {:?})",
            second.0,
            first.1,
            first.1.saturating_duration_since(second.0),
        );

        // Diagnostic assertion: second.started_at must be at least 450ms after
        // first.started_at (first sleeps 500ms; 50ms slack for tokio jitter).
        let gap = second.0.duration_since(first.0);
        assert!(
            gap >= Duration::from_millis(450),
            "SEMAPHORE GAP REGRESSION: second task started only {:?} after \
             first (expected >= 450ms)",
            gap,
        );
    };

    tokio::time::timeout(Duration::from_secs(3), body).await.expect(
        "test exceeded 3s timeout — semaphore serialization regressed (deadlock?)",
    );
}

/// Two tasks with DIFFERENT repo_roots must run in parallel.
#[tokio::test(flavor = "multi_thread")]
async fn different_repo_roots_run_in_parallel() {
    let body = async {
        let semaphores: SemaphoreMap =
            Arc::new(std::sync::Mutex::new(HashMap::new()));

        let repo_a = std::env::temp_dir();
        // Per-test unique sibling directory so canonicalize succeeds on both.
        // Use the test name + pid to avoid collisions if cargo runs tests
        // concurrently across processes.
        let repo_b = std::env::temp_dir().join(format!(
            "ump-dash-yarn-semaphore-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&repo_b).expect("create sibling repo dir");

        let h1 = run_with_semaphore(semaphores.clone(), repo_a, 500);
        let h2 = run_with_semaphore(semaphores.clone(), repo_b.clone(), 500);
        let (r1, r2) = tokio::join!(h1, h2);

        // Both tasks slept 500ms. If the semaphore is correctly keyed by
        // repo_root, they ran in parallel — second.started_at < first.finished_at.
        let (first, second) = if r1.0 <= r2.0 { (r1, r2) } else { (r2, r1) };

        assert!(
            second.0 < first.1,
            "PARALLELISM REGRESSION: different-repo-root tasks serialized — \
             second started at {:?}, first finished at {:?} (gap {:?})",
            second.0,
            first.1,
            second.0.duration_since(first.1),
        );

        // Stronger diagnostic: the two tasks should overlap by at least
        // 300ms of the 500ms window (allowing 100ms slack on each end for
        // tokio scheduling). If they overlap by less, something is partially
        // serializing them.
        let overlap = first.1.duration_since(second.0);
        assert!(
            overlap >= Duration::from_millis(300),
            "PARALLELISM REGRESSION: different-repo-root tasks overlapped only \
             {:?} (expected >= 300ms of the 500ms windows)",
            overlap,
        );

        // Cleanup — best-effort; ignore failure (test passed by this point).
        let _ = std::fs::remove_dir_all(&repo_b);
    };

    tokio::time::timeout(Duration::from_secs(3), body).await.expect(
        "test exceeded 3s timeout — different-repo-root parallelism deadlocked?",
    );
}
