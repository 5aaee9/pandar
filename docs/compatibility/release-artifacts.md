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
| local-a79bcae | `linux-amd64` | `pandar-release-local-a79bcae-linux-amd64.tar.gz` | `passed` | `passed` | `passed` | `passed` | `untested` | `unsupported` | Local Ubuntu-style archive built from `target/release/pandar` and `target/release/libpandar_network_plugin.so` after commit `a79bcae`; `tools/release-smoke` validated checksum sidecar, exact top-level layout, `pandar --help`, and 129 packaged plugin ABI exports. This is local artifact smoke evidence, not a GitHub Release or real host install. |
| workflow_dispatch 28098334876 | `linux-amd64` | `pandar-release-main-linux-amd64.tar.gz` | `untested` | `untested` | `untested` | `untested` | `untested` | `unsupported` | GitHub Actions uploaded this artifact from head `6443b818`; local download verification is still pending because the full run failed before all targets completed. |
| workflow_dispatch 28098334876 | `linux-arm64` | `pandar-release-main-linux-arm64.tar.gz` | `untested` | `untested` | `untested` | `untested` | `untested` | `unsupported` | GitHub Actions uploaded this artifact from head `6443b818`; local download verification is still pending because the full run failed before all targets completed. |
| workflow_dispatch 28098334876 | `windows-amd64` | `pandar-release-main-windows-amd64.tar.gz` | `failed` | `failed` | `untested` | `failed` | `blocked` | `unsupported` | The Windows CLI build completed, but the plugin build failed before packaging because `crates/pandar-network-plugin/build.rs` did not find the `cc` shim object. |
| workflow_dispatch 28098334876 | `windows-arm64` | `pandar-release-main-windows-arm64.tar.gz` | `failed` | `failed` | `untested` | `failed` | `blocked` | `unsupported` | The Windows CLI build completed, but the plugin build failed before packaging because `crates/pandar-network-plugin/build.rs` did not find the `cc` shim object. |
| workflow_dispatch 28098334876 | `macos-amd64` | `pandar-release-main-macos-amd64.tar.gz` | `failed` | `failed` | `failed` | `untested` | `blocked` | `unsupported` | The macOS CLI build failed before packaging while linking through cargo-zigbuild on a native macOS runner. |
| workflow_dispatch 28098334876 | `macos-arm64` | `pandar-release-main-macos-arm64.tar.gz` | `failed` | `failed` | `failed` | `untested` | `blocked` | `unsupported` | The macOS CLI build failed before packaging while linking through cargo-zigbuild on a native macOS runner. |
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
| 2026-06-24 | Local generated artifact | `cargo build --release -p pandar-app --bin pandar`; `cargo build --release -p pandar-network-plugin`; package `target/release/pandar` + `target/release/libpandar_network_plugin.so` as `dist/pandar-release-local-a79bcae-linux-amd64.tar.gz`; run `cargo run --manifest-path tools/release-smoke/Cargo.toml -- --label linux-amd64 --runner-os linux --archive dist/pandar-release-local-a79bcae-linux-amd64.tar.gz --checksum dist/pandar-release-local-a79bcae-linux-amd64.tar.gz.sha256 --cli-name pandar --plugin-name libpandar_network_plugin.so --repo-root .` | `passed` | Local linux-amd64 release-smoke passed archive layout, CLI startup, and packaged plugin export checks. GitHub Release artifacts, cross-platform targets, and real host installation remain unverified. |
| 2026-06-24 | GitHub Actions workflow_dispatch run 28098334876 | `gh workflow run release.yml --ref main`; `gh run view 28098334876 --json url,headSha,conclusion,jobs`; `gh run view 28098334876 --job <failed-job-id> --log` | `failed` | Run URL: `https://github.com/5aaee9/pandar/actions/runs/28098334876`. Linux amd64/arm64 artifact jobs succeeded and uploaded archives, but Windows plugin jobs failed at `cc produced shim object`, and macOS CLI jobs failed while linking through cargo-zigbuild. The workflow/build-script fixes are staged for the next evidence run. |

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
