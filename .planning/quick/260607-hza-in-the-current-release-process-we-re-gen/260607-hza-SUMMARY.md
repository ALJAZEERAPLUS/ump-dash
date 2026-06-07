---
quick_id: 260607-hza
status: complete
completed: 2026-06-07
commit: c6ca701
---

# Quick Task 260607-hza Summary

## Goal

Use the curated changelog section as the GitHub Release description.

## Completed

- Added `scripts/extract-release-notes.sh` to extract a single `CHANGELOG.md`
  version section and fail when the section is missing or empty.
- Updated `scripts/release.sh --finalize` to validate the exact changelog body
  that GitHub Actions will publish.
- Updated `.github/workflows/release.yml` to check out the tagged source,
  extract release notes from `CHANGELOG.md`, and pass them to
  `softprops/action-gh-release` via `body_path`.
- Updated `RELEASING.md` to document that the GitHub Release body comes from
  the matching changelog section.

## Verification

- `bash -n scripts/extract-release-notes.sh`
- `bash -n scripts/release.sh`
- `scripts/extract-release-notes.sh 1.3.0 CHANGELOG.md`
- `scripts/extract-release-notes.sh v1.3.0 CHANGELOG.md`
- `scripts/extract-release-notes.sh v9.9.9 CHANGELOG.md` fails as expected
  with a missing-section error.
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml")'`
- `git diff --check`

## Commit

Implementation commit: `c6ca701`
