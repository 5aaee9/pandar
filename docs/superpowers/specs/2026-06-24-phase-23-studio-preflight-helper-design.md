# Phase 23 Studio Preflight Helper Design

## Scope

Add a small, local preflight helper for Phase 23 real Bambu Studio plugin compatibility testing. The helper prepares an operator to run the existing manual Studio smoke checklist by validating prerequisite inputs and printing a redacted evidence row template. It does not launch Bambu Studio, install or replace plugins, contact Pandar services, or claim real Studio compatibility.

This is a Phase 23 readiness milestone. Real compatibility still requires `docs/compatibility/bambu-studio-plugin.md` rows from actual Bambu Studio runs on Linux, Windows, and macOS.

## Current Problem

Phase 23 currently has a manual runbook and compatibility manifest, but no testable local command that catches common operator setup mistakes before a real Studio run:

- wrong plugin artifact filename for the target operating system;
- missing Studio executable path or plugin artifact path;
- malformed Hub or frontend URL values;
- incomplete evidence metadata before the operator starts replacing Studio plugin files.

The current workspace also has no Bambu Studio installation, so the next useful local step is to make future real-host evidence capture less error-prone without pretending local checks are real compatibility proof.

## Proposed Tool

Create a standalone Rust helper crate:

```text
tools/studio-plugin-smoke
```

The binary name is:

```text
pandar-studio-plugin-smoke
```

The first supported mode is preflight:

```bash
cargo run --manifest-path tools/studio-plugin-smoke/Cargo.toml -- \
  --preflight \
  --studio-path /path/to/BambuStudio \
  --plugin-artifact /path/to/libpandar_network_plugin.so \
  --hub-url https://hub.example \
  --frontend-url https://web.example \
  --os linux \
  --arch x86_64 \
  --studio-version "1.10.2" \
  --test-date 2026-06-24 \
  --pandar-commit "$(git rev-parse HEAD)"
```

## Behavior

The helper validates only local prerequisites and metadata shape:

- `--preflight` is required and is the only mode.
- `--studio-path` must exist.
- `--plugin-artifact` must exist and must be a file.
- The plugin artifact filename must match the selected OS:
  - Linux: `libpandar_network_plugin.so`
  - Windows: `pandar_network_plugin.dll`
  - macOS: `libpandar_network_plugin.dylib`
- `--hub-url` and `--frontend-url` must be absolute `http://` or `https://` URLs without embedded credentials.
- `--os` accepts only `linux`, `windows`, or `macos`.
- `--arch` accepts only `x86_64`, `amd64`, `aarch64`, or `arm64`. It is evidence metadata only; it does not change the plugin filename rule, which is selected by `--os`.
- `--studio-version` must be non-empty.
- `--test-date` must be a `YYYY-MM-DD` calendar-shaped value: four ASCII digits, `-`, two ASCII digits, `-`, two ASCII digits.
- `--pandar-commit` must be non-empty and must not contain `/`, `\`, or ASCII whitespace.
- `--preflight` is a leading mode token, matching `tools/scaled-artifact-smoke --live-preflight`; all other inputs are `--name value` pairs after the mode token.

On success, the helper prints:

- a `PASS studio-plugin-preflight` line;
- the OS, architecture, Studio version, plugin artifact filename, and Pandar commit;
- a markdown evidence row template matching the exact column order in `docs/compatibility/bambu-studio-plugin.md`:

```markdown
| <studio-version> | <os> | <arch> | `<plugin-artifact-filename>` | `<pandar-commit>` | <test-date> | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | Preflight passed only; replace this evidence note after a real Studio run. |
```

The row columns are, in order: Studio Version, OS, Arch, Plugin Artifact, Pandar Commit, Test Date, Load, Sign-In Page, Localhost Ticket, Token Exchange, Profile, Printers, Jobs, Print Submission, Logout, Unsupported ABI, Evidence. All actual Studio checklist statuses remain `untested` because preflight does not run Studio.

On failure, the helper exits non-zero and prints every discovered preflight issue in one run. Error output must not include full local filesystem paths; use only input labels and artifact filenames where possible.

## Non-Goals

- No Bambu Studio launch automation.
- No plugin replacement, copy, backup, or rollback operation.
- No network requests to Hub or frontend.
- No parsing of Studio logs.
- No new release artifact validation; Phase 24 remains owned by `tools/release-smoke`.
- No claim that preflight success proves Bambu Studio compatibility.

## Documentation Updates

Update:

- `docs/compatibility/bambu-studio-plugin-smoke.md` to add the preflight command before the manual replacement checklist.
- `docs/compatibility/bambu-studio-plugin.md` to record the local preflight helper as readiness coverage, separate from real Studio evidence.
- `docs/roadmap.md` Phase 23 completed/local-scaffolding bullet list to record the new readiness milestone while keeping the real Studio evidence line blocked.

## Acceptance Criteria

- The helper passes unit tests for valid Linux preflight, wrong OS/plugin filename combinations, missing files, malformed URLs, URL credentials, missing required flags, invalid commit values containing `/`, `\`, or whitespace, invalid date shape, and evidence template output.
- A valid local invocation with temporary fake Studio/plugin files exits zero and prints `PASS studio-plugin-preflight`.
- An invalid plugin filename exits non-zero and reports a filename/OS mismatch without leaking the full supplied path.
- The evidence row template has the same column count and order as `docs/compatibility/bambu-studio-plugin.md`, uses the explicit `--test-date` value, and keeps every real Studio status field as `untested`.
- Documentation clearly states that this is prerequisite readiness only and not real Studio compatibility evidence.
- `cargo fmt --check --manifest-path tools/studio-plugin-smoke/Cargo.toml`, `cargo fmt --check`, `cargo clippy --manifest-path tools/studio-plugin-smoke/Cargo.toml`, `cargo test --manifest-path tools/studio-plugin-smoke/Cargo.toml`, and the workspace `cargo nextest run --manifest-path "Cargo.toml" --workspace` pass after implementation.
