# Better Auth External Onboarding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Phase 30/31 Better Auth-compatible external onboarding: provider profile claims, `/api/v1/me`, tenant self-creation, tenant-admin join links, provider-configured frontend onboarding, and docs.

**Architecture:** Keep `pandar-hub` provider-neutral. JWT verification extracts external account profile data; repository methods create tenant-local user projections and identity links only through explicit onboarding flows. Join links are hash-only database secrets, accepted through JSON bodies, and administered by tenant admins.

**Tech Stack:** Rust, axum, SeaORM, SQLx migrations, SQLite/PostgreSQL, jsonwebtoken, Next.js 16, React 19, server actions, Tailwind CSS.

---

## File Structure

- Modify `crates/pandar-hub/src/identity/verifier.rs`: add profile claims to `JwtClaims` and `VerifiedExternalIdentity`.
- Create `crates/pandar-hub/src/entities/join_links.rs`: SeaORM entity for join links.
- Modify `crates/pandar-hub/src/entities/mod.rs`: export `join_links`.
- Create migrations:
  - `crates/pandar-hub/migrations/sqlite/20260625000000_phase_31_external_onboarding.sql`
  - `crates/pandar-hub/migrations/postgres/20260625000000_phase_31_external_onboarding.sql`
- Create `crates/pandar-hub/src/repositories/auth/onboarding.rs`: external membership listing, self-create tenant, join-link create/list/revoke/accept.
- Modify `crates/pandar-hub/src/repositories/auth.rs`: export onboarding structs and wire module.
- Modify `crates/pandar-hub/src/repositories/auth/identities.rs`: expose transaction-scoped identity lookup helpers; reuse existing generic insert helpers on transactions.
- Modify `crates/pandar-hub/src/repositories/auth/users.rs`: expose transaction-scoped user lookup helpers; reuse existing generic insert helpers on transactions.
- Modify `crates/pandar-hub/src/repositories/mod.rs`: add repository errors for join links and unverified external profile.
- Modify `crates/pandar-hub/src/lib.rs`: add self-create config field and `AuthRepository` access remains unchanged.
- Create `crates/pandar-hub/src/routes/onboarding.rs`: `/api/v1/me`, `/api/v1/onboarding/tenants`, `/api/v1/join-links/accept`.
- Create `crates/pandar-hub/src/routes/join_links.rs`: tenant-admin create/list/revoke join-link routes.
- Modify `crates/pandar-hub/src/routes.rs`: route registration and error-code mapping.
- Modify `crates/pandar-hub/src/routes/auth.rs`: expose helper for external-only bearer verification without tenant-token fallback.
- Add/modify tests:
  - `crates/pandar-hub/src/identity/verifier_tests.rs`
  - `crates/pandar-hub/src/repositories/tests/auth.rs`
  - `crates/pandar-hub/src/repositories/tests/postgres.rs`
  - `crates/pandar-hub/src/routes/tests.rs`
  - `crates/pandar-hub/src/routes/tests/onboarding.rs`
  - `crates/pandar-hub/src/routes/tests/provisioning/workflow.rs`
  - `crates/pandar-hub/src/routes/tests/bootstrap.rs`
- Modify frontend files:
  - `frontend/app/api-auth.ts`
  - `frontend/app/actions.ts`
  - `frontend/app/dashboard-types.ts`
  - `frontend/app/page.tsx`
  - `frontend/app/admin-panel.tsx`
  - create `frontend/app/join/page.tsx`
  - create `frontend/app/auth-provider.ts`
  - create `frontend/app/onboarding-panel.tsx`
- Update docs:
  - `docs/development.md`
  - `docs/architecture.md`
  - `docs/release-installation.md`
  - `docker-compose.sqlite.yml`
  - `docker-compose.postgres.yml`
  - `docs/roadmap.md`

## Task 1: External Identity Profile Claims

**Files:**
- Modify: `crates/pandar-hub/src/identity/verifier.rs`
- Modify: `crates/pandar-hub/src/identity/verifier_tests.rs`
- Test: `crates/pandar-hub/src/identity/verifier_tests.rs`

- [ ] **Step 1: Add failing tests for profile extraction**

Add or extend verifier tests so a valid RS256 JWT with profile claims returns:

```rust
assert_eq!(verified.provider, "clerk");
assert_eq!(verified.subject, "user_profile");
assert_eq!(verified.email.as_deref(), Some("alice@example.test"));
assert_eq!(verified.email_verified, Some(true));
assert_eq!(verified.name.as_deref(), Some("Alice Doe"));
assert_eq!(verified.preferred_username.as_deref(), Some("alice"));
assert_eq!(verified.display_name(), "Alice Doe");
```

Also add tests for display-name fallback:

```rust
assert_eq!(verified_with_username.display_name(), "alice");
assert_eq!(verified_with_email_only.display_name(), "alice@example.test");
```

- [ ] **Step 2: Run verifier tests and confirm they fail**

Run:

```bash
cargo test -p pandar-hub identity::verifier_tests -- --nocapture
```

Expected: tests fail because `VerifiedExternalIdentity` lacks profile fields and display-name helper.

- [ ] **Step 3: Implement profile fields**

Update `JwtClaims`:

```rust
#[serde(default)]
email: Option<String>,
#[serde(default)]
email_verified: Option<bool>,
#[serde(default)]
name: Option<String>,
#[serde(default)]
preferred_username: Option<String>,
```

Update `VerifiedExternalIdentity`:

```rust
pub email: Option<String>,
pub email_verified: Option<bool>,
pub name: Option<String>,
pub preferred_username: Option<String>,
```

Add:

```rust
impl VerifiedExternalIdentity {
    pub fn verified_email(&self) -> Option<&str> {
        match (self.email.as_deref(), self.email_verified) {
            (Some(email), Some(true)) if !email.trim().is_empty() => Some(email.trim()),
            _ => None,
        }
    }

    pub fn display_name(&self) -> String {
        self.name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                self.preferred_username
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .or_else(|| self.verified_email())
            .unwrap_or("")
            .to_owned()
    }
}
```

Populate the new fields in `verified_identity`.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test -p pandar-hub identity::verifier_tests -- --nocapture
```

Expected: verifier tests pass.

## Task 2: Join Link Schema And Entity

**Files:**
- Create: `crates/pandar-hub/migrations/sqlite/20260625000000_phase_31_external_onboarding.sql`
- Create: `crates/pandar-hub/migrations/postgres/20260625000000_phase_31_external_onboarding.sql`
- Create: `crates/pandar-hub/src/entities/join_links.rs`
- Modify: `crates/pandar-hub/src/entities/mod.rs`

- [ ] **Step 1: Create equivalent SQLite migration**

Add:

```sql
CREATE TABLE join_links (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL,
    email_constraint TEXT,
    expires_at TEXT NOT NULL,
    max_uses INTEGER NOT NULL,
    used_count INTEGER NOT NULL DEFAULT 0,
    created_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    revoked_at TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX join_links_tenant_id_created_at_idx ON join_links (tenant_id, created_at);
```

- [ ] **Step 2: Create equivalent PostgreSQL migration**

Use the same schema shape:

```sql
CREATE TABLE join_links (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL,
    email_constraint TEXT,
    expires_at TEXT NOT NULL,
    max_uses INTEGER NOT NULL,
    used_count INTEGER NOT NULL DEFAULT 0,
    created_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    revoked_at TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX join_links_tenant_id_created_at_idx ON join_links (tenant_id, created_at);
```

- [ ] **Step 3: Add SeaORM entity**

Create entity with fields matching the migration:

```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "join_links")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub tenant_id: String,
    pub token_hash: String,
    pub role: String,
    pub email_constraint: Option<String>,
    pub expires_at: String,
    pub max_uses: i32,
    pub used_count: i32,
    pub created_by_user_id: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

Export it from `entities/mod.rs`.

- [ ] **Step 4: Run migration-backed smoke tests**

Run:

```bash
cargo test -p pandar-hub repositories::tests::phase1::sqlite_migrations_create_phase_1_schema -- --nocapture
```

Expected: SQLite migration still succeeds.

## Task 3: Onboarding Repository

**Files:**
- Create: `crates/pandar-hub/src/repositories/auth/onboarding.rs`
- Modify: `crates/pandar-hub/src/repositories/auth.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/auth/identities.rs`
- Modify: `crates/pandar-hub/src/repositories/auth/users.rs`
- Test: `crates/pandar-hub/src/repositories/tests/auth.rs`
- Test: `crates/pandar-hub/src/repositories/tests/postgres.rs`

- [ ] **Step 1: Write SQLite repository tests first**

Add tests for:

```rust
list_external_memberships_returns_linked_tenants_and_roles
self_create_tenant_links_external_admin
self_create_tenant_records_audit_without_external_subject
join_link_create_list_revoke_hashes_token
accept_join_link_creates_member_and_consumes_use
accept_join_link_existing_member_keeps_role_and_does_not_consume
accept_join_link_rejects_expired_revoked_used_up_and_email_mismatch
concurrent_single_use_join_link_accept_creates_one_member
```

Expected assertions:

```rust
assert!(created.plaintext_token.starts_with("pandar_join"));
assert_ne!(created.plaintext_token, listed[0].id);
assert_eq!(accepted.membership.role, UserRole::Operator);
assert_eq!(link_after.used_count, 1);
```

- [ ] **Step 2: Write PostgreSQL parity tests**

In `repositories/tests/postgres.rs`, add one grouped optional test:

```rust
#[tokio::test]
async fn postgres_external_onboarding_repository_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    // cover self-create, join-link accept, existing-member accept, revoke, and concurrent single-use accept
}
```

- [ ] **Step 3: Run tests and confirm they fail**

Run:

```bash
cargo test -p pandar-hub repositories::tests::auth:: -- --nocapture
```

Expected: fails because repository methods do not exist.

- [ ] **Step 4: Implement repository structs and methods**

Define public structs:

```rust
pub struct ExternalIdentityProfile {
    pub provider: String,
    pub subject: String,
    pub email: String,
    pub display_name: String,
}

pub struct ExternalMembership {
    pub tenant: pandar_core::Tenant,
    pub user: User,
}

pub struct JoinLink {
    pub id: String,
    pub tenant_id: String,
    pub role: UserRole,
    pub email_constraint: Option<String>,
    pub expires_at: String,
    pub max_uses: i32,
    pub used_count: i32,
    pub created_by_user_id: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
}

pub struct JoinLinkWithPlaintext {
    pub join_link: JoinLink,
    pub plaintext_token: String,
}

pub struct AcceptedJoinLink {
    pub tenant: pandar_core::Tenant,
    pub user: User,
    pub created: bool,
}
```

Use `pandar_core::TenantId` only at API boundaries and repository method inputs that already accept tenant IDs. Storage-shaped structs (`JoinLink`, `AcceptedJoinLink` internals, SeaORM models) keep `tenant_id` as `String` to match existing entities and migrations.

Implement:

```rust
list_external_memberships(provider, subject)
self_create_tenant_for_external_identity(slug, display_name, profile)
create_join_link_with_audit(tenant_id, role, email, expires_in_seconds, max_uses, actor)
list_join_links_for_tenant(tenant_id)
revoke_join_link_with_audit(tenant_id, join_link_id, actor)
accept_join_link(plaintext_token, profile)
```

Use `auth::secrets::generate_secret("pandar_join")` and `hash_token(&plaintext)`.

Audit events must be written for:

```text
user.external_projection_create
tenant.self_create
join_link.create
join_link.revoke
join_link.accept
```

Audit metadata must include tenant/user/join-link identifiers and non-secret policy values only. It must not include raw external provider subjects, plaintext tokens, token hashes, or JWT material. Add assertions in repository or route tests that serialized audit metadata does not contain the test subject string, plaintext token, or token hash.

`self_create_tenant_for_external_identity` must start with the same `begin_onboarding_write_transaction` helper used by join-link accept, so SQLite uses `SqliteTransactionMode::Immediate`. It must create the tenant, create the tenant-admin user projection, link the external identity, insert `user.external_projection_create`, insert `tenant.self_create`, then commit. Duplicate-slug or identity-link failures must roll back the tenant and user rows. The `tenant.self_create` audit actor may be the newly created local user projection; its metadata must not contain `profile.subject`.

- [ ] **Step 5: Implement concurrency-safe accept**

Within one `sea_orm::DatabaseTransaction`:

1. Find link by `token_hash`.
2. Reject revoked/expired/used-up.
3. If membership already exists, return it without updating `used_count`.
4. Perform conditional update:

```rust
UPDATE join_links
SET used_count = used_count + 1
WHERE id = ?
  AND used_count < max_uses
  AND revoked_at IS NULL
  AND expires_at > ?
```

Do not execute this update through the bare SQLx pool. It must run on the same `&sea_orm::DatabaseTransaction` as the later user, identity, membership, and audit writes. Add a transaction helper in `onboarding.rs` following the existing `jobs/audit.rs` pattern:

```rust
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, Statement,
    SqliteTransactionMode, TransactionOptions, TransactionTrait, Value,
};

async fn begin_onboarding_write_transaction(
    connection: &DatabaseConnection,
) -> Result<DatabaseTransaction, sea_orm::DbErr> {
    match connection.get_database_backend() {
        DbBackend::Sqlite => {
            connection
                .begin_with_options(TransactionOptions {
                    sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
                    ..Default::default()
                })
                .await
        }
        _ => connection.begin().await,
    }
}
```

Execute the conditional update with `tx.execute(Statement::from_sql_and_values(...))` so SQLite and PostgreSQL placeholders are selected from `tx.get_database_backend()`:

```rust
async fn consume_join_link_use_tx(
    tx: &DatabaseTransaction,
    join_link_id: &str,
    now: &str,
) -> RepositoryResult<bool> {
    let (sql, values) = match tx.get_database_backend() {
        DbBackend::Postgres => (
            "UPDATE join_links SET used_count = used_count + 1 WHERE id = $1 AND used_count < max_uses AND revoked_at IS NULL AND expires_at > $2",
            vec![join_link_id.into(), now.into()],
        ),
        _ => (
            "UPDATE join_links SET used_count = used_count + 1 WHERE id = ? AND used_count < max_uses AND revoked_at IS NULL AND expires_at > ?",
            vec![join_link_id.into(), now.into()],
        ),
    };
    let result = tx
        .execute(Statement::from_sql_and_values(
            tx.get_database_backend(),
            sql,
            values,
        ))
        .await?;
    Ok(result.rows_affected() == 1)
}
```

5. If the conditional update affects zero rows, re-read the link in the same transaction and return the correct invalid/used-up error.
6. Insert user and identity link by reusing the existing generic `insert_user(...)` and `insert_identity(...)` helpers on `&sea_orm::DatabaseTransaction`; add only the transaction-scoped lookup helpers needed for existing-member checks.
7. Insert `user.external_projection_create` and `join_link.accept` audit events through `insert_audit_event_tx`.
8. Commit only after all writes succeed. If any later write fails, the `used_count` increment must roll back with the transaction.

SQLite repository tests use a fresh in-memory database per test through `repositories::tests::sqlite_database()`, so no SQLite cleanup table list is needed. PostgreSQL tests share a configured database across tests; add `join_links` to the TRUNCATE list in `crates/pandar-hub/src/repositories/tests/postgres.rs`.

- [ ] **Step 6: Run repository tests**

Run:

```bash
cargo test -p pandar-hub repositories::tests::auth:: -- --nocapture
cargo test -p pandar-hub repositories::tests::postgres::postgres_external_onboarding_repository_behavior_when_configured -- --nocapture
```

Expected: SQLite tests pass; PostgreSQL test passes when `PANDAR_TEST_POSTGRES_URL` is set or prints skip message otherwise.

## Task 4: Hub Config And Onboarding Routes

**Files:**
- Modify: `crates/pandar-hub/src/lib.rs`
- Modify: `crates/pandar-hub/src/routes/auth.rs`
- Create: `crates/pandar-hub/src/routes/onboarding.rs`
- Create: `crates/pandar-hub/src/routes/join_links.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Modify: `crates/pandar-hub/src/routes/tests.rs`
- Modify: `crates/pandar-hub/src/routes/tests/bootstrap.rs`
- Test: `crates/pandar-hub/src/routes/tests/onboarding.rs`
- Test: `crates/pandar-hub/src/routes/tests/provisioning/workflow.rs`

- [ ] **Step 1: Write route tests first**

Add tests:

```rust
me_returns_external_identity_and_memberships_without_side_effects
me_succeeds_with_unverified_email_and_reports_onboarding_blocked
me_rejects_tenant_tokens
self_create_tenant_creates_admin_projection
self_create_tenant_can_be_disabled
self_create_tenant_allows_identity_with_existing_membership
tenant_admin_can_create_list_and_revoke_join_links
join_link_accept_creates_member_from_body_token
join_link_accept_rejects_email_mismatch
join_link_accept_existing_member_keeps_role
join_link_list_redacts_plaintext_and_hash
join_link_audit_metadata_redacts_subject_and_secret
```

- [ ] **Step 1a: Wire route test harness**

Modify `crates/pandar-hub/src/routes/tests.rs`:

```rust
mod onboarding;
```

Extend the existing `ExternalAuthClaims` / `jwt_for` helpers so route tests can emit optional profile claims:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
email: Option<&'a str>,
#[serde(skip_serializing_if = "Option::is_none")]
email_verified: Option<bool>,
#[serde(skip_serializing_if = "Option::is_none")]
name: Option<&'a str>,
#[serde(skip_serializing_if = "Option::is_none")]
preferred_username: Option<&'a str>,
```

Add a profile-aware helper such as:

```rust
fn jwt_for_profile(
    subject: &str,
    email: &str,
    email_verified: bool,
    name: &str,
) -> String
```

Keep existing helper behavior by defaulting these fields to `None` for current tests.

Modify `crates/pandar-hub/src/routes/tests/bootstrap.rs`: add `join_links` to its PostgreSQL `TRUNCATE` statement. This is required because route-level PostgreSQL tests share the configured database.

- [ ] **Step 2: Run route tests and confirm they fail**

Run:

```bash
cargo test -p pandar-hub routes::tests::onboarding -- --nocapture
```

Expected: fails because routes do not exist.

- [ ] **Step 3: Add self-create config**

Add to `AppState`:

```rust
self_create_tenant_allowed: bool,
```

Parse env:

```rust
PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE
```

Default true. Accept only `true` or `false`; invalid values should error at startup with the env var name.

Add test helper:

```rust
pub(crate) fn with_tenant_self_create_for_tests(mut self, allowed: bool) -> Self
```

- [ ] **Step 4: Add external-only auth helper**

In `routes/auth.rs`, add:

```rust
pub(super) async fn verify_external_identity(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<VerifiedExternalIdentity, ApiError>
```

It requires `Authorization: Bearer`, requires `state.external_auth()`, verifies the JWT, and never accepts tenant tokens.

Add helper:

```rust
pub(super) fn external_profile(verified: &VerifiedExternalIdentity) -> Result<ExternalIdentityProfile, ApiError>
```

Return `400 external_email_unverified` when verified email is absent.

- [ ] **Step 5: Implement routes**

Register:

```rust
.route("/api/v1/me", get(onboarding::me))
.route("/api/v1/onboarding/tenants", post(onboarding::create_tenant))
.route("/api/v1/join-links/accept", post(onboarding::accept_join_link))
.route("/api/v1/tenants/{tenant_id}/join-links", get(join_links::list_join_links).post(join_links::create_join_link))
.route("/api/v1/tenants/{tenant_id}/join-links/{join_link_id}", axum::routing::delete(join_links::revoke_join_link))
```

Ensure join-link create/list/revoke requires `tenant_admin`.

`/api/v1/me` must call only `verify_external_identity` and must not call `external_profile`. It returns the external profile and memberships even when `email_verified` is `false` or absent, with `can_self_create_tenant` reflecting config and with onboarding actions still blocked by later route validation. Only `create_tenant` and `accept_join_link` enforce verified email through `external_profile` and return `400 external_email_unverified`.

- [ ] **Step 6: Map repository errors**

Add `ApiError` mapping for:

```rust
invalid_join_link -> 404
join_link_email_mismatch -> 403
tenant_self_create_disabled -> 403
external_email_unverified -> 400
```

- [ ] **Step 7: Run route tests**

Run:

```bash
cargo test -p pandar-hub routes::tests::onboarding -- --nocapture
cargo test -p pandar-hub routes::tests::provisioning -- --nocapture
```

Expected: tests pass.

## Task 5: Frontend Onboarding And Join Links

**Files:**
- Modify: `frontend/app/api-auth.ts`
- Modify: `frontend/app/actions.ts`
- Modify: `frontend/app/dashboard-types.ts`
- Modify: `frontend/app/page.tsx`
- Modify: `frontend/app/admin-panel.tsx`
- Create: `frontend/app/join/page.tsx`
- Create: `frontend/app/onboarding-panel.tsx`
- Create: `frontend/app/auth-provider.ts`

- [ ] **Step 1: Add frontend types**

Add types:

```ts
export type AuthProvider = "clerk" | "logto" | "betterauth" | "none";

export type MeResponse = {
  identity: {
    provider: string;
    subject: string;
    email: string | null;
    email_verified: boolean | null;
    display_name: string;
  };
  tenants: Array<{
    tenant_id: string;
    tenant_slug: string;
    display_name: string;
    role: "tenant_admin" | "operator" | "viewer";
  }>;
  can_self_create_tenant: boolean;
};
```

Add `JoinLink` type without token hash.

- [ ] **Step 2: Add provider config helper**

Read:

```ts
APP_AUTH_PROVIDER
APP_AUTH_CLERK_PUBLISHABLE_KEY
APP_AUTH_LOGTO_ENDPOINT
APP_AUTH_LOGTO_APP_ID
APP_AUTH_BETTER_AUTH_BASE_URL
```

For this phase, do not add npm dependencies. Implement a provider-config helper that renders configured provider sign-in links and preserves the existing cookie/static bearer-token path:

```ts
export function authProviderConfig() {
  const provider = process.env.APP_AUTH_PROVIDER ?? "none";
  return {
    provider,
    cookieName: process.env.APP_AUTH_COOKIE_NAME ?? "pandar_auth_token",
    clerkPublishableKey: process.env.APP_AUTH_CLERK_PUBLISHABLE_KEY ?? null,
    logtoEndpoint: process.env.APP_AUTH_LOGTO_ENDPOINT ?? null,
    logtoAppId: process.env.APP_AUTH_LOGTO_APP_ID ?? null,
    betterAuthBaseUrl: process.env.APP_AUTH_BETTER_AUTH_BASE_URL ?? null,
  };
}
```

Provider sign-in UI uses configured links only; bearer acquisition remains through the existing request cookie/static bearer token path until provider SDK wiring is implemented in a later provider-specific frontend phase.

- [ ] **Step 3: Add server actions**

Add actions:

```ts
createTenantFromExternal(formData)
createJoinLink(previousState, formData)
revokeJoinLink(formData)
acceptJoinLink(formData)
```

`acceptJoinLink` posts JSON body `{ token }` to `/api/v1/join-links/accept`.

- [ ] **Step 4: Add onboarding UI**

On `page.tsx`, call `/api/v1/me` when external auth is available. If tenants are empty, render create-tenant and join-link entry actions. Existing dashboard rendering remains for selected tenants.

- [ ] **Step 5: Add join page**

Create `/join` page that reads token from URL fragment client-side and posts it through a server action or client fetch to a local action endpoint. Do not place the token in query params or path.

- [ ] **Step 6: Replace manual user/link forms in primary admin UI**

Hide manual create-user and manual identity-link forms from `admin-panel.tsx`. Keep list users, role update, tenant tokens, agent pairing, and audit views. Add join-link create/list/revoke UI for tenant admins.

- [ ] **Step 7: Build frontend**

Run:

```bash
npm --prefix frontend run build
```

Expected: build passes.

## Task 6: Documentation

**Files:**
- Modify: `docs/development.md`
- Modify: `docs/architecture.md`
- Modify: `docs/release-installation.md`
- Modify: `docker-compose.sqlite.yml`
- Modify: `docker-compose.postgres.yml`
- Modify: `docs/roadmap.md`
- Inspect: `docs/deployment/nixos/options.md`

- [ ] **Step 1: Update development docs**

Document:

```bash
PANDAR_EXTERNAL_AUTH_PROVIDER=betterauth
PANDAR_EXTERNAL_AUTH_ISSUER=https://auth.example.com
PANDAR_EXTERNAL_AUTH_JWKS_URL=https://auth.example.com/jwks
PANDAR_EXTERNAL_AUTH_AUDIENCE=https://api.example.com
PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256
PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE=true
```

Show Better Auth configuration with:

```ts
jwt({
  jwks: {
    keyPairConfig: {
      alg: "RSA256",
    },
  },
})
```

State that emitted JWT/JWK must verify with Pandar `RS256`.

Add a manual compatibility smoke checklist that satisfies the Better Auth JWT/JWK acceptance criterion through documentation, not automated tests:

```bash
node -e 'const token=process.env.BETTER_AUTH_TEST_JWT; console.log(JSON.parse(Buffer.from(token.split(".")[0],"base64url").toString()).alg)'
curl -fsS "$PANDAR_EXTERNAL_AUTH_JWKS_URL" | jq '.keys[] | {kty, alg, kid}'
```

Expected: Better Auth is configured with `keyPairConfig.alg: "RSA256"`, the emitted JWT header is compatible with Pandar `PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256`, and the JWKS exposes RSA key material (`kty: "RSA"`). If JWKS `alg` is present, it must not conflict with `RS256`.

- [ ] **Step 2: Update architecture docs**

Document account identity versus tenant membership:

```text
External provider owns user account identity.
Pandar owns tenant membership and role.
Pandar users are tenant-local projections of external identities.
```

Add `/api/v1/me`, join links, JSON-body accept, fragment join URLs, and self-hosted Better Auth as a future new-deployment-only phase.

- [ ] **Step 3: Update deployment docs and examples**

Update `docs/release-installation.md` with the new hub and web auth environment variables:

```bash
PANDAR_EXTERNAL_AUTH_PROVIDER
PANDAR_EXTERNAL_AUTH_ISSUER
PANDAR_EXTERNAL_AUTH_JWKS_URL
PANDAR_EXTERNAL_AUTH_AUDIENCE
PANDAR_EXTERNAL_AUTH_ALGORITHMS
PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE
APP_AUTH_PROVIDER
APP_AUTH_COOKIE_NAME
APP_AUTH_CLERK_PUBLISHABLE_KEY
APP_AUTH_LOGTO_ENDPOINT
APP_AUTH_LOGTO_APP_ID
APP_AUTH_BETTER_AUTH_BASE_URL
```

Update both `docker-compose.sqlite.yml` and `docker-compose.postgres.yml` with explicit web examples for:

```yaml
APP_AUTH_PROVIDER: none
APP_AUTH_COOKIE_NAME: pandar_auth_token
```

Also add commented or documented examples for Better Auth hub variables in the API service section. Inspect `docs/deployment/nixos/options.md`; because the NixOS module exposes generic `services.pandar.web.extraEnvironment` and `services.pandar.hub.extraEnvironment`, update the generated options doc only if the implementation adds explicit typed auth options.

- [ ] **Step 4: Update roadmap**

Move Phase 30/31 completed items into `Completed` only after implementation and verification pass. Keep Phase 32 and Phase 33 planned unless implemented.

## Task 7: Verification And Final Review

**Files:** all modified files.

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt
```

Expected: exits 0.

- [ ] **Step 2: Rust tests**

Run targeted tests first:

```bash
cargo test -p pandar-hub identity::verifier_tests -- --nocapture
cargo test -p pandar-hub repositories::tests::auth:: -- --nocapture
cargo test -p pandar-hub routes::tests::onboarding -- --nocapture
cargo test -p pandar-hub routes::tests::provisioning -- --nocapture
```

Then run workspace tests:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all available tests pass.

- [ ] **Step 3: Clippy**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: exits 0.

- [ ] **Step 4: Frontend build**

Run:

```bash
npm --prefix frontend run build
```

Expected: exits 0.

- [ ] **Step 5: Final diff review**

Run:

```bash
git status --short
git diff --stat
git diff -- docs/superpowers/specs/2026-06-25-betterauth-external-onboarding-design.md docs/superpowers/plans/2026-06-25-betterauth-external-onboarding.md
```

Expected: only intended files changed; no plaintext secrets or token hashes in response/list/audit paths.
