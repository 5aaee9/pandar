# Phase 7 SeaORM Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce SeaORM 2.0 into `pandar-hub` and migrate the tenant repository as the first staged ORM-backed persistence path.

**Architecture:** Keep the existing SQLx pools and SQL migration directories as schema/runtime truth. Add a SeaORM connection accessor on `Database`, define one hand-written `tenants` entity, and rewrite only `TenantRepository` to use SeaORM while preserving its public API and error mapping.

**Tech Stack:** Rust 2024, SeaORM `2.0.0-rc.41`, SQLx `0.9.0`, SQLite, PostgreSQL, Axum, Tokio.

---

## Files

- Modify: `/home/indexyz/pandar/Cargo.toml`
- Modify: `/home/indexyz/pandar/Cargo.lock`
- Modify: `/home/indexyz/pandar/crates/pandar-hub/Cargo.toml`
- Modify: `/home/indexyz/pandar/crates/pandar-hub/src/lib.rs`
- Modify: `/home/indexyz/pandar/crates/pandar-hub/src/db.rs`
- Create: `/home/indexyz/pandar/crates/pandar-hub/src/entities/mod.rs`
- Create: `/home/indexyz/pandar/crates/pandar-hub/src/entities/tenants.rs`
- Modify: `/home/indexyz/pandar/crates/pandar-hub/src/repositories/tenants.rs`
- Modify: `/home/indexyz/pandar/crates/pandar-hub/src/repositories/tests/phase1.rs`
- Modify: `/home/indexyz/pandar/crates/pandar-hub/src/repositories/tests/postgres.rs`
- Modify: `/home/indexyz/pandar/docs/roadmap.md`

## Task 1: Dependency Alignment

**Files:**
- Modify: `/home/indexyz/pandar/Cargo.toml`
- Modify: `/home/indexyz/pandar/crates/pandar-hub/Cargo.toml`
- Modify: `/home/indexyz/pandar/Cargo.lock`

- [ ] **Step 1: Update workspace dependencies**

In `/home/indexyz/pandar/Cargo.toml`, change `sqlx` to 0.9-compatible features and add `sea-orm`:

```toml
sea-orm = { version = "2.0.0-rc.41", default-features = false, features = ["macros", "runtime-tokio-rustls", "sqlx-sqlite", "sqlx-postgres", "with-json", "with-time"] }
sqlx = { version = "0.9.0", default-features = false, features = ["runtime-tokio", "tls-rustls", "sqlite", "postgres", "migrate", "macros"] }
```

- [ ] **Step 2: Add hub dependency**

In `/home/indexyz/pandar/crates/pandar-hub/Cargo.toml`, add:

```toml
sea-orm.workspace = true
```

- [ ] **Step 3: Resolve dependencies**

Run:

```bash
cargo check -p pandar-hub
```

Expected: Cargo resolves `sea-orm 2.0.0-rc.41` and `sqlx 0.9.0`. Compilation may fail on source compatibility issues that Task 2 or Task 3 will address, but dependency resolution must succeed.

## Task 2: SeaORM Database Accessor

**Files:**
- Modify: `/home/indexyz/pandar/crates/pandar-hub/src/db.rs`

- [ ] **Step 1: Add SeaORM imports**

Add imports for SeaORM SQLx pool wrappers:

```rust
use sea_orm::{DatabaseConnection, SqlxPostgresConnector, SqlxSqliteConnector};
```

- [ ] **Step 2: Add connection accessor**

Add this method to `impl Database`:

```rust
pub fn sea_orm_connection(&self) -> DatabaseConnection {
    match self {
        Self::Sqlite(pool) => SqlxSqliteConnector::from_sqlx_sqlite_pool(pool.clone()),
        Self::Postgres(pool) => SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone()),
    }
}
```

- [ ] **Step 3: Compile check**

Run:

```bash
cargo check -p pandar-hub
```

Expected: `db.rs` compiles or remaining failures are unrelated SQLx 0.9 compatibility issues surfaced by Task 1.

## Task 3: Tenant Entity

**Files:**
- Modify: `/home/indexyz/pandar/crates/pandar-hub/src/lib.rs`
- Create: `/home/indexyz/pandar/crates/pandar-hub/src/entities/mod.rs`
- Create: `/home/indexyz/pandar/crates/pandar-hub/src/entities/tenants.rs`

- [ ] **Step 1: Expose entities module**

In `/home/indexyz/pandar/crates/pandar-hub/src/lib.rs`, add:

```rust
pub mod entities;
```

- [ ] **Step 2: Create entity module root**

Create `/home/indexyz/pandar/crates/pandar-hub/src/entities/mod.rs`:

```rust
pub mod tenants;
```

- [ ] **Step 3: Create tenant entity**

Create `/home/indexyz/pandar/crates/pandar-hub/src/entities/tenants.rs` with a hand-written SeaORM entity for the existing `tenants` table:

```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "tenants")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub slug: String,
    pub display_name: String,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 4: Compile check**

Run:

```bash
cargo check -p pandar-hub
```

Expected: entity derives compile with SeaORM 2.0.

## Task 4: Rewrite TenantRepository With SeaORM

**Files:**
- Modify: `/home/indexyz/pandar/crates/pandar-hub/src/repositories/tenants.rs`

- [ ] **Step 1: Replace direct SQLx imports**

Remove:

```rust
use sqlx::Row;
```

Add:

```rust
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, DbErr, EntityTrait, PaginatorTrait, QueryOrder, SqlErr,
};
```

Also import:

```rust
use crate::entities::tenants;
```

- [ ] **Step 2: Implement create with SeaORM**

Replace the insert `match &self.database` block with:

```rust
let model = tenants::ActiveModel {
    id: Set(tenant.id.to_string()),
    slug: Set(tenant.slug.clone()),
    display_name: Set(tenant.display_name.clone()),
    created_at: Set(tenant.created_at.clone()),
};

let result = model
    .insert(&self.database.sea_orm_connection())
    .await
    .map(|_| ());
```

Replace the `match result` block with:

```rust
match result {
    Ok(_) => Ok(tenant),
    Err(err) if is_duplicate_tenant_slug(&err) => Err(RepositoryError::DuplicateTenantSlug),
    Err(err) => Err(anyhow::Error::new(err)
        .context("failed to insert tenant")
        .into()),
}
```

- [ ] **Step 3: Implement list with SeaORM**

Replace backend-specific query branches with:

```rust
tenants::Entity::find()
    .order_by_asc(tenants::Column::CreatedAt)
    .order_by_asc(tenants::Column::Id)
    .all(&self.database.sea_orm_connection())
    .await
    .context("failed to list tenants")?
    .into_iter()
    .map(tenant_from_model)
    .collect()
```

- [ ] **Step 4: Implement count with SeaORM**

Replace backend-specific count branches with:

```rust
let count = tenants::Entity::find()
    .count(&self.database.sea_orm_connection())
    .await
    .context("failed to count tenants")?;

Ok(count.try_into().expect("tenant count should fit in i64"))
```

- [ ] **Step 5: Add model mapping helpers**

Replace `tenant_from_parts` with:

```rust
fn tenant_from_model(model: tenants::Model) -> RepositoryResult<Tenant> {
    Tenant::from_parts(
        TenantId::parse(&model.id).map_err(anyhow::Error::from)?,
        model.slug,
        model.display_name,
        model.created_at,
    )
    .map_err(anyhow::Error::from)
    .context("failed to rehydrate tenant")
    .map_err(RepositoryError::from)
}

fn is_duplicate_tenant_slug(err: &DbErr) -> bool {
    let Some(SqlErr::UniqueConstraintViolation(message)) = err.sql_err() else {
        return false;
    };

    message.contains("tenants.slug") || message.contains("tenants_slug_key")
}
```

- [ ] **Step 6: Compile and targeted tests**

Run:

```bash
cargo test -p pandar-hub repositories::tests::phase1::
```

Expected: Phase 1 repository tests, including tenant create/list/count and duplicate slug rejection, pass.

## Task 5: SQLx 0.9 Compatibility Pass

**Files:**
- Modify only files that fail to compile under SQLx 0.9.

- [ ] **Step 1: Run workspace check**

Run:

```bash
cargo check --workspace
```

Expected: if SQLx 0.9 introduces compile errors, identify exact files and fix only those compatibility issues.

- [ ] **Step 2: Keep fixes surgical**

Allowed examples:

```rust
use sqlx::Executor;
```

or API import/type adjustments required by SQLx 0.9.

Do not rewrite repository behavior beyond what compile/test failures require.

- [ ] **Step 3: Run hub repository tests**

Run:

```bash
cargo nextest run -p pandar-hub repositories::
```

Expected: all hub repository tests pass.

## Task 6: PostgreSQL Coverage And No Generated Files

**Files:**
- Modify: `/home/indexyz/pandar/crates/pandar-hub/src/repositories/tests/postgres.rs`

- [ ] **Step 1: Ensure optional Postgres tenant coverage remains present**

Confirm `/home/indexyz/pandar/crates/pandar-hub/src/repositories/tests/postgres.rs` still exercises `TenantRepository::create` and `TenantRepository::list` when `PANDAR_TEST_POSTGRES_URL` is configured.

If `postgres_core_repository_behavior_when_configured` does not assert tenant count, add this assertion after the existing tenant list assertion:

```rust
assert_eq!(tenants.count().await.unwrap(), 1);
```

- [ ] **Step 2: Run optional test command**

Run:

```bash
cargo test -p pandar-hub repositories::tests::postgres::postgres_core_repository_behavior_when_configured
```

Expected: test passes. If `PANDAR_TEST_POSTGRES_URL` is not set, the test returns early and passes without opening a PostgreSQL connection.

- [ ] **Step 3: Check generated files**

Run:

```bash
find . -path ./target -prune -o \( -name '*.pb.rs' -o -name '*.tonic.rs' \) -print
```

Expected: no output.

## Task 7: Documentation And Roadmap

**Files:**
- Modify: `/home/indexyz/pandar/docs/roadmap.md`

- [ ] **Step 1: Add Phase 7 SeaORM Migration section**

Replace the current `## Phase 7: Compatibility Expansion` heading with:

```markdown
## Phase 7: SeaORM Migration
```

Add bullets:

```markdown
- Completed the first staged SeaORM 2.0 migration by adding SeaORM `2.0.0-rc.41` behind the existing SQLx pool boundary.
- Completed workspace SQLx `0.9.0` alignment required by SeaORM 2.0.
- Completed hand-written SeaORM entity coverage for `tenants`.
- Completed `TenantRepository` create/list/count migration to SeaORM while preserving the existing repository API and SQLite/PostgreSQL behavior.
- Deferred auth, audit, agents, printers, commands, jobs, and SeaORM migration-system adoption to later phases.
```

Move the old compatibility bullets into a new later section named:

```markdown
## Later: Compatibility Expansion
```

- [ ] **Step 2: Update Immediate Next**

Add:

```markdown
- Continue staged SeaORM migration for auth/audit and then command/job repositories only after Phase 7 tenant behavior stays green.
```

- [ ] **Step 3: Verify docs**

Run:

```bash
rg -n "SeaORM|Compatibility Expansion|Phase 7" docs/roadmap.md
```

Expected: roadmap clearly shows Phase 7 as SeaORM migration and compatibility expansion as later work.

## Task 8: Final Verification

**Files:**
- No code edits unless verification exposes required fixes.

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt --check
```

Expected: exit 0.

- [ ] **Step 2: Lint**

Run:

```bash
cargo clippy --workspace
```

Expected: exit 0 with no warnings requiring code changes.

- [ ] **Step 3: Workspace tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all tests pass.

- [ ] **Step 4: No-default-features hub tests**

Run:

```bash
cargo test -p pandar-hub --no-default-features
```

Expected: all tests pass.

- [ ] **Step 5: Frontend build**

Run:

```bash
npm run build
```

from `/home/indexyz/pandar/frontend`.

Expected: Next.js build succeeds.

- [ ] **Step 6: Diff hygiene**

Run:

```bash
git diff --check
find . -path ./target -prune -o \( -name '*.pb.rs' -o -name '*.tonic.rs' \) -print
```

Expected: `git diff --check` exits 0 and generated protobuf check prints no files.
