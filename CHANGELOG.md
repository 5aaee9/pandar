# Changelog

All notable changes to Pandar are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

### Planned distribution

- The release workflow will publish archives containing the CLI, ABI-series-matched network plugin, and BambuSource companion with SHA-256 sidecars; Windows also publishes the Studio hook bundle.
- The tag will publish Hub and Web images at `ghcr.io/projectpandar/pandar/hub:v0.1.1` and `ghcr.io/projectpandar/pandar/web:v0.1.1`.
- The tag will publish Helm chart `0.1.1` at `oci://ghcr.io/projectpandar/pandar/chart/pandar`.

### Known limitations

- Desktop archives remain unsigned. Verify the supplied SHA-256 sidecar; Windows SmartScreen and macOS Gatekeeper may warn.
- Real-host installation and real Bambu Studio replacement evidence is not complete for every target and ABI series.
- The planned container images target Linux amd64 only.
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

[Unreleased]: https://github.com/ProjectPandar/pandar/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/ProjectPandar/pandar/releases/tag/v0.1.1
[0.1.0]: https://github.com/ProjectPandar/pandar/releases/tag/v0.1.0
