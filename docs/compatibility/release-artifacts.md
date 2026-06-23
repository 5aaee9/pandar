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
| untested | `linux-amd64` | `pandar-release-<tag-or-sanitized-ref>-linux-amd64.tar.gz` | `untested` | `untested` | `untested` | `untested` | `untested` | `unsupported` | Unsigned artifacts accepted for Phase 24; no real host install evidence recorded. |
| untested | `linux-arm64` | `pandar-release-<tag-or-sanitized-ref>-linux-arm64.tar.gz` | `untested` | `untested` | `untested` | `untested` | `untested` | `unsupported` | Unsigned artifacts accepted for Phase 24; no real host install evidence recorded. |
| untested | `windows-amd64` | `pandar-release-<tag-or-sanitized-ref>-windows-amd64.tar.gz` | `untested` | `untested` | `untested` | `untested` | `untested` | `unsupported` | Unsigned artifacts accepted for Phase 24; expect platform warnings; no real host install evidence recorded. |
| untested | `windows-arm64` | `pandar-release-<tag-or-sanitized-ref>-windows-arm64.tar.gz` | `untested` | `untested` | `untested` | `untested` | `untested` | `unsupported` | Unsigned artifacts accepted for Phase 24; expect platform warnings; no real host install evidence recorded. |
| untested | `macos-amd64` | `pandar-release-<tag-or-sanitized-ref>-macos-amd64.tar.gz` | `untested` | `untested` | `untested` | `untested` | `untested` | `unsupported` | Unsigned artifacts accepted for Phase 24; expect Gatekeeper warnings; no real host install evidence recorded. |
| untested | `macos-arm64` | `pandar-release-<tag-or-sanitized-ref>-macos-arm64.tar.gz` | `untested` | `untested` | `untested` | `untested` | `untested` | `unsupported` | Unsigned artifacts accepted for Phase 24; expect Gatekeeper warnings; no real host install evidence recorded. |

## Evidence Rules

- Use only `passed`, `failed`, `blocked`, `unsupported`, or `untested` in status columns.
- Signing is `unsupported` while Phase 24 uses the `unsigned-accepted` decision.
- Record one row per release run or tag and target label when evidence exists.
- Keep CI release-smoke evidence distinct from real host installation evidence.
- Do not mark real host installation `passed` from CI-only release-smoke output.
- Keep failed and blocked rows because they are release compatibility evidence.
