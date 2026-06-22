# Phase 10: External Identity Authentication Design

Phase 10 lets browser users authenticate with Clerk or Logto while keeping tenant membership and tenant roles inside Pandar. The identity provider proves who the user is; Pandar decides which tenants that identity can access and which role it has there.

## Current Context

Phase 6 already added tenant API tokens, role checks, audit events, and WebSocket authorization. Phase 10 must preserve that service-token path for automation while adding signed identity-provider JWTs as another bearer-token credential type.

The current hub `users` table is tenant-scoped and stores the tenant role directly on the user row. Phase 10 will add identity links to those existing tenant-scoped users instead of introducing a global-user plus membership model. That keeps this phase focused and still satisfies the product rule that Pandar owns user-to-tenant access. Phase 11 can add provisioning screens and invite/link flows, and Phase 12 can continue the SeaORM repository migration.

## External Provider Facts

Clerk and Logto both issue JWTs that a backend API validates through provider public keys or JWKS.

- Clerk session-token verification requires signature validation, accepted signing algorithm, expiration and not-before checks, and optional authorized-party (`azp`) validation for trusted frontend origins.
- Logto API access-token validation requires signature validation through JWKS, issuer, audience/API resource, expiration, and optional scope or organization-context checks.

Pandar will not trust Clerk organizations or Logto organizations as tenant authorization sources in Phase 10.

## Design Choice

Use a provider-neutral OIDC/JWT verifier with one active external identity profile per hub process:

- `PANDAR_EXTERNAL_AUTH_PROVIDER`: provider key stored with identity links, normally `clerk` or `logto`.
- `PANDAR_EXTERNAL_AUTH_ISSUER`: required issuer claim.
- `PANDAR_EXTERNAL_AUTH_JWKS_URL`: required JWKS URL.
- `PANDAR_EXTERNAL_AUTH_AUDIENCE`: optional expected audience/API resource.
- `PANDAR_EXTERNAL_AUTH_ALGORITHMS`: comma-separated accepted algorithms, default `RS256`.
- `PANDAR_EXTERNAL_AUTH_AUTHORIZED_PARTIES`: optional comma-separated allowed `azp` values for Clerk-style session tokens.
- `PANDAR_EXTERNAL_AUTH_REQUIRED_SCOPES`: optional comma-separated scopes for Logto-style API tokens.
- `PANDAR_EXTERNAL_AUTH_LEEWAY_SECONDS`: optional clock leeway for `exp` and `nbf`, default `60`.

If `PANDAR_EXTERNAL_AUTH_PROVIDER` is unset, external identity auth is disabled and Phase 6 API-token auth remains the only accepted tenant credential.

If any other `PANDAR_EXTERNAL_AUTH_*` variable is set while `PANDAR_EXTERNAL_AUTH_PROVIDER` is unset, hub startup must fail with config context. If `PANDAR_EXTERNAL_AUTH_PROVIDER` is set, missing issuer or JWKS URL must also fail startup. Partial config must never silently disable external auth.

Running Clerk and Logto simultaneously in one hub process is out of scope for Phase 10. The table schema supports multiple provider keys so a future phase can enable multiple active profiles without changing existing identity records.

## Dependency Strategy

Phase 10 may add these dependencies because hand-rolled JOSE/JWT validation would be security-sensitive and harder to review:

- `jsonwebtoken` in `pandar-hub` for JWT header parsing, JWK-based decoding, signature verification, issuer/audience/time validation, and RS-family algorithm enforcement.
- `reqwest` in `pandar-hub` for HTTPS JWKS retrieval.

No Clerk or Logto backend SDK dependency is required in Rust. Provider integration stays at the OIDC/JWKS boundary.

## Authentication Flow

All tenant-scoped HTTP routes and the tenant WebSocket route continue to use `Authorization: Bearer <token>`.

Authorization checks run in this order:

1. Authenticate the bearer token as a Phase 6 tenant API token.
2. If no API token matches and external identity auth is configured, verify the bearer token as a JWT.
3. For a valid JWT, extract `sub` and resolve `(tenant_id, provider, subject)` through Pandar's `user_identities` repository.
4. If no tenant-local identity link exists, return `403 tenant_forbidden`.
5. If the linked user role is too weak, return `403 role_forbidden`.

This order preserves existing API-token behavior and lets automation tokens keep working even when external auth is configured.

JWT verification must reject:

- invalid token format
- missing `kid`
- unknown `kid`
- unsupported signing algorithm
- invalid signature
- wrong issuer
- wrong audience when audience is configured
- expired token
- token not yet valid when `nbf` is present
- missing or blank subject
- unauthorized `azp` when authorized parties are configured
- missing required scope when required scopes are configured

Verification errors return `401 invalid_auth_token`. A cryptographically valid token whose identity is not linked to the requested tenant returns `403 tenant_forbidden`.

Exact claim rules:

- Supported algorithms are `RS256`, `RS384`, and `RS512`; the configured allow-list defaults to only `RS256`. `none`, HMAC, ECDSA, and EdDSA are not accepted in Phase 10.
- The JWT header `alg` must be present and included in the configured allow-list. If the JWK contains an `alg`, it must match the JWT header algorithm.
- Required claims are `iss`, `sub`, and `exp`. `aud` is required only when `PANDAR_EXTERNAL_AUTH_AUDIENCE` is configured.
- `aud` may be a string or an array. Audience validation passes when any token audience exactly equals the configured audience.
- `nbf` is optional. When present, it is validated with the configured leeway.
- `iat` is not required and is not used for authorization.
- `azp` is optional unless authorized parties are configured. When configured, the token `azp` must exactly match one configured value.
- Scope validation requires every configured required scope to be present. Token scopes may come from a space-delimited `scope` string or a string-array `scp` claim.
- Provider organization claims are ignored for tenant authorization. Logto organization tokens may pass JWT validation, but tenant access still requires a local `(tenant_id, provider, subject)` identity link.

## Persistence

Add equivalent SQLite and PostgreSQL migrations for:

```text
user_identities
- id text primary key
- tenant_id text not null references tenants(id) on delete cascade
- user_id text not null
- provider text not null
- subject text not null
- created_at text not null
- foreign key (tenant_id, user_id) references users(tenant_id, id) on delete cascade
- unique (tenant_id, provider, subject)
- unique (tenant_id, user_id, provider)
```

`tenant_id` is part of the identity key because Phase 10 keeps the existing tenant-scoped user model. The same Clerk or Logto subject may be linked to different tenant-local users in different tenants, with each tenant role stored by Pandar.

Repository additions:

- `link_external_identity(tenant_id, user_id, provider, subject)` creates an identity link for an existing tenant user.
- `authenticate_external_identity(tenant_id, provider, subject)` returns the linked tenant user and identity id.

Both methods must keep SQLite/PostgreSQL behavior equivalent and return stable repository errors for missing users and duplicate links.

## Hub Runtime Boundary

Add a small `identity` module in `pandar-hub`:

- `ExternalAuthConfig` parses environment variables and validates required external-auth fields together.
- `JwtVerifier` owns JWKS retrieval, key selection by `kid`, algorithm allow-list enforcement, and claim validation.
- `VerifiedExternalIdentity` contains provider, subject, issuer, optional audience values, optional authorized party, and scopes.

JWKS retrieval is async and cached in memory. The verifier may refresh JWKS when the token `kid` is not present in the cache. Cache failures and verification failures must preserve lower-level error context in logs or returned internal errors.

The verifier must be injectable for tests so route tests can use local JWKS fixtures and avoid network access.

## Frontend Integration

The frontend remains provider-neutral. It will forward an externally supplied bearer token to Rust as `Authorization: Bearer`.

Phase 10 frontend changes:

- Keep existing `APP_API_TOKEN` service-token support for automation and local deployments.
- Add a shared frontend helper `frontend/app/api-auth.ts` used by server components and server actions.
- Token precedence is:
  1. Request cookie named by `APP_AUTH_COOKIE_NAME`, default `pandar_auth_token`.
  2. Static deployment bridge `APP_AUTH_BEARER_TOKEN`.
  3. Existing service token `APP_API_TOKEN`.
- The cookie path is for Clerk/Logto middleware or hosting glue that runs before the current server-rendered UI and writes a per-browser token. The helper only reads the cookie and forwards the bearer token; it does not implement sign-in.
- `APP_AUTH_BEARER_TOKEN` is a static deployment bridge for smoke tests or single-user deployments. It is not a per-browser identity source and must be documented as unsuitable for multi-user browser deployments.
- Document that provider SDK wiring, sign-in UI, invite flow, and user-facing identity-link management are Phase 11/15 product UX work.

This keeps Phase 10 focused on Rust token verification and authorization correctness while giving the current frontend a concrete bearer-forwarding integration point.

## Tests

Use local test keys and JWKS fixtures. Tests must not contact Clerk, Logto, or any external JWKS endpoint.

Required hub tests:

- API token auth still succeeds when external auth is configured.
- Valid JWT plus linked identity can read a tenant-scoped resource.
- Valid JWT plus linked operator identity can create a print job.
- Valid JWT with unknown `kid` returns `401 invalid_auth_token`.
- Validly signed JWT with wrong issuer returns `401 invalid_auth_token`.
- Validly signed JWT with wrong audience returns `401 invalid_auth_token` when audience is configured.
- Expired JWT returns `401 invalid_auth_token`.
- Valid JWT without a tenant-local identity link returns `403 tenant_forbidden`.
- Linked viewer JWT attempting an operator/admin route returns `403 role_forbidden`.
- Tenant WebSocket accepts a valid linked JWT and rejects an unlinked valid JWT.

Required repository tests:

- Linking an identity to an existing tenant user succeeds.
- Resolving a linked identity returns the tenant-local user and role.
- Linking a missing user returns the same missing-user behavior on SQLite and PostgreSQL.
- Duplicate `(tenant_id, provider, subject)` links are rejected.

## Documentation Updates

Update:

- `docs/architecture.md` with the Phase 10 auth flow and provider-neutral config.
- `docs/roadmap.md` to mark Phase 10 complete and move Immediate Next to Phase 11.
- `README.md` with the new environment variables and the rule that Clerk/Logto only authenticate identity; Pandar tenant roles authorize access.

## Acceptance Criteria

- Clerk- or Logto-style JWTs are verified by Rust with configured issuer, JWKS, algorithm, audience, time, authorized-party, and scope checks.
- A verified `{provider, subject}` can access tenant APIs only when Pandar has a tenant-local identity link for that tenant.
- Tenant roles are enforced from Pandar's local user records, not provider organizations or scopes.
- A valid identity-provider token without a Pandar tenant link is authenticated but not authorized.
- Existing Phase 6 API-token auth remains available for HTTP and WebSocket routes.
- SQLite and PostgreSQL migrations remain behaviorally equivalent.
- Tests cover local JWKS verification and tenant authorization failures without external network access.

## Out Of Scope

- Provider-hosted login UI.
- Clerk or Logto organization synchronization.
- Multiple active external identity profiles in one hub process.
- First-user bootstrap, invites, admin user management, and identity-linking APIs for end users.
- Replacing tenant-scoped users with global users plus `tenant_memberships`.
- Completing the SeaORM migration for auth repositories.
