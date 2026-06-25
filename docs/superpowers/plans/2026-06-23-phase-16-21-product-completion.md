# Phase 16-21 Product Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Phases 16-21 by replacing user-owned API tokens with tenant-scoped credentials, enforcing authenticated agent sessions, adding tenant-admin/recovery/ops/artifact UI, and scaffolding the Bambu Studio network plugin.

**Architecture:** Phase 16 becomes the credential foundation: tenant tokens, plugin tickets, and agent credentials are represented in hub persistence and funneled through one authorization principal. Later routes and frontend flows consume that principal rather than duplicating role logic. Phase 21 uses a C++17 shim linked into a Rust `cdylib` so Bambu Studio C++ ABI symbols are exported safely while hub behavior stays in Rust/HTTP.

**Tech Stack:** Rust 2024, Axum, SeaORM 2, SQLx migrations for SQLite/PostgreSQL, Tonic/prost, Next.js 16/React 19, Cargo build scripts, C++17 shim compiled by `cc`.

---

## Reviewed Inputs

- Spec: `docs/superpowers/specs/2026-06-23-phase-16-21-design.md`
- ABI symbols: `docs/superpowers/specs/2026-06-23-phase-21-network-plugin-abi-symbols.txt`
- Reference ABI: `reference/open-bamboo-networking/tests/probe_plugin.cpp`
- Existing protocol: `proto/pandar/agent/v1/agent.proto`

## Execution Rules

- Do not commit per task. SDD requires one final Lore-format commit after final implementation review, docs update, and fresh verification.
- Keep old user-owned API token bearer auth removed, not hidden behind compatibility.
- Add SQLite and PostgreSQL migrations together for every persistent change.
- Keep PostgreSQL tests optional behind `PANDAR_TEST_POSTGRES_URL`, but all SQLite tests must run locally.
- Every new database-dependent repository/query path needs SQLite coverage and PostgreSQL parity coverage behind `PANDAR_TEST_POSTGRES_URL`, including tenant tokens, agent credentials, plugin tickets/exchange, audit history listing, retry/reprint/duplicate job mutations, cleanup, and any new material/artifact persistence query.
- Preserve lower-level error causes with `{err:#}` / equivalent full-chain context.
- Do not add real pause/resume/stop machine commands in this package; render those controls as unavailable.

## File Structure

Create:

- `crates/pandar-hub/migrations/sqlite/20260623020000_phase_16_tenant_tokens_agent_credentials.sql`
- `crates/pandar-hub/migrations/postgres/20260623020000_phase_16_tenant_tokens_agent_credentials.sql`
- `crates/pandar-hub/src/entities/tenant_tokens.rs`
- `crates/pandar-hub/src/entities/plugin_login_tickets.rs`
- `crates/pandar-hub/src/repositories/auth/tenant_tokens.rs`
- `crates/pandar-hub/src/repositories/auth/plugin_tickets.rs`
- `crates/pandar-hub/src/repositories/auth/secrets.rs`
- `crates/pandar-hub/src/routes/tenant_tokens.rs`
- `crates/pandar-hub/src/routes/plugin.rs`
- `crates/pandar-hub/src/metrics.rs`
- `crates/pandar-hub/src/redaction.rs`
- `crates/pandar-app/src/cleanup.rs`
- `frontend/app/admin-panel.tsx`
- `frontend/app/recovery-actions.tsx`
- `frontend/app/plugin-sign-in/page.tsx`
- `crates/pandar-network-plugin/Cargo.toml`
- `crates/pandar-network-plugin/build.rs`
- `crates/pandar-network-plugin/src/lib.rs`
- `crates/pandar-network-plugin/src/shim.cpp`
- `crates/pandar-network-plugin/tests/abi_symbols.rs`
- `crates/pandar-hub/src/routes/tests/tenant_tokens.rs`
- `crates/pandar-hub/src/routes/tests/plugin.rs`
- `crates/pandar-hub/src/repositories/tests/tenant_tokens.rs`
- `crates/pandar-hub/src/grpc/tests/authentication.rs`

Modify:

- `Cargo.toml`
- `proto/pandar/agent/v1/agent.proto`
- `crates/pandar-agent/src/lib.rs`
- `crates/pandar-agent/src/commands.rs`
- `crates/pandar-core/src/agent.rs`
- `crates/pandar-hub/src/entities/mod.rs`
- `crates/pandar-hub/src/repositories/auth.rs`
- `crates/pandar-hub/src/repositories/agents.rs`
- `crates/pandar-hub/src/repositories/agents/pairing.rs`
- `crates/pandar-hub/src/repositories/audit.rs`
- `crates/pandar-hub/src/repositories/jobs.rs`
- `crates/pandar-hub/src/routes.rs`
- `crates/pandar-hub/src/routes/auth.rs`
- `crates/pandar-hub/src/routes/bootstrap.rs`
- `crates/pandar-hub/src/routes/provisioning.rs`
- `crates/pandar-hub/src/routes/provisioning/tokens.rs`
- `crates/pandar-hub/src/routes/provisioning/agents.rs`
- `crates/pandar-hub/src/routes/jobs.rs`
- `crates/pandar-hub/src/routes/printer_events.rs`
- `crates/pandar-hub/src/routes/tests.rs`
- `crates/pandar-hub/src/grpc.rs`
- `crates/pandar-hub/src/lib.rs`
- `crates/pandar-hub/src/runtime.rs`
- `crates/pandar-hub/src/sessions.rs`
- `crates/pandar-app/Cargo.toml`
- `crates/pandar-app/src/main.rs`
- `frontend/app/actions.ts`
- `frontend/app/command-result-parser.ts`
- `frontend/app/dashboard-runtime-sections.tsx`
- `frontend/app/dashboard-runtime.tsx`
- `frontend/app/dashboard-types.ts`
- `frontend/app/dispatch-form.tsx`
- `frontend/app/page.tsx`
- `README.md`
- `docs/development.md`
- `docs/architecture.md`
- `docs/roadmap.md`

## Task 1: Phase 16 Persistence And Domain Entities

**Files:**

- Create migrations and entities listed above.
- Modify `crates/pandar-hub/src/entities/mod.rs`, `crates/pandar-core/src/agent.rs`, `crates/pandar-hub/src/repositories/tests/mod.rs`.

- [x] **Step 1: Add migrations**

SQLite migration must add:

```sql
CREATE TABLE tenant_tokens (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    scopes_json TEXT NOT NULL,
    created_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    expires_at TEXT,
    revoked_at TEXT
);
CREATE INDEX idx_tenant_tokens_tenant_id ON tenant_tokens(tenant_id);
CREATE INDEX idx_tenant_tokens_hash ON tenant_tokens(token_hash);
CREATE INDEX idx_tenant_tokens_revoked_at ON tenant_tokens(revoked_at);

CREATE TABLE plugin_login_tickets (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    ticket_hash TEXT NOT NULL UNIQUE,
    redirect_url TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    revoked_at TEXT
);
CREATE INDEX idx_plugin_login_tickets_tenant_id ON plugin_login_tickets(tenant_id);
CREATE INDEX idx_plugin_login_tickets_hash ON plugin_login_tickets(ticket_hash);

ALTER TABLE agents ADD COLUMN credential_hash TEXT;
ALTER TABLE agents ADD COLUMN credential_rotated_at TEXT;
ALTER TABLE agents ADD COLUMN credential_revoked_at TEXT;
CREATE INDEX idx_agents_credential_hash ON agents(credential_hash);
```

PostgreSQL migration must use the same table/column names and externally visible behavior:

```sql
CREATE TABLE tenant_tokens (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    scopes_json TEXT NOT NULL,
    created_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    expires_at TEXT,
    revoked_at TEXT
);
CREATE INDEX idx_tenant_tokens_tenant_id ON tenant_tokens(tenant_id);
CREATE INDEX idx_tenant_tokens_hash ON tenant_tokens(token_hash);
CREATE INDEX idx_tenant_tokens_revoked_at ON tenant_tokens(revoked_at);

CREATE TABLE plugin_login_tickets (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    ticket_hash TEXT NOT NULL UNIQUE,
    redirect_url TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    revoked_at TEXT
);
CREATE INDEX idx_plugin_login_tickets_tenant_id ON plugin_login_tickets(tenant_id);
CREATE INDEX idx_plugin_login_tickets_hash ON plugin_login_tickets(ticket_hash);

ALTER TABLE agents ADD COLUMN credential_hash TEXT;
ALTER TABLE agents ADD COLUMN credential_rotated_at TEXT;
ALTER TABLE agents ADD COLUMN credential_revoked_at TEXT;
CREATE INDEX idx_agents_credential_hash ON agents(credential_hash);
```

- [x] **Step 2: Add SeaORM entities**

Add hand-written entities matching the style of `api_tokens.rs` and `agents.rs`. `tenant_tokens.scopes_json` remains a string at the entity layer; repository methods parse/serialize `Vec<String>`.

- [x] **Step 3: Extend core agent shape only where needed**

Keep `pandar_core::Agent` public response free of credential fields. If repository internals need credential columns, add a hub-local `AgentCredentialRecord` rather than exposing secrets in `pandar-core`.

- [x] **Step 4: Add repository tests for migration shape**

In `repositories/tests/tenant_tokens.rs`, create a tenant, insert token/ticket rows through repository methods added in Task 2, and assert migrated agents with null `credential_hash` cannot authenticate.

- [x] **Step 5: Run focused test**

Run:

```bash
cargo nextest run -p pandar-hub repositories::tests::tenant_tokens
```

Expected: new SQLite repository tests pass.

## Task 2: Tenant Token Repository, Authorization Principal, And Retired Routes

**Files:**

- Create `auth/tenant_tokens.rs`, `routes/tenant_tokens.rs`.
- Modify `auth.rs`, `routes/auth.rs`, `routes.rs`, `bootstrap.rs`, `provisioning/tokens.rs`, `audit.rs`.

- [x] **Step 1: Add token model and scope parser**

Implement `TenantToken`, `TenantTokenScope`, and `AuthenticatedPrincipal`:

```rust
pub enum AuthenticatedPrincipal {
    User(AuthenticatedUser),
    TenantToken(AuthenticatedTenantToken),
}

pub enum TenantTokenScope {
    All,
    AgentRegister,
    PluginStudio,
}
```

Unknown scope strings return `RepositoryError::InvalidTokenScope`.

- [x] **Step 2: Generate plaintext secrets**

Create `crates/pandar-hub/src/repositories/auth/secrets.rs` and use two UUIDv4 values per secret without adding a new randomness dependency:

```rust
fn generate_secret(prefix: &str) -> String {
    format!(
        "{prefix}{}_{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
```

Hash the full plaintext with existing SHA-256 helper.

This helper is the only plaintext secret generator for tenant tokens, plugin login tickets, plugin tenant tokens, and agent credentials. Prefixes are:

- `pandar_tenant_` for ordinary and rotated tenant tokens;
- `pandar_plugin_ticket_` for plugin login tickets;
- `pandar_plugin_` for plugin-session tenant tokens created by ticket exchange;
- `pandar_ac_` for agent credentials.

Route tests must assert the exact public prefixes returned by tenant-token creation/rotation/bootstrap, plugin ticket creation, and plugin ticket exchange.

- [x] **Step 3: Implement tenant token repository methods**

Required methods:

- `create_tenant_token_with_audit`
- `list_tenant_tokens`
- `authenticate_tenant_token`
- `revoke_tenant_token_with_audit`
- `rotate_tenant_token_with_audit`
- `create_plugin_token_from_ticket_tx`

Authentication updates `last_used_at` only for accepted non-expired, non-revoked tokens.

- [x] **Step 4: Replace route authorization**

Change `authorize_tenant` to return `AuthenticatedPrincipal`. Enforce the approved matrix:

- viewer reads: user viewer+ or empty token or `*`;
- operator mutations: user operator+ or `*`;
- tenant-admin APIs: user tenant_admin or `*`;
- agent registration: user tenant_admin, `*`, or `agent:register`;
- plugin routes: only `plugin:studio`.

External provider JWT auth remains through Phase 10 verifier; user-owned `api_tokens` are no longer checked.

- [x] **Step 5: Add tenant token routes**

Wire:

- `GET /api/v1/tenants/{tenant_id}/tenant-tokens`
- `POST /api/v1/tenants/{tenant_id}/tenant-tokens`
- `DELETE /api/v1/tenants/{tenant_id}/tenant-tokens/{token_id}`
- `POST /api/v1/tenants/{tenant_id}/tenant-tokens/{token_id}/rotate`

Responses follow the spec exactly.

- [x] **Step 6: Retire old routes**

Current routes to retire:

- `GET /api/v1/tenants/{tenant_id}/users/{user_id}/api-tokens`
- `POST /api/v1/tenants/{tenant_id}/users/{user_id}/api-tokens`
- `DELETE /api/v1/tenants/{tenant_id}/api-tokens/{token_id}`

These retired routes bypass normal authorization entirely and always return:

```json
{ "error": "api_tokens_retired" }
```

with status `410 Gone`, regardless of bearer principal, missing bearer, malformed bearer, or tenant/user/token ids.

- [x] **Step 7: Migrate route test authentication harness**

Update `crates/pandar-hub/src/routes/tests.rs`:

- replace `auth_token_for_role` internals so role-bearing route tests authenticate with an external-user bearer or a tenant token, not `api_tokens`;
- add tenant-token helpers for empty-scope read-only tokens, `*`, `agent:register`, and `plugin:studio`;
- delete or invert `api_token_auth_still_succeeds_when_external_auth_is_configured` so old user-owned API tokens are asserted rejected/retired;
- update existing route tests that call `create_api_token` only for authentication so they use the new helpers.

- [x] **Step 8: Update bootstrap**

Bootstrap tenant-admin creation returns `tenant_token` + plaintext `token` with `["*"]` scope, not user-owned `api_token`.

- [x] **Step 9: Add route and repository tests**

Cover old token rejection, empty read-only token, `*`, `agent:register`, expired token, revoked token, rotation, and last-used update.

- [x] **Step 10: Run focused tests**

Run:

```bash
cargo nextest run -p pandar-hub routes::tests::tenant_tokens repositories::tests::tenant_tokens routes::tests::bootstrap
```

Expected: all focused auth/bootstrap tests pass.

## Task 3: Agent Credentials And gRPC Authentication

**Files:**

- Modify proto, agent config, hub gRPC service, agent repository, pairing route, tests.

- [x] **Step 1: Update proto**

Change `AgentHello`:

```proto
message AgentHello {
  string name = 1;
  string version = 2;
  string credential = 3;
}
```

- [x] **Step 2: Update agent CLI/config**

Add required:

```rust
#[arg(long, env = "PANDAR_AGENT_CREDENTIAL")]
pub agent_credential: String,
```

`hello_event` includes the credential. Other events do not.

- [x] **Step 3: Create and rotate agent credentials**

Pairing route returns:

```text
PANDAR_TENANT_ID=...
PANDAR_AGENT_ID=...
PANDAR_AGENT_NAME=...
PANDAR_AGENT_CREDENTIAL=...
```

Add:

- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/credential:rotate`
- `POST /api/v1/tenants/{tenant_id}/agents/{agent_id}/credential:revoke`

Rotate sets new hash and clears `credential_revoked_at`. Revoke sets `credential_revoked_at` and closes current session if present.

Use `auth::secrets::generate_secret("pandar_ac_")` for plaintext credentials and store only SHA-256 hashes.

- [x] **Step 4: Enforce gRPC hello authentication**

In `connect_stream`, after loading the agent by id:

- reject missing agent with `not_found`;
- reject tenant mismatch with `permission_denied`;
- reject missing/null/revoked/wrong credential with `unauthenticated`;
- register session only after successful credential match.

Mismatched later event ids stay bound to the authenticated session and must not mutate other agents.

- [x] **Step 5: Update shared gRPC test fixtures**

Update `crates/pandar-hub/src/grpc/tests/mod.rs` so all existing gRPC tests inherit authenticated hello behavior:

- `tenant_agent` persists a non-null `credential_hash`;
- `hello_event` includes the matching plaintext credential;
- `connect_live` callers continue using the helper without per-test credential setup;
- authentication-specific tests can pass an explicit wrong/missing/revoked credential.

This fixture change must keep lifecycle, commands, print_jobs, print_reports, and printer_snapshots tests passing.

- [x] **Step 6: Add tests**

Add gRPC tests for missing credential, wrong credential, null migrated credential, rotated credential, revoked credential, tenant mismatch, and stale replacement behavior.

- [x] **Step 7: Run focused tests**

Run:

```bash
cargo nextest run -p pandar-hub grpc::tests routes::tests::provisioning::agents
cargo test -p pandar-agent parses_agent_cli_config
```

Expected: all focused tests pass.

## Task 4: Plugin Tickets, Plugin Routes, And Audit History

**Files:**

- Create `routes/plugin.rs`, `auth/plugin_tickets.rs`.
- Modify `routes.rs`, `jobs.rs`, `audit.rs`, route tests.

- [x] **Step 1: Add plugin ticket repository methods**

Methods:

- `create_plugin_login_ticket_with_audit`
- `exchange_plugin_login_ticket`
- `validate_plugin_redirect_url`

Use `auth::secrets::generate_secret("pandar_plugin_ticket_")` for plugin ticket plaintext values. Plugin `["plugin:studio"]` tenant tokens created during ticket exchange use `auth::secrets::generate_secret("pandar_plugin_")`.

Redirect validation:

- scheme `http`;
- host exactly `localhost`, `127.0.0.1`, or `[::1]`;
- port present and `1..=65535`;
- no username/password/fragment;
- path/query preserved.

- [x] **Step 2: Add plugin ticket routes**

Wire:

- `POST /api/v1/tenants/{tenant_id}/plugin/login-tickets`
- `POST /api/v1/plugin/login-tickets/exchange`

Login-ticket creation auth is viewer+ external user or `*` tenant token. Empty-scope tenant tokens, `agent:register` tokens, and `plugin:studio` tokens are denied. Ticket exchange is unauthenticated except for validating the one-use ticket.

Exchange creates a `["plugin:studio"]` token with 30-day expiration in the same transaction that marks the ticket used.

- [x] **Step 3: Add plugin wrapper routes**

Wire:

- `GET /api/v1/plugin/printers`
- `GET /api/v1/plugin/jobs`
- `POST /api/v1/plugin/prints`

Only `plugin:studio` tokens can use these routes. Responses match `PluginPrinter`, `PluginJob`, and print result shapes in the spec.

- [x] **Step 4: Add audit-history route**

Wire:

- `GET /api/v1/tenants/{tenant_id}/audit-events`

Support `limit`, `before`, and `action`. Sort newest first. Parse metadata JSON into an object and redact forbidden keys. If persisted `metadata_json` is invalid, return `{}` for that event metadata and log the parse error with full context.

- [x] **Step 5: Standardize audit actors**

Use:

- `actor_type = "user"` for external users;
- `actor_type = "tenant_token"` for tenant-token actions;
- `actor_type = "plugin_token"` for plugin-token print actions.

Metadata includes token id and scopes, never plaintext or hashes.

- [x] **Step 6: Add tests**

Cover ticket create/exchange/replay/expiry, redirect validation, plugin route auth, plugin print response shape, audit listing auth/pagination/redaction, invalid persisted audit metadata fallback/logging, token actor metadata, and SQLite/PostgreSQL parity for plugin tickets/exchange and audit history.

Do not assert plugin print stable artifact-validation codes in Task 4; those are implemented and tested in Task 5 after the shared validation helper is aligned.

- [x] **Step 7: Run focused tests**

Run:

```bash
cargo nextest run -p pandar-hub routes::tests::plugin routes::tests::tenant_tokens routes::tests::provisioning
```

Expected: all focused tests pass.

## Task 5: Recovery, Duplicate, Reprint, And Artifact UX Backend

**Files:**

- Modify `repositories/jobs.rs`, `routes/jobs.rs`, jobs tests.

- [x] **Step 1: Add repository operations**

Add:

- `retry_dispatch_with_audit`
- `reprint_with_audit`
- `duplicate_and_print_with_audit`
- `validate_artifact_submission`

`retry_dispatch` eligible predicate:

```text
job.status == failed
command.status == failed
print.status == pending
print.started_at == null
progress_percent == null || 0
current_layer == null || 0
```

Reject all other states with `RepositoryError::RetryNotSafe`.

- [x] **Step 2: Add route endpoints**

Wire:

- `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/retry-dispatch`
- `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/reprint`
- `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/duplicate`

Use operator auth. Audit actions: `job.retry_dispatch`, `job.reprint`, `job.duplicate`.

`duplicate_and_print_with_audit` must accept this request schema:

```rust
pub struct DuplicateJobRequest {
    pub printer_id: Option<String>,
    pub plate_id: Option<i32>,
    pub use_ams: Option<bool>,
    pub flow_cali: Option<bool>,
    pub timelapse: Option<bool>,
    pub ams_mapping: Option<serde_json::Value>,
    pub ams_mapping2: Option<serde_json::Value>,
}
```

It must allow a currently running or otherwise non-terminal source job when it has an artifact. It creates an independent queued follow-up and does not mutate the source job. It copies source printer/settings/artifact values and overrides only fields that are `Some(_)`; `None` means "copy source value". `reprint_with_audit` remains restricted to terminal physical print states.

- [x] **Step 3: Ensure artifact path trust boundary**

Reprint/duplicate must load stored artifact metadata by id and never accept browser-supplied storage paths.

- [x] **Step 4: Add artifact validation**

Centralize validation for ordinary print submission, Phase 18 duplicate/reprint, and plugin print delegation:

- `400 artifact_empty` when decoded artifact bytes are empty.
- `400 artifact_invalid_base64` when submitted artifact payload is not valid base64.
- `400 artifact_invalid_plate` when `plate_id` is negative or cannot be represented by the command model.
- `413 artifact_too_large` when decoded bytes exceed `PANDAR_MAX_ARTIFACT_BYTES`.
- `404 printer_not_found` when the selected printer does not belong to the tenant or does not exist.

The existing print submission route and plugin `POST /api/v1/plugin/prints` must return these stable codes. Reprint/duplicate must not accept browser-supplied artifact bytes or storage paths; they validate source artifact existence and any override printer/plate fields.

- [x] **Step 5: Add timeout/stale-agent messaging data**

Expose enough command/job state for the frontend to distinguish:

- hub enqueue failure;
- agent offline before dispatch;
- file transfer failure;
- MQTT publish failure;
- physical print started/running/terminal state.

Keep this as structured status/detail fields already present in command result JSON where possible. Update `frontend/app/command-result-parser.ts` and dashboard rendering to map those states into operator-visible labels without adding new machine-control commands.

- [x] **Step 6: Add tests**

Test safe retry, unsafe retry after physical start, reprint terminal states, duplicate nullable override behavior, duplicate source-job no-mutation for running/non-terminal jobs, stable artifact validation errors on ordinary print submission, plugin print validation delegation, audit actions, tenant isolation, and SQLite/PostgreSQL parity for retry/reprint/duplicate job mutations.

- [x] **Step 7: Run focused tests**

Run:

```bash
cargo nextest run -p pandar-hub routes::tests::jobs repositories::tests::jobs
```

Expected: all job route/repository tests pass.

## Task 6: Operational Readiness, Metrics, Redaction, And Cleanup CLI

**Files:**

- Create `metrics.rs`, `redaction.rs`, `pandar-app/src/cleanup.rs`.
- Modify `routes.rs`, `runtime.rs`, `printer_events.rs`, `sessions.rs`, `pandar-app` manifest/main.

- [x] **Step 1: Add readiness route**

`GET /readyz` returns JSON checks for database, gRPC listener config, spool directory, and external auth config. It sets a non-2xx status when any check fails.

External auth disabled is ready, not failing: the `external_auth` JSON detail is `"disabled"` and `pandar_readyz{check="external_auth"}` is `1`. Invalid configured external auth returns not-ready and sets the metric value to `0`.

- [x] **Step 2: Add metrics route**

`GET /metrics` returns Prometheus text with exactly:

- `pandar_agent_sessions{state}`
- `pandar_commands_total{kind,status}`
- `pandar_websocket_subscriptions{tenant_id_hash}`
- `pandar_websocket_tickets_total{result}`
- `pandar_jobs_total{status,print_status}`
- `pandar_print_reports_total{result}`
- `pandar_readyz{check}`

`tenant_id_hash` is first 16 lowercase hex chars of SHA-256 over tenant id.

- [x] **Step 3: Track WebSocket ticket/subscription counts**

Store counters in a `MetricsState` field on `AppState`. Increment `MetricsState` from `PrinterEventHub`, websocket ticket creation/validation, command dispatch/report handling, and readiness checks. Do not expose raw tenant ids.

- [x] **Step 4: Add redaction helper and tests**

Add redaction for bearer tokens, WebSocket tickets, Bambu access codes, agent credentials, plugin tickets, and artifact paths. Use it in new logs/routes where user-controlled secret-like values may appear.

- [x] **Step 5: Add cleanup CLI**

`pandar cleanup --dry-run` default, `pandar cleanup --execute` mutating. Reads `PANDAR_DATABASE_URL`, artifact root, and these retention env vars/defaults:

- `PANDAR_RETENTION_COMPLETED_JOBS_DAYS=90`
- `PANDAR_RETENTION_COMMANDS_DAYS=90`
- `PANDAR_RETENTION_MACHINE_EVENTS_DAYS=30`
- `PANDAR_RETENTION_AUDIT_DAYS=365`
- `PANDAR_RETENTION_EXPIRED_TICKETS_DAYS=7`
- `PANDAR_RETENTION_REVOKED_TOKENS_DAYS=365`

Dry run prints selected counts/bytes. Execute deletes database rows by category transaction and removes unreferenced artifact files after commit.

The CLI emits structured logs only; do not write audit events from the CLI.

Implement the spec cleanup selection rules exactly:

- Jobs: terminal jobs older than `PANDAR_RETENTION_COMPLETED_JOBS_DAYS`; terminal means dispatch `succeeded` or `failed` and physical print `completed`, `failed`, `cancelled`, or `pending` with no queued/sent/acknowledged command. Exclude queued/sent/acknowledged commands and running physical prints.
- Artifacts: delete rows/files only after selected job deletion and only when no remaining job references the artifact id; delete files after transaction commit.
- Commands: terminal commands older than `PANDAR_RETENTION_COMMANDS_DAYS` only when no retained job references them.
- Machine events: rows older than `PANDAR_RETENTION_MACHINE_EVENTS_DAYS`.
- Audit events: rows older than `PANDAR_RETENTION_AUDIT_DAYS`, preserving newer directly-related retained records where relation data exists.
- Plugin login tickets: used, revoked, or expired tickets older than `PANDAR_RETENTION_EXPIRED_TICKETS_DAYS`.
- Tenant tokens: revoked or expired rows older than `PANDAR_RETENTION_REVOKED_TOKENS_DAYS`; never active unexpired tokens.

SQLite and PostgreSQL implementations run database deletions in one transaction per cleanup category. Dry-run uses the same selection queries without mutation. SQLite cleanup tests always run; PostgreSQL cleanup parity tests run when `PANDAR_TEST_POSTGRES_URL` is configured.

- [x] **Step 6: Add tests**

Cover readyz success and forced database/spool failure, metrics label redaction, cleanup dry-run no mutation, cleanup execute exclusions for active jobs/commands, and redaction patterns.

- [x] **Step 7: Run focused tests**

Run:

```bash
cargo nextest run -p pandar-hub routes::tests::printer_events_ws routes::tests::agents repositories::tests::jobs
cargo test -p pandar-app
```

Expected: focused ops/CLI tests pass.

Completed verification:

```bash
cargo nextest run -p pandar-hub routes::tests::readiness_metrics redaction::tests::redacts_tokens_credentials_and_artifact_paths repositories::tests::cleanup repositories::tests::postgres::postgres_cleanup_when_configured
cargo check -p pandar-hub -p pandar-app
cargo clippy -p pandar-hub --tests --no-deps
cargo test -p pandar-app
```

## Task 7: Frontend Tenant Admin, Recovery, Artifact, And Plugin Sign-In UI

**Files:**

- Create `admin-panel.tsx`, `recovery-actions.tsx`, `plugin-sign-in/page.tsx`.
- Modify `actions.ts`, `dashboard-runtime*.tsx`, `dashboard-types.ts`, `dispatch-form.tsx`, `page.tsx`.

- [x] **Step 1: Extend server actions**

Add actions for:

- tenant token create/revoke/rotate;
- user create/role update;
- identity link;
- agent pairing;
- refresh;
- retry/reprint/duplicate;
- plugin ticket create.

Do not store plaintext secrets in URLs, cookies, local storage, or Zustand.

- [x] **Step 2: Add tenant admin panel**

Render users, identity links, tenant tokens, agent pairing, and audit events. If API returns forbidden, show a compact unavailable state rather than duplicating policy.

- [x] **Step 3: Add recovery controls**

Add manual refresh, retry dispatch, reprint, duplicate-and-print, and unavailable pause/resume/stop indicators. Preserve dispatch vs physical print wording.

- [x] **Step 4: Improve artifact upload UX**

Show selected filename/size, conversion progress, max size, and stable backend error codes. Keep base64 conversion browser-side only for form submission.

Use `PANDAR_MAX_ARTIFACT_BYTES` default `268435456` bytes as the displayed configured limit when the server exposes it; otherwise show the default. The backend error labels surfaced by the UI are `artifact_empty`, `artifact_invalid_base64`, `artifact_invalid_plate`, `artifact_too_large`, and `printer_not_found`.

- [x] **Step 5: Add plugin sign-in page**

If external auth is disabled or tenant is not selected, show configuration/selection error. Otherwise create a plugin ticket and redirect to validated local callback.

- [x] **Step 6: Verify frontend**

Run:

```bash
npm --prefix frontend run build
```

Expected: Next.js build succeeds.

Completed verification:

```bash
npm --prefix frontend run build
```

Result: passed.

## Task 8: Phase 21 Network Plugin Crate

**Files:**

- Create `crates/pandar-network-plugin/*`.
- Modify workspace `Cargo.toml`.

- [x] **Step 1: Add workspace member and manifest**

Add `crates/pandar-network-plugin` with crate type:

```toml
[lib]
crate-type = ["cdylib"]
```

Use a build script and `cc` build-dependency for `src/shim.cpp`.

- [x] **Step 2: Add C++ shim**

`shim.cpp` exports every symbol from `docs/superpowers/specs/2026-06-23-phase-21-network-plugin-abi-symbols.txt`. Copy the ABI-compatible C++ signatures from `reference/open-bamboo-networking/src/abi_*.cpp` into the checked-in shim or a checked-in ABI probe harness; do not reduce signatures to `extern "C"` Rust guesses. The local symbol file is the normative export list, and the reference source is the normative signature source.

C++ owns `std::string` returns. Rust boundary uses C structs/pointers only.

- [x] **Step 3: Add Rust hub client boundary**

Keep Phase 21 minimal: environment/config value for frontend/hub URL, ticket exchange, profile storage, plugin printers/jobs/prints HTTP calls. Preserve error chains in Rust logs.

- [x] **Step 4: Implement behavior classes**

Follow the ABI behavior table in the spec. Unsupported paths return stable no-op/unsupported values without opening LAN sockets.

- [x] **Step 5: Add ABI symbol test**

Build the cdylib and assert all symbol names from the local symbol file are exported. Implement the test as Rust code that locates the built dynamic library, runs `nm -g` on Unix targets, runs `dumpbin /exports` on Windows targets, and fails with a clear message if the platform export tool is missing. Add a checked-in C++ ABI probe derived from `reference/open-bamboo-networking/tests/probe_plugin.cpp` when symbol export tests alone cannot verify signature compatibility.

- [x] **Step 6: Run focused plugin verification**

Run:

```bash
cargo test -p pandar-network-plugin
cargo build -p pandar-network-plugin
```

Expected: plugin tests pass and cdylib builds.

Completed verification:

```bash
cargo test -p pandar-network-plugin
cargo build -p pandar-network-plugin
```

Result: passed.

## Task 9: Documentation, Roadmap, And Final Verification

**Files:**

- Modify `README.md`, `docs/development.md`, `docs/architecture.md`, `docs/roadmap.md`.

- [x] **Step 1: Update README and development docs**

Replace user API token examples with tenant-token examples. Document:

- `PANDAR_AGENT_CREDENTIAL`;
- tenant token scopes;
- plugin login-ticket flow;
- cleanup CLI;
- readyz/metrics;
- backup/restore commands for SQLite/PostgreSQL.
- Phase 21 network plugin replacement/install paths for Linux, Windows, and macOS. Document the platform-specific Bambu Studio plugin-library location or replacement workflow, mark packaging/signing as optional/not completed in this package, and state that Studio compatibility still requires real Studio testing.

- [x] **Step 2: Update architecture**

Move Phase 16-21 sections from planned to implemented language and document limitations: no slicer parser, pause/resume/stop unavailable, plugin scaffold not packaged.

- [x] **Step 3: Update roadmap**

Add Phases 16-21 completion summary and adjust Immediate Next to post-plugin hardening and real Studio compatibility testing.

- [x] **Step 4: Run full verification**

Run:

```bash
cargo fmt
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
npm --prefix frontend run build
```

Expected: all commands exit 0.

Completed verification:

```bash
cargo fmt
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
npm --prefix frontend run build
cargo test -p pandar-network-plugin
cargo build -p pandar-network-plugin
```

Result: passed before final review. After review fixes for plugin hub-backed calls, portable build-script flags, frontend copy-once secret display, external-auth plugin sign-in gating, and `PluginHttpResult` capacity-safe deallocation, reran:

```bash
npm --prefix frontend run build
cargo test -p pandar-network-plugin
cargo build -p pandar-network-plugin
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
cargo fmt
git diff --check
```

Result: passed.

- [x] **Step 5: Inspect diff**

Run:

```bash
git status --short
git diff --stat
```

Expected: only Phase 16-21 source/docs/spec/plan files changed.

- [x] **Step 6: Final SDD implementation review, commit, and push**

Final SDD implementation review approved after the FFI deallocation blocker was fixed. This checkpoint is closed by the Lore-format commit and branch push.
