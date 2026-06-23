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

- [Architecture](docs/architecture.md): component boundaries, reference-derived machine behavior, data model, and protocol notes.
- [Development and deployment notes](docs/development.md): environment variables, local setup, auth/provisioning examples, live WebSocket notes, and verification commands.
- [Roadmap](docs/roadmap.md): completed phases and planned next phases.

## Workspace

- `crates/pandar-core` - shared domain types.
- `crates/pandar-hub` - Axum API server for users and reverse agent connections.
- `crates/pandar-agent` - deployable local agent for Bambu machine access.
- `crates/pandar-network-plugin` - Bambu Studio network plugin ABI replacement scaffold that connects Studio sign-in to `pandar-hub`.
- `crates/pandar-app` - operator CLI.
- `frontend` - Next.js frontend.
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
npm --prefix frontend run build
```
