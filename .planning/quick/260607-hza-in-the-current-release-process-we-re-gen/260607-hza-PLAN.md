---
quick_id: 260607-hza
status: planned
created: 2026-06-07
---

# Quick Task 260607-hza Plan

## Goal

Use the curated `CHANGELOG.md` section as the GitHub Release description instead
of GitHub-generated release notes.

## Tasks

### 1. Extract release notes from the changelog

files: `scripts/extract-release-notes.sh`, `scripts/release.sh`

action: Add a reusable script that extracts the `## [X.Y.Z]` changelog section
for a version and fails if that section is missing or empty. Use it during
`scripts/release.sh --finalize` so local finalization catches the same problem
the release workflow would catch.

verify: Run the extractor against the current `1.3.0` changelog entry and an
absent version.

done: The extractor emits only the selected changelog body and fails loudly for
missing or empty sections.

### 2. Wire the GitHub release body to that extraction

files: `.github/workflows/release.yml`

action: Check out the tagged source in the release job, generate a temporary
release-notes file from `CHANGELOG.md`, and pass it to
`softprops/action-gh-release` through `body_path` instead of using
`generate_release_notes`.

verify: Inspect the workflow and run a local shell syntax check for the new
extractor.

done: The release job creates GitHub releases with the curated changelog body.

### 3. Document the release behavior

files: `RELEASING.md`

action: Update the release docs so the agent/user knows the changelog section is
used directly as the GitHub release description.

verify: Confirm docs match the script and workflow behavior.

done: Release docs accurately describe the CI release body source.
