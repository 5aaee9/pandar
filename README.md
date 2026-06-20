# Pandar

Bambu Studio cloud alternative.

## Architecture

```text
Client -(HTTP / WebSocket)-> pandar-hub
pandar-agent -(gRPC)-> pandar-hub
pandar-agent -(SFTP / MQTT)-> Bambu machines
```

See [docs/architecture.md](docs/architecture.md) for the reference-derived architecture notes and [docs/roadmap.md](docs/roadmap.md) for the implementation roadmap.

## Workspace

- `crates/pandar-core` - shared domain types.
- `crates/pandar-hub` - Axum API server for users and reverse agent connections.
- `crates/pandar-agent` - deployable local agent for Bambu machine access.
- `crates/pandar-app` - operator CLI.
- `frontend` - Next.js frontend.
- `proto` - gRPC contracts.

Communication with Bambu machines should be implemented from the behavior in `reference/BambuStudio` and `reference/bambuddy`, without copying unrelated application code into the main workspace.

## Development

`pandar-hub` reads `PANDAR_DATABASE_URL` on startup and defaults to:

```bash
sqlite://pandar.db
```

The hub runs backend-specific SQLx migrations automatically when it connects. SQLite migrations live under `crates/pandar-hub/migrations/sqlite`; PostgreSQL migrations live under `crates/pandar-hub/migrations/postgres`.

Repository and HTTP tests use SQLite by default, including `sqlite::memory:` for API tests. Optional PostgreSQL repository tests run only when `PANDAR_TEST_POSTGRES_URL` points at a disposable PostgreSQL database.

```bash
cargo fmt
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Focused hub checks:

```bash
cargo test -p pandar-hub
cargo fmt --check -p pandar-hub
```
