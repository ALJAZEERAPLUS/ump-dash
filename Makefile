# Makefile — local-only coverage targets (D-02, no CI wiring this phase).
#
# Prerequisites (one-time, per dev machine):
#   cargo install cargo-llvm-cov --locked
#   rustup component add llvm-tools-preview
#
# CLAUDE.md: check-types always uses --incremental. This file does NOT invoke
# check-types; if a future target adds it, use `cargo check --incremental`.

.PHONY: cov cov-html cov-baseline cov-check

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
