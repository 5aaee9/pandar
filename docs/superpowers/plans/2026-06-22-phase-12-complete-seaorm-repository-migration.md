# Phase 12 Complete SeaORM Repository Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the hub repository migration to SeaORM 2.0 while preserving SQLite/PostgreSQL behavior and documenting any narrow SQLx adapters.

**Architecture:** Keep SQLx as the connection and migration layer, and use the existing `Database::sea_orm_connection()` bridge for repository queries and transactions. Add hand-written SeaORM entities for every persistent table, then migrate repositories in dependency order so transaction-coupled behavior can be verified before moving to the next milestone. Raw SQL remains only in migrations, backend setup test fixtures, or small documented backend-neutral adapter functions when SeaORM cannot preserve existing behavior cleanly.

**Tech Stack:** Rust 2024, SeaORM 2.0.0-rc.41, SQLx migrations, SQLite, PostgreSQL, Tokio, Axum, Tonic.

---

## SDD Constraints

- Stay on `main`; do not create a branch.
- Do not commit per task. Phase 12 gets one final Lore-format commit after final SDD implementation review, docs, and verification pass.
- Preserve the current public REST, WebSocket, gRPC, protobuf, and frontend behavior.
- Preserve SQLite/PostgreSQL parity for every migrated repository behavior.
- Preserve existing mapped `RepositoryError` variants, and for unmapped database failures retain the underlying SeaORM `DbErr` or SQLx cause through `anyhow` context.
- Keep protobuf and SeaORM generated outputs out of git.
- If a planned SeaORM operation cannot preserve current behavior, isolate the raw SQL in `crates/pandar-hub/src/repositories/adapters/`, document the reason in the adapter comment, and keep a backend-neutral public function.

## File Structure

- Create `crates/pandar-hub/src/entities/users.rs`: SeaORM entity for the `users` table.
- Create `crates/pandar-hub/src/entities/agents.rs`: SeaORM entity for the `agents` table.
- Create `crates/pandar-hub/src/entities/printers.rs`: SeaORM entity for the `printers` table.
- Create `crates/pandar-hub/src/entities/commands.rs`: SeaORM entity for the `commands` table.
- Create `crates/pandar-hub/src/entities/api_tokens.rs`: SeaORM entity for the `api_tokens` table, including `revoked_at`.
- Create `crates/pandar-hub/src/entities/audit_events.rs`: SeaORM entity for the `audit_events` table.
- Create `crates/pandar-hub/src/entities/user_identities.rs`: SeaORM entity for the `user_identities` table.
- Create `crates/pandar-hub/src/entities/job_artifacts.rs`: SeaORM entity for the `job_artifacts` table.
- Create `crates/pandar-hub/src/entities/jobs.rs`: SeaORM entity for the `jobs` table, including Phase 9 print-state columns.
- Create `crates/pandar-hub/src/entities/machine_events.rs`: SeaORM entity for the `machine_events` table.
- Modify `crates/pandar-hub/src/entities/mod.rs`: export all new entities.
- Modify `crates/pandar-hub/src/repositories/mod.rs`: replace SQLx-only constraint helpers with SeaORM-aware helpers while keeping SQLx fixture helpers for tests.
- Modify `crates/pandar-hub/src/repositories/audit.rs`: migrate audit insert/list to SeaORM and expose transaction helper functions for other repositories.
- Modify `crates/pandar-hub/src/repositories/auth.rs` and `crates/pandar-hub/src/repositories/auth/**/*.rs`: migrate users, tokens, identities, bootstrap, and provisioning helpers.
- Modify `crates/pandar-hub/src/repositories/agents.rs` and `crates/pandar-hub/src/repositories/agents/pairing.rs`: migrate agent CRUD, connection updates, and audit-coupled pairing helpers.
- Modify `crates/pandar-hub/src/repositories/printers.rs`: migrate list/get/count/ownership and upsert snapshot, using an adapter only if SeaORM upsert cannot preserve current `(tenant_id, serial_number)` semantics.
- Modify `crates/pandar-hub/src/repositories/commands.rs` and `crates/pandar-hub/src/repositories/commands/**/*.rs`: migrate command rows, inserts, queue lookup, ownership, guarded transitions, and audit-coupled enqueue.
- Modify `crates/pandar-hub/src/repositories/jobs.rs` and `crates/pandar-hub/src/repositories/jobs/**/*.rs`: migrate job artifacts, jobs, listing/detail, command/job transitions, print report reconciliation, and machine event dedupe.
- Optionally create `crates/pandar-hub/src/repositories/adapters/mod.rs` plus narrowly scoped adapter modules only when a raw SQL escape hatch is used.
- Modify `docs/architecture.md`: document the final persistence boundary after implementation.
- Modify `docs/roadmap.md`: mark Phase 12 complete after final implementation review.

## Task 1: Entity And Mapping Foundation

**Files:**
- Create: `crates/pandar-hub/src/entities/users.rs`
- Create: `crates/pandar-hub/src/entities/agents.rs`
- Create: `crates/pandar-hub/src/entities/printers.rs`
- Create: `crates/pandar-hub/src/entities/commands.rs`
- Create: `crates/pandar-hub/src/entities/api_tokens.rs`
- Create: `crates/pandar-hub/src/entities/audit_events.rs`
- Create: `crates/pandar-hub/src/entities/user_identities.rs`
- Create: `crates/pandar-hub/src/entities/job_artifacts.rs`
- Create: `crates/pandar-hub/src/entities/jobs.rs`
- Create: `crates/pandar-hub/src/entities/machine_events.rs`
- Modify: `crates/pandar-hub/src/entities/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`

- [x] **Step 1: Add one hand-written SeaORM entity per persistent table**

  Use the existing `crates/pandar-hub/src/entities/tenants.rs` style. Each entity must use the real table name and all persisted columns from migrations:

  - `users`: `id`, `tenant_id`, `email`, `display_name`, `role`, `created_at`
  - `agents`: `id`, `tenant_id`, `name`, `status`, `version`, `last_seen_at`, `created_at`
  - `printers`: `id`, `tenant_id`, `agent_id`, `serial_number`, `name`, `model`, `status`, `last_seen_at`, `created_at`
  - `commands`: `id`, `tenant_id`, `agent_id`, `printer_id`, `kind`, `status`, `payload_json`, `error`, `created_at`, `updated_at`
  - `api_tokens`: `id`, `tenant_id`, `user_id`, `name`, `token_hash`, `created_at`, `last_used_at`, `revoked_at`
  - `audit_events`: `id`, `tenant_id`, `actor_type`, `user_id`, `action`, `target_type`, `target_id`, `metadata_json`, `created_at`
  - `user_identities`: `id`, `tenant_id`, `user_id`, `provider`, `subject`, `created_at`
  - `job_artifacts`: `id`, `tenant_id`, `filename`, `content_type`, `size_bytes`, `storage_path`, `created_at`
  - `jobs`: `id`, `tenant_id`, `printer_id`, `agent_id`, `artifact_id`, `command_id`, `status`, `error`, `created_at`, `updated_at`, `print_status`, `printer_state`, `progress_percent`, `remaining_time_minutes`, `current_layer`, `total_layers`, `active_file`, `last_progress_percent`, `last_layer`, `print_error`, `print_started_at`, `print_finished_at`, `print_updated_at`
  - `machine_events`: `id`, `tenant_id`, `agent_id`, `printer_id`, `job_id`, `event_key`, `kind`, `severity`, `message`, `code`, `payload_json`, `observed_at`, `created_at`

- [x] **Step 2: Export entities**

  Update `crates/pandar-hub/src/entities/mod.rs` so each new entity module is public:

  ```rust
  pub mod agents;
  pub mod api_tokens;
  pub mod audit_events;
  pub mod commands;
  pub mod job_artifacts;
  pub mod jobs;
  pub mod machine_events;
  pub mod printers;
  pub mod tenants;
  pub mod user_identities;
  pub mod users;
  ```

- [x] **Step 3: Add SeaORM-aware repository helpers**

  In `crates/pandar-hub/src/repositories/mod.rs`, add helpers that can be reused by migrated repositories:

  - `fn is_sea_orm_unique_violation(err: &sea_orm::DbErr, sqlite_name: &str, postgres_name: &str) -> bool`
  - `fn is_sea_orm_foreign_key_violation(err: &sea_orm::DbErr) -> bool`

  Implement them with `err.sql_err()` for structured SeaORM SQL errors and message fallback for current SQLite/PostgreSQL constraint names. Do not remove SQLx helpers until all non-test SQLx callers are gone.

  When a database failure is not intentionally mapped to a specific `RepositoryError` variant, wrap the original `DbErr` or SQLx error with `anyhow::Error::new(err).context("specific operation context")` so the lower-level cause chain is still available to logs and callers.

- [x] **Step 4: Verify foundation**

  Run:

  ```bash
  cargo fmt
  cargo check -p pandar-hub
  ```

  Expected: both commands pass. If `cargo check` exposes entity type mismatches, fix the entity definitions before moving on.

## Task 2: Auth, Audit, Agent, And Printer Repositories

**Files:**
- Modify: `crates/pandar-hub/src/repositories/audit.rs`
- Modify: `crates/pandar-hub/src/repositories/auth.rs`
- Modify: `crates/pandar-hub/src/repositories/auth/bootstrap.rs`
- Modify: `crates/pandar-hub/src/repositories/auth/users.rs`
- Modify: `crates/pandar-hub/src/repositories/auth/users/provisioning.rs`
- Modify: `crates/pandar-hub/src/repositories/auth/tokens.rs`
- Modify: `crates/pandar-hub/src/repositories/auth/tokens/provisioning.rs`
- Modify: `crates/pandar-hub/src/repositories/auth/identities.rs`
- Modify: `crates/pandar-hub/src/repositories/auth/identities/provisioning.rs`
- Modify: `crates/pandar-hub/src/repositories/agents.rs`
- Modify: `crates/pandar-hub/src/repositories/agents/pairing.rs`
- Modify: `crates/pandar-hub/src/repositories/printers.rs`
- Optionally create: `crates/pandar-hub/src/repositories/adapters/printers.rs`

- [x] **Step 1: Migrate audit insert and list**

  Replace direct SQLx queries in `AuditEventRepository` with SeaORM `ActiveModel::insert`, `Entity::find`, filters, and ordering. Keep the public `AuditEvent` and `RecordAuditEvent` structs unchanged. Add a transaction helper that accepts `&sea_orm::DatabaseTransaction` for audit-coupled repository operations.

- [x] **Step 2: Migrate tenant-admin bootstrap, users, and provisioning user helpers**

  Replace SQLx tenant-admin bootstrap and user insert/list/update/select paths with SeaORM. Use `Database::sea_orm_connection().begin().await` for bootstrap/provisioning transactions and commit only after every row in the intended mutation group succeeds.

  For the Phase 11 tenant-admin bootstrap helper, the transaction boundary must include:

  - tenant row creation,
  - admin user row creation,
  - first API token row creation,
  - all bootstrap audit event rows.

  Preserve these error mappings:

  - duplicate tenant slug -> `RepositoryError::DuplicateTenantSlug`
  - duplicate user email -> `RepositoryError::DuplicateUserEmail`
  - missing user -> `RepositoryError::MissingUser`
  - invalid persisted role -> `RepositoryError::InvalidPersistedUserRole`
  - foreign-key failures during bootstrap/provisioning -> existing missing tenant/user variants

- [x] **Step 3: Migrate API tokens**

  Replace SQLx token create/list/revoke/authenticate paths with SeaORM. Preserve:

  - duplicate `(tenant_id, name)` -> `DuplicateApiTokenName`
  - duplicate `token_hash` -> `DuplicateApiTokenHash`
  - revoked tokens do not authenticate
  - `last_used_at` is updated only for accepted non-revoked tokens

- [x] **Step 4: Migrate external identities**

  Replace SQLx identity link/list/authenticate paths with SeaORM. Preserve:

  - duplicate `(tenant_id, provider, subject)` -> `DuplicateExternalIdentity`
  - duplicate `(tenant_id, user_id, provider)` -> `DuplicateUserExternalIdentity`
  - missing linked user -> `MissingUser`
  - authentication still resolves the local tenant-scoped Pandar user and role

- [x] **Step 5: Migrate agents and pairing bundles**

  Replace direct SQLx in `AgentRepository` and `agents/pairing.rs` with SeaORM inserts, filters, updates, counts, and transactions. Preserve duplicate agent name mapping, offline marking, connection update behavior, and audit-coupled agent pairing atomicity.

- [x] **Step 6: Migrate printers**

  Replace direct SQLx list/get/count/ownership checks with SeaORM. Implement snapshot upsert through SeaORM if the current SeaORM version can preserve insert-or-update by `(tenant_id, serial_number)` across SQLite/PostgreSQL. If not, isolate the raw SQL in `crates/pandar-hub/src/repositories/adapters/printers.rs` behind one backend-neutral function and document the reason in the module comment.

- [x] **Step 7: Verify migrated auth/audit/agent/printer behavior**

  Run:

  ```bash
  cargo fmt
  cargo test -p pandar-hub repositories::tests::auth -- --nocapture
  cargo test -p pandar-hub repositories::tests::phase1 -- --nocapture
  cargo test -p pandar-hub repositories::tests::printers -- --nocapture
  cargo test -p pandar-hub agents -- --nocapture
  ```

  Expected: all targeted tests pass. If optional PostgreSQL is configured, also run `cargo test -p pandar-hub repositories::tests::postgres -- --nocapture`.

## Task 3: Command Repository Migration

**Files:**
- Modify: `crates/pandar-hub/src/repositories/commands.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/audit.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/inserts.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/ownership.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/rows.rs`
- Modify: `crates/pandar-hub/src/repositories/commands/transitions.rs`
- Optionally create: `crates/pandar-hub/src/repositories/adapters/commands.rs`

- [x] **Step 1: Migrate command row mapping**

  Replace SQLx row structs and `FromRow` mapping with conversion from `entities::commands::Model` to the existing domain command types. Preserve invalid status handling through `RepositoryError::InvalidPersistedCommandStatus`.

- [x] **Step 2: Migrate inserts and queue lookup**

  Use SeaORM `ActiveModel::insert` for enqueue operations and `Entity::find` with `Column::TenantId`, `Column::AgentId`, `Column::Status`, and `Column::CreatedAt` filters/order for dispatch queue lookup. Preserve the existing oldest-queued-first behavior.

- [x] **Step 3: Migrate ownership checks**

  Use SeaORM count/find queries for command tenant/agent ownership checks. Preserve `RepositoryError::MissingCommand` for missing commands and `RepositoryError::CommandOwnershipMismatch` for tenant/agent mismatches.

- [x] **Step 4: Migrate guarded transitions**

  Use SeaORM `Entity::update_many()` with status filters so transition guards still depend on affected row count. If a transition cannot preserve current affected-row behavior through SeaORM, isolate it in `crates/pandar-hub/src/repositories/adapters/commands.rs` with a backend-neutral function and a comment naming the affected-row guard requirement.

- [x] **Step 5: Migrate audit-coupled enqueue**

  Replace SQLx transactions in `commands/audit.rs` with `DatabaseTransaction`. Insert command and audit event in the same transaction and commit only after both succeed.

- [x] **Step 6: Verify command behavior**

  Run:

  ```bash
  cargo fmt
  cargo test -p pandar-hub repositories::tests::commands -- --nocapture
  cargo test -p pandar-hub grpc -- --nocapture
  ```

  Expected: command repository tests and gRPC command lifecycle tests pass.

## Task 4: Job, Artifact, Print Report, And Machine Event Migration

**Files:**
- Modify: `crates/pandar-hub/src/repositories/jobs.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/audit.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/create.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/rows.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/transitions.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/print_reports.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/print_reports/correlation.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/print_reports/events.rs`
- Modify: `crates/pandar-hub/src/repositories/jobs/print_reports/state.rs`
- Optionally create: `crates/pandar-hub/src/repositories/adapters/jobs.rs`
- Optionally create: `crates/pandar-hub/src/repositories/adapters/print_reports.rs`

- [x] **Step 1: Migrate artifact and job inserts**

  Replace SQLx artifact/job insert paths with SeaORM active models. Keep print job creation atomic: artifact row, command row, job row, and audit event must commit together or roll back together.

- [x] **Step 2: Migrate job list/detail mapping**

  Replace SQLx list/detail queries with SeaORM queries. Use explicit lookups rather than broad relation modeling if that keeps the code simpler. Preserve nested artifact and print-state response data.

- [x] **Step 3: Migrate command/job coupled transitions**

  Use SeaORM transactions for command and job state updates. Preserve current guarded-transition semantics and affected-row checks. If SeaORM cannot preserve a specific coupled update clearly, isolate it in `crates/pandar-hub/src/repositories/adapters/jobs.rs`.

- [x] **Step 4: Migrate print report reconciliation**

  Replace SQLx machine event insert/dedupe and job print-state updates with SeaORM where practical. Preserve:

  - idempotency through `(tenant_id, event_key)`
  - no duplicated terminal events after repeated reports
  - no regression from terminal print states
  - machine event payload JSON exactly as currently stored

  If raw SQL is needed for idempotent insert/ignore or affected-row guarded updates, isolate it in `crates/pandar-hub/src/repositories/adapters/print_reports.rs` with backend-neutral functions and comments explaining the exact SeaORM limitation.

- [x] **Step 5: Verify job and print report behavior**

  Run:

  ```bash
  cargo fmt
  cargo test -p pandar-hub repositories::tests::jobs -- --nocapture
  cargo test -p pandar-hub print_reports -- --nocapture
  cargo test -p pandar-hub http -- --nocapture
  ```

  Expected: job repository, print report reconciliation, and route-level behavior remain green.

## Task 5: SQLx Escape Hatch Audit And Documentation

**Files:**
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `docs/superpowers/specs/2026-06-22-phase-12-complete-seaorm-repository-migration-design.md`
- Modify: `docs/superpowers/plans/2026-06-22-phase-12-complete-seaorm-repository-migration.md`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Audit remaining SQLx usage**

  Run:

  ```bash
  rg "sqlx::query|query_as|Transaction<|SqlitePool|PgPool|sqlx::" crates/pandar-hub/src/repositories crates/pandar-hub/src/entities
  ```

  Result: production repository/entity hits are limited to `crates/pandar-hub/src/repositories/adapters/printers.rs`, plus `#[cfg(test)]` helpers in `repositories/mod.rs` and repository tests.

- [x] **Step 2: Remove obsolete SQLx helpers**

  No obvious obsolete production SQLx helper remained. Keep SQLx fixture helpers under `#[cfg(test)]` because they still make backend setup and corruption fixtures clearer.

- [x] **Step 3: Document persistence boundary**

  Final migration boundary:

  - Actual SQLx adapter module kept: `crates/pandar-hub/src/repositories/adapters/printers.rs`.
  - Operation covered: atomic printer snapshot upsert on `(tenant_id, serial_number)`.
  - Reason: the SeaORM generic path for this repository flow would be select-then-write; the adapter preserves SQLite/PostgreSQL `ON CONFLICT` atomic upsert behavior and concurrency semantics.
  - Error context: unmapped database failures preserve lower-level cause/context through operation-specific `anyhow` context.

  Update `docs/architecture.md` to state:

  - repositories use SeaORM 2.0 entities and transactions for persistent behavior,
  - SQLx remains the connection/migration layer,
  - any remaining raw SQL repository usage is isolated in documented adapters with SQLite/PostgreSQL parity tests.

- [x] **Step 4: Update roadmap**

  Update `docs/roadmap.md` Phase 12 to record implementation progress without marking the phase complete until final SDD review and full verification pass. Immediate Next must finish Phase 12 final SDD review/full verification first, then start Phase 13.

## Task 6: Final Verification And SDD Review Inputs

**Files:**
- No implementation files unless verification exposes a bug.

- [x] **Step 1: Run required formatting and linting**

  ```bash
  cargo fmt
  cargo clippy --workspace --all-targets -- -D warnings
  ```

  Result: both passed with no warnings.

- [x] **Step 2: Run workspace tests**

  ```bash
  cargo nextest run --manifest-path "Cargo.toml" --workspace
  ```

  Result: passed, 252 tests run and 252 passed. Optional PostgreSQL tests ran in skip-when-unconfigured form.

- [x] **Step 3: Run repository grep guards**

  ```bash
  rg "sqlx::query|query_as|Transaction<|sqlx::" crates/pandar-hub/src/repositories crates/pandar-hub/src/entities
  git status --short
  git diff --check
  git diff --name-only | rg "\\.(pb|tonic)\\.rs$" && exit 1 || true
  ```

  Result: SQLx hits are limited to the documented printer adapter, `#[cfg(test)]` setup helpers, and repository tests. No generated protobuf output is present, and `git diff --check` passed.

- [x] **Step 4: Prepare final SDD implementation review packet**

  Collect:

  - reviewed spec path and content,
  - reviewed plan path and content,
  - `git diff --stat`,
  - relevant `git diff`,
  - verification command outputs.

  Result: native code reviewer and opencode implementation reviewer both returned `VERDICT: APPROVE`, with no blockers or required changes.
