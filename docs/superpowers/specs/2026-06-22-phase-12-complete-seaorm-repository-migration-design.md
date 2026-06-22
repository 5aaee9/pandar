# Phase 12 Complete SeaORM Repository Migration Design

## Goal

Phase 12 completes the staged SeaORM 2.0 repository migration that began in Phase 7. At the end of this phase, Pandar hub persistent repository behavior must be implemented through SeaORM entity/query/transaction APIs, except for schema migrations and explicitly documented backend-specific SQL adapters where SeaORM cannot express the operation cleanly without changing behavior.

The external repository APIs, HTTP/gRPC behavior, database schema, and SQLite/PostgreSQL parity must remain unchanged.

## Current State

- `Database` exposes `sea_orm_connection()` wrapping the existing SQLx SQLite/PostgreSQL pools.
- SQLx migrations remain the schema source.
- Hub repositories use hand-written SeaORM entities and SeaORM transactions for persistent behavior.
- Phase 11 transaction-sensitive provisioning helpers stay atomic across mutation and audit event insertion.
- Raw SQL repository business logic remains only in `crates/pandar-hub/src/repositories/adapters/printers.rs`.

## Non-Goals

- Do not adopt SeaORM migrations in Phase 12. SQLx migration files remain authoritative.
- Do not change public REST, WebSocket, gRPC, protobuf, or frontend behavior.
- Do not redesign the tenant user model. Users remain tenant-scoped with the role on the user row.
- Do not add new product features while migrating persistence.
- Do not remove SQLx from the workspace. SQLx is still required for migrations, existing pool creation, and explicitly documented adapters.

## SeaORM Scope

### Required Entities

Add hand-written SeaORM entities under `crates/pandar-hub/src/entities/` for every persistent table:

- `tenants` already exists and remains unchanged unless shared helpers require small mapping changes.
- `users`
- `agents`
- `printers`
- `commands`
- `api_tokens`
- `audit_events`
- `user_identities`
- `job_artifacts`
- `jobs`
- `machine_events`

Entities must map all currently persisted columns, including nullable Phase 9/11 fields. Primary keys are non-auto-increment string IDs. Relations are optional unless directly useful for query clarity; this phase should prefer simple explicit filters over broad relationship modeling.

### Repository Migration

Persistent operations in these repositories use SeaORM except for the documented printer snapshot upsert adapter:

- `AuthRepository`
  - user create/list/role update
  - API token create/list/revoke/authenticate
  - external identity link/list/authenticate
  - tenant-admin bootstrap transaction
  - provisioning mutation plus audit transactions
- `AuditEventRepository`
  - record
  - list for tenant
- `AgentRepository`
  - create/list/get/update connection/mark offline/count
  - create-with-audit and pairing-bundle transaction helpers
- `PrinterRepository`
  - count/list/get/upsert snapshot
  - tenant and agent ownership checks
- `CommandRepository`
  - count/enqueue/dispatch queue lookup
  - transition guards and terminal transitions
  - enqueue-with-audit transaction helper
  - ownership checks
- `JobRepository`
  - artifact and job creation transactions
  - list/detail
  - command/job coupled transitions
  - print report reconciliation, machine event dedupe, and transaction-coupled state updates

## SQLx Escape Hatch

SQLx may remain only in these cases:

1. Database connection and SQLx migration execution in `db.rs`.
2. Test fixtures that are intentionally backend setup helpers, if using SeaORM would obscure the test intent.
3. Backend-specific SQL adapters documented in code comments and this spec when SeaORM cannot preserve behavior cleanly.

Actual repository adapter kept:

- `crates/pandar-hub/src/repositories/adapters/printers.rs`
  - Operation: atomic printer snapshot upsert on `(tenant_id, serial_number)`.
  - Reason: the SeaORM generic path for this repository flow would be select-then-write. The adapter preserves the existing SQLite/PostgreSQL `ON CONFLICT` atomic upsert behavior and concurrency semantics.
  - Coverage: SQLite/PostgreSQL repository parity tests cover the printer snapshot behavior.

Each escape hatch must be isolated in a small module with a backend-neutral public function, must preserve SQLite/PostgreSQL behavior, and must be covered by existing or new repository tests. Route code must not call raw SQL directly.

## Error Mapping

Repository error behavior is part of the public persistence contract. Phase 12 must preserve existing mappings:

- duplicate tenant slug
- duplicate user email
- duplicate API token name
- duplicate API token hash
- duplicate external identity
- duplicate user/provider identity link
- duplicate agent name
- missing tenant/user/api token/agent/printer/command/job
- command ownership mismatch
- invalid persisted status/role values
- invalid command transitions

SeaORM `DbErr` unique and foreign-key failures must map to the same `RepositoryError` variants as the current SQLx code. Lower-level causes must remain available through `anyhow` context when errors are logged or bubbled as database errors. The documented printer adapter also preserves unmapped SQLx database failures with operation context so the lower-level cause remains available.

## Transaction Semantics

The following transactions are correctness boundaries and must stay atomic:

- Phase 11 tenant-admin bootstrap: tenant, admin user, first API token, and audit events.
- Provisioning user/role/identity/token changes plus audit events.
- Agent creation/pairing plus audit events.
- Refresh-printers command enqueue plus audit event.
- Print job creation: artifact row, command row, job row.
- Command/job coupled status transitions.
- Print report reconciliation: job print-state update and machine event insert/dedupe effects.

SeaORM `DatabaseTransaction` should be the default transaction mechanism. SQLx transactions are allowed only inside documented escape-hatch adapters.

## Test Strategy

Phase 12 must rely on behavior tests rather than generated SQL snapshots.

Required verification during implementation:

- Existing SQLite default repository, route, gRPC, and frontend-adjacent tests remain green.
- Optional PostgreSQL repository tests still run when `PANDAR_TEST_POSTGRES_URL` is set and skip cleanly otherwise.
- Add focused tests only where migration changes risk behavior that existing tests do not cover:
  - duplicate and missing-row error mapping for migrated repositories,
  - transaction rollback behavior for migrated audit-coupled helpers,
  - idempotent print report/machine event reconciliation if touched by adapters.

Full completion verification:

- `cargo fmt`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `npm run build` in `frontend/` if frontend docs or server component behavior changes
- protobuf generated output guard: no `.pb.rs` / `.tonic.rs` files staged
- `git diff --check`

## Documentation

Update:

- `docs/architecture.md` with the final Phase 12 persistence boundary: SeaORM repositories, SQLx migrations, and allowed adapters.
- `docs/roadmap.md` to mark Phase 12 complete only after all persistent repository operations have migrated or are documented adapters.
- This spec and the implementation plan under `docs/superpowers/`.

## Milestones

Phase 12 is too broad to treat as one unreviewed patch, but its exit criteria remain complete repository migration. Implementation should proceed in these SDD-reviewed milestones:

1. **Entity and shared mapping foundation**
   - Add entities for every remaining table.
   - Add shared SeaORM error/row mapping helpers where they reduce duplication.
   - No behavior change except compilation.
2. **Auth, audit, agent, and printer repositories**
   - Migrate Phase 10/11 auth surfaces first.
   - Preserve provisioning/audit transaction semantics.
   - Migrate agents and printers next because they are smaller and heavily route-tested.
3. **Command repository**
   - Migrate command insert, queue lookup, ownership checks, guarded transitions, and audit-coupled enqueue.
   - Preserve stale/invalid transition behavior exactly.
4. **Job, artifact, and print-report repositories**
   - Migrate job creation/list/detail and command/job coupled transitions.
   - Migrate or explicitly adapter-isolate print report reconciliation and machine event dedupe.
5. **Documentation and final cleanup**
   - Remove obsolete SQLx repository helpers not used by migrations/tests/adapters.
   - Document remaining adapters.
   - Update roadmap and architecture.

Each milestone must have targeted tests before moving to the next, and final implementation review must verify Phase 12 as a whole.

## Acceptance Criteria

- All persistent repository operations use SeaORM APIs or an explicitly documented backend-specific adapter.
- SQLx remains only for connection/migrations, test fixtures, and documented adapters.
- SQLite and PostgreSQL behavior parity is preserved.
- Existing public APIs and route/gRPC/frontend behavior do not change.
- Transaction boundaries listed in this spec remain atomic.
- No generated SeaORM files, protobuf outputs, or build artifacts are committed.
- `docs/roadmap.md` and `docs/architecture.md` accurately describe the completed persistence boundary.

## Rollback

Rollback is a normal git revert because Phase 12 should not introduce schema changes. If a documented adapter is introduced, it must preserve the old SQL behavior and can be reverted independently with the repository migration that uses it.
