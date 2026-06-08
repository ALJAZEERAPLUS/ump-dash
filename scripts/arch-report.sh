#!/usr/bin/env bash
set -u

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT" || exit 1

STATUS=0

section() {
  printf '\n## %s\n' "$1"
}

run_required() {
  local label="$1"
  shift

  section "$label"
  if "$@"; then
    printf '%s_status=pass\n' "$label"
  else
    printf '%s_status=fail\n' "$label"
    STATUS=1
  fi
}

section "ump-dash architecture report"
printf 'repo=%s\n' "$ROOT"
printf 'commit=%s\n' "$(git rev-parse --short HEAD)"
printf 'branch=%s\n' "$(git rev-parse --abbrev-ref HEAD)"

run_required "arch-lint" make arch-lint

section "cargo metadata"
METADATA_PATH="${TMPDIR:-/tmp}/ump-dash-cargo-metadata.json"
if cargo metadata --no-deps --format-version 1 > "$METADATA_PATH"; then
  printf 'cargo_metadata_status=pass\n'
  printf 'cargo_metadata_path=%s\n' "$METADATA_PATH"
  printf 'cargo_metadata_bytes=%s\n' "$(wc -c < "$METADATA_PATH" | tr -d ' ')"
else
  printf 'cargo_metadata_status=fail\n'
  STATUS=1
fi

section "test inventory"
if cargo test -- --list; then
  printf 'test_inventory_status=pass\n'
else
  printf 'test_inventory_status=fail\n'
  STATUS=1
fi

section "largest rust files"
find src tests -name '*.rs' -print0 \
  | xargs -0 wc -l \
  | sort -nr \
  | sed -n '1,25p'

section "recent rust/doc churn"
git log --since='30 days ago' --name-only --pretty=format: -- src tests docs Makefile \
  | awk 'NF { count[$0]++ } END { for (file in count) print count[file], file }' \
  | sort -nr \
  | sed -n '1,25p'

section "boundary scan: app/ui/domain importing infra"
rg -n 'crate::infra::' src/app src/ui src/domain 2>/dev/null \
  | rg -v '^[^:]+:[0-9]+:\s*//' \
  | rg -v 'src/app/effect_runner\.rs.*(jira_cache|sim_history|task_handle)' \
  || true

section "boundary scan: infra importing app Action"
rg -n 'use crate::(domain::)?action|crate::domain::action::Action' src/infra 2>/dev/null || true

section "purity scan: app side-effect APIs"
rg -n 'tokio::spawn|spawn_blocking|tokio::process|reqwest|std::process::Command|Command::new' src/app 2>/dev/null || true

section "architecture hotspots"
for file in src/app/update.rs src/app/effect_runner.rs src/app/state.rs src/app/keybindings.rs src/domain/command.rs src/infra/native_cache.rs; do
  if [ -f "$file" ]; then
    printf '%s %s lines\n' "$file" "$(wc -l < "$file" | tr -d ' ')"
  fi
done

section "report result"
if [ "$STATUS" -eq 0 ]; then
  printf 'arch_report_status=pass\n'
else
  printf 'arch_report_status=fail\n'
fi

exit "$STATUS"
