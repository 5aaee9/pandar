# Phase 1 Foundation Implementation Plan

## Approved Spec

- `docs/superpowers/specs/2026-06-20-phase-1-foundation-design.md`
- Independent spec review: `VERDICT: APPROVE`

## Success Criteria

- `pandar-hub` uses SQLx-backed persistence instead of in-memory vectors.
- SQLite and PostgreSQL are first-class backends behind the same repository API.
- Hub startup reads `PANDAR_DATABASE_URL`, defaults to `sqlite://pandar.db`, runs migrations, and serves the existing API plus Phase 1 tenant/agent routes.
- Repository tests cover SQLite by default and PostgreSQL when `PANDAR_TEST_POSTGRES_URL` is configured.
- File-SQLite restart durability is tested.
- `README.md`, `docs/architecture.md`, and `docs/roadmap.md` describe the implemented Phase 1 behavior.
- Verification passes:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

## Implementation Steps

### 1. Core Domain Contracts

Files:

- `crates/pandar-core/src/lib.rs`

Changes:

- Add UUID parsing for `TenantId` and `AgentId`.
- Add `created_at: String` to `Tenant` and `Agent`.
- Add constructors for newly-created records and `from_parts` constructors for repository rehydration.
- Keep validation only at boundary constructors: tenant slug/display name and agent name.
- Add `AgentStatus` string conversion for persistence/API.

Verification:

- Core unit tests for ID parse errors, tenant validation, and agent default offline status.

### 2. Database Boundary And Migrations

Files:

- `Cargo.toml`
- `crates/pandar-hub/Cargo.toml`
- `crates/pandar-hub/src/db.rs`
- `crates/pandar-hub/migrations/sqlite/20260620000000_phase_1_foundation.sql`
- `crates/pandar-hub/migrations/postgres/20260620000000_phase_1_foundation.sql`

Changes:

- Add SQLx dependency with Tokio rustls runtime, SQLite, PostgreSQL, and migrations.
- Implement `DatabaseConfig`, `DatabaseBackend`, `Database`, and `migrate`.
- For SQLite, use `SqliteConnectOptions::create_if_missing(true)` and enable foreign keys before migrations and queries.
- For `sqlite::memory:`, use a single connection so migrations and repository calls see the same database.
- Use static SQLx migrators for backend-specific migration directories.
- Keep concrete SQLx pool types inside `db` and repository modules.
- Both SQLite and PostgreSQL migrations must create all Phase 1 schema tables:
  - `tenants`
  - `users`
  - `agents`
  - `printers`
  - `commands`
- Both migrations must include the approved constraints, text timestamp columns, and indexes:
  - primary keys on every `id`
  - `tenants.slug` unique
  - `users.tenant_id` foreign key and unique `(tenant_id, email)`
  - `agents.tenant_id` foreign key and unique `(tenant_id, name)`
  - `printers.tenant_id` / `printers.agent_id` foreign keys and unique `(tenant_id, serial_number)`
  - `commands.tenant_id` / `commands.agent_id` / `commands.printer_id` foreign keys
  - `idx_users_tenant_id`
  - `idx_agents_tenant_id`
  - `idx_printers_tenant_id`
  - `idx_printers_agent_id`
  - `idx_commands_tenant_id`
  - `idx_commands_agent_id`
  - `idx_commands_printer_id`

Verification:

- Tests confirm backend URL parsing and SQLite in-memory migration/query continuity.

### 3. Repositories

Files:

- `crates/pandar-hub/src/repositories/mod.rs`
- `crates/pandar-hub/src/repositories/tenants.rs`
- `crates/pandar-hub/src/repositories/agents.rs`
- `crates/pandar-hub/src/repositories/counts.rs`

Changes:

- Implement `TenantRepository::create`, `list`, and `count`.
- Implement `AgentRepository::create`, `list_for_tenant`, and `count`.
- Implement `PrinterRepository::count` and `CommandRepository::count`.
- Add repository error type with variants for duplicate tenant slug, duplicate agent name, missing tenant, invalid persisted status, and SQLx source errors.
- Preserve lower-level context using `anyhow::Context` where operations cross database boundaries.
- Add test-only backend-neutral fixture helpers for printer/command rows used by summary-count tests.

Verification:

- SQLite repository tests:
  - migrations create schema
  - tenant create/list/count
  - duplicate tenant slug rejection
  - agent create/list/count scoped by tenant
  - missing tenant on agent create/list maps to repository not-found
  - summary counts include tenant/agent/printer/command fixtures
  - file-SQLite restart durability
  - SQLite in-memory setup works
- PostgreSQL tests:
  - skipped cleanly when `PANDAR_TEST_POSTGRES_URL` is absent
  - same core repository behavior when the env var points to a disposable PostgreSQL database
  - reconnect durability when configured: create tenant/agent records, drop hub/database state, reconnect to the same URL, rerun migrations, and verify those tenant/agent records remain

### 4. Hub HTTP/API Wiring

Files:

- `crates/pandar-hub/src/lib.rs`
- `crates/pandar-hub/src/main.rs`

Changes:

- Split route code if needed to keep files below 400 LOC.
- Replace in-memory `AppState` with repository-backed state.
- Add `AppState::connect(database_url).await` and SQLite test constructor.
- Main reads `PANDAR_DATABASE_URL`, defaults to `sqlite://pandar.db`, connects, migrates, then serves.
- Implement API DTOs exactly as specified:
  - `GET /healthz`
  - `GET /api/v1/summary`
  - `POST /api/v1/tenants`
  - `GET /api/v1/tenants`
  - `POST /api/v1/tenants/{tenant_id}/agents`
  - `GET /api/v1/tenants/{tenant_id}/agents`
- Map errors:
  - invalid JSON/empty fields/malformed IDs -> `400`
  - missing tenant -> `404`
  - duplicate tenant slug or agent name -> `409`
  - unexpected database errors -> `500`

Verification:

- HTTP tests use SQLite-backed state and assert exact response status and body for:
  - health response
  - summary counts including `printers` and `commands`
  - tenant create response
  - tenant list response
  - duplicate tenant slug: `409` with `{ "error": "tenant_slug_exists" }`
  - invalid tenant ID on agent routes: `400` with `{ "error": "invalid_tenant_id" }`
  - missing tenant on agent routes: `404` with `{ "error": "tenant_not_found" }`
  - agent create response with `status: "offline"`
  - agent list response
  - duplicate agent name: `409` with `{ "error": "agent_name_exists" }`

### 5. Documentation And Roadmap

Files:

- `README.md`
- `docs/architecture.md`
- `docs/roadmap.md`

Changes:

- Document `PANDAR_DATABASE_URL` and default SQLite behavior.
- Mention backend-specific migrations and optional PostgreSQL test env.
- Update roadmap Phase 1 completed items and next Phase 2 work.

Verification:

- Documentation matches implemented endpoints and database behavior.

## Subagent Execution

Use subagent-driven development for implementation:

1. Assign core domain changes to an implementation subagent.
2. Assign database/migration/repository changes to an implementation subagent.
3. Assign hub HTTP/API wiring and docs to an implementation subagent after repository contracts exist.
4. Main agent integrates results, resolves conflicts, and runs verification.
5. Independent final reviewer checks implementation against spec and plan and must return `VERDICT: APPROVE` before commit/push.

## Risks

- SQLx SQLite in-memory pools can accidentally use one empty database per connection. Mitigation: force single connection for `sqlite::memory:`.
- PostgreSQL tests can mutate a shared database. Mitigation: require `PANDAR_TEST_POSTGRES_URL` to point at a disposable test database and clear Phase 1 tables before optional PostgreSQL tests.
- API error body consistency can drift. Mitigation: HTTP tests assert exact error codes.
