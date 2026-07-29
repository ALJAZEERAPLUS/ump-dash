# Phase 1 (CommandSpec info-card) — SDD progress ledger

Branch: remove-git-checkout-rebase-commands
Base (before Task 1): 5a2a37a
Plan: docs/superpowers/plans/2026-06-26-command-info-card-phase1.md

## Tasks
- [x] Task 1: golden metadata matrix (characterization safety net)
- [x] Task 2: CommandMeta + meta(); rewire 4 readers; RefreshSet: Copy
- [x] Task 3: refresh_needed() -> meta().refresh
- [x] Task 4: retire redundant collision drift-guard

## Log
Task 1: complete (commits 5a2a37a..1f87a7c, review clean). Reviewer's "Critical missing trailer" was a FALSE POSITIVE — verified trailer present in 1f87a7c. Minor (cosmetic): RefreshSet::none().clone() x10 could inline; plan-mandated, deferred to final review.
Task 2: complete (commits 1f87a7c..11ba416, review clean — all 19 meta() rows verified correct, golden test untouched). Minor: stale doc-comments on collision_policy()/is_cancellable() still reference the old "exhaustive match / no _ arm" drift-guard rationale (now lives on meta()). FOLD INTO Task 4.
Task 3: complete (commits 11ba416..79b15ab, review clean — only refresh.rs, 1-line body, 369 passing).
Task 4: complete (commits 79b15ab..8e5ec3c, review clean — drift-guard removed, 2 stale doc-comments fixed, drift protection intact via meta() exhaustiveness + matrix count). ALL 4 TASKS DONE.

## Final review (opus): READY TO MERGE
- Spec §4 fully covered; behavior provably preserved (variant-for-variant); net -67 lines; out-of-scope clean.
- New Minor (deferred): src/ui/indicators.rs doc-comment references the deleted collision_policy_covers_every_variant test (dangling). Out of Phase 1 scope; clean up in Phase 2 / backlog.
- Logged Minors: matrix .clone() on Copy temporaries (cosmetic, leave — don't edit golden test); Task 2 stale docs already fixed in Task 4.

# Phase 2a (dependency resolver) — base: e63ff01
Plan: docs/superpowers/plans/2026-06-26-command-dependency-resolver-phase2a.md
- [x] P2a-Task 1: deps field on CommandMeta + populate 19 rows
- [x] P2a-Task 2: is_satisfied() + resolve() in pipeline.rs (incl resolve==SyncThenRun equivalence test)
- [x] P2a-Task 3: migrate 4 SyncThenRun call sites to resolve()
- [x] P2a-Task 4: remove dead Recipe::SyncThenRun
P2a-Task1: complete 4310830 (done inline after agent infra hang; 369 passing; dropped Eq from CommandMeta)
P2a-Task2: complete 22e7952 (resolve==SyncThenRun equivalence proven)
P2a-Task3: complete 0bf86c5 (4 sites migrated, 375 passing, dispatch suite green)
P2a-Task4: complete f74ceb4 (SyncThenRun gone, --all-targets clean, 371 passing). ALL P2a TASKS DONE.

## Phase 2a final review (opus): READY TO MERGE
- resolve() ≡ old SyncThenRun.expand() proven for all combos; invariant stale_pods⟹iOS verified in source (update.rs:1950). Out-of-scope clean. Eq-drop safe.
- Minor (FIXED): pipeline.rs comment referenced non-existent deps() -> meta().deps.
- Minor (DEFERRED): DependencyState.is_ios_target is now vestigial (constructed at ~8 sites, zero reads). Recommend a fast-follow to drop the field + 3rd new() arg + stale doc at pipeline.rs:89. Non-blocking.

## is_ios_target cleanup (the deferred Minor): DONE
- Removed the vestigial field + 3rd DependencyState::new arg across ~10 call sites + stale docs/comments (commit 45355fd). Compile-guarded (removing the arg forces every site to update). --all-targets clean, 371 passing.
## PHASE 2a FULLY COMPLETE — branch ready to merge (Phase 2b/metro deferred by decision).
