# Releasing ump-dash

The release pipeline is driven by `scripts/release.sh` and a changelog entry
written by the agent. GitHub Actions handles the build + publish once a tag is
pushed, and uses that version's `CHANGELOG.md` section as the GitHub Release
description. The release job also uploads `SHA256SUMS` beside the platform
tarballs; `ump-dash update` uses that checksum file to verify downloads before
replacing a local binary.

## Agent-facing quick reference

When the user says **"release 1.2"** or **"release the next version"**:

1. **Prepare.** Run one of:
   ```
   scripts/release.sh 1.2.0       # explicit version
   scripts/release.sh patch       # bump last tag's patch (1.1.0 → 1.1.1)
   scripts/release.sh minor       # bump minor (1.1.0 → 1.2.0)
   scripts/release.sh major       # bump major (1.1.0 → 2.0.0)
   ```
   This validates the tree, prints the commit list since the last tag, and
   writes `.release-notes-draft.txt` for reference. Nothing is mutated yet.

2. **Write the changelog entry.** Prepend a new section to `CHANGELOG.md`
   using the style below. Use the prompt in the next section to synthesize it.

3. **Show the user the draft.** Let them edit or ask for revisions.

4. **Finalize.** Run:
   ```
   scripts/release.sh 1.2.0 --finalize
   ```
   This verifies the changelog section can be extracted, bumps `Cargo.toml`,
   updates `Cargo.lock`, commits as `chore(release): v1.2.0`, tags `v1.2.0`,
   and pushes both. GitHub Actions takes it from there.

5. **GitHub Release body.** The release workflow extracts the matching
   `## [X.Y.Z]` section from `CHANGELOG.md` and passes it to
   `softprops/action-gh-release` as the release description. If the section is
   missing or empty, the release job fails instead of publishing generated
   notes. The same workflow generates `SHA256SUMS` for every `ump-dash-*.tar.gz`
   asset and uploads it to the release for the self-updater.

## Changelog prompt (style guide)

When writing a new `## [X.Y.Z]` section, apply these rules:

- **Group by user-facing theme**, not by commit type. A single theme may
  cover multiple commits; a single commit may belong to no theme if it's not
  user-visible.
- **Section headings**: `### New`, `### Fixed`, `### Improved`, `### Changed`,
  `### Removed`. Only include sections that have content.
- **Merge related commits into one bullet.** Three commits that together
  deliver one feature = one bullet.
- **Skip internal-only work**: refactors, test additions, docs, CI tweaks,
  dependency bumps — unless they directly change user experience.
- **Only claim what actually shipped and works.** If a commit adds
  infrastructure but the feature isn't verified end-to-end (e.g., codesigning
  workflow exists but no signed binary has shipped yet, feature is behind a
  flag that's still off), either omit it or describe conservatively.
- **Plain language.** "Metro process is now killed cleanly on stop" beats
  "Fix SIGTERM propagation in MetroRunner::kill()".
- **One line per bullet.** Scan-friendly. Parenthetical detail is fine if
  the fix is surprising enough to matter.
- **No commit hashes, no PR links, no scope tags** (`(quick-260410-mu7)`).

### Example

```markdown
## [1.2.0] - 2026-04-12

### New
- **Stale-dependency guard** — pressing Enter on Metro now checks for stale
  yarn/pods and prompts to sync before launch. Bypass with `auto_sync = true`
  in config.

### Fixed
- Metro process group is killed cleanly on stop (previously only yarn's PID
  was killed, leaving orphan node processes).
- iOS device picker correctly lists simulators when pressing `i>e`.
- Race condition when switching worktrees during an in-flight operation.
```

## Manual verification before `--finalize`

Glance at the draft and check:

- Does `Cargo.toml`'s current version look sane relative to the new tag?
  (Historically it's drifted from tags; the script will overwrite it either
  way, but if you see `0.1.0` when the last tag was `1.1.0`, that's the
  reason.)
- Anything in the commit list that claims a user-visible feature but isn't
  actually wired up / configured? Drop it or describe conservatively.
- Anything marked as fixed that you haven't actually verified works? Same.

## Hotfix / re-tag

The script refuses to re-use an existing tag. If you need to redo a release,
delete the tag locally and remotely first (`git tag -d v1.2.0` +
`git push --delete origin v1.2.0`), then re-run. Don't do this for already-
published releases — cut a new patch version instead.
