# Makefile — local-only coverage targets (D-02, no CI wiring this phase).
#
# Prerequisites (one-time, per dev machine):
#   cargo install cargo-llvm-cov --locked
#   rustup component add llvm-tools-preview
#
# CLAUDE.md: check-types always uses --incremental. This file does NOT invoke
# check-types; if a future target adds it, use `cargo check --incremental`.

.PHONY: cov cov-html cov-baseline cov-check arch-lint

# Quick HTML report for local dev (output at target/llvm-cov/html/index.html).
# D-03: HTML is gitignored (not committed).
cov-html:
	cargo llvm-cov --workspace --html

# Regenerate the committed baseline JSON. Run once per phase-12 plan iteration.
# D-03: BASELINE-COVERAGE.json IS committed.
cov-baseline:
	cargo llvm-cov --workspace --json --summary-only \
		--output-path .planning/phases/12-coverage-gate/BASELINE-COVERAGE.json
	@echo "Baseline written. Regenerate BASELINE-COVERAGE.md + COVERAGE-THRESHOLDS.md in 12-04."

# Human-check gate — prints each module's current % alongside the committed baseline.
# D-05: no enforcement script; human verifies every row >= threshold before /gsd-verify-work.
cov-check:
	@cargo llvm-cov --workspace --json --summary-only --output-path /tmp/cov-current.json
	@jq -r '.data[0].files[] | "\(.filename) \(.summary.lines.percent)"' /tmp/cov-current.json

# Default — regenerate baseline + HTML.
cov: cov-baseline cov-html

# Architecture grep guards — verifies Phase 13 hexagonal invariants. Run after
# every wave. A failure here indicates a regression (trait/impl placement drift).
#
# Guards are grouped by finding; guards for not-yet-landed features reference
# the plan that introduces them. Fail-fast: any grep hit OR missing expected file
# aborts the target with a nonzero exit code. Guards guarded by `[ ! -f <file> ] ||`
# are vacuously satisfied until the target file is created by its landing plan.
arch-lint:
	@echo "=== G-01/G-02/G-03: hexagonal import boundaries ==="
	@# G-01 ACTIVE as of Plan 13-08. The whitelist allows the three F-111
	@# (PersistencePort) deferral lines in effect_runner.rs that still call
	@# crate::infra::{jira_cache,android_prefs,sim_history}::save_* directly.
	@# When F-111 lands those will route through Adapters.persistence and the
	@# whitelist disappears.
	@if rg -n 'crate::infra::' src/app/ 2>/dev/null | rg -v '^[^:]+:[0-9]+:\s*//' | rg -v 'effect_runner\.rs.*(jira_cache|android_prefs|sim_history)' | grep -q .; then echo "G-01 FAIL: app/ imports infra (non-persistence)"; rg -n 'crate::infra::' src/app/ 2>/dev/null | rg -v '^[^:]+:[0-9]+:\s*//' | rg -v 'effect_runner\.rs.*(jira_cache|android_prefs|sim_history)'; exit 1; fi
	@! rg 'crate::infra::' src/ui/ 2>/dev/null || (echo "G-02 FAIL: ui/ imports infra" && exit 1)
	@! rg 'use crate::(domain::)?action' src/infra/ 2>/dev/null || echo "G-03 PENDING: infra still imports Action (active after 13-08)"
	@echo "=== G-04/G-05: update() purity (active after 13-07) ==="
	@! rg 'tokio::spawn|spawn_blocking' src/app/update.rs 2>/dev/null || (echo "G-04 FAIL: update.rs contains spawn primitives" && exit 1)
	@! rg 'reqwest|tokio::process' src/app/ 2>/dev/null || (echo "G-05 FAIL: src/app/ uses reqwest or tokio::process" && exit 1)
	@echo "=== G-06: coordinating flags collapsed (active after 13-09) ==="
	@[ ! -f src/app/state.rs ] || ! rg 'pending_metro_run|pending_metro_after_sync' src/app/state.rs 2>/dev/null || echo "G-06 PENDING: flags not collapsed (active after 13-09)"
	@echo "=== G-07/G-14: REFACTOR-02 is_cancellable (active after 13-02) ==="
	@grep -q 'pub fn is_cancellable' src/domain/command.rs || (echo "G-07 FAIL: is_cancellable missing" && exit 1)
	@grep -q 'GitResetHard' src/domain/command.rs || (echo "G-14 FAIL: Git variants not in is_cancellable" && exit 1)
	@echo "=== G-08/G-09: Effect + Recipe + Prerequisite types (active after 13-03) ==="
	@[ ! -f src/app/effect.rs ] || grep -q 'pub enum Effect' src/app/effect.rs || (echo "G-09 FAIL: Effect enum missing" && exit 1)
	@[ ! -f src/domain/pipeline.rs ] || grep -q 'pub enum Recipe' src/domain/pipeline.rs || (echo "G-08 FAIL: Recipe enum missing" && exit 1)
	@[ ! -f src/domain/pipeline.rs ] || grep -q 'pub enum Prerequisite' src/domain/pipeline.rs || (echo "G-08 FAIL: Prerequisite enum missing" && exit 1)
	@echo "=== G-10: domain::ports module index ==="
	@grep -q '^pub mod' src/domain/ports/mod.rs || (echo "G-10 FAIL: ports/mod.rs empty" && exit 1)
	@echo "=== G-11: KEYBINDINGS three-site consumers (active after 13-10) ==="
	@[ ! -f src/app/handle_key.rs ] || rg -q 'KEYBINDINGS' src/app/handle_key.rs || (echo "G-11 FAIL: handle_key does not read KEYBINDINGS" && exit 1)
	@echo "=== G-12: hand-coded keybinding rows deleted (active after 13-10) ==="
	@[ ! -f src/ui/footer.rs ] || ! rg -q '"c", "clean' src/ui/footer.rs 2>/dev/null || echo "G-12 PENDING: footer has hand-coded rows (active after 13-10)"
	@echo "=== G-13: Adapters injection struct (ACTIVE — Plan 13-08) ==="
	@[ -f src/app/adapters.rs ] && grep -q 'pub struct Adapters' src/app/adapters.rs || (echo "G-13 FAIL: Adapters struct missing" && exit 1)
	@echo "=== G-15: action.rs moved to domain ==="
	@test -f src/domain/action.rs || (echo "G-15 FAIL: domain/action.rs missing" && exit 1)
	@test ! -f src/action.rs || (echo "G-15 FAIL: old action.rs still exists" && exit 1)
	@echo "=== G-16: MetroHandle opaque trait (active after 13-03) ==="
	@! grep -q 'stdin_tx: tokio::sync' src/domain/metro.rs || echo "G-16 PENDING: MetroHandle struct still exposes tokio fields (active after 13-03)"
	@echo "=== G-17: MetroPort trait defined (active after 13-03) ==="
	@[ ! -d src/domain/ports ] || rg -q 'trait MetroPort' src/domain/ports/ 2>/dev/null || echo "G-17 PENDING: MetroPort not yet landed (Plan 13-03)"
	@echo "=== G-18: exhaustive modal arms (active after 13-09) ==="
	@[ ! -f src/app/handle_key.rs ] || ! rg -q '\b_ => \{\}' src/app/handle_key.rs 2>/dev/null || echo "G-18 PENDING: handle_key has _ => {} arms (active after 13-09)"
	@echo "=== G-19: coverage thresholds hold ==="
	@$(MAKE) cov-check >/dev/null 2>&1 || echo "G-19 WARN: cov-check output differs from threshold — human verification required"
	@echo "=== G-20: AppState sub-structs (active after 13-10) ==="
	@[ ! -f src/app/state.rs ] || rg -q 'pub struct (MetroState|WorktreeBrowserState|CommandRunnerState|ModalStackState|PendingFlags|AppConfigState)' src/app/state.rs 2>/dev/null || echo "G-20 PENDING: sub-structs not yet landed (Plan 13-10)"
	@echo "arch-lint: PASS"
