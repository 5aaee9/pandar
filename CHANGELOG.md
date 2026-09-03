# Changelog

All notable changes to Pandar are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1] - 2026-09-03

### Changed

- Centralized the release version in `Cargo.toml` `[workspace.package]` and a single `nix/pandar.nix` literal; workspace members inherit `version.workspace = true` and `scripts/check-release-version.sh` enforces the single source of truth.

### Removed

- Removed the test-only bulk `retryDispatchJobs`/`duplicateJob` server actions with their orphaned `retry_partial`/`duplicate_queued` statuses and translations; the single-job retry action and Hub recovery endpoints are unchanged.
- Removed orphaned UI and script leaves: the unused dashboard `Separator` primitive, the unused Android `MonoText` composable, and the nested frontend `build:plugin-local` script.
- Removed the one-shot bundle-size measurement script with its stale baseline, the superseded Phase 21 ABI symbol listing plus its vestigial Nix source filter, and the completed implementation-tracking plans; durable contracts remain with the specs, architecture, compatibility, and changelog owners.

### Fixed

- Restored the manually selected Pandar server before Studio login evaluation: a Web URL chosen on the plugin's local sign-in page and its discovered canonical Hub identity now persist (`pandar-plugin-server-selection.json`, typed, durable replacement) and are restored on the next clean launch without URL environment variables, so a valid same-Hub login is visible through the Studio ABI without another sign-in. Explicit `PANDAR_PLUGIN_*`/`APP_*` URL configuration stays authoritative, a credential belonging to a different Hub is never restored, and a malformed selection fails closed with its diagnostic logged.

### Distribution

- The release publishes seven ABI-series-specific CLI, network plugin, and BambuSource archive sets with SHA-256 sidecars; Windows also publishes Studio hook bundles.
- Hub and Web images are published at `ghcr.io/projectpandar/pandar/hub:v0.2.1` and `ghcr.io/projectpandar/pandar/web:v0.2.1`.
- Helm chart `0.2.1` is published at `oci://ghcr.io/projectpandar/pandar/chart/pandar`.

### Known limitations

- Desktop archives remain unsigned. Verify the supplied SHA-256 sidecar; Windows SmartScreen and macOS Gatekeeper may warn.
- Real-host installation and real Bambu Studio replacement evidence is not complete for every target and ABI series.
- The container images target Linux amd64 only.
- Native firmware and recovery ownership still requires one active Hub; the firmware package catalog is intentionally empty.
- The Android client is built separately and is not attached to the GitHub Release.

## [0.2.0] - 2026-08-31

### Added

- Added ABI-series-specific support for Bambu Studio `02.08.02.61`, including its appended `PrintParams::queue_plate_id` layout and `bambu_network_sync_slot_mappings` export. Slot-mapping cloud synchronization remains an explicit unsupported operation; target-native package validation passed, while real-Studio validation remains separate and is not claimed.
- Added authenticated Bambu Studio personal-preset synchronization and a versioned live printer-event stream with generation-fenced caching and recovery.
- Added one generated OpenAPI Hub-client contract for Web and Android, canonical printer capability projections, and shared printer-operation and Agent wire models.
- Added a durable artifact reservation, deletion, retry, and cleanup lifecycle with equivalent SQLite and PostgreSQL behavior.

### Changed

- Consolidated network-plugin runtime, freshness, account, firmware, and dispatch ownership behind Rust `PluginCore` operations while keeping the C++ layer limited to ABI, STL, callback, and thread adaptation.
- Made Agent command transitions, Hub printer snapshots, cleanup planning, print execution input, and artifact persistence atomic around their owning domain boundaries.
- Reworked Android around one reactive authenticated Hub session and one generation-fenced domain state owner; Web routes now use focused views and resource-owned query state.
- Replaced resource-specific job-storage compatibility facades with the backend-neutral `ArtifactStorage` boundary and one opaque storage key.

### Fixed

- Fixed protected FTPS uploads on affected printers by reusing the control-channel TLS session for protected data connections while preserving complete causal errors.
- Fixed BambuSource reader-thread self-close, file-transfer callback replacement races, recursive callback dispatch, and result-handle lifetime during foreign callbacks.
- Fixed cross-replica command-result delivery, tenant-scoped event invalidation, concurrent last-admin demotion, stale Agent session transitions, and MQTT overflow recovery.
- Fixed required-nullable Android wire presence, dashboard membership fail-closed behavior, redirected action feedback, repeated secret redaction, and healthy WebSocket reconnect backoff.

### Security

- Removed production artifact-path and compatibility fallbacks, retired plugin FFI surfaces, and duplicate low-level storage APIs.
- Marked every pointer-validity-dependent Rust/C export unsafe with explicit ownership, callback, nested-view, and lifetime contracts.
- Made release-version invariants, Android tests and lint, Nix auth smoke tests, pinned Studio contracts, module-size checks, and dual-backend parity durable CI gates.

### Distribution

- The release publishes seven ABI-series-specific CLI, network plugin, and BambuSource archive sets with SHA-256 sidecars; Windows also publishes Studio hook bundles.
- Hub and Web images are published at `ghcr.io/projectpandar/pandar/hub:v0.2.0` and `ghcr.io/projectpandar/pandar/web:v0.2.0`.
- Helm chart `0.2.0` is published at `oci://ghcr.io/projectpandar/pandar/chart/pandar`.

### Known limitations

- Desktop archives remain unsigned. Verify the supplied SHA-256 sidecar; Windows SmartScreen and macOS Gatekeeper may warn.
- Real-host installation and real Bambu Studio replacement evidence is not complete for every target and ABI series.
- The container images target Linux amd64 only.
- Native firmware and recovery ownership still requires one active Hub; the firmware package catalog is intentionally empty.
- The Android client is built separately and is not attached to the GitHub Release.

## [0.1.4] - 2026-08-11

### Fixed

- Changed Studio and mobile sign-in tenant discovery to use the authenticated identity's `/api/v1/me` memberships instead of the bootstrap-only global tenant list, eliminating `Tenant lookup returned 403` after external sign-in.

### Distribution

- The release publishes ABI-series-specific CLI, network plugin, and BambuSource archives with SHA-256 sidecars; Windows also publishes Studio hook bundles.
- Hub and Web images are published at `ghcr.io/projectpandar/pandar/hub:v0.1.4` and `ghcr.io/projectpandar/pandar/web:v0.1.4`.
- Helm chart `0.1.4` is published at `oci://ghcr.io/projectpandar/pandar/chart/pandar`.

### Known limitations

- Desktop archives remain unsigned. Verify the supplied SHA-256 sidecar; Windows SmartScreen and macOS Gatekeeper may warn.
- Real-host installation and real Bambu Studio replacement evidence is not complete for every target and ABI series.
- The container images target Linux amd64 only.
- Native firmware and recovery ownership still requires one active Hub; the firmware package catalog is intentionally empty.
- The Android client is built separately and is not attached to the GitHub Release.

## [0.1.3] - 2026-08-11

### Fixed

- Moved the Studio sign-in external-auth probe from the private observability-only `/readyz` route to the public, sanitized `/api/v1/auth/status` endpoint, eliminating the `Readiness check returned 404` failure on normal Hub deployments.

### Distribution

- The release publishes ABI-series-specific CLI, network plugin, and BambuSource archives with SHA-256 sidecars; Windows also publishes Studio hook bundles.
- Hub and Web images are published at `ghcr.io/projectpandar/pandar/hub:v0.1.3` and `ghcr.io/projectpandar/pandar/web:v0.1.3`.
- Helm chart `0.1.3` is published at `oci://ghcr.io/projectpandar/pandar/chart/pandar`.

### Known limitations

- Desktop archives remain unsigned. Verify the supplied SHA-256 sidecar; Windows SmartScreen and macOS Gatekeeper may warn.
- Real-host installation and real Bambu Studio replacement evidence is not complete for every target and ABI series.
- The container images target Linux amd64 only.
- Native firmware and recovery ownership still requires one active Hub; the firmware package catalog is intentionally empty.
- The Android client is built separately and is not attached to the GitHub Release.

## [0.1.2] - 2026-08-10

### Changed

- Simplified Bambu Studio plugin targeting to a single Web URL and added Web-hosted Hub URL discovery through `/.well-known/pandar`.
- Added `APP_PUBLIC_API_URL` for deployments whose Studio-reachable Hub URL differs from the Web server's internal `APP_API_URL`.

### Fixed

- Changed the Studio sign-in external-auth probe from `/healthz` to `/readyz`; a post-release follow-up was required because `/readyz` is available only on the private observability listener.

### Distribution

- The release publishes ABI-series-specific CLI, network plugin, and BambuSource archives with SHA-256 sidecars; Windows also publishes Studio hook bundles.
- Hub and Web images are published at `ghcr.io/projectpandar/pandar/hub:v0.1.2` and `ghcr.io/projectpandar/pandar/web:v0.1.2`.
- Helm chart `0.1.2` is published at `oci://ghcr.io/projectpandar/pandar/chart/pandar`.

### Known limitations

- Desktop archives remain unsigned. Verify the supplied SHA-256 sidecar; Windows SmartScreen and macOS Gatekeeper may warn.
- Real-host installation and real Bambu Studio replacement evidence is not complete for every target and ABI series.
- The container images target Linux amd64 only.
- Native firmware and recovery ownership still requires one active Hub; the firmware package catalog is intentionally empty.
- The Android client is built separately and is not attached to the GitHub Release.

## [0.1.1] - 2026-08-10

### Added

- Added Bambu Studio ABI profiles from 2.6 through 2.8.1, ABI-series-specific plugin packages, and macOS amd64/arm64 release archives.
- Added fail-closed H2C nozzle-rack telemetry and controls, mixed AMS Lite routing, AMS filament drying controls, and broader printer motion support.
- Added secure Studio local-camera streaming plus persistent browser picture-in-picture playback.
- Added live cooling-system telemetry and fan controls, current print-speed display and switching, and catalog-backed HMS messages in printer details.
- Added richer Agents, settings, printer-link, printer-control, and attention workflows in the Web dashboard.

### Changed

- Moved Studio status projection, File Transfer ABI behavior, and connection-delivery policy behind typed Rust interfaces while keeping the C++ shim ABI-only.
- Centralized Web printer-control contracts and unified interactive surfaces on the shared Button component.
- Moved the repository and published artifact paths to the ProjectPandar organization.

### Fixed

- Required protected FTPS data channels, verified Bambu v1 MQTT certificates, and added bounded direct-host printer discovery when multicast is unavailable.
- Corrected full-status authority, H2C sequencing, A2L usage routing, printer-model projection, and several printer-control and responsive-layout issues.
- Preserved actionable printer-link errors and camera picture-in-picture sessions across dashboard navigation.

### Security

- Updated Rust and frontend dependencies, including patched Nano ID and PostCSS releases, and restored strict dependency-audit and Clippy CI gates.

### Distribution

- The release publishes archives containing the CLI, ABI-series-matched network plugin, and BambuSource companion with SHA-256 sidecars; Windows also publishes the Studio hook bundle.
- Hub and Web images are published at `ghcr.io/projectpandar/pandar/hub:v0.1.1` and `ghcr.io/projectpandar/pandar/web:v0.1.1`.
- Helm chart `0.1.1` is published at `oci://ghcr.io/projectpandar/pandar/chart/pandar`.

### Known limitations

- Desktop archives remain unsigned. Verify the supplied SHA-256 sidecar; Windows SmartScreen and macOS Gatekeeper may warn.
- Real-host installation and real Bambu Studio replacement evidence is not complete for every target and ABI series.
- The container images target Linux amd64 only.
- Native firmware and recovery ownership still requires one active Hub; the firmware package catalog is intentionally empty.
- The Android client is built separately and is not attached to the GitHub Release.

## [0.1.0] - 2026-07-31

### Added

- Multi-tenant Rust Hub, local-network Agent, operator CLI, and Next.js dashboard for managing users, Agents, Bambu printers, and print jobs.
- SQLite and PostgreSQL storage backends, with NATS and S3-compatible storage support for scaled Hub deployments.
- Bambu MQTT, implicit FTPS, and BRTC integration for printer discovery, telemetry, file transfer, print dispatch, monitoring, controls, and recovery workflows.
- Bambu Studio network-plugin replacement with Hub-backed sign-in, printer/job synchronization, print submission, native controls, and firmware protocol support.
- Print artifact preview, plate selection, AMS/nozzle-aware material mapping, reprint, stalled-job recovery, job cleanup, and audit events.
- Responsive Web operator surfaces for printers, live camera, jobs, Agents, users, tenant credentials, settings, and authentication.
- Optional self-hosted Better Auth service plus Clerk and Logto JWT verification support.
- NixOS modules, Docker images, Docker Compose deployment shapes, a Kubernetes Helm chart, and a Jetpack Compose Android client.

### Security

- Encrypted persisted printer access codes with tenant/printer-bound AES-256-GCM envelopes.
- Added tenant-scoped authorization, short-lived plugin/mobile login tickets, credential redaction, bounded protocol payloads, hardened container defaults, and secret-safe NixOS environment-file handling.
- Updated Next.js and Better Auth to their release-preparation patch versions and pinned patched Lodash/PostCSS transitive resolutions; the production npm audit is clean.
- Made release publication wait for the exact tagged commit's successful Checks workflow.

### Distribution

- The release publishes native Linux amd64 and Windows amd64 archives containing the CLI, network plugin, and BambuSource companion, each with a SHA-256 sidecar. It also publishes the native Windows Studio hook bundle and checksum.
- Hub and Web images are published at `ghcr.io/projectpandar/pandar/hub:v0.1.0` and `ghcr.io/projectpandar/pandar/web:v0.1.0`.
- Helm chart `0.1.0` is published at `oci://ghcr.io/projectpandar/pandar/chart/pandar`.

### Known limitations

- Desktop archives are unsigned. Verify the supplied SHA-256 sidecar; Windows SmartScreen and macOS Gatekeeper may warn.
- Real-host installation and real Bambu Studio replacement evidence is not yet complete for every target. Consult `docs/compatibility/release-artifacts.md` and `docs/compatibility/bambu-studio-plugin.md` before production use.
- The container images target Linux amd64 only.
- Native firmware and recovery ownership still requires one active Hub; the firmware package catalog is intentionally empty.
- The Android client is built separately and will not be attached to the GitHub Release.

[Unreleased]: https://github.com/ProjectPandar/pandar/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ProjectPandar/pandar/releases/tag/v0.2.0
[0.1.4]: https://github.com/ProjectPandar/pandar/releases/tag/v0.1.4
[0.1.3]: https://github.com/ProjectPandar/pandar/releases/tag/v0.1.3
[0.1.2]: https://github.com/ProjectPandar/pandar/releases/tag/v0.1.2
[0.1.1]: https://github.com/ProjectPandar/pandar/releases/tag/v0.1.1
[0.1.0]: https://github.com/ProjectPandar/pandar/releases/tag/v0.1.0
