# Phase 10 External Identity Authentication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Clerk/Logto-compatible JWT authentication to `pandar-hub` while preserving Phase 6 API tokens and Pandar-owned tenant authorization.

**Architecture:** `pandar-hub` gets a provider-neutral external auth config and JWT verifier. Tenant route auth first checks existing API tokens, then verifies JWTs and resolves `(tenant_id, provider, subject)` through a new `user_identities` table linked to existing tenant-scoped users. The frontend gains a shared bearer-token helper that reads a per-request auth cookie before deployment/static tokens.

**Tech Stack:** Rust 2024, axum, SQLx migrations/repositories, `jsonwebtoken 10.4.0`, `reqwest 0.13.4` with rustls/json, Next.js server components/actions, SQLite and PostgreSQL.

---

## File Structure

- Modify `Cargo.toml` and `crates/pandar-hub/Cargo.toml` for `jsonwebtoken` and `reqwest`.
- Create `crates/pandar-hub/migrations/sqlite/20260622030000_phase_10_external_identity.sql`.
- Create `crates/pandar-hub/migrations/postgres/20260622030000_phase_10_external_identity.sql`.
- Modify `crates/pandar-hub/src/repositories/mod.rs` for duplicate identity errors.
- Modify `crates/pandar-hub/src/repositories/auth.rs` for identity link/resolve methods.
- Modify `crates/pandar-hub/src/repositories/tests/auth.rs` and `crates/pandar-hub/src/repositories/tests/postgres.rs` for SQLite/PostgreSQL identity behavior.
- Create `crates/pandar-hub/src/identity.rs` for config parsing, JWKS source abstraction, JWT verification, claims, and unit tests.
- Modify `crates/pandar-hub/src/lib.rs` to store optional external auth verifier in `AppState`.
- Modify `crates/pandar-hub/src/routes/auth.rs` to authenticate API tokens first, then external JWTs.
- Modify `crates/pandar-hub/src/routes/tests.rs` and `crates/pandar-hub/src/routes/tests/printers.rs` for route and WebSocket JWT tests.
- Create `frontend/app/api-auth.ts` for bearer header construction.
- Modify `frontend/app/page.tsx` and `frontend/app/actions.ts` to use the shared helper.
- Update `README.md`, `docs/architecture.md`, and `docs/roadmap.md` after implementation approval.

## Task 1: Add Dependencies And Identity Schema

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/pandar-hub/Cargo.toml`
- Create: `crates/pandar-hub/migrations/sqlite/20260622030000_phase_10_external_identity.sql`
- Create: `crates/pandar-hub/migrations/postgres/20260622030000_phase_10_external_identity.sql`

- [ ] **Step 1: Add workspace dependencies**

In `Cargo.toml`, add:

```toml
jsonwebtoken = { version = "10.4.0", default-features = true, features = ["aws-lc-rs"] }
reqwest = { version = "0.13.4", default-features = false, features = ["json", "rustls"] }
```

`async-trait` already exists in `[workspace.dependencies]`; do not duplicate it.

In `crates/pandar-hub/Cargo.toml`, add:

```toml
async-trait.workspace = true
jsonwebtoken.workspace = true
reqwest.workspace = true
```

- [ ] **Step 2: Add SQLite migration**

Create `crates/pandar-hub/migrations/sqlite/20260622030000_phase_10_external_identity.sql`:

```sql
CREATE TABLE user_identities (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    subject TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, user_id) REFERENCES users(tenant_id, id) ON DELETE CASCADE,
    UNIQUE (tenant_id, provider, subject),
    UNIQUE (tenant_id, user_id, provider)
);
```

- [ ] **Step 3: Add PostgreSQL migration**

Create `crates/pandar-hub/migrations/postgres/20260622030000_phase_10_external_identity.sql` with the same SQL as the SQLite migration. Do not add extra indexes for `(tenant_id, provider, subject)` or `(tenant_id, user_id)`; the unique constraints already back those lookups.

- [ ] **Step 4: Verify migrations compile**

Run:

```bash
cargo test -p pandar-hub db::tests::database_config_detects_sqlite_backend
```

Expected: the test passes and SQLx compile-time migration embedding succeeds.

## Task 2: Add Repository Identity Link And Resolve

**Files:**
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/repositories/auth.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/auth.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/postgres.rs`

- [ ] **Step 1: Add repository errors**

In `RepositoryError`, add:

```rust
#[error("external identity already exists for tenant")]
DuplicateExternalIdentity,
#[error("external identity provider already linked to user")]
DuplicateUserExternalIdentity,
```

- [ ] **Step 2: Add identity model**

In `auth.rs`, add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIdentity {
    pub id: String,
    pub tenant_id: TenantId,
    pub user_id: String,
    pub provider: String,
    pub subject: String,
    pub created_at: String,
}
```

- [ ] **Step 3: Add `link_external_identity`**

Implement on `AuthRepository`:

```rust
pub async fn link_external_identity(
    &self,
    tenant_id: TenantId,
    user_id: &str,
    provider: impl Into<String>,
    subject: impl Into<String>,
) -> RepositoryResult<UserIdentity>
```

Implementation requirements:

- Generate a UUID string id and `created_at_now()`.
- Insert only when `users.id` exists for the same tenant.
- Map missing user to `RepositoryError::MissingUser`.
- Map unique `(tenant_id, provider, subject)` to `DuplicateExternalIdentity`.
- Map unique `(tenant_id, user_id, provider)` to `DuplicateUserExternalIdentity`.
- Preserve database context with `anyhow::Context` for unknown errors.

- [ ] **Step 4: Add `authenticate_external_identity`**

Implement on `AuthRepository`:

```rust
pub async fn authenticate_external_identity(
    &self,
    tenant_id: TenantId,
    provider: &str,
    subject: &str,
) -> RepositoryResult<Option<AuthenticatedUser>>
```

Return `AuthenticatedUser { token_id: identity_id, user }` by joining `user_identities` to `users` on `(tenant_id, user_id)`.

- [ ] **Step 5: Export identity model**

In `repositories/mod.rs`, export `UserIdentity`.

- [ ] **Step 6: Add SQLite repository tests**

Add tests in `repositories/tests/auth.rs`:

```rust
#[tokio::test]
async fn external_identity_resolves_tenant_user_role() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants.create("acme-identity", "Acme Identity").await.unwrap();
    let user = auth
        .create_user(tenant.id, "viewer@example.test", "Viewer", UserRole::Viewer)
        .await
        .unwrap();

    let identity = auth
        .link_external_identity(tenant.id, &user.id, "clerk", "user_123")
        .await
        .unwrap();
    let authenticated = auth
        .authenticate_external_identity(tenant.id, "clerk", "user_123")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(identity.tenant_id, tenant.id);
    assert_eq!(identity.user_id, user.id);
    assert_eq!(authenticated.token_id, identity.id);
    assert_eq!(authenticated.user.id, user.id);
    assert_eq!(authenticated.user.role, UserRole::Viewer);
}

#[tokio::test]
async fn external_identity_rejects_missing_and_duplicate_links() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants.create("acme-identity-duplicates", "Acme Identity").await.unwrap();
    let user = auth
        .create_user(tenant.id, "admin@example.test", "Admin", UserRole::TenantAdmin)
        .await
        .unwrap();

    let missing = auth
        .link_external_identity(tenant.id, "missing-user", "clerk", "user_missing")
        .await
        .unwrap_err();
    assert!(matches!(missing, RepositoryError::MissingUser));

    auth.link_external_identity(tenant.id, &user.id, "clerk", "user_123")
        .await
        .unwrap();

    let duplicate_identity = auth
        .link_external_identity(tenant.id, &user.id, "clerk", "user_123")
        .await
        .unwrap_err();
    assert!(matches!(
        duplicate_identity,
        RepositoryError::DuplicateExternalIdentity
    ));

    let duplicate_user_provider = auth
        .link_external_identity(tenant.id, &user.id, "clerk", "user_456")
        .await
        .unwrap_err();
    assert!(matches!(
        duplicate_user_provider,
        RepositoryError::DuplicateUserExternalIdentity
    ));
}
```

The first test creates a tenant user, links `("clerk", "user_123")`, resolves it, and asserts user id plus role.

The second test asserts:

- linking a random user id returns `MissingUser`
- linking the same `(tenant, provider, subject)` twice returns `DuplicateExternalIdentity`
- linking another subject for the same `(tenant, user, provider)` returns `DuplicateUserExternalIdentity`

- [ ] **Step 7: Extend PostgreSQL auth tests**

In `postgres_auth_and_audit_repository_behavior_when_configured`, link and resolve `("logto", "logto-user")` and assert the resolved user id and role.

Add a PostgreSQL parity test named:

```rust
#[tokio::test]
async fn postgres_external_identity_error_behavior_when_configured()
```

The test must skip when `PANDAR_TEST_POSTGRES_URL` is unset, matching existing PostgreSQL tests. When configured, it must assert the same stable errors as the SQLite duplicate test:

```rust
let missing = auth
    .link_external_identity(tenant.id, "missing-user", "logto", "missing")
    .await
    .unwrap_err();
assert!(matches!(missing, RepositoryError::MissingUser));

auth.link_external_identity(tenant.id, &user.id, "logto", "subject-1")
    .await
    .unwrap();

let duplicate_identity = auth
    .link_external_identity(tenant.id, &user.id, "logto", "subject-1")
    .await
    .unwrap_err();
assert!(matches!(
    duplicate_identity,
    RepositoryError::DuplicateExternalIdentity
));

let duplicate_user_provider = auth
    .link_external_identity(tenant.id, &user.id, "logto", "subject-2")
    .await
    .unwrap_err();
assert!(matches!(
    duplicate_user_provider,
    RepositoryError::DuplicateUserExternalIdentity
));
```

- [ ] **Step 8: Run repository tests**

Run:

```bash
cargo test -p pandar-hub repositories::tests::auth
```

Expected: all auth repository tests pass. If `PANDAR_TEST_POSTGRES_URL` is unset, PostgreSQL test prints the existing skip message.

## Task 3: Add External Auth Config And JWT Verifier

**Files:**
- Create: `crates/pandar-hub/src/identity.rs`
- Modify: `crates/pandar-hub/src/lib.rs`

- [ ] **Step 1: Define config and errors**

Create `identity.rs` with:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAuthConfig {
    pub provider: String,
    pub issuer: String,
    pub jwks_url: String,
    pub audience: Option<String>,
    pub algorithms: Vec<jsonwebtoken::Algorithm>,
    pub authorized_parties: Vec<String>,
    pub required_scopes: Vec<String>,
    pub leeway_seconds: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ExternalAuthConfigError {
    #[error("partial external auth config without provider")]
    PartialWithoutProvider,
    #[error("missing external auth config value: {0}")]
    Missing(&'static str),
    #[error("unsupported external auth algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("invalid external auth leeway seconds")]
    InvalidLeeway,
}

#[derive(Debug, thiserror::Error)]
pub enum JwtVerifyError {
    #[error("invalid jwt header")]
    InvalidHeader(#[source] jsonwebtoken::errors::Error),
    #[error("missing jwt key id")]
    MissingKeyId,
    #[error("unsupported jwt algorithm")]
    UnsupportedAlgorithm,
    #[error("failed to load jwks")]
    Jwks(#[source] anyhow::Error),
    #[error("unknown jwt key id")]
    UnknownKeyId,
    #[error("unsupported jwk")]
    UnsupportedJwk,
    #[error("jwk algorithm mismatch")]
    JwkAlgorithmMismatch,
    #[error("invalid jwt claims")]
    InvalidClaims(#[source] jsonwebtoken::errors::Error),
    #[error("missing jwt subject")]
    MissingSubject,
    #[error("unauthorized jwt authorized party")]
    UnauthorizedParty,
    #[error("missing required jwt scope")]
    MissingScope,
}
```

Required config behavior:

- `from_env()` returns `Ok(None)` when no `PANDAR_EXTERNAL_AUTH_*` vars are present.
- `from_env()` returns an error when non-provider external vars are set without `PANDAR_EXTERNAL_AUTH_PROVIDER`.
- Provider set requires issuer and JWKS URL.
- Algorithms parse only `RS256`, `RS384`, `RS512`; default is `RS256`.
- Leeway parses as `u64` and defaults to `60`.

- [ ] **Step 2: Define claims**

Add:

```rust
#[derive(Debug, Clone, serde::Deserialize)]
struct JwtClaims {
    iss: String,
    sub: String,
    exp: u64,
    nbf: Option<u64>,
    aud: Option<AudienceClaim>,
    azp: Option<String>,
    scope: Option<String>,
    scp: Option<Vec<String>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
enum AudienceClaim {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedExternalIdentity {
    pub provider: String,
    pub subject: String,
    pub issuer: String,
    pub audiences: Vec<String>,
    pub authorized_party: Option<String>,
    pub scopes: Vec<String>,
}
```

- [ ] **Step 3: Add JWKS source abstraction**

Use:

```rust
#[async_trait::async_trait]
trait JwksSource: Send + Sync {
    async fn load_jwks(&self) -> anyhow::Result<jsonwebtoken::jwk::JwkSet>;
}
```

Production `RemoteJwksSource` uses `reqwest::Client` to GET `jwks_url` and deserialize JSON.

- [ ] **Step 4: Implement `JwtVerifier`**

Implement:

```rust
#[derive(Clone)]
pub struct JwtVerifier {
    config: ExternalAuthConfig,
    jwks_source: Arc<dyn JwksSource>,
    cache: Arc<tokio::sync::RwLock<Option<jsonwebtoken::jwk::JwkSet>>>,
}

impl JwtVerifier {
    pub fn remote(config: ExternalAuthConfig) -> Self;
    pub async fn verify(&self, token: &str) -> Result<VerifiedExternalIdentity, JwtVerifyError>;
}
```

Also add a test-only constructor used by route tests:

```rust
#[cfg(test)]
pub fn static_jwks(config: ExternalAuthConfig, jwks: jsonwebtoken::jwk::JwkSet) -> Self;
```

`static_jwks` must build a verifier with a `StaticJwksSource` and a preloaded cache so tests never open network sockets.

Verification behavior:

- Decode header and require `kid`.
- Require header algorithm in config allow-list.
- Load cached JWKS or fetch it.
- If `kid` is missing from cache, refresh JWKS once and retry lookup.
- Require supported JWK and matching JWK `alg` when present.
- Use `DecodingKey::from_jwk`.
- Use `Validation` with configured algorithms, issuer, audience when configured, `validate_exp = true`, `validate_nbf = true`, required claims from the spec, and configured leeway.
- After decoding, reject blank `sub`.
- Validate `azp` exactly when authorized parties are configured.
- Validate all required scopes against `scope` string plus `scp` array.

- [ ] **Step 5: Add unit tests**

Add unit tests in `identity.rs` for:

- partial env config returns an error
- default algorithm is RS256
- unsupported algorithm config is rejected
- `AudienceClaim` accepts string and array
- required scopes can be satisfied by `scope` string and `scp` array

JWT signature route tests are covered in Task 5 using the injectable verifier or local JWKS fixture.

- [ ] **Step 6: Wire `AppState` config**

In `lib.rs`, add `pub mod identity;` and an `external_auth: Option<identity::JwtVerifier>` field.

Add:

```rust
pub async fn connect_with_auth_config(
    database_url: impl Into<String>,
    job_storage: JobStorageConfig,
    external_auth: Option<identity::JwtVerifier>,
) -> anyhow::Result<Self>
```

Keep existing `connect` and `connect_with_config` behavior by parsing `ExternalAuthConfig::from_env()` and constructing `JwtVerifier::remote` only when configured.

Add test-only constructor:

```rust
#[cfg(test)]
pub fn with_external_auth(self, verifier: identity::JwtVerifier) -> Self
```

Add accessor:

```rust
pub fn external_auth(&self) -> Option<&identity::JwtVerifier>
```

- [ ] **Step 7: Run identity tests**

Run:

```bash
cargo test -p pandar-hub identity
```

Expected: config and claim helper tests pass.

## Task 4: Integrate External JWT Auth Into HTTP And WebSocket Authorization

**Files:**
- Modify: `crates/pandar-hub/src/routes/auth.rs`

- [ ] **Step 1: Keep bearer extraction unchanged**

Do not change the current missing/invalid header behavior:

- missing header returns `401 missing_auth_token`
- non-`Bearer ` header returns `401 invalid_auth_token`

- [ ] **Step 2: Replace credential resolution with API-token-first JWT fallback**

Use this single control flow:

```rust
let authenticated = if let Some(authenticated) = state.auth().authenticate_bearer(token).await? {
    authenticated
} else if let Some(verifier) = state.external_auth() {
    let verified = verifier.verify(token).await.map_err(|err| {
        tracing::debug!(
            error = %format!("{err:#}"),
            "external bearer token verification failed"
        );
        ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token")
    })?;
    state
        .auth()
        .authenticate_external_identity(tenant_id, &verified.provider, &verified.subject)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::FORBIDDEN, "tenant_forbidden"))?
} else {
    return Err(ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token"));
};
```

This is the only place where external JWT auth is attempted. API tokens remain first, external JWT verification runs only after no API token matches, and unconfigured external auth preserves the old `401 invalid_auth_token` behavior.

- [ ] **Step 3: Share role and tenant checks**

Keep final role enforcement exactly as today:

```rust
if authenticated.user.tenant_id != tenant_id {
    return Err(ApiError::new(StatusCode::FORBIDDEN, "tenant_forbidden"));
}
if !authenticated.user.role.allows(required_role) {
    return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
}
```

- [ ] **Step 4: Run route auth tests**

Run:

```bash
cargo test -p pandar-hub routes::tests::agents::missing_token_on_agent_list_returns_unauthorized
```

Expected: existing auth error behavior still passes.

## Task 5: Add Local JWKS Route Tests

**Files:**
- Modify: `crates/pandar-hub/src/routes/tests.rs`
- Modify: `crates/pandar-hub/src/routes/tests/agents.rs`
- Modify: `crates/pandar-hub/src/routes/tests/jobs.rs`
- Modify: `crates/pandar-hub/src/routes/tests/printers.rs`

- [ ] **Step 1: Add JWT test helper**

In `routes/tests.rs`, add helpers:

```rust
const TEST_PRIVATE_KEY_PEM: &str = include_str!("tests/fixtures/external_auth_private.pem");
const TEST_PUBLIC_JWK_JSON: &str = include_str!("tests/fixtures/external_auth_jwks.json");
const TEST_ISSUER: &str = "https://identity.example.test";
const TEST_AUDIENCE: &str = "https://api.pandar.test";

fn external_auth_state(state: AppState) -> AppState {
    let config = crate::identity::ExternalAuthConfig {
        provider: "clerk".to_owned(),
        issuer: TEST_ISSUER.to_owned(),
        jwks_url: "https://identity.example.test/.well-known/jwks.json".to_owned(),
        audience: Some(TEST_AUDIENCE.to_owned()),
        algorithms: vec![jsonwebtoken::Algorithm::RS256],
        authorized_parties: Vec::new(),
        required_scopes: Vec::new(),
        leeway_seconds: 60,
    };
    let jwks = serde_json::from_str(TEST_PUBLIC_JWK_JSON).unwrap();
    state.with_external_auth(crate::identity::JwtVerifier::static_jwks(config, jwks))
}

fn jwt_for(
    subject: &str,
    issuer: &str,
    audience: &str,
    kid: &str,
    exp_offset_seconds: i64,
) -> String {
    #[derive(serde::Serialize)]
    struct Claims<'a> {
        iss: &'a str,
        sub: &'a str,
        aud: &'a str,
        exp: u64,
        nbf: u64,
    }

    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let exp = now.saturating_add(exp_offset_seconds) as u64;
    let nbf = now.saturating_sub(30) as u64;
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(kid.to_owned());
    jsonwebtoken::encode(
        &header,
        &Claims {
            iss: issuer,
            sub: subject,
            aud: audience,
            exp,
            nbf,
        },
        &jsonwebtoken::EncodingKey::from_rsa_pem(TEST_PRIVATE_KEY_PEM.as_bytes()).unwrap(),
    )
    .unwrap()
}

async fn external_auth_token_for_role(
    state: &AppState,
    tenant_id: TenantId,
    role: UserRole,
    subject: &str,
) -> String {
    let user = state
        .auth()
        .create_user(
            tenant_id,
            format!("{subject}@example.test"),
            "External User",
            role,
        )
        .await
        .unwrap();
    state
        .auth()
        .link_external_identity(tenant_id, &user.id, "clerk", subject)
        .await
        .unwrap();
    jwt_for(subject, TEST_ISSUER, TEST_AUDIENCE, "test-key", 3600)
}
```

Create the test fixtures under `crates/pandar-hub/src/routes/tests/fixtures/`. Use an RSA keypair generated for tests only. The public JWK must include `kid: "test-key"` and `alg: "RS256"`. The verifier uses a test-only static JWKS source so no network sockets are opened.

- [ ] **Step 2: Add API-token preservation test**

Add route test:

```rust
#[tokio::test]
async fn api_token_auth_still_succeeds_when_external_auth_is_configured()
```

Configure external auth on `AppState`, create a normal API token, call an existing tenant route, and assert success.

- [ ] **Step 3: Add valid linked JWT read test**

Add route test:

```rust
#[tokio::test]
async fn linked_external_jwt_can_read_tenant_resource()
```

Create tenant, user, identity link, signed JWT, and call `GET /api/v1/tenants/{tenant_id}/agents`; expect `200`.

- [ ] **Step 4: Add operator job creation test**

In `jobs.rs`, add:

```rust
#[tokio::test]
async fn linked_operator_jwt_can_create_print_job()
```

Use the same artifact fixture pattern as existing API-token job tests. Assert `201`.

- [ ] **Step 5: Add JWT rejection tests**

Add tests for:

- unknown `kid` returns `401 invalid_auth_token`
- wrong issuer returns `401 invalid_auth_token`
- wrong audience returns `401 invalid_auth_token`
- expired token returns `401 invalid_auth_token`
- valid unlinked JWT returns `403 tenant_forbidden`
- linked viewer JWT creating an agent or job returns `403 role_forbidden`

- [ ] **Step 6: Add WebSocket JWT tests**

In `printers.rs`, add or extend WebSocket tests:

- valid linked viewer JWT can connect to `/printer-events`
- valid unlinked JWT receives pre-upgrade `403 tenant_forbidden`

Reuse existing `tokio_tungstenite` setup.

- [ ] **Step 7: Run route tests**

Run:

```bash
cargo test -p pandar-hub routes::tests
```

Expected: all route tests pass with no external network access.

## Task 6: Add Frontend Bearer Helper

**Files:**
- Create: `frontend/app/api-auth.ts`
- Modify: `frontend/app/page.tsx`
- Modify: `frontend/app/actions.ts`

- [ ] **Step 1: Create helper**

Create `frontend/app/api-auth.ts`:

```ts
import { cookies } from 'next/headers'

const apiToken = process.env.APP_API_TOKEN
const staticAuthToken = process.env.APP_AUTH_BEARER_TOKEN
const authCookieName = process.env.APP_AUTH_COOKIE_NAME ?? 'pandar_auth_token'

export async function apiHeaders(contentType?: string) {
  const headers: Record<string, string> = {}
  if (contentType) {
    headers['content-type'] = contentType
  }

  const cookieStore = await cookies()
  const cookieToken = cookieStore.get(authCookieName)?.value
  const token = cookieToken || staticAuthToken || apiToken
  if (token) {
    headers.authorization = `Bearer ${token}`
  }

  return Object.keys(headers).length > 0 ? headers : undefined
}
```

- [ ] **Step 2: Update page fetches**

In `page.tsx`, remove local `apiToken` and `apiHeaders()` and import:

```ts
import { apiHeaders } from './api-auth'
```

In `fetchJson`, call:

```ts
headers: await apiHeaders(),
```

- [ ] **Step 3: Update server action**

In `actions.ts`, remove local `apiToken` and `apiHeaders()`, import `apiHeaders`, and use:

```ts
headers: await apiHeaders('application/json'),
```

- [ ] **Step 4: Build frontend**

Run:

```bash
cd frontend && npm run build
```

Expected: Next.js build succeeds.

## Task 7: Update Documentation

**Files:**
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update README auth config**

Document:

- `PANDAR_EXTERNAL_AUTH_PROVIDER`
- `PANDAR_EXTERNAL_AUTH_ISSUER`
- `PANDAR_EXTERNAL_AUTH_JWKS_URL`
- `PANDAR_EXTERNAL_AUTH_AUDIENCE`
- `PANDAR_EXTERNAL_AUTH_ALGORITHMS`
- `PANDAR_EXTERNAL_AUTH_AUTHORIZED_PARTIES`
- `PANDAR_EXTERNAL_AUTH_REQUIRED_SCOPES`
- `PANDAR_EXTERNAL_AUTH_LEEWAY_SECONDS`
- `APP_AUTH_COOKIE_NAME`
- `APP_AUTH_BEARER_TOKEN`

State that provider tokens authenticate identity only; Pandar local user identity links and roles authorize tenants.

- [ ] **Step 2: Update architecture**

Add Phase 10 as implemented:

- API tokens are checked first.
- External JWTs are verified through JWKS.
- `(tenant_id, provider, subject)` resolves local user records.
- Clerk/Logto organization claims do not authorize tenants.

- [ ] **Step 3: Update roadmap**

Move Phase 10 bullets to completed tense, add it under `Completed`, and change `Immediate Next` to Phase 11 provisioning/admin boundaries.

## Task 8: Full Verification, Review, Commit, Push

**Files:**
- All Phase 10 files.

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt
```

Expected: exits 0.

- [ ] **Step 2: Clippy**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: exits 0.

- [ ] **Step 3: Rust tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all workspace tests pass.

- [ ] **Step 4: Frontend build**

Run:

```bash
cd frontend && npm run build
```

Expected: build succeeds.

- [ ] **Step 5: Generated protobuf guard**

Run:

```bash
find . -path './target' -prune -o -path './.git' -prune -o \( -name '*.pb.rs' -o -name '*.tonic.rs' \) -print
```

Expected: no output.

- [ ] **Step 6: Diff review**

Run:

```bash
git status --short
git diff --check
```

Expected: only intended Phase 10 files are changed and diff check passes.

- [ ] **Step 7: Required `$sdd-workflow` implementation review**

Dispatch the required `$sdd-workflow` reviewers with the spec, this plan, final diff, and verification outputs. This always includes an independent reviewer subagent. If `opencode-agent` is available, also run the same bounded review through `opencode-agent`. All required reviewers must return `VERDICT: APPROVE` before docs/fresh verification/commit/push are treated as complete.

Required verdict:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- [implemented requirement or missing requirement]
BLOCKERS:
- [blocking gap or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

Fix and re-review until the verdict is `VERDICT: APPROVE`.

- [ ] **Step 8: Commit and push after implementation approval**

Do this only after all required implementation reviewers return `VERDICT: APPROVE` and all verification above passes. This is required by the active user objective and `$sdd-workflow`; it is not part of spec/plan review.

Commit with Lore protocol:

```text
Authenticate browser users through external identity

Constraint: Clerk/Logto authenticate identity while Pandar owns tenant authorization.
Rejected: Provider organization authorization | tenant access must remain in Rust-managed records.
Confidence: high
Scope-risk: broad
Directive: Preserve API-token auth before external JWT fallback.
Tested: cargo fmt; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path "Cargo.toml" --workspace; cd frontend && npm run build; git diff --check
Not-tested: Live Clerk/Logto tenants and remote JWKS rotation against production providers.
```

Push to the current `main` upstream.
