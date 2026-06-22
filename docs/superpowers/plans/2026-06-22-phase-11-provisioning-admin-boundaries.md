# Phase 11 Provisioning And Admin Boundaries Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add bootstrap/global admin protection, tenant-admin provisioning APIs, token revocation, audit coverage, and docs for Phase 11.

**Architecture:** Keep tenant authorization in Rust repositories and route modules. Add a small bootstrap-auth boundary to `AppState`, keep cross-tenant routes bootstrap-only, and split provisioning routes into focused files so `routes.rs` does not grow past the project limit. Add backend-neutral repository methods with SQLx SQLite/PostgreSQL branches and optional PostgreSQL tests behind `PANDAR_TEST_POSTGRES_URL`.

**Tech Stack:** Rust, axum, SQLx, SQLite/PostgreSQL migrations, existing JWT/API-token auth, Next.js server components, cargo nextest.

---

## File Structure

- Create `crates/pandar-hub/migrations/sqlite/20260622040000_phase_11_provisioning.sql`: add `api_tokens.revoked_at`.
- Create `crates/pandar-hub/migrations/postgres/20260622040000_phase_11_provisioning.sql`: same schema change.
- Modify `crates/pandar-hub/src/lib.rs`: store bootstrap token config in `AppState`, load `PANDAR_BOOTSTRAP_TOKEN`, add test helper constructor.
- Create `crates/pandar-hub/src/bootstrap.rs`: parse/check bootstrap bearer authorization.
- Modify `crates/pandar-hub/src/repositories/mod.rs`: add `DuplicateUserEmail` and `MissingApiToken`.
- Modify `crates/pandar-hub/src/repositories/auth.rs`: add `revoked_at` to `ApiToken`, user list/update methods, token list/revoke methods, duplicate email mapping, revoked-token auth filter.
- Modify `crates/pandar-hub/src/repositories/auth/identities.rs`: add identity list method.
- Modify `crates/pandar-hub/src/repositories/tests/auth.rs`: repository coverage for duplicate email, user list/role update, token list/revoke, identity list, optional PostgreSQL behavior.
- Create `crates/pandar-hub/src/routes/bootstrap.rs`: bootstrap auth-protected `POST /api/v1/bootstrap/tenant-admin`.
- Create `crates/pandar-hub/src/routes/admin.rs`: bootstrap auth-protected cross-tenant summary/list/create handlers.
- Create `crates/pandar-hub/src/routes/provisioning.rs`: tenant-admin user/token/identity/agent-pairing handlers.
- Modify `crates/pandar-hub/src/routes.rs`: register new modules/routes, remove inline cross-tenant handlers, map new repository errors.
- Modify `crates/pandar-hub/src/routes/tests.rs`: add bootstrap-enabled app helpers, adjust tenant fixtures to use repositories or bootstrap-aware helpers.
- Create `crates/pandar-hub/src/routes/tests/provisioning.rs`: route tests for Phase 11 behavior.
- Modify existing route tests under `crates/pandar-hub/src/routes/tests/*.rs`: update assumptions broken by bootstrap-protected `/tenants` and `/summary`.
- Modify `frontend/app/page.tsx`: when `APP_TENANT_ID` is configured, skip `/summary` and `/tenants` global reads and avoid surfacing bootstrap-only errors.
- Modify docs: `docs/architecture.md`, `docs/roadmap.md`, and any deployment README present with bootstrap/provisioning examples.

## Task 1: Migrations And Repository Behavior

**Files:**
- Create: `crates/pandar-hub/migrations/sqlite/20260622040000_phase_11_provisioning.sql`
- Create: `crates/pandar-hub/migrations/postgres/20260622040000_phase_11_provisioning.sql`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/auth.rs`
- Modify: `crates/pandar-hub/src/repositories/auth/identities.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/auth.rs`

- [ ] **Step 1: Add failing repository tests**

Add tests in `crates/pandar-hub/src/repositories/tests/auth.rs` that exercise:

- `users_can_be_listed_and_roles_updated`: create one tenant and one viewer user, assert `list_users_for_tenant` returns that user, call `update_user_role(..., UserRole::Operator)`, then assert the returned user and a second list call show `UserRole::Operator`.
- `duplicate_user_email_is_reported`: create two users with the same email inside one tenant and assert the second call returns `RepositoryError::DuplicateUserEmail`; create the same email in a different tenant and assert it succeeds.
- `api_tokens_can_be_listed_and_revoked`: create one token, assert `list_api_tokens_for_user` returns it with `revoked_at == None`, assert `authenticate_bearer` succeeds, call `revoke_api_token`, assert `revoked_at.is_some()`, assert `authenticate_bearer` returns `None`, and assert a second revoke returns the same revoked token metadata.
- `external_identities_can_be_listed_for_user`: link one identity, assert `list_external_identities_for_user` returns exactly that identity, and assert another user in the same tenant gets an empty list.

Extend `postgres_auth_and_audit_repository_behavior_when_configured` or add a new optional PostgreSQL test to cover the same token revocation and duplicate email behavior.

- [ ] **Step 2: Run repository tests to verify failure**

Run:

```bash
cargo test -p pandar-hub repositories::tests::auth -- --nocapture
```

Expected: FAIL because new repository methods/errors and `revoked_at` do not exist yet.

- [ ] **Step 3: Add equivalent migrations**

SQLite migration:

```sql
ALTER TABLE api_tokens ADD COLUMN revoked_at TEXT;
CREATE INDEX idx_api_tokens_revoked_at ON api_tokens(revoked_at);
```

PostgreSQL migration:

```sql
ALTER TABLE api_tokens ADD COLUMN revoked_at TEXT;
CREATE INDEX idx_api_tokens_revoked_at ON api_tokens(revoked_at);
```

- [ ] **Step 4: Add repository errors**

In `crates/pandar-hub/src/repositories/mod.rs`, add:

```rust
DuplicateUserEmail,
MissingApiToken,
```

Keep existing error style and update every exhaustive `match` that needs these variants.

- [ ] **Step 5: Implement auth repository changes**

In `crates/pandar-hub/src/repositories/auth.rs`:

- add `pub revoked_at: Option<String>` to `ApiToken`;
- set `revoked_at: None` in `create_api_token`;
- include `revoked_at` in token row conversion;
- add `AND api_tokens.revoked_at IS NULL` to both `authenticate_bearer` queries;
- map unique `(tenant_id, email)` violations in `create_user` to `RepositoryError::DuplicateUserEmail`;
- add backend-neutral `list_users_for_tenant`, `update_user_role`, `list_api_tokens_for_user`, and `revoke_api_token` methods.

`revoke_api_token` should:

- select the existing row by `(tenant_id, token_id)`;
- return `MissingApiToken` if absent;
- if `revoked_at` is already set, return the existing metadata;
- otherwise set `revoked_at = created_at_now()` and return updated metadata.

- [ ] **Step 6: Implement identity listing**

In `crates/pandar-hub/src/repositories/auth/identities.rs`, add:

```rust
pub async fn list_external_identities_for_user(
    &self,
    tenant_id: TenantId,
    user_id: &str,
) -> RepositoryResult<Vec<UserIdentity>>
```

Query by tenant/user and order by `created_at ASC, id ASC`.

- [ ] **Step 7: Run repository tests**

Run:

```bash
cargo test -p pandar-hub repositories::tests::auth -- --nocapture
```

Expected: PASS. If `PANDAR_TEST_POSTGRES_URL` is unset, PostgreSQL tests print skip messages and SQLite coverage passes.

## Task 2: Bootstrap Authority And Cross-Tenant Admin Routes

**Files:**
- Create: `crates/pandar-hub/src/bootstrap.rs`
- Create: `crates/pandar-hub/src/routes/admin.rs`
- Create: `crates/pandar-hub/src/routes/bootstrap.rs`
- Modify: `crates/pandar-hub/src/lib.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Modify: `crates/pandar-hub/src/routes/tests.rs`

- [ ] **Step 1: Add failing route tests**

In `crates/pandar-hub/src/routes/tests.rs` or a new module, add tests for:

- `summary_and_tenant_listing_require_bootstrap_token`: create one tenant directly through the repository, create a tenant API token, then assert `/summary` and `/tenants` return `401 missing_auth_token` without auth, `401 invalid_auth_token` with a bad token, `401 invalid_auth_token` with the tenant API token, and `200 OK` with the configured bootstrap token.
- `bootstrap_tenant_admin_creates_tenant_user_token_and_audit_events`: call `POST /api/v1/bootstrap/tenant-admin` with bootstrap auth, assert the response includes tenant/user/token and `role = "tenant_admin"`, use the returned plaintext token to call `GET /api/v1/tenants/{tenant_id}/agents`, then assert audit events include `tenant.bootstrap`, `user.create`, and `api_token.create`.
- `bootstrap_tenant_admin_rolls_back_on_late_failure`: cover the bootstrap transaction helper with a deterministic duplicate token-hash failure. Pre-create a tenant/user/API token with plaintext `fixed-bootstrap-secret`, then call the bootstrap helper for a different tenant while passing the same plaintext token through a repository-level test path. Assert the final tenant count, user count, token count, and audit count are unchanged from before the failed bootstrap call.
- `postgres_bootstrap_tenant_admin_transaction_when_configured`: when `PANDAR_TEST_POSTGRES_URL` is set, run the same bootstrap transaction helper against PostgreSQL and assert successful tenant/user/token/audit creation plus the duplicate token-hash rollback case. This test must use the same repository transaction path as the SQLite bootstrap route, not a separate SQL shortcut.
- `bootstrap_disabled_rejects_bootstrap_only_endpoints`: use an `AppState` without bootstrap token, call `/summary` with any Bearer value, and assert `403 bootstrap_disabled`.

Update existing `summary_reports_repository_counts` and tenant-list tests to use bootstrap token helpers.

- [ ] **Step 2: Run route tests to verify failure**

Run:

```bash
cargo test -p pandar-hub routes::tests -- --nocapture
```

Expected: FAIL because bootstrap auth and routes are not implemented.

- [ ] **Step 3: Add bootstrap config to AppState**

In `crates/pandar-hub/src/lib.rs`:

- add `mod bootstrap;`;
- add `bootstrap_token: Option<String>` to `AppState`;
- load it in `connect_with_auth_config` from `std::env::var("PANDAR_BOOTSTRAP_TOKEN").ok().filter(|v| !v.trim().is_empty())`;
- add `pub fn bootstrap_token(&self) -> Option<&str>`;
- add a `#[cfg(test)] pub fn with_bootstrap_token(mut self, token: impl Into<String>) -> Self`.

- [ ] **Step 4: Implement bootstrap auth helper**

Create `crates/pandar-hub/src/bootstrap.rs` with:

```rust
pub(crate) fn authorize_bootstrap(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), ApiError>
```

Use the same bearer parsing behavior as tenant auth. If the state token is absent after a Bearer token was supplied, return `403 bootstrap_disabled`.

- [ ] **Step 5: Move cross-tenant handlers**

Create `crates/pandar-hub/src/routes/admin.rs` with bootstrap-protected `summary`, `list_tenants`, and `create_tenant` handlers. Register them in `routes.rs`:

```rust
.route("/api/v1/summary", get(admin::summary))
.route("/api/v1/tenants", get(admin::list_tenants).post(admin::create_tenant))
```

`create_tenant` records `tenant.create` audit with `actor_type = "bootstrap"`.

- [ ] **Step 6: Add bootstrap tenant-admin route**

Create `crates/pandar-hub/src/routes/bootstrap.rs` with `create_tenant_admin`.

Implementation should create tenant, admin user, first API token, and audit events in one repository transaction. If route-level transaction support is too invasive, add a focused repository helper on `AuthRepository` or a small provisioning repository that owns this atomic operation for both SQLite and PostgreSQL.

Generated token format:

```rust
format!("pandar_{}", uuid::Uuid::new_v4().simple())
```

- [ ] **Step 7: Update route error mapping**

In `routes.rs`, map:

```rust
RepositoryError::DuplicateUserEmail => 409 "user_email_exists"
RepositoryError::MissingApiToken => 404 "api_token_not_found"
```

- [ ] **Step 8: Run route tests**

Run:

```bash
cargo test -p pandar-hub routes::tests -- --nocapture
```

Expected: PASS for updated route tests.

## Task 3: Tenant-Admin Provisioning Routes

**Files:**
- Create: `crates/pandar-hub/src/routes/provisioning.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Create: `crates/pandar-hub/src/routes/tests/provisioning.rs`
- Modify: `crates/pandar-hub/src/routes/tests.rs`

- [ ] **Step 1: Add failing provisioning route tests**

Create tests for:

- `tenant_admin_can_manage_users_identities_and_tokens`: use a tenant admin token to create an operator user, list users, patch the user role to viewer, link a `clerk` subject, list identities, create a user API token, list tokens, revoke the token, and assert the revoked plaintext token cannot call a viewer route.
- `operator_and_viewer_cannot_use_provisioning_routes`: create operator and viewer tokens and assert `POST /users`, `POST /api-tokens`, and `POST /agent-pairings` return `403 role_forbidden`.
- `tenant_admin_cannot_manage_other_tenant_users`: create two tenants and a tenant admin in each; assert tenant A admin cannot list tenant B users and cannot create/revoke tenant B tokens.
- `tenant_admin_can_create_agent_pairing_bundle`: call `POST /agent-pairings`, assert response includes `agent.id`, `agent.tenant_id`, and an `agent_env` string containing `PANDAR_TENANT_ID=`, `PANDAR_AGENT_ID=`, and `PANDAR_AGENT_NAME=`, then assert an `agent.pairing_bundle` audit event exists.

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p pandar-hub routes::tests::provisioning -- --nocapture
```

Expected: FAIL because routes do not exist.

- [ ] **Step 3: Implement response structs and role parsing**

In `routes/provisioning.rs`, add response structs:

- `UserResponse`
- `UserListResponse`
- `UserIdentityResponse`
- `UserIdentityListResponse`
- `ApiTokenResponse`
- `ApiTokenWithPlaintextResponse`
- `ApiTokenListResponse`
- `AgentPairingResponse`

Parse role strings with `UserRole::parse`, converting invalid input to `400 invalid_user_role`.

- [ ] **Step 4: Implement user and identity routes**

Add handlers for:

```rust
GET /api/v1/tenants/{tenant_id}/users
POST /api/v1/tenants/{tenant_id}/users
PATCH /api/v1/tenants/{tenant_id}/users/{user_id}/role
GET /api/v1/tenants/{tenant_id}/users/{user_id}/identities
POST /api/v1/tenants/{tenant_id}/users/{user_id}/identities
```

Every handler calls `authorize_tenant(..., UserRole::TenantAdmin)` first and records the required audit event after successful mutation.

- [ ] **Step 5: Implement API token routes**

Add handlers for:

```rust
GET /api/v1/tenants/{tenant_id}/users/{user_id}/api-tokens
POST /api/v1/tenants/{tenant_id}/users/{user_id}/api-tokens
DELETE /api/v1/tenants/{tenant_id}/api-tokens/{token_id}
```

Create returns plaintext token once; list/revoke never return plaintext or hash. Revoke records `api_token.revoke`.

- [ ] **Step 6: Implement agent pairing bundle route**

Add:

```rust
POST /api/v1/tenants/{tenant_id}/agent-pairings
```

Use existing agent repository creation, then record `agent.pairing_bundle` audit. Return `agent_env` with `PANDAR_TENANT_ID`, `PANDAR_AGENT_ID`, and `PANDAR_AGENT_NAME`.

- [ ] **Step 7: Register routes and run provisioning tests**

Register all routes in `routes.rs` and run:

```bash
cargo test -p pandar-hub routes::tests::provisioning -- --nocapture
```

Expected: PASS.

## Task 4: Frontend And Documentation

**Files:**
- Modify: `frontend/app/page.tsx`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update frontend tenant-bound dashboard reads**

In `frontend/app/page.tsx`, when `configuredTenantId` is set:

- do not fetch `/api/v1/summary`;
- do not add summary failure to `errors`;
- render metrics as unavailable or tenant-local counts derived from fetched tenant data where available.

Keep the existing no-`APP_TENANT_ID` behavior for bootstrap/global dashboards.

- [ ] **Step 2: Build frontend**

Run:

```bash
npm run build
```

from `frontend/`.

Expected: PASS.

- [ ] **Step 3: Update architecture docs**

In `docs/architecture.md`, add:

- `PANDAR_BOOTSTRAP_TOKEN`;
- bootstrap tenant-admin curl example;
- tenant admin user/token/identity examples;
- API-token revocation behavior;
- agent pairing bundle example and future token-rotation flow.

- [ ] **Step 4: Update roadmap**

In `docs/roadmap.md`, move Phase 11 bullets to completed language and leave Phase 12 as next phase.

## Task 5: Full Verification, Final Review, Commit, Push

**Files:**
- All changed files from Tasks 1-4.

- [ ] **Step 1: Run formatting**

Run:

```bash
cargo fmt
```

Expected: command succeeds.

- [ ] **Step 2: Run Rust lint**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: command succeeds with no warnings.

- [ ] **Step 3: Run Rust tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all tests pass. Optional PostgreSQL tests may skip when `PANDAR_TEST_POSTGRES_URL` is unset.

- [ ] **Step 4: Run frontend build**

Run:

```bash
npm run build
```

from `frontend/`.

Expected: build succeeds.

- [ ] **Step 5: Check generated protobuf files are ignored**

Run:

```bash
git status --short | rg '\\.(pb|tonic)\\.rs$'
```

Expected: no output.

- [ ] **Step 6: Run final diff checks**

Run:

```bash
git diff --check
git status --short
```

Expected: no whitespace errors; only intended Phase 11 files are changed.

- [ ] **Step 7: Independent implementation review**

Dispatch required SDD implementation reviewer with the final diff, this plan, the approved spec, and verification output. Continue fixing and re-reviewing until the reviewer returns exactly `VERDICT: APPROVE`.

- [ ] **Step 8: Commit and push**

Commit with Lore protocol:

```text
Harden provisioning behind explicit admin boundaries

Constraint: Phase 11 requires bootstrapable multi-tenant provisioning without development fixtures.
Rejected: Keeping cross-tenant tenant listing public | It lets tenant users enumerate other tenants.
Confidence: high
Scope-risk: broad
Directive: Keep Clerk/Logto as authentication only; Pandar owns tenant membership and roles.
Tested: cargo fmt; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path "Cargo.toml" --workspace; npm run build
Not-tested: Live Clerk/Logto tenants and live Bambu printers.
```

Push to the current `main` upstream.
