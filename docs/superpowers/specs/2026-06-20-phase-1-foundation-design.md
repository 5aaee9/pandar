# Phase 1 Foundation Design

## Goal

Implement Pandar Phase 1 foundation: durable multi-tenant core records, a backend-neutral persistence boundary, SQLite and PostgreSQL database support, and hub routes backed by repositories instead of in-memory vectors.

## Scope

This phase is limited to foundational persistence and contracts. It does not implement authentication, full user invitation flows, real agent gRPC sessions, Bambu MQTT/FTPS transport, print dispatch, or frontend data fetching.

## Current Baseline

- `pandar-core` contains basic `Tenant` and `Agent` domain types.
- `pandar-hub` stores tenants and agents in an in-memory `RwLock`.
- `proto/pandar/agent/v1/agent.proto` has an initial agent control stream.
- `docs/architecture.md` requires tenant-scoped hub state, backend-neutral persistence, and SQLite/PostgreSQL support.
- `AGENTS.md` requires both SQLite and PostgreSQL as first-class database backends.

## Architecture

Use SQLx for persistence because it supports async Rust access to SQLite and PostgreSQL with one library. Use runtime-checked SQL through `sqlx::query` / `sqlx::query_as` rather than compile-time database-bound macros, so the workspace can build without a live database.

Add a `pandar-hub::db` module with:

- `DatabaseConfig`: parses a database URL and identifies `sqlite` or `postgres`.
- `Database`: enum wrapper around `SqlitePool` and `PgPool`.
- `DatabaseBackend`: enum with `Sqlite` and `Postgres`.
- `migrate()`: runs backend-specific migrations.

Add a `pandar-hub::repositories` module with repository structs that hide backend-specific SQL:

- `TenantRepository`
- `AgentRepository`
- `PrinterRepository`
- `CommandRepository`

Each repository accepts a cloned `Database` handle and branches internally on backend when SQL syntax differs.

Hub `AppState` should hold repositories instead of raw in-memory vectors. HTTP handlers should depend on `AppState` and repository methods.

`pandar-core` remains the owner of public domain records used by the hub API. Phase 1 should extend it only where the persistence contract needs stable core types:

- Add parsing helpers for `TenantId` and `AgentId` from path/database UUID strings.
- Add created-at fields to persisted `Tenant` and `Agent` records.
- Add minimal `User`, `Printer`, and `Command` records plus IDs/status enums only if repository APIs return them directly.

Hub-only row structs may exist inside `pandar-hub::repositories`, but HTTP responses must be built from core domain records or explicit API response DTOs, not raw SQL rows.

## Data Model

Phase 1 persists these tables:

- `tenants`
  - `id` text primary key
  - `slug` text unique not null
  - `display_name` text not null
  - `created_at` text not null
- `users`
  - `id` text primary key
  - `tenant_id` text not null references `tenants(id)` on delete cascade
  - `email` text not null
  - `display_name` text not null
  - `role` text not null
  - `created_at` text not null
  - unique `(tenant_id, email)`
- `agents`
  - `id` text primary key
  - `tenant_id` text not null references `tenants(id)` on delete cascade
  - `name` text not null
  - `status` text not null
  - `version` text nullable
  - `last_seen_at` text nullable
  - `created_at` text not null
  - unique `(tenant_id, name)`
- `printers`
  - `id` text primary key
  - `tenant_id` text not null references `tenants(id)` on delete cascade
  - `agent_id` text not null references `agents(id)` on delete cascade
  - `serial_number` text not null
  - `name` text not null
  - `model` text nullable
  - `status` text not null
  - `created_at` text not null
  - unique `(tenant_id, serial_number)`
- `commands`
  - `id` text primary key
  - `tenant_id` text not null references `tenants(id)` on delete cascade
  - `agent_id` text not null references `agents(id)` on delete cascade
  - `printer_id` text nullable references `printers(id)` on delete set null
  - `kind` text not null
  - `status` text not null
  - `payload_json` text not null
  - `error` text nullable
  - `created_at` text not null
  - `updated_at` text not null

Use text UUIDs for IDs across SQLite and PostgreSQL to keep schema behavior aligned. Use ISO-8601 UTC timestamp strings in this phase to avoid backend-specific timestamp decoding differences.

Required indexes:

- `idx_users_tenant_id` on `users(tenant_id)`
- `idx_agents_tenant_id` on `agents(tenant_id)`
- `idx_printers_tenant_id` on `printers(tenant_id)`
- `idx_printers_agent_id` on `printers(agent_id)`
- `idx_commands_tenant_id` on `commands(tenant_id)`
- `idx_commands_agent_id` on `commands(agent_id)`
- `idx_commands_printer_id` on `commands(printer_id)`

SQLite connections must execute `PRAGMA foreign_keys = ON` before migrations and normal repository access. PostgreSQL migrations should use the same logical constraints.

## Repository Contracts

Required Phase 1 repository methods:

- `TenantRepository::create(slug, display_name) -> Tenant`
- `TenantRepository::list() -> Vec<Tenant>`
- `TenantRepository::count() -> i64`
- `AgentRepository::create(tenant_id, name) -> Agent`
- `AgentRepository::list_for_tenant(tenant_id) -> Vec<Agent>`
- `AgentRepository::count() -> i64`
- `PrinterRepository::count() -> i64`
- `CommandRepository::count() -> i64`

`PrinterRepository` and `CommandRepository` are schema-backed count repositories in Phase 1. They do not need create/list APIs until Bambu discovery and command dispatch are implemented. Tests may insert printer/command fixtures through repository-private test helpers or backend-neutral test helpers; production HTTP routes should not expose printer/command writes in Phase 1.

Duplicate tenant slug and duplicate agent name within a tenant must return a typed repository error that the HTTP layer maps to `409 Conflict`. Missing tenant IDs on agent creation/list must map to `404 Not Found`.

## API Behavior

Keep existing hub endpoints and move them to repository-backed behavior:

- `GET /healthz`
  - `200 OK`
  - Response: `{ "status": "ok" }`
- `GET /api/v1/summary`
  - `200 OK`
  - Response: `{ "tenants": number, "agents": number, "printers": number, "commands": number }`
- `POST /api/v1/tenants`
  - Request: `{ "slug": string, "display_name": string }`
  - `201 Created`
  - Response: `{ "id": string, "slug": string, "display_name": string, "created_at": string }`
  - Duplicate slug: `409 Conflict` with `{ "error": "tenant_slug_exists" }`
  - Empty slug/display name or malformed JSON: `400 Bad Request`

Add minimal Phase 1 endpoints:

- `GET /api/v1/tenants`
  - `200 OK`
  - Response: `{ "tenants": [{ "id": string, "slug": string, "display_name": string, "created_at": string }] }`
- `POST /api/v1/tenants/{tenant_id}/agents`
  - Request: `{ "name": string }`
  - `201 Created`
  - Response: `{ "id": string, "tenant_id": string, "name": string, "status": "offline", "created_at": string }`
  - Malformed `tenant_id`: `400 Bad Request` with `{ "error": "invalid_tenant_id" }`
  - Missing tenant: `404 Not Found` with `{ "error": "tenant_not_found" }`
  - Duplicate agent name in tenant: `409 Conflict` with `{ "error": "agent_name_exists" }`
  - Empty name or malformed JSON: `400 Bad Request`
- `GET /api/v1/tenants/{tenant_id}/agents`
  - `200 OK`
  - Response: `{ "agents": [{ "id": string, "tenant_id": string, "name": string, "status": string, "created_at": string }] }`
  - Malformed `tenant_id`: `400 Bad Request` with `{ "error": "invalid_tenant_id" }`
  - Missing tenant: `404 Not Found` with `{ "error": "tenant_not_found" }`

All endpoint behavior must work the same on SQLite and PostgreSQL.

## Configuration

`pandar-hub` reads `PANDAR_DATABASE_URL`.

Default for local development:

```text
sqlite://pandar.db
```

Tests may use in-memory SQLite:

```text
sqlite::memory:
```

The SQLite in-memory test pool must use a single connection or a shared-cache URL so migrations and queries see the same database.

PostgreSQL tests should run only when `PANDAR_TEST_POSTGRES_URL` is set, so local contributors without PostgreSQL can still run the default workspace test suite.

## Migrations

Store migrations under:

- `crates/pandar-hub/migrations/sqlite`
- `crates/pandar-hub/migrations/postgres`

Use one initial migration per backend with equivalent table definitions and indexes. Do not use backend features that create behavioral differences in Phase 1.

Migration SQL must declare `created_at`, `updated_at`, and `last_seen_at` as text columns. Do not use backend-native timestamp column types in Phase 1.

## Testing

Add repository integration tests that run against SQLite by default and PostgreSQL when `PANDAR_TEST_POSTGRES_URL` exists.

Required tests:

- Migrations create the schema.
- Tenant creation and listing works.
- Duplicate tenant slug is rejected.
- Agent creation and tenant-scoped listing works.
- Summary counts include tenants, agents, printers, and commands.
- Existing hub HTTP tests run against SQLite-backed state.
- File-SQLite restart durability: create a tenant/agent using a file database, drop the state, reconnect, migrate, and verify the records remain.
- SQLite in-memory setup: migrations plus repository operations work on the in-memory URL used by tests.
- PostgreSQL skip behavior: when `PANDAR_TEST_POSTGRES_URL` is absent, PostgreSQL-specific tests report a skip path instead of failing.

The default verification commands remain:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

## Documentation Impact

Update:

- `README.md` with database configuration and migration notes.
- `docs/architecture.md` if implementation details differ from this design.
- `docs/roadmap.md` to mark Phase 1 completed items and list the next immediate Phase 2 work.

## Acceptance Criteria

- `pandar-hub` starts with `PANDAR_DATABASE_URL` or defaults to SQLite.
- `pandar-hub` automatically runs migrations on startup.
- Existing health and tenant endpoints still work.
- Tenant and agent state survives process restart when using a file SQLite database or PostgreSQL.
- Repository tests pass on SQLite by default.
- PostgreSQL repository tests are implemented and skipped unless `PANDAR_TEST_POSTGRES_URL` is configured.
- No production code imports a concrete SQLx pool outside the database/repository boundary.
- All new persistent behavior keeps lower-level error context using `anyhow::Context` or equivalent.
