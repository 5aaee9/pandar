# Phase 7 SeaORM Migration Design

## Goal

Introduce SeaORM as the hub persistence ORM without rewriting the whole storage layer at once. Phase 7 migrates one low-risk repository path, tenants, onto SeaORM 2.0 so the project has a tested pattern for future repository migrations.

## Scope

Phase 7 includes:

- Add SeaORM `2.0.0-rc.41` to the workspace because the user explicitly requested SeaORM 2.0.
- Upgrade the workspace `sqlx` dependency from `0.8.6` to `0.9.0` so existing SQLx pools have the same concrete types expected by SeaORM 2.0.
- Keep the existing `sqlx` connection pools and SQL migration directories as the source of schema truth for this phase.
- Expose a SeaORM `DatabaseConnection` from the existing `Database` enum by wrapping the existing SQLx pool with SeaORM's SQLx connector APIs.
- Add SeaORM entity definitions for the `tenants` table only.
- Rewrite `TenantRepository` create/list/count operations to use SeaORM.
- Preserve the public repository API and existing route behavior.
- Add or keep tests proving tenant create/list/count, duplicate slug handling, file SQLite persistence, and optional PostgreSQL behavior still work.
- Update roadmap and documentation to mark Phase 7 as a staged SeaORM migration, not a full ORM conversion.

Phase 7 does not include:

- Migrating auth, audit, agents, printers, commands, or jobs repositories.
- Replacing existing SQL migration files with SeaORM migrations.
- Introducing SeaORM CLI or generated entity files.
- Changing public HTTP, WebSocket, gRPC, or frontend behavior.

## Architecture

The existing `Database` enum remains the single hub database handle. Each variant keeps its SQLx pool so current migrations, raw SQL repositories, and tests continue to work. `Database` also exposes `sea_orm_connection()` returning a SeaORM `DatabaseConnection` built from the same pool, using `SqlxSqliteConnector::from_sqlx_sqlite_pool` or `SqlxPostgresConnector::from_sqlx_postgres_pool`.

SeaORM entities live under `crates/pandar-hub/src/entities/`. Phase 7 creates `entities::tenants` only. Repository code maps between SeaORM models and `pandar_core::Tenant`; the domain model remains the external contract.

Tenant uniqueness and persistence errors continue to map into `RepositoryError`. SeaORM duplicate slug errors must still become `RepositoryError::DuplicateTenantSlug`; other database errors must keep lower-level cause/context through `anyhow`.

## Dependency Choice

Use SeaORM `2.0.0-rc.41`.

Reasons:

- The user explicitly requested 2.0.
- The local Rust toolchain is `rustc 1.96.0`, satisfying SeaORM 2.0 RC's MSRV of 1.94.
- SeaORM 2.0 RC supports SQLx-backed SQLite/PostgreSQL connection wrapping and entity macros needed for this staged migration.

Dependency details:

- `sea-orm = { version = "2.0.0-rc.41", default-features = false, features = ["macros", "runtime-tokio-rustls", "sqlx-sqlite", "sqlx-postgres", "with-json", "with-time"] }`
- `sqlx = { version = "0.9.0", default-features = false, features = ["runtime-tokio", "tls-rustls", "sqlite", "postgres", "migrate", "macros"] }`
- Do not add `sea-orm-migration` in Phase 7 because existing SQLx migrations remain the source of schema truth.
- Do not enable SeaORM default features implicitly; this avoids unrelated MySQL, stream, chrono, rust_decimal, and SQLite RETURNING behavior changes.

Risk:

- SeaORM 2.0 is release-candidate software. Phase 7 limits blast radius to one repository and documents the RC dependency in the roadmap.
- Upgrading SQLx from `0.8.6` to `0.9.0` can affect all existing raw-SQL repositories. Phase 7 must keep the full hub repository test suite green and fix only compatibility issues required by SQLx 0.9.

## Acceptance Criteria

- `TenantRepository` no longer uses direct `sqlx::query` calls for create/list/count.
- `tenants` entity compiles with SeaORM 2.0 and maps all existing columns: `id`, `slug`, `display_name`, `created_at`.
- SQLite and PostgreSQL stay first-class backends.
- Existing SQLx migrations still run before SeaORM repository access.
- Existing non-tenant SQLx repositories continue compiling and passing their tests under SQLx 0.9.
- Tenant tests pass unchanged or with only test additions for SeaORM-backed behavior.
- Optional PostgreSQL tenant repository coverage continues to run when `PANDAR_TEST_POSTGRES_URL` is set and safely no-ops when it is not.
- No generated SeaORM entity files or protobuf target files are committed.
- Documentation and roadmap identify Phase 7 as the first staged SeaORM migration and list remaining repository migration work.

## Verification

Run fresh before completion:

- `cargo fmt --check`
- `cargo clippy --workspace`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `cargo test -p pandar-hub --no-default-features`
- `npm run build` in `frontend/`
- `git diff --check`
- Generated protobuf check: `find . -path ./target -prune -o \( -name '*.pb.rs' -o -name '*.tonic.rs' \) -print`

## Rollback

Because Phase 7 preserves SQLx pools, SQL migrations, and the public repository API, rollback is a normal git revert of the SeaORM dependency, entity module, `Database` SeaORM accessor, and tenant repository rewrite. No database schema rollback is required.
