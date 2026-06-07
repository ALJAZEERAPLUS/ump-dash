#!/usr/bin/env bash
# Extract a single version section from CHANGELOG.md for GitHub Releases.

set -euo pipefail

die() { echo "error: $*" >&2; exit 1; }

[[ $# -ge 1 && $# -le 2 ]] || die "usage: $0 <version|vversion> [CHANGELOG.md]"

VERSION="${1#v}"
CHANGELOG="${2:-CHANGELOG.md}"

[[ -f "$CHANGELOG" ]] || die "changelog not found: $CHANGELOG"

set +e
awk -v version="$VERSION" '
  BEGIN {
    prefix = "## [" version "]"
    found = 0
    capture = 0
    line_count = 0
    last_content = 0
  }

  $0 == prefix || index($0, prefix " ") == 1 {
    found = 1
    capture = 1
    next
  }

  capture && /^## \[[^]]+\]/ {
    capture = 0
    exit
  }

  capture {
    lines[++line_count] = $0
    if ($0 ~ /[^[:space:]]/) {
      last_content = line_count
    }
  }

  END {
    if (!found) {
      exit 2
    }

    if (last_content == 0) {
      exit 3
    }

    first_content = 1
    while (first_content <= last_content && lines[first_content] ~ /^[[:space:]]*$/) {
      first_content++
    }

    for (i = first_content; i <= last_content; i++) {
      print lines[i]
    }
  }
' "$CHANGELOG"
status=$?
set -e

case "$status" in
  0)
    ;;
  2)
    die "$CHANGELOG is missing '## [$VERSION]' section"
    ;;
  3)
    die "$CHANGELOG has an empty '## [$VERSION]' section"
    ;;
  *)
    die "failed to parse $CHANGELOG"
    ;;
esac
