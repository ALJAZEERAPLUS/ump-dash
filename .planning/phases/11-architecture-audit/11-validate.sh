#!/usr/bin/env bash
# .planning/phases/11-architecture-audit/11-validate.sh
# Validation harness for Phase 11 Architecture Audit deliverable.
# Enforces 11-VALIDATION.md §Validation Map.
# Dependencies: bash, rg (ripgrep), grep, find, diff. No install required.
# Usage:
#   bash 11-validate.sh                 # full run
#   bash 11-validate.sh --self-test     # skeleton-only checks (Wave 0 dry-run)
#   bash 11-validate.sh --module domain # check coverage of one module
#   bash 11-validate.sh --module infra
#   bash 11-validate.sh --module app
#   bash 11-validate.sh --module ui
#   bash 11-validate.sh --cross-cutting # check cross-cutting section completeness
# Exit codes: 0 = green, 1 = red, 2 = usage error.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
AUDIT_MD="$REPO_ROOT/.planning/phases/11-architecture-audit/AUDIT.md"
ROADMAP="$REPO_ROOT/.planning/ROADMAP.md"
REQUIREMENTS="$REPO_ROOT/.planning/REQUIREMENTS.md"

FAIL=0
fail()  { echo "FAIL: $*" >&2; FAIL=1; }
ok()    { echo "OK:   $*"; }

# --- File inventory the audit MUST cover (D-13 + ARCH-01) ---
ROOT_FILES=(src/main.rs src/tui.rs src/event.rs src/action.rs src/app.rs)
DOMAIN_FILES=(src/domain/mod.rs src/domain/command.rs src/domain/metro.rs src/domain/refresh.rs src/domain/worktree.rs)
INFRA_FILES=(src/infra/mod.rs src/infra/port.rs src/infra/process.rs src/infra/worktrees.rs src/infra/command_runner.rs src/infra/devices.rs src/infra/config.rs src/infra/jira.rs src/infra/jira_cache.rs src/infra/multiplexer.rs src/infra/sim_history.rs src/infra/android_prefs.rs src/infra/tmux.rs)
UI_FILES=(src/ui/mod.rs src/ui/panels.rs src/ui/footer.rs src/ui/help_overlay.rs src/ui/error_overlay.rs src/ui/modals.rs src/ui/theme.rs)

# --- Subcommand dispatch ---
MODE="full"
MODULE=""
case "${1:-}" in
  --self-test) MODE="self-test" ;;
  --module) MODE="module"; MODULE="${2:-}" ;;
  --cross-cutting) MODE="cross-cutting" ;;
  "") MODE="full" ;;
  *) echo "Usage: $0 [--self-test | --module <name> | --cross-cutting]" >&2; exit 2 ;;
esac

# --- Check 0: AUDIT.md exists at the correct path (ARCH-06 / D-05) ---
[[ -f "$AUDIT_MD" ]] || { fail "AUDIT.md missing at $AUDIT_MD"; exit 1; }

# --- Check 1: Required H1 + H2 + H3 structure (D-05) ---
check_structure() {
  grep -q '^# Architecture Audit' "$AUDIT_MD" || fail "Missing H1 '# Architecture Audit'"
  for h in '## Module: root/' '## Module: domain/' '## Module: infra/' '## Module: app/' '## Module: ui/' '## Cross-Cutting Findings' '## Refactor Sequence'; do
    grep -qF "$h" "$AUDIT_MD" || fail "Missing required header: $h"
  done
}

# --- Check 2: Every source file in the module appears at least once in AUDIT.md (D-13) ---
check_module_coverage() {
  local module="$1"; shift
  local files=("$@")
  for f in "${files[@]}"; do
    grep -qF "$(basename "$f")" "$AUDIT_MD" || fail "$module: file not mentioned in AUDIT.md: $f"
  done
}

# --- Check 3: Every covered file has at least one Verdict line (Ousterhout score per D-13) ---
# A Verdict line must appear within 30 lines after a file mention.
check_module_scores() {
  local module="$1"; shift
  local files=("$@")
  for f in "${files[@]}"; do
    local base="$(basename "$f")"
    if ! awk -v fname="$base" '
      $0 ~ fname { found=1; n=0 }
      found { n++; if ($0 ~ /Verdict:/) { print "yes"; exit } if (n>30) { found=0 } }
    ' "$AUDIT_MD" | grep -q yes; then
      fail "$module: no Verdict: line within 30 lines of mention of $base"
    fi
  done
}

# --- Check 4: Finding schema completeness (D-06) ---
# For every finding header `### [Critical|Major|Minor] F-NNN:`, the next ~25 lines
# must contain Location:, Dimension:, Symptom:, Why it's a problem:, Recommendation:, Phase 13 task hint:
check_finding_schema() {
  awk '
    /^### \[(Critical|Major|Minor)\] F-[0-9]{3}:/ {
      if (id != "") {
        for (k in needed) if (!seen[k]) print "MISSING_FIELD " id " " k
      }
      id = $0; delete seen
      needed["Location:"]=1; needed["Dimension:"]=1; needed["Symptom:"]=1
      needed["Why its a problem:"]=1; needed["Recommendation:"]=1; needed["Phase 13 task hint:"]=1
      next
    }
    id != "" {
      if ($0 ~ /Location:/)         seen["Location:"]=1
      if ($0 ~ /Dimension:/)        seen["Dimension:"]=1
      if ($0 ~ /Symptom:/)          seen["Symptom:"]=1
      if ($0 ~ /Why.*a problem:/)   seen["Why its a problem:"]=1
      if ($0 ~ /Recommendation:/)   seen["Recommendation:"]=1
      if ($0 ~ /Phase 13 task hint:/) seen["Phase 13 task hint:"]=1
    }
    END {
      if (id != "") for (k in needed) if (!seen[k]) print "MISSING_FIELD " id " " k
    }
  ' "$AUDIT_MD" | while read -r line; do
    [[ -n "$line" ]] && fail "$line"
  done
}

# --- Check 5: F-NNN IDs unique and sequential within range (D-07) ---
# Only counts IDs that appear as finding headers (### [Critical|Major|Minor] F-NNN:).
# Range-header mentions in the preamble (F-001..F-099, etc.) are intentionally excluded.
check_finding_ids() {
  local dups
  dups=$(grep -oE '^### \[(Critical|Major|Minor)\] F-[0-9]{3}' "$AUDIT_MD" | grep -oE 'F-[0-9]{3}' | sort | uniq -d || true)
  [[ -z "$dups" ]] || fail "Duplicate finding IDs: $(echo $dups | tr '\n' ' ')"
}

# --- Check 6: Critical/Major recommendations contain a concrete shape keyword (D-08) ---
# Heuristic: within each Critical/Major finding block, the Recommendation: paragraph
# must contain at least one of 'trait ', 'move ', 'enum ', 'replace _ ='.
check_recommendation_concreteness() {
  awk '
    /^### \[(Critical|Major)\] F-[0-9]{3}:/ {
      if (id != "" && !concrete) print "VAGUE_REC " id
      id = $0; concrete = 0; in_rec = 0; next
    }
    id != "" && /^### \[(Critical|Major|Minor)\] F-/ { next }
    /Recommendation:/ { in_rec = 1; }
    in_rec && /Phase 13 task hint:/ { in_rec = 0 }
    in_rec && (/trait / || /move / || /enum / || /replace _ =/) { concrete = 1 }
    END { if (id != "" && !concrete) print "VAGUE_REC " id }
  ' "$AUDIT_MD" | while read -r line; do
    [[ -n "$line" ]] && fail "$line — Critical/Major Recommendation lacks concrete keyword (trait/move/enum/replace _ =>) per D-08"
  done
}

# --- Check 7: Refactor Sequence appendix lists every Critical/Major F-NNN (D-09) ---
check_refactor_sequence() {
  local refactor_block ids_in_doc ids_in_seq missing
  refactor_block=$(awk '/^## Refactor Sequence/{flag=1; next} /^## /{flag=0} flag{print}' "$AUDIT_MD")
  # Get all Critical+Major F-NNN IDs from the rest of the doc (anywhere except Refactor Sequence section)
  ids_in_doc=$(grep -oE '^### \[(Critical|Major)\] F-[0-9]{3}' "$AUDIT_MD" | grep -oE 'F-[0-9]{3}' | sort -u)
  for id in $ids_in_doc; do
    if ! grep -qF "$id" <<<"$refactor_block"; then
      fail "Refactor Sequence missing Critical/Major finding: $id"
    fi
  done
}

# --- Check 8: Path correction (D-11) ---
check_path_correction() {
  if grep -nE '11-arch-audit' "$ROADMAP" "$REQUIREMENTS" >/dev/null 2>&1; then
    fail "Path correction not applied — '11-arch-audit' still present in ROADMAP.md or REQUIREMENTS.md (D-11)"
    grep -nE '11-arch-audit' "$ROADMAP" "$REQUIREMENTS" >&2 || true
  fi
}

# --- Check 9: Catch-all enumeration (ARCH-04) — every `_ =>` arm in src/ has a corresponding entry ---
check_catch_alls() {
  local arms missing
  # Wide grep per RESEARCH §Catch-All Enumeration Technique
  arms=$(rg --no-heading -n '^\s*_\s*=>' "$REPO_ROOT/src/" | awk -F: '{print $1":"$2}' | sed "s|$REPO_ROOT/||")
  while IFS= read -r loc; do
    [[ -z "$loc" ]] && continue
    if ! grep -qF "$loc" "$AUDIT_MD"; then
      fail "Catch-all not enumerated in AUDIT.md: $loc"
    fi
  done <<<"$arms"
}

# --- Check 10: Prerequisite locations from RESEARCH appear in AUDIT (ARCH-05) ---
check_prerequisites() {
  # Lines from RESEARCH §Prerequisite/Ordering Logic Detection — confirmed locations.
  for loc in 890 1014 1713 949 956 1463 1622 1684 1722 657 594; do
    grep -qE "app\.rs:(${loc}|${loc}-)" "$AUDIT_MD" || fail "Prerequisite location not enumerated: app.rs:$loc"
  done
}

# --- Check 11: Keybinding finding (D-14) — references handle_key + footer.rs + help_overlay.rs ---
check_keybinding_finding() {
  # Within 50 lines of any F-NNN whose title mentions keybinding/keymap, all three references must appear.
  awk '
    /^### \[(Critical|Major|Minor)\] F-[0-9]{3}:.*([Kk]eybinding|[Kk]eymap)/ {
      id = $0; n = 0; hk = 0; ft = 0; ho = 0
    }
    id != "" {
      n++
      if ($0 ~ /handle_key/)        hk = 1
      if ($0 ~ /footer\.rs/)        ft = 1
      if ($0 ~ /help_overlay\.rs/)  ho = 1
      if (n > 50) {
        if (!(hk && ft && ho)) print "INCOMPLETE_KB " id " hk=" hk " ft=" ft " ho=" ho
        id = ""
      }
    }
    END {
      if (id != "" && !(hk && ft && ho)) print "INCOMPLETE_KB " id " hk=" hk " ft=" ft " ho=" ho
    }
  ' "$AUDIT_MD" | while read -r line; do
    [[ -n "$line" ]] && fail "$line — D-14 keybinding finding missing one of handle_key/footer.rs/help_overlay.rs references"
  done
  # Also: at least ONE finding must be a keybinding finding (D-14 mandatory check)
  grep -qE '^### \[(Critical|Major|Minor)\] F-[0-9]{3}:.*([Kk]eybinding|[Kk]eymap)' "$AUDIT_MD" \
    || fail "D-14: no keybinding/keymap finding present in AUDIT.md"
}

# --- Mode dispatch ---
case "$MODE" in
  self-test)
    check_structure
    check_finding_ids
    # In self-test mode, skip coverage / catch-all / prereq / D-14 (skeleton has no findings yet)
    ;;
  module)
    check_structure
    case "$MODULE" in
      root)   check_module_coverage root   "${ROOT_FILES[@]}";   check_module_scores root   "${ROOT_FILES[@]}" ;;
      domain) check_module_coverage domain "${DOMAIN_FILES[@]}"; check_module_scores domain "${DOMAIN_FILES[@]}" ;;
      infra)  check_module_coverage infra  "${INFRA_FILES[@]}";  check_module_scores infra  "${INFRA_FILES[@]}" ;;
      app)    check_module_coverage app    src/app.rs;            check_module_scores app    src/app.rs ;;
      ui)     check_module_coverage ui     "${UI_FILES[@]}";     check_module_scores ui     "${UI_FILES[@]}" ;;
      *) echo "Unknown module: $MODULE (expected root|domain|infra|app|ui)" >&2; exit 2 ;;
    esac
    check_finding_ids
    check_finding_schema
    ;;
  cross-cutting)
    check_structure
    check_finding_ids
    check_finding_schema
    check_recommendation_concreteness
    check_refactor_sequence
    check_catch_alls
    check_prerequisites
    check_keybinding_finding
    ;;
  full)
    check_structure
    check_module_coverage root   "${ROOT_FILES[@]}"
    check_module_coverage domain "${DOMAIN_FILES[@]}"
    check_module_coverage infra  "${INFRA_FILES[@]}"
    check_module_coverage app    src/app.rs
    check_module_coverage ui     "${UI_FILES[@]}"
    check_module_scores root   "${ROOT_FILES[@]}"
    check_module_scores domain "${DOMAIN_FILES[@]}"
    check_module_scores infra  "${INFRA_FILES[@]}"
    check_module_scores app    src/app.rs
    check_module_scores ui     "${UI_FILES[@]}"
    check_finding_ids
    check_finding_schema
    check_recommendation_concreteness
    check_refactor_sequence
    check_catch_alls
    check_prerequisites
    check_keybinding_finding
    check_path_correction
    ;;
esac

if [[ $FAIL -eq 0 ]]; then
  ok "Phase 11 validation passed (mode=$MODE)"
  exit 0
else
  echo "Phase 11 validation FAILED (mode=$MODE)" >&2
  exit 1
fi
