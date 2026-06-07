# Self-Update Command Design

Date: 2026-06-07

## Context

`ump-dash` is an internal Rust CLI distributed either as a GitHub Release binary
or by cloning the repository and building locally. There are no package-manager
installs to support. The release workflow publishes platform-specific tarballs
to an internal GitHub repository, and GitHub Release descriptions are populated
from curated `CHANGELOG.md` sections.

Users authenticate to internal GitHub repositories through normal company
tooling. For release asset downloads, the updater will require the GitHub CLI
(`gh`) to be installed and authenticated. The updater will not handle GitHub
tokens directly.

## User-Facing Behavior

Add a pre-TUI command:

```text
ump-dash update
```

The same command must work if the user renamed the executable, because the
replacement target is `std::env::current_exe()`, not a hard-coded filename.

Command behavior:

- Require `gh` to be installed and authenticated.
- Query releases for `ALJAZEERAPLUS/ump-dash`.
- Ignore drafts and prereleases.
- Compare release tags against the compiled `CARGO_PKG_VERSION`.
- If no newer release exists, print that the installed version is current and
  exit `0`.
- If newer releases exist, print release notes for every release newer than the
  installed version, oldest to newest.
- Download and install the newest applicable release.
- Refuse to update when running from a source checkout build path such as
  `target/debug/ump-dash` or `target/release/ump-dash`; tell the user to update
  the checkout with `git pull && cargo build --release`.
- Refuse to update if the current executable's parent directory is not writable.

Initial CLI dispatch should stay small:

- no args: start the TUI as today
- `update`: run the updater and exit
- `--version` / `-V`: print the compiled version and exit
- unknown args: print usage and exit nonzero

## Changelog Display

GitHub Release bodies are the updater's changelog source of truth.

The updater will:

1. Parse the installed version from `CARGO_PKG_VERSION`.
2. List non-draft, non-prerelease GitHub Releases through `gh`.
3. Select all releases with a semantic version greater than the installed
   version.
4. Sort selected releases from oldest to newest.
5. Print each version header and release body before downloading anything.
6. Use the newest selected release as the update target.

If a newer release has an empty body, the updater should still show the version
header and print `No release notes provided.`. Missing notes should not block an
otherwise valid update.

Example output shape:

```text
Installed: 1.3.0
Latest:    1.5.0

Changes:

## 1.4.0
...release body...

## 1.5.0
...release body...

Updating to 1.5.0...
```

## Release Assets

The updater maps the current platform to one exact asset name:

| Platform | Asset |
| --- | --- |
| macOS Apple Silicon | `ump-dash-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `ump-dash-x86_64-apple-darwin.tar.gz` |
| Linux x64 | `ump-dash-x86_64-unknown-linux-gnu.tar.gz` |

Unsupported platforms fail before download with a clear message.

The release workflow should also publish a `SHA256SUMS` asset containing hashes
for the platform tarballs. The updater must verify the downloaded tarball
against `SHA256SUMS` before extraction.

## Download And Verification

The updater delegates private GitHub authentication and download mechanics to
`gh` subprocesses.

Expected commands are conceptually:

```text
gh release list --repo ALJAZEERAPLUS/ump-dash --json ...
gh release view <tag> --repo ALJAZEERAPLUS/ump-dash --json ...
gh release download <tag> --repo ALJAZEERAPLUS/ump-dash --pattern <asset> --dir <tempdir>
gh release download <tag> --repo ALJAZEERAPLUS/ump-dash --pattern SHA256SUMS --dir <tempdir>
```

The implementation can choose the exact `gh` JSON fields, but it must not parse
human-formatted `gh` output for release selection.

Verification requirements:

- Confirm the downloaded tarball filename exactly matches the expected asset.
- Parse `SHA256SUMS` and find exactly one checksum row for the expected asset.
- Compute SHA-256 for the downloaded tarball and compare it to the checksum.
- Reject checksum mismatches, missing checksum rows, or duplicate checksum rows.
- Inspect the archive and extract only the expected `ump-dash` binary entry.
- Reject path traversal, absolute paths, symlinks, directories, unexpected file
  names, and archives that contain no valid binary entry.
- Confirm the extracted binary is non-empty.

The updater must never execute downloaded content during verification.

## Replacement Mechanics

Replacement target:

- Resolve the current executable path with `std::env::current_exe()`.
- Preserve the local filename, including user-renamed executables.
- Place temporary and backup files in the same directory as the current
  executable so filesystem renames stay on the same mount.

Replacement sequence:

1. Extract the verified new binary into a temp file next to the current
   executable.
2. Set executable permissions on Unix.
3. Rename the current executable to a backup path.
4. Rename the temp binary to the original executable path.
5. Remove the backup after the new binary is in place.

If replacement fails after the backup is created, the updater should attempt to
restore the backup to the original executable path and print exact recovery
guidance. The updater should not request elevated privileges or try to write
outside the current executable directory.

## Error Handling

Errors should be plain command-line messages, not TUI overlays.

Important cases:

- `gh` missing: tell the user to install GitHub CLI.
- `gh` unauthenticated or unauthorized: tell the user to run `gh auth login` and
  ensure access to `ALJAZEERAPLUS/ump-dash`.
- no matching release asset: show the current platform and expected asset name.
- checksum missing or mismatched: abort before extraction.
- source checkout build detected: explain source update commands.
- non-writable install directory: show the executable path and ask the user to
  move/reinstall manually.
- replacement restore failed: show backup path and original path.

## Code Structure

Keep updater code outside the TUI reducer/runtime. A reasonable structure:

- `src/main.rs`: small argument dispatch before terminal setup.
- `src/infra/self_update.rs`: `gh` calls, download, verification, extraction,
  and replacement.
- Small pure helper types/functions inside the updater module, or a separate
  module if needed, for release parsing, target mapping, and archive validation.

Dependencies may include:

- `sha2` for checksum verification.
- `tar` and `flate2` for `.tar.gz` inspection/extraction.
- A semver parser, if the implementation chooses not to write a tiny local
  parser for `vX.Y.Z` tags.

## Tests

Add focused tests for the pure and boundary-adjacent pieces:

- Platform-to-asset mapping.
- Tag/semver parsing and release range selection.
- Draft/prerelease filtering.
- Source-checkout executable detection.
- `SHA256SUMS` parsing, including missing and duplicate rows.
- Checksum verification behavior.
- Archive entry validation, including path traversal and unexpected entries.
- Replacement planning paths where practical without replacing the test binary.

CI tests must not require GitHub authentication. Actual `gh` calls and live
release downloads should stay behind boundaries that can be exercised manually
or with mocked command output.

## Documentation Updates

Update:

- `README.md`: document `ump-dash update`, the `gh` requirement, and source
  checkout behavior.
- `RELEASING.md`: document that releases publish `SHA256SUMS` and that release
  bodies come from curated changelog sections.
- Release workflow: generate and upload `SHA256SUMS` with the release assets.

## Security Notes

The design intentionally avoids direct token handling. Authentication is
delegated to `gh`, which already owns GitHub credential storage and private repo
access.

SHA-256 verification is not equivalent to detached signature verification,
because the checksum is stored beside the release assets. For this internal
tool, authenticated GitHub Releases plus explicit checksums provide a pragmatic
first layer: they catch corrupted downloads, accidental asset mismatches, and
unsafe archive contents. A future hardening step could add signed checksums if
the release threat model grows to include compromised GitHub release assets.
