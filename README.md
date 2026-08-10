# Pandar

Bambu Studio cloud alternative.

## Architecture

```text
Client -(HTTP / WebSocket)-> pandar-hub
pandar-agent -(gRPC)-> pandar-hub
pandar-agent -(MQTT + machine file transfer)-> Bambu machines
```

`pandar-hub` is the multi-tenant Rust API server. `pandar-agent` runs on a user's local network and bridges hub commands to Bambu machines. The frontend is a Next.js product UI that talks only to the hub.

## Documentation

- [0.1.2 changelog](CHANGELOG.md): release highlights, distribution channels, and known limitations.
- [Release installation](docs/release-installation.md): archive selection, checksum verification, deployment, and plugin replacement.
- [Maintainer release runbook](docs/releasing.md): version checks, quality gates, tagging, and publication verification.
- [Architecture](docs/architecture.md): component boundaries, reference-derived machine behavior, data model, and protocol notes.
- [Development and deployment notes](docs/development.md): environment variables, local setup, auth/provisioning examples, live WebSocket notes, and verification commands.
- [NixOS module options](docs/deployment/nixos/options.md): generated `services.pandar` deployment options for hub, web, and agent services.
- [Roadmap](docs/roadmap.md): completed phases and planned next phases.

## Release 0.1.2

The `v0.1.2` release provides native Linux amd64, macOS amd64/arm64, and Windows amd64 CLI/plugin/BambuSource archives plus the Windows Studio hook bundle on [GitHub Releases](https://github.com/ProjectPandar/pandar/releases/tag/v0.1.2), Hub and Web images as `ghcr.io/projectpandar/pandar/hub:v0.1.2` and `ghcr.io/projectpandar/pandar/web:v0.1.2`, and Helm chart `0.1.2` at `oci://ghcr.io/projectpandar/pandar/chart/pandar`.

Desktop archives will be unsigned and accompanied by SHA-256 sidecars. Read the [installation guide](docs/release-installation.md) and [current compatibility evidence](docs/compatibility/release-artifacts.md) before production use.

## Workspace

- `crates/pandar-core` - shared domain types.
- `crates/pandar-hub` - Axum API server for users and reverse agent connections.
- `crates/pandar-agent` - deployable local agent for Bambu machine access.
- `crates/pandar-network-plugin` - Bambu Studio network plugin ABI replacement scaffold that connects Studio sign-in to `pandar-hub`.
- `crates/pandar-app` - operator CLI.
- `frontend` - Next.js frontend.
- `mobile/android` - Jetpack Compose + Material 3 Android app (see `docs/android.md`).
- `proto` - gRPC contracts.
- `reference` - protocol and behavior references.

## References

- [BambuStudio](https://github.com/bambulab/BambuStudio): Studio product behavior, network-agent ABI caller, and print workflow reference.
- [bambuddy](https://github.com/maziggy/bambuddy): Bambu MQTT, discovery, file transfer, and printer-state behavior reference.
- [open-bamboo-networking](https://github.com/ClusterM/open-bamboo-networking): Bambu Studio network plugin ABI replacement reference.

Communication with Bambu machines should be implemented from reference behavior without copying unrelated application code into the main workspace.

## Quick Checks

```bash
cargo fmt
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
npm run build:web
```
