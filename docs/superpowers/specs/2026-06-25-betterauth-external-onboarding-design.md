# Phase 30-33 Better Auth And External Onboarding Design

## Goal

Support Better Auth as a first-class external auth provider alongside Clerk and Logto, then move browser onboarding away from manually managed Pandar users toward external-account sign-in, tenant self-creation, and tenant-admin-issued join links.

## Decisions From Design Review

- Better Auth is consumed as an external OAuth/OIDC/JWT issuer, not as a Pandar-owned session cookie.
- The first Better Auth-compatible path supports Better Auth RSA JWT keys exposed through JWKS. Better Auth's JWT plugin key-generation option is `RSA256`, while Pandar verifies the emitted JWT/JWK with the JWA algorithm `RS256`. `ES256`, `EdDSA`, and `PS256` are future work unless Pandar's Rust verifier is explicitly expanded.
- External account managers own human account identity. Pandar owns tenant membership and tenant role.
- Pandar `users` remain tenant-local projections of external accounts, not global accounts.
- Automatic user projection creation requires a verified email claim.
- A signed-in external user with no tenant can create a tenant or accept a join link.
- Creating a tenant makes the current external user a `tenant_admin`.
- Accepting a join link creates the current external user as a tenant-local user with the role encoded in the link.
- Join links are tenant-admin-managed, can optionally restrict email, default to single-use, and default to seven-day expiry.
- Existing members accepting a join link keep their current role and do not consume the link.
- Tenant self-creation is enabled by default and can be disabled by configuration.
- A verified external user may create additional tenants even when already a member of other tenants.
- Existing manual user creation and manual identity linking APIs remain as transitional/admin-only paths in the first onboarding phase and are removed in a later phase.
- `pandar-web` must be able to choose Clerk, Logto, or Better Auth through configuration.
- A later self-hosted Better Auth bundle phase provides a new-deployment option only. It uses a Better Auth-owned database or schema and still integrates with `pandar-hub` through JWT/JWKS.

## Current-State Constraints

`pandar-hub` already has a provider-neutral JWT verifier configured by:

- `PANDAR_EXTERNAL_AUTH_PROVIDER`
- `PANDAR_EXTERNAL_AUTH_ISSUER`
- `PANDAR_EXTERNAL_AUTH_JWKS_URL`
- `PANDAR_EXTERNAL_AUTH_AUDIENCE`
- `PANDAR_EXTERNAL_AUTH_ALGORITHMS`
- optional authorized parties, scopes, and leeway.

The verifier currently accepts RSA JWKs with `RS256`, `RS384`, or `RS512`. Better Auth deployments must configure Better Auth RSA key generation and confirm the emitted JWT header/JWK are compatible with Pandar's `RS256` verifier.

Current external auth resolves `(tenant_id, provider, subject)` against `user_identities`. That remains the tenant-scoped membership lookup. The new onboarding work adds controlled creation of those local rows from a verified external bearer identity.

`pandar-web` currently forwards bearer values from a request cookie, `APP_AUTH_BEARER_TOKEN`, or `APP_API_TOKEN`. The new provider integration must preserve this bearer-token boundary so `pandar-hub` stays provider-neutral.

## Phase 30: Better Auth Provider Compatibility

Phase 30 makes Better Auth a documented and test-covered external provider without changing tenant authorization behavior.

### Backend Behavior

- `pandar-hub` accepts `PANDAR_EXTERNAL_AUTH_PROVIDER=betterauth`.
- Documentation shows Better Auth configured with `jwt({ jwks: { keyPairConfig: { alg: "RSA256" } } })`, and explicitly distinguishes that Better Auth configuration value from Pandar's `PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256` verifier setting.
- The JWT verifier extracts identity profile claims needed by later onboarding:
  - `sub`
  - `email`
  - `email_verified`
  - `name`
  - `preferred_username`
- `VerifiedExternalIdentity` carries optional profile fields in addition to provider, subject, issuer, audiences, authorized party, and scopes.
- Existing tenant route authorization remains unchanged: tenant APIs still require an existing `(tenant_id, provider, subject)` identity link until Phase 31 onboarding creates one.

### Tests

- Unit tests cover verified profile claim extraction.
- Unit tests reject missing or false `email_verified` only in onboarding helpers, not in ordinary token verification.
- Route tests continue proving linked external identities can authorize tenant APIs.
- Documentation examples cover Clerk, Logto, and Better Auth config.

## Phase 31: External Self-Service Tenant Onboarding

Phase 31 adds the product path for signed-in external users to discover memberships, create tenants, and join tenants without manual Pandar user creation.

### Identity Bootstrap API

Add:

```http
GET /api/v1/me
Authorization: Bearer <external JWT>
```

Behavior:

- Requires external JWT auth. Tenant tokens and bootstrap tokens do not authenticate this route.
- Returns the verified external identity profile and all tenant-local Pandar memberships linked to `(provider, subject)`.
- If the token has no verified email, the response exposes that onboarding cannot continue until the auth provider supplies one.
- Does not create tenants, users, or identities.

Response shape:

```json
{
  "identity": {
    "provider": "betterauth",
    "subject": "user_123",
    "email": "alice@example.com",
    "email_verified": true,
    "display_name": "Alice"
  },
  "tenants": [
    {
      "tenant_id": "...",
      "tenant_slug": "acme",
      "display_name": "Acme",
      "role": "operator"
    }
  ],
  "can_self_create_tenant": true
}
```

### Tenant Self-Creation API

Add:

```http
POST /api/v1/onboarding/tenants
Authorization: Bearer <external JWT>
```

Request:

```json
{
  "slug": "acme",
  "display_name": "Acme"
}
```

Behavior:

- If `PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE` is unset or true, a verified external user can create a tenant.
- If the setting is false, return `403 tenant_self_create_disabled`.
- Requires `sub`, verified `email`, and a display name fallback.
- Existing memberships in other tenants do not block creating a new tenant.
- Creates the tenant, tenant-local user, and `user_identities` link in one database transaction.
- The created user role is `tenant_admin`.
- Duplicate tenant slug returns the existing duplicate-slug error behavior.
- Existing bootstrap-protected `POST /api/v1/tenants` remains unchanged for cross-tenant admin creation.

### Join Link Model

Add a `join_links` table with equivalent SQLite and PostgreSQL migrations:

- `id`
- `tenant_id`
- `token_hash`
- `role`
- `email_constraint` nullable
- `expires_at`
- `max_uses`
- `used_count`
- `created_by_user_id`
- `revoked_at` nullable
- `created_at`

Token plaintext is returned only once on creation. Only token hashes are stored.

Defaults:

- `max_uses = 1`
- `expires_at = now + 7 days`
- `email_constraint = null`

### Join Link Admin API

Add tenant-admin-only routes:

```http
POST /api/v1/tenants/{tenant_id}/join-links
GET /api/v1/tenants/{tenant_id}/join-links
DELETE /api/v1/tenants/{tenant_id}/join-links/{join_link_id}
```

Create request:

```json
{
  "role": "operator",
  "email": "alice@example.com",
  "expires_in_seconds": 604800,
  "max_uses": 1
}
```

Behavior:

- Only `tenant_admin` can create, list, and revoke join links.
- Create response includes the plaintext join token or URL once.
- List response never includes plaintext tokens or token hashes.
- Revoke sets `revoked_at` and is idempotent for already revoked links.

### Join Link Accept API

Add:

```http
POST /api/v1/join-links/accept
Authorization: Bearer <external JWT>
Content-Type: application/json
```

Request:

```json
{
  "token": "pandar_join..."
}
```

Behavior:

- Requires external JWT auth with verified email.
- Looks up by token hash; the request does not include tenant id.
- Browser join URLs may use a fragment such as `/join#<token>` so the plaintext secret is not sent in HTTP request paths, query strings, access logs, or `Referer` headers. The frontend reads the fragment client-side and submits the token in the JSON body.
- Invalid, expired, revoked, or used-up token returns `404 invalid_join_link`.
- If `email_constraint` is set and does not match the verified email claim, return `403 join_link_email_mismatch`.
- If the external identity is not yet a member of the target tenant, create tenant-local user and identity link with the link role, increment `used_count`, and record audit events.
- If the external identity is already a member of the target tenant, return the existing membership, do not change role, and do not increment `used_count`.

Response:

```json
{
  "tenant": {
    "id": "...",
    "slug": "acme",
    "display_name": "Acme"
  },
  "membership": {
    "user_id": "...",
    "role": "operator",
    "created": true
  }
}
```

### Repository Behavior

Add repository methods that work identically on SQLite and PostgreSQL:

- list memberships for external identity across tenants.
- create tenant with external admin projection transactionally.
- create/list/revoke join links.
- accept join link transactionally with concurrency-safe use counting.

Concurrency requirements:

- Accept uses a transaction and a conditional persistence step that increments `used_count` only when `used_count < max_uses`, `revoked_at is null`, and `expires_at` is still in the future.
- Two concurrent accepts of a single-use link must create at most one new membership and increment `used_count` at most once.
- Used-count updates must not allow more than `max_uses` successful new memberships, on both SQLite and PostgreSQL.
- Existing-member accepts do not consume uses.

### Join Token Requirements

- Generate join tokens with Pandar's existing CSPRNG-backed secret helper or an equivalent helper with at least 192 bits of entropy.
- Use prefix `pandar_join` so generated secrets are recognizable as join-link tokens without revealing tenant or role data.
- Store only `SHA-256(token)` in `join_links.token_hash`, matching existing token-hash practice in the hub.
- Return plaintext only in the create response. Do not persist plaintext in the database.
- Do not write plaintext tokens or token hashes to audit metadata, metrics labels, structured logs, route list responses, or frontend persisted state.

### External Identity Profile Rules

- `sub` is always required for external identity verification.
- Automatic onboarding actions require `email` and `email_verified = true`.
- `display_name` is derived as `name`, then `preferred_username`, then `email`.
- If `email` is absent or `email_verified` is not true, tenant self-create and join-link accept return `400 external_email_unverified`.
- If `display_name` cannot be derived after the email requirement is satisfied, use the verified email as the display name.

### Frontend Behavior

`pandar-web` gains provider-configured onboarding:

- `APP_AUTH_PROVIDER=clerk|logto|betterauth|none`
- provider-specific public settings:
  - Clerk: `APP_AUTH_CLERK_PUBLISHABLE_KEY`
  - Logto: `APP_AUTH_LOGTO_ENDPOINT`, `APP_AUTH_LOGTO_APP_ID`
  - Better Auth external: `APP_AUTH_BETTER_AUTH_BASE_URL`
  - `none`: development/static-token mode using the existing cookie/static bearer-token path; it renders no provider SDK sign-in button.
- sign-in/sign-out UI for the configured provider.
- bearer acquisition that writes or forwards the existing `APP_AUTH_COOKIE_NAME` value.
- `/api/v1/me` loading before dashboard tenant selection.
- empty-membership state with create-tenant and join-link actions.
- tenant-admin join link management UI.
- manual create-user and manual identity-link forms hidden from the primary UI.

The first implementation may use a small provider adapter boundary with a static-token/cookie adapter retained for development. It must not hard-code Better Auth as the only frontend provider.

## Phase 32: Remove Manual Pandar User Creation And Linking

Phase 32 removes transitional product/API paths after Phase 31 is stable.

Remove:

- `POST /api/v1/tenants/{tenant_id}/users`
- `POST /api/v1/tenants/{tenant_id}/users/{user_id}/identities`
- frontend forms that manually create users or manually link provider subjects.

Retain:

- list users.
- list external identities for audit/debugging.
- update tenant-local role.
- join link creation/list/revoke.

Migration behavior:

- Existing `users` and `user_identities` rows remain valid.
- No data migration is required solely to remove manual creation routes.
- Docs must explain that Pandar user rows are external-account tenant projections.
- Route tests must assert the removed paths no longer exist or no longer accept the old manual create/link requests.

## Phase 33: Self-Hosted Better Auth Bundle

Phase 33 provides an integrated deployment option for new installations.

Scope:

- `pandar-web` hosts Better Auth routes or a sidecar-compatible handler.
- Better Auth uses its own database, database schema, or SQLite file.
- Better Auth is configured with its `RSA256` JWT key-pair option and emits JWT/JWK material compatible with Pandar's `RS256` verifier.
- `pandar-hub` still verifies Better Auth through `PANDAR_EXTERNAL_AUTH_*`.
- Pandar does not read Better Auth database tables.

Out of scope:

- Clerk or Logto migration into Better Auth.
- Relinking existing external identities from one provider subject to another.
- Shared Pandar/Better Auth user tables.

## Security And Error Handling

- Bearer JWT verification failures return `401 invalid_auth_token`.
- Authenticated external identities without tenant membership remain unauthorized for tenant APIs until they create or join a tenant.
- Join links are secrets. URLs generated by Pandar must keep plaintext tokens in client-side URL fragments, and accept requests must submit tokens in JSON bodies. Logs, audit metadata, metrics, and list responses must not expose plaintext tokens or token hashes.
- Email-constrained join links require verified email.
- Audit events record user projection creation, tenant self-creation, join link creation, join link revocation, and successful new-member accepts. Audit metadata must not include raw external provider subject values.
- Tenant role authorization continues to use Pandar's `UserRole`.
- Rate limiting for tenant self-create and join-link accept is delegated to deployment reverse proxies for these phases. Pandar returns deterministic authorization and validation errors but does not add an in-process rate limiter.

## Documentation And Deployment Impact

- Update `docs/roadmap.md` after implementation to move completed items into the completed section and keep Phase 32/33 as future work until implemented.
- Update `docs/development.md` with Better Auth `RSA256` key-pair configuration, Pandar `RS256` verifier configuration, frontend provider environment variables, verified-email requirements, tenant self-create, and join-link usage.
- Update `docs/architecture.md` with the account-identity versus tenant-membership boundary, `/api/v1/me`, join links, and the self-hosted Better Auth future phase.
- Update Docker/NixOS/deployment examples when `pandar-web` provider selection requires new environment variables.

## Acceptance Criteria

- Better Auth RSA JWTs can be configured and documented like Clerk and Logto, with docs distinguishing Better Auth `keyPairConfig.alg = "RSA256"` from Pandar `PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256`.
- Existing linked external identity authorization still works.
- `/api/v1/me` returns external identity and linked tenant memberships without side effects.
- `/api/v1/me` exposes whether tenant self-create is currently allowed.
- A verified external user can self-create a tenant when enabled and becomes tenant admin.
- A verified external user who already belongs to another tenant can still self-create a new tenant when self-create is enabled.
- Self-create can be disabled by configuration.
- Tenant admins can create, list, and revoke join links.
- A verified external user can accept a valid join link and receive the link role.
- Email-constrained join links reject non-matching verified emails.
- Existing members accepting a join link keep their role and do not consume link usage.
- Equivalent SQLite and PostgreSQL migrations exist for every new table or column.
- Repository tests cover SQLite behavior for membership listing, tenant self-create, join-link create/list/revoke, join-link accept, existing-member accept, and duplicate/expired/revoked/used-up links.
- PostgreSQL repository tests cover the same database-dependent behavior when `PANDAR_TEST_POSTGRES_URL` is configured.
- Concurrent accept tests prove a single-use join link creates at most one new membership and increments `used_count` at most once on SQLite and on PostgreSQL when `PANDAR_TEST_POSTGRES_URL` is configured.
- Route tests prove plaintext join tokens and token hashes are absent from list responses, audit metadata, and metrics output.
- Audit tests prove tenant self-create, join-link create, join-link revoke, and successful new-member accept events are recorded without raw external subjects or secrets.
- Manual user creation/linking is hidden from the primary frontend path in Phase 31 and scheduled for removal in Phase 32.
- `pandar-web` chooses auth provider from configuration and keeps a provider-neutral bearer boundary to `pandar-hub`.
- Docs cover hub env vars, frontend provider env vars, Better Auth `RSA256`/JWKS setup, Pandar `RS256` verification, verified-email requirements, join-link behavior, and self-hosted Better Auth as later new-deployment-only scope.
- A documentation or smoke-test step confirms Better Auth's emitted JWT header/JWK is hub-compatible with `PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256`.
- Self-hosted Better Auth is documented as a later new-deployment-only phase with an independent Better Auth database/schema.
