# Release Artifact Compatibility

Phase 24 tracks release archive evidence separately from real Bambu Studio compatibility evidence. CI release-smoke checks can prove archive shape, checksum validity, CLI startup where executable on the runner, and packaged plugin exports. Real host installation evidence must be recorded separately after installing the archive on the target OS family.

## Status Values

| Status | Meaning |
| --- | --- |
| `passed` | Verified in the named environment with evidence captured. |
| `failed` | Attempted and failed; reproduction notes are recorded. |
| `blocked` | Could not complete because of a documented environment or dependency blocker. |
| `unsupported` | Intentionally unsupported by Pandar. |
| `untested` | No evidence has been recorded. |

## Release Artifact Evidence

| Run/Tag | Target | Archive Filename | Checksum | Layout | CLI Startup | Plugin Exports | Real Host Install | Signing | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| blocked | `linux-amd64` | `pandar-release-<tag-or-sanitized-ref>-linux-amd64.tar.gz` | `blocked` | `blocked` | `blocked` | `blocked` | `blocked` | `unsupported` | No release artifact exists for this target yet; create a tag/release artifact before validation. |
| blocked | `linux-arm64` | `pandar-release-<tag-or-sanitized-ref>-linux-arm64.tar.gz` | `blocked` | `blocked` | `blocked` | `blocked` | `blocked` | `unsupported` | No release artifact exists for this target yet; create a tag/release artifact before validation. |
| blocked | `windows-amd64` | `pandar-release-<tag-or-sanitized-ref>-windows-amd64.tar.gz` | `blocked` | `blocked` | `blocked` | `blocked` | `blocked` | `unsupported` | No release artifact exists for this target yet; create a tag/release artifact before validation. |
| blocked | `windows-arm64` | `pandar-release-<tag-or-sanitized-ref>-windows-arm64.tar.gz` | `blocked` | `blocked` | `blocked` | `blocked` | `blocked` | `unsupported` | No release artifact exists for this target yet; create a tag/release artifact before validation. |
| blocked | `macos-amd64` | `pandar-release-<tag-or-sanitized-ref>-macos-amd64.tar.gz` | `blocked` | `blocked` | `blocked` | `blocked` | `blocked` | `unsupported` | No release artifact exists for this target yet; create a tag/release artifact before validation. |
| blocked | `macos-arm64` | `pandar-release-<tag-or-sanitized-ref>-macos-arm64.tar.gz` | `blocked` | `blocked` | `blocked` | `blocked` | `blocked` | `unsupported` | No release artifact exists for this target yet; create a tag/release artifact before validation. |

## Release Availability Check

| Date | Source | Command | Result | Evidence |
| --- | --- | --- | --- | --- |
| 2026-06-24 | GitHub Releases, release workflow runs, git tags, local archive search | `gh release list --limit 20`; `gh run list --workflow release.yml --limit 20 --json databaseId,headBranch,headSha,event,status,conclusion,createdAt,updatedAt,url`; `git tag --sort=-creatordate | head -20`; `find dist target -name 'pandar-release-*.tar.gz' -o -name 'pandar-release-*.tar.gz.sha256'` | `blocked` | No GitHub releases, no release workflow runs, no local tags, and no local `pandar-release-*.tar.gz` archives were found. Real artifact validation cannot run until a release artifact exists. |

## Local Release-Smoke Coverage

| Date | Commit | Command | Coverage | Result | Notes |
| --- | --- | --- | --- | --- | --- |
| 2026-06-24 | post-Phase 28 smoke-sync commit | `cargo test --manifest-path tools/release-smoke/Cargo.toml` | checksum sidecar parsing, archive path normalization, exact top-level layout checks, target/runner support routing, and exported-symbol parsing | `passed` | 17 tests passed. This proves the local release-smoke checker behavior, not any real release archive install. |

## Evidence Rules

- Use only `passed`, `failed`, `blocked`, `unsupported`, or `untested` in status columns.
- Signing is `unsupported` while Phase 24 uses the `unsigned-accepted` decision.
- Record one row per release run or tag and target label when evidence exists.
- Keep CI release-smoke evidence distinct from real host installation evidence.
- Do not mark real host installation `passed` from CI-only release-smoke output.
- Keep failed and blocked rows because they are release compatibility evidence.
