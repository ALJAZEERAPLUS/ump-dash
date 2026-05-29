#!/usr/bin/env bash
# Release pipeline — see RELEASING.md for the full workflow.
#
# Usage:
#   scripts/release.sh <version>              # prepare: dump commits, stop for changelog
#   scripts/release.sh <version> --finalize   # bump Cargo.toml, commit, tag, push
#
# <version> is either an explicit semver (1.2.0) or patch|minor|major.

set -euo pipefail

die() { echo "error: $*" >&2; exit 1; }

[[ $# -ge 1 ]] || die "usage: $0 <version|patch|minor|major> [--finalize]"

SPEC="$1"
FINALIZE="${2:-}"

# Must be on main with a clean tree.
BRANCH=$(git rev-parse --abbrev-ref HEAD)
[[ "$BRANCH" == "main" ]] || die "must be on main (currently on $BRANCH)"
[[ -z "$(git status --porcelain)" ]] || die "working tree is dirty — commit or stash first"

# Ensure local main is up to date with origin.
git fetch --tags origin main --quiet
LOCAL=$(git rev-parse main)
REMOTE=$(git rev-parse origin/main)
[[ "$LOCAL" == "$REMOTE" ]] || die "main is not in sync with origin/main"

# Resolve version.
LAST_TAG=$(git describe --tags --abbrev=0 --match='v*' 2>/dev/null || echo "v0.0.0")
LAST_VER="${LAST_TAG#v}"
# Pad 1.0 → 1.0.0 for semver math.
IFS='.' read -r MAJ MIN PAT <<<"$LAST_VER"
MAJ=${MAJ:-0}; MIN=${MIN:-0}; PAT=${PAT:-0}

case "$SPEC" in
  patch) NEW_VER="$MAJ.$MIN.$((PAT+1))" ;;
  minor) NEW_VER="$MAJ.$((MIN+1)).0" ;;
  major) NEW_VER="$((MAJ+1)).0.0" ;;
  [0-9]*.[0-9]*.[0-9]*) NEW_VER="$SPEC" ;;
  *) die "bad version spec: $SPEC (expected semver or patch|minor|major)" ;;
esac

NEW_TAG="v$NEW_VER"
git rev-parse "$NEW_TAG" >/dev/null 2>&1 && die "tag $NEW_TAG already exists"

if [[ "$FINALIZE" == "--finalize" ]]; then
  # Changelog must have the new section.
  grep -q "^## \[$NEW_VER\]" CHANGELOG.md \
    || die "CHANGELOG.md is missing '## [$NEW_VER]' section — write release notes first"

  # Bump Cargo.toml (only the [package] version line — first 'version =' occurrence).
  awk -v v="$NEW_VER" '
    !done && /^version = / { sub(/"[^"]*"/, "\"" v "\""); done=1 }
    { print }
  ' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml

  # Update Cargo.lock.
  cargo check --quiet

  git add CHANGELOG.md Cargo.toml Cargo.lock
  git commit -m "chore(release): $NEW_TAG"
  git tag -a "$NEW_TAG" -m "Release $NEW_TAG"
  git push origin main
  git push origin "$NEW_TAG"

  echo ""
  echo "✓ Pushed $NEW_TAG — GitHub Actions will build and publish the release."
  echo "  https://github.com/ALJAZEERAPLUS/ump-dash/actions"
  exit 0
fi

# Prepare phase: dump commits for the agent to turn into changelog notes.
DRAFT=".release-notes-draft.txt"
{
  echo "Release: $NEW_TAG"
  echo "Previous: $LAST_TAG"
  echo "Range: $LAST_TAG..HEAD"
  echo ""
  echo "Commits:"
  git log "$LAST_TAG..HEAD" --pretty=format:"- %s"
  echo ""
} > "$DRAFT"

cat "$DRAFT"
echo ""
echo "─────────────────────────────────────────────────────────────"
echo "Next steps (see RELEASING.md for the prompt):"
echo "  1. Write '## [$NEW_VER] - $(date +%Y-%m-%d)' section at the top of CHANGELOG.md"
echo "  2. Review with the user"
echo "  3. Run: scripts/release.sh $NEW_VER --finalize"
echo "─────────────────────────────────────────────────────────────"
