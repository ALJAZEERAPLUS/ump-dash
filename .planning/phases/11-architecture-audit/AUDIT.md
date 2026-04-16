# Architecture Audit — Phase 11

> Phase 11 deliverable. Read-only audit of the codebase against four lenses:
> Ousterhout deep-module/narrow-interface, Fowler 4-layer model,
> hexagonal ports-and-adapters discipline, and two completeness sweeps
> (catch-all match arms, misplaced prerequisite/ordering logic).
>
> Severity calibration is **Aggressive** per CONTEXT.md D-01:
> - **Critical:** cross-layer leak, god-object behavior, TEA impurity in `update()`,
>   shallow modules on the v1.3 critical path.
> - **Major:** clear lens violation with refactor cost <1 day; misplaced
>   prerequisite logic; catch-all `_ => {}` arms with reachable variants.
> - **Minor:** cosmetic, naming, small extractions, documentation gaps.
>
> Phase 13 will resolve every Critical and Major finding (REFACTOR-01).
>
> Findings use sequential IDs F-NNN. ID ranges per plan:
> 11-01 domain: F-001..F-099 · 11-02 infra: F-100..F-199 ·
> 11-03 app: F-200..F-299 · 11-04 ui: F-300..F-399 · 11-05 cross: F-400..F-499.

## Module: root/
<!-- Coverage: src/main.rs, src/tui.rs, src/event.rs, src/action.rs (per Pitfall 9 — root files in scope) -->
<!-- Wave 1 Plan 11-01 appends Ousterhout scores + findings here -->

### Critical
### Major
### Minor

## Module: domain/
<!-- Coverage: src/domain/mod.rs, command.rs, metro.rs, refresh.rs, worktree.rs -->
<!-- Wave 1 Plan 11-01 appends here -->

### Critical
### Major
### Minor

## Module: infra/
<!-- Coverage: src/infra/{mod,port,process,worktrees,command_runner,devices,config,jira,jira_cache,multiplexer,sim_history,android_prefs,tmux}.rs -->
<!-- Wave 1 Plan 11-02 appends here -->

### Critical
### Major
### Minor

## Module: app/
<!-- Coverage: src/app.rs (the single 2,425-LOC file) -->
<!-- Wave 1 Plan 11-03 appends here, INCLUDING D-04 target shapes for Criticals -->

### Critical
### Major
### Minor

## Module: ui/
<!-- Coverage: src/ui/{mod,panels,footer,help_overlay,error_overlay,modals,theme}.rs -->
<!-- Wave 1 Plan 11-04 appends here, plus initial keybinding evidence (D-14) -->

### Critical
### Major
### Minor

## Cross-Cutting Findings

### Catch-all match arms (ARCH-04)
<!-- Wave 2 Plan 11-05 enumerates every `_ => {}` and `_ =>` arm here -->

### Misplaced prerequisite/ordering logic (ARCH-05)
<!-- Wave 2 Plan 11-05 enumerates the prerequisite locations from RESEARCH §Prerequisite/Ordering Logic Detection -->

### Hexagonal port violations (cross-module — ARCH-03)
<!-- Wave 2 Plan 11-05 captures cross-module hexagonal findings not already attached to a single per-module section -->

### Keybinding source-of-truth (D-14)
<!-- Wave 2 Plan 11-05 finalizes the D-14 finding, referencing handle_key + footer.rs + help_overlay.rs -->

## Refactor Sequence

<!-- Wave 2 Plan 11-05 lists every Critical and Major F-NNN here in dependency order, per D-09 -->
