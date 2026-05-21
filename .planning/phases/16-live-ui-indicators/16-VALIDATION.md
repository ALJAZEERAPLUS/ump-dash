---
phase: 16
slug: live-ui-indicators
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-22
---

# Phase 16 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[cfg(test)]` + `cargo test` |
| **Config file** | none — workspace `Cargo.toml` |
| **Quick run command** | `cargo test --lib ui::` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~10 seconds (incremental) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib ui::`
- **After every plan wave:** Run `cargo test`
- **Before `/gsd:verify-work`:** Full suite must be green + `make arch-lint` green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| {N}-01-01 | 01 | 1 | REQ-{XX} | T-{N}-01 / — | {expected secure behavior or "N/A"} | unit | `{command}` | ✅ / ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Pure-helper unit tests (spinner frame index at millis boundaries; elapsed format at 59s/60s/600s) — co-located `#[cfg(test)] mod tests`
- [ ] Exhaustive `task_short_label` match coverage (one assertion per `CommandSpec` variant)

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Half-circle glyph column alignment in target terminal | UI-02 | Terminal cell-width rendering not observable from unit tests | Run app in tmux + iTerm2, start a yarn/pod/jest task, confirm columns stay aligned; if broken, swap `SPINNER_FRAMES` to braille fallback |

*If none: "All phase behaviors have automated verification."*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
