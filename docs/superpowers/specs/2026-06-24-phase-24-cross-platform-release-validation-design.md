# Phase 24 Cross-Platform Release Validation And Packaging Design

## Objective

Make Pandar release artifacts predictable enough for operators to install without building from source. Phase 24 turns the current tag-driven release workflow into an evidence-producing release gate for archive layout, checksums, CLI startup, plugin loadability, exported plugin symbols, and installation documentation.

Phase 24 does not claim that a release is usable on Linux, Windows, or macOS until evidence from the relevant target family is recorded. CI release-smoke checks may prove artifact shape and binary-level loadability, but real host installation evidence remains separate from local or cross-compiled checks.

## Current State

- `.github/workflows/release.yml` builds `pandar` CLI and `pandar-network-plugin` archives for Linux, Windows, and macOS target labels and publishes tag releases.
- Archives currently contain the CLI binary plus the plugin dynamic library and a `.sha256` file.
- Phase 23 added local ABI probe coverage and Bambu Studio compatibility evidence docs, but release artifacts are not yet inspected after packaging.
- Linux arm64 plugin release is still risky because `build.rs` uses a GNU linker version-script export strategy for `cdylib` plus the C++ shim.
- Operator docs are spread across `docs/development.md`, `docs/architecture.md`, the Nix module, and Phase 23 compatibility docs. There is no single installation guide for release archives.

## Release Artifact Contract

Every release archive that Phase 24 treats as usable must have a documented contract:

- Archive name: `pandar-release-<tag>-<target-label>.tar.gz`.
- Checksum sidecar: same basename plus `.sha256`.
- Archive contents:
  - `pandar` or `pandar.exe`
  - platform plugin library:
    - Linux: `libpandar_network_plugin.so`
    - Windows: `pandar_network_plugin.dll`
    - macOS: `libpandar_network_plugin.dylib`
- The archive must not contain absolute paths, target build directories, temporary directories, duplicate binaries, or nested layout that operators need to infer.
- The checksum file must contain exactly the archive checksum and archive filename, not a local path.

If a target cannot meet this contract, Phase 24 must mark that target unsupported or blocked in docs and CI output instead of uploading an ambiguous artifact.

## Release-Smoke Checks

Add a release-smoke check that can run against a packaged artifact directory before upload. It must verify:

1. The archive and `.sha256` file exist for the target label.
2. The checksum sidecar validates the archive.
3. The archive contains exactly the expected top-level files for the target label.
4. The CLI binary exists and has the expected filename.
5. The plugin library exists and has the expected filename.
6. For artifacts that can execute on the runner, `pandar --help` exits successfully.
7. For plugin libraries that can be inspected on the runner, the required Bambu Studio ABI symbols are exported from the packaged library, not only from a local development build.
8. Unsupported cross-target checks must produce an explicit skip reason tied to the target label and runner OS.

The required plugin symbol list is the full canonical export list in `docs/superpowers/specs/2026-06-23-phase-21-network-plugin-abi-symbols.txt`, matching the existing local export-list test. The release-smoke checker should reuse that file or the same parsing rule rather than carrying a second hand-maintained symbol list.

The smoke checker should prefer stable platform tools:

- Linux ELF: `nm -D` or `readelf -Ws`
- macOS Mach-O: `nm -gU`
- Windows PE: an available LLVM binutils tool such as `llvm-nm` / `llvm-objdump`, or an explicit skip if unavailable

## CI Integration

The release workflow must run the release-smoke check before `actions/upload-artifact`. A failed smoke check must fail the release build job for that target label.

Required behavior:

- Keep tag-driven GitHub Release publishing intact.
- Keep per-target archives and checksums downloadable from workflow artifacts.
- Do not silently publish a plugin for targets whose plugin smoke cannot be built or inspected according to the target's declared support status.
- Linux arm64 plugin handling must be explicit:
  - If Phase 24 keeps publishing Linux arm64 plugin artifacts, the release-smoke check must inspect the packaged `libpandar_network_plugin.so` exports for the arm64 artifact.
  - If the current GNU export-map strategy cannot reliably support Linux arm64, Phase 24 must remove or mark that plugin artifact unsupported before release publication and document the unsupported status.

## Operator Installation Docs

Add operator-facing docs that let a user install without reading CI internals:

- Release archive selection by OS/architecture.
- Checksum verification.
- `pandar` CLI placement and startup smoke check.
- Hub/web/agent service deployment overview.
- Docker Compose deployment for SQLite and PostgreSQL shapes.
- NixOS deployment through `services.pandar`.
- `pandar-network-plugin` replacement paths per OS, linking to the Phase 23 real Studio smoke runbook.
- Unsupported target table with the reason and next action.
- Signing/notarization decision for the next release.

The docs must state that Phase 24 artifacts are unsigned unless signing/notarization is implemented in this phase. If unsigned artifacts remain acceptable, the docs must call out the operator trust model and platform warning behavior at a high level.

## Signing And Notarization Decision

Phase 24 must make one explicit decision:

- `unsigned-accepted`: unsigned archives remain acceptable for the next release, with documented platform warnings and checksum verification; signing/notarization is deferred to a later phase.
- `signing-required`: release artifacts are not treated as usable until signing/notarization is implemented and verified for the affected platforms.

The decision must be recorded in the installation docs and roadmap. Do not leave signing as an implicit TODO.

## Evidence Manifest

Add a release evidence document, separate from Phase 23 Studio compatibility evidence, that records:

- release tag or workflow run identifier;
- target label;
- archive filename;
- checksum verification status;
- archive layout status;
- CLI startup status;
- plugin export inspection status;
- real host install smoke status;
- signing/notarization status;
- notes and blocker links.

Allowed status values are `passed`, `failed`, `blocked`, `unsupported`, and `untested`. Initial rows may be `untested` where no real release run has been executed, but CI-local release-smoke behavior must have tests or dry-run evidence before Phase 24 is considered locally implemented.

## Non-Goals

- Do not implement object storage or upload pipeline changes; that belongs to Phase 25.
- Do not claim real Bambu Studio compatibility; that remains Phase 23 evidence.
- Do not add virtual-printer/proxy behavior.
- Do not rewrite the full release system or replace GitHub Releases.
- Do not add package-manager distribution channels such as Homebrew, Scoop, apt, or nixpkgs publication.

## Acceptance Criteria

- A repo-local release-smoke checker can validate a staged release artifact archive and checksum for at least the current host's executable/plugin inspection path.
- `.github/workflows/release.yml` runs the release-smoke checker before upload.
- Required plugin ABI exports are checked from packaged release artifacts, not only from local build outputs.
- Release docs explain archive selection, checksum verification, CLI/service install shape, Docker Compose deployment, NixOS deployment, plugin replacement paths, unsupported targets, and signing status.
- Release evidence docs distinguish CI artifact validation from real host install evidence.
- `docs/roadmap.md` records what Phase 24 completed and any targets that remain unsupported or untested.
- Existing Phase 23 compatibility docs remain linked but not conflated with release packaging evidence.
