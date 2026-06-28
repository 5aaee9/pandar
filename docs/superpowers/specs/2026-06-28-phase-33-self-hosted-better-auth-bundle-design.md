# Phase 33 — Self-Hosted Better Auth Issuer Design

Date: 2026-06-28
Status: Revised after independent spec review
Related: `2026-06-25-betterauth-external-onboarding-design.md` (Phases 30–32 external onboarding; this implements the Phase 33 "self-hosted bundle" called out there as future work).

## 1. Goal

Ship a self-hosted **Better Auth** issuer as a sibling application inside the
pandar repo, so a deployment can authenticate users without any external
identity SaaS. Pandar already consumes Better Auth as an external JWT/JWKS
issuer (Phase 30); this provides the issuer itself.

The driving deployment shape is a real multi-user, self-hosted instance:
passwordless passkey sign-in, open self-signup, and automatic tenant
self-creation.

## 2. Why a sibling issuer app (not inside pandar-web)

Phase 33 in the prior design left two options open: "pandar-web hosts Better
Auth routes or a sidecar-compatible handler." We choose the sidecar/sibling
form, not embedding in `pandar-web`, because:

- `pandar-web` is a prebuilt standalone Next.js server; embedding would mix a
  provider-specific auth server into the provider-neutral frontend and force a
  rebuild/rearchitecture of `frontend/`.
- Better Auth owns its own database/schema (Phase 33 boundary: "Better Auth
  uses its own database, database schema, or SQLite file. Pandar does not read
  Better Auth database tables"). A sibling app keeps that boundary clean.
- `pandar-hub` keeps verifying through the unchanged `PANDAR_EXTERNAL_AUTH_*`
  contract — no Rust changes required.

## 3. Requirements (target deployment)

- Multi-user identity with a user directory, self-hosted only.
- Sign-in: passkey (WebAuthn), passwordless.
- Open self-signup; pandar `PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE=true`.
- Issuer data in its own SQLite file, separate from pandar-hub databases.

The issuer module itself stays configurable (origin, trusted origins, secrets,
database path); the above are the defaults/flags for the driving deployment.

## 4. Architecture

```
browser ──HTTPS──> pandar-web   (public dashboard origin, prebuilt Next.js)
   │  APP_AUTH_PROVIDER=betterauth, APP_AUTH_BETTER_AUTH_BASE_URL
   ▼
pandar-auth issuer (public auth origin) — Better Auth: passkey + jwt plugins — sqlite
   │  issues RS256 JWT (sub, email, email_verified, name)
   ▼
pandar-hub verifies signature via the issuer's JWKS (PANDAR_EXTERNAL_AUTH_*)
```

A user completes a passkey ceremony at the auth origin, the issuer obtains an
RS256 JWT from Better Auth's JWT client plugin, redirects back to
`pandar-web`'s new auth callback, and `pandar-web` stores that JWT in the
`pandar_auth_token` cookie. Subsequent dashboard server requests forward that
cookie as the bearer token, and `pandar-hub` validates it via JWKS. First
verified dashboard onboarding can then create a tenant (`tenant_admin`) through
the existing Phase 31 tenant-create action.

## 5. Issuer application (`frontend/auth/`)

Directory `frontend/auth/` — a minimal Next.js (App Router) app:

- `app/api/auth/[...all]/route.ts` mounts the Better Auth handler with
  `basePath: "/api/auth"`.
- Passkey sign-in/up pages using Better Auth's React client. Returning-user
  sign-in calls `authClient.signIn.passkey()` to create a Better Auth session,
  then fetches `/api/auth/token` and redirects to
  `${PANDAR_AUTH_DASHBOARD_CALLBACK_URL}#token=<jwt>`. Signup calls
  `authClient.passkey.addPasskey({ context })` to register the passkey, then
  immediately calls `authClient.signIn.passkey()` to create a Better Auth
  session. Only after a session exists does the client fetch Better Auth's
  mounted JWT token endpoint with
  `fetch("/api/auth/token", { credentials: "include" })`, reads the returned
  `{ token }`, and redirects to
  `${PANDAR_AUTH_DASHBOARD_CALLBACK_URL}#token=<jwt>`. The callback URL is
  configurable and defaults in the NixOS module to
  `http://127.0.0.1:3000/auth/betterauth/callback`. The fragment is not sent to
  `pandar-web`; a single
  `frontend/app/auth/betterauth/callback/route.ts` handles both methods. Its
  `GET` returns minimal HTML/JS that reads `location.hash`, POSTs the token in
  the request body to the same path, then follows the POST redirect. Its `POST`
  imports no Better Auth, accepts only non-empty compact-JWT-shaped tokens,
  stores the value in the configured `APP_AUTH_COOKIE_NAME` cookie
  (`pandar_auth_token` by default), and redirects to `/`.
- Sign-out is paired across both origins. The issuer provides `/sign-out`,
  calls Better Auth `authClient.signOut()`, then redirects to
  `${PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL}`. That dashboard URL defaults in the
  NixOS module to `http://127.0.0.1:3000/auth/betterauth/sign-out`.
  `frontend/app/auth/betterauth/sign-out/route.ts` clears the same
  `APP_AUTH_COOKIE_NAME` cookie with matching `Path=/`/`SameSite=Lax`/`Secure`
  policy and redirects to `/`.
- Better Auth `auth.ts`:
  - SQLite via `better-sqlite3`: `import Database from "better-sqlite3"` and
    `database: new Database(env.databaseFile)`. The Nix package must include
    the native build/runtime inputs required by `better-sqlite3`: `python3`,
    `pkg-config`, `node-gyp`-compatible npm rebuild support, and SQLite runtime
    linkage through nixpkgs' normal Node native-addon build environment.
  - `baseURL` (configurable) — the WebAuthn relying
    party id/origin and the JWT issuer are derived from it.
  - `trustedOrigins` (configurable) for
    cross-origin browser calls from the dashboard.
  - plugins: `passkey()` from `@better-auth/passkey` and `jwt()` from
    `better-auth/plugins`.
  - Better Auth's JWT plugin config sets RS256 JWKS at `/jwks` and supplies
    `definePayload`, `issuer`, and `audience`. Notes:
    - Better Auth 1.6.22 delegates key generation to `jose.generateKeyPair`,
      whose RSA signing algorithm value is `"RS256"`, matching
      `PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256`. (The jwt plugin default is
      EdDSA; the target deployment configures pandar for RS256 verification, so
      RS256 key generation is mandatory.) A smoke check signs a token and
      confirms the JWT header is `alg: "RS256"` and the matching JWKS entry is
      `kty: "RSA"`.
    - Better Auth generates the RSA keypair and persists it in its own `jwks`
      database table, so the `kid` is stable across restarts. **No external JWT
      signing key secret is required.**
    - Better Auth's JWT plugin default JWKS path is `/jwks`; because the app is
      mounted at `basePath: "/api/auth"`, the public JWKS URL is
      `/api/auth/jwks` -> `PANDAR_EXTERNAL_AUTH_JWKS_URL`.
    - Better Auth's JWT server plugin exposes `/token`; mounted under
      `basePath: "/api/auth"` this becomes `/api/auth/token`. Better Auth
      1.6.22's `jwtClient()` only exposes `jwks`, so the issuer page retrieves
      the bearer JWT with a same-origin
      `fetch("/api/auth/token", { credentials: "include" })` instead of a typed
      client helper.
    - `definePayload` must emit pandar's expected snake-case profile claims:
      `{ email, email_verified, name, preferred_username }` (Better Auth's
      default payload uses camelCase `emailVerified`, which pandar does not
      read). The JWT `sub` claim is not emitted by `definePayload`; Better Auth
      1.6.22 sets it after `definePayload` using `jwt.getSubject` or the default
      `session.user.id`, and this phase keeps that default. For passkey-only
      users, `preferred_username` is the normalized email local part before `@`.
    - `issuer` and `audience` default to `baseURL`; pandar is configured with
      matching `PANDAR_EXTERNAL_AUTH_ISSUER`/`PANDAR_EXTERNAL_AUTH_AUDIENCE`.
      Better Auth's JWT plugin emits `iss`/`aud` from this `jwt.issuer` /
      `jwt.audience` configuration; `definePayload` only supplies identity
      profile claims.
    - The JWT lifetime is configured from `PANDAR_AUTH_JWT_MAX_AGE_SECONDS`
      and mapped to Better Auth `jwt.expirationTime` as a seconds string such as
      `"43200s"`. It defaults to the same 12-hour value as the dashboard cookie.
      There is no refresh path in this phase; users sign in again when the JWT
      expires.
- `BETTER_AUTH_SECRET` for Better Auth session/cookie signing and default JWKS
  private-key encryption, injected via env.
- `PANDAR_AUTH_DASHBOARD_CALLBACK_URL` for the post-token dashboard callback.
- `PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL` for the dashboard cookie-clearing
  callback after issuer sign-out.
- `frontend/auth/next.config.ts` sets `output: "standalone"` so the Nix package can
  install the same standalone server shape as `pandar-web`.
- `frontend/auth/lib/env.ts` maps process env to typed config, including
  `PANDAR_AUTH_DATABASE_FILE -> databaseFile`,
  `PANDAR_AUTH_BASE_URL -> baseURL`, trusted origins, dashboard callback/signout
  URLs, and `PANDAR_AUTH_JWT_MAX_AGE_SECONDS -> jwtMaxAgeSeconds`.
- `APP_AUTH_COOKIE_MAX_AGE_SECONDS` is a new `pandar-web` env var introduced by
  the callback route in this phase; existing frontend auth forwarding does not
  read it today.
- `frontend/auth/package.json` scripts:
  - `build`: `next build`
  - `start`: `next start`
  - `migrate`: `auth migrate --config ./lib/auth.ts --yes`
- Database migration: Better Auth needs its `passkey` and `jwks` tables. The
  CLI is the `auth@1.6.22` npm package, not the runtime `better-auth` package
  itself. `frontend/auth/package.json` must include `auth: "1.6.22"` as a local
  dependency so `npm run migrate` resolves the pinned CLI from
  `node_modules/.bin/auth`; do not use `npx auth@latest` in packaged runtime
  paths. The package installs a migration source tree at
  `$out/share/pandar-auth/migrate-src` containing `package.json`,
  `package-lock.json`, `node_modules`, `lib/auth.ts`, `lib/env.ts`, and the
  Better Auth config dependencies required by the CLI. The
  `pandar-auth-migrate` wrapper changes directory to that installed source tree
  and runs `npm run migrate`. The systemd module runs
  `${cfg.auth.package}/bin/pandar-auth-migrate` as `ExecStartPre` before the
  Next.js server starts, with `PANDAR_AUTH_DATABASE_FILE` pointing at the
  configured SQLite file. The unit `WorkingDirectory` remains
  `/var/lib/pandar-auth` for runtime state, but the migrate wrapper does not
  depend on it.
- The package also installs `${cfg.auth.package}/bin/pandar-auth`, a Node
  wrapper around the Next standalone `server.js` equivalent to the existing
  `pandar-web` wrapper. `systemd.services.pandar-auth.serviceConfig.ExecStart`
  uses that wrapper.

### 5.1 Verified-email resolution (passkey + open signup, no SMTP)

Pandar onboarding requires a JWT claim `email_verified: true`. Pure passkey
enrolment does not prove email ownership, and the target deployment runs no
SMTP. Resolution: use Better Auth's **passkey-first onboarding**
(`passkey({ registration: { requireSession: false, resolveUser } })`). The
signup page passes the entered email/name through the passkey registration
`context`. `resolveUser` parses that context, normalizes the email, rejects the
registration if `ctx.context.internalAdapter.findUserByEmail(email)` already
returns a user, otherwise creates a new user with
`ctx.context.internalAdapter.createUser({ email, name, emailVerified: true })`,
and returns `{ id, name, displayName }` for that new user. After passkey
registration, the page performs passkey sign-in so Better Auth creates the
session used by `/api/auth/token`. The JWT (via `definePayload`) then carries
`email_verified: true`.

Implementation evidence for Better Auth 1.6.22 was checked from the published
packages before planning:

- `@better-auth/passkey` exports `PasskeyRegistrationOptions` with
  `requireSession?: boolean` and `resolveUser?: ({ ctx, context }) => ...`.
- The passkey client exposes `authClient.passkey.addPasskey({ context })` and
  `authClient.signIn.passkey()`.
- Better Auth's internal adapter exposes `findUserByEmail` and `createUser`.
  The signup path must not attach a new passkey to a pre-existing email account;
  if the email already exists, the user must use sign-in instead.
- The JWT server plugin exposes `/token`; Better Auth 1.6.22's `jwtClient()` only
  exposes `jwks`, so direct same-origin `fetch("/api/auth/token", {
credentials: "include" })` is intentional.

Trade-off (documented for operators): an unverified email can be claimed.
Acceptable for this open-signup, tenant-isolated self-hosted deployment; no
trust is placed in the email beyond dashboard display. Moving to invite-only
later only needs `PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE=false` plus SMTP.

## 6. Nix packaging and module

- `nix/pandar.nix`: add a `pandar-auth` package built with `buildNpmPackage`
  (same pattern as `pandar-web`), and pass it into the NixOS module as
  `pandarAuthPackage`.
- `nix/nixos-module.nix`: add a top-level `services.pandar-auth` option set
  (not nested under `services.pandar`, because the issuer is independently
  deployable):
  - `enable`, `package`, `bind` (default `127.0.0.1:3001`), `baseURL`,
    `trustedOrigins`, `dashboardCallbackUrl`, `dashboardSignOutUrl`,
    `databaseFile` (default `/var/lib/pandar-auth/auth.db`),
    `jwtMaxAgeSeconds`, `environmentFile` (secrets), and `extraEnvironment`.
  - a `systemd.services.pandar-auth` unit: `DynamicUser`, `StateDirectory =
"pandar-auth"`, env from options + `EnvironmentFile`, `ExecStart` to
    `${cfg.auth.package}/bin/pandar-auth`.
  - Keep it independent of `services.pandar.enable` so the issuer can run with
    or without the hub/web on the same host. Implementation must place
    `services.pandar-auth` options and config under their own
    `cfgAuth = config.services.pandar-auth` / `lib.mkIf cfgAuth.enable` path,
    outside the existing `lib.mkIf config.services.pandar.enable` block.
  - Parse `bind` as `host:port` in the NixOS module and pass `HOSTNAME=host`
    plus `PORT=port` to the Next.js standalone server.
- Update the flake module wiring to pass `pandarAuthPackage` into every
  `import ./nixos-module.nix` call.
- Extend the NixOS module check to evaluate `services.pandar-auth.enable =
true`, assert `ExecStart`, `ExecStartPre`, core environment variables, and
  `EnvironmentFile` wiring.
- Extend generated option documentation to include both `services.pandar` and
  `services.pandar-auth`, then refresh `docs/deployment/nixos/options.md`.
- Flake exposes `packages.pandar-auth`; the module is reachable through the
  existing `nixosModules.default`/`nixosModules.pandar` (same `moduleWithSystem`
  wiring). No new flake output name is strictly required.

## 7. Consumer wiring

A deployment consumes the existing `pandar` flake input, which now also provides
`packages.pandar-auth` and `services.pandar-auth`; no new flake input is
required.

- Enable `services.pandar-auth` with the sqlite backend and inject
  `BETTER_AUTH_SECRET` through the configured `EnvironmentFile`. No JWT signing
  key secret is needed (Better Auth persists its keypair in its own DB).
- Route the chosen auth origin to `127.0.0.1:3001` through the deployment's
  reverse proxy and TLS configuration.
- `pandar-web` env: `APP_AUTH_PROVIDER=betterauth`,
  `APP_AUTH_BETTER_AUTH_BASE_URL=https://auth.example.com`,
  `APP_BASE_URL=https://pandar.example.com`, and optional
  `APP_AUTH_COOKIE_NAME=pandar_auth_token`,
  `APP_AUTH_COOKIE_MAX_AGE_SECONDS=43200`.
- `pandar-auth` env: `PANDAR_AUTH_BASE_URL=https://auth.example.com`,
  `PANDAR_AUTH_TRUSTED_ORIGINS=https://pandar.example.com`,
  `PANDAR_AUTH_DASHBOARD_CALLBACK_URL=https://pandar.example.com/auth/betterauth/callback`,
  `PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL=https://pandar.example.com/auth/betterauth/sign-out`,
  `PANDAR_AUTH_DATABASE_FILE=/var/lib/pandar-auth/auth.db`,
  `PANDAR_AUTH_JWT_MAX_AGE_SECONDS=43200`, and `BETTER_AUTH_SECRET` from the
  environment file.
- `pandar-hub` env: `PANDAR_EXTERNAL_AUTH_PROVIDER=betterauth`,
  `PANDAR_EXTERNAL_AUTH_ISSUER=https://auth.example.com`,
  `PANDAR_EXTERNAL_AUTH_JWKS_URL=https://auth.example.com/api/auth/jwks`,
  `PANDAR_EXTERNAL_AUTH_AUDIENCE=https://auth.example.com`,
  `PANDAR_EXTERNAL_AUTH_ALGORITHMS=RS256`,
  `PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE=true`.

## 8. Security notes

- Open self-signup lets any internet user reach the dashboard, enrol a passkey,
  and create their own tenant. Pandar isolates tenant data; no cross-tenant
  access is granted by signup.
- The `email_verified` shortcut (§5.1) is the main trade-off and is called out
  for operators.
- While this shortcut is enabled, Phase 31 join links that restrict by verified
  email are only as strong as the issuer's self-asserted email field. That is
  acceptable for the target open-signup deployment because tenant creation is
  already public; deployments that rely on email-constrained invites should add
  real email verification or disable open self-signup.
- `pandar-web` does not verify the JWT before setting its bearer cookie; the
  Rust hub remains the verification authority for every API request. The
  callback only rejects empty/non-compact-JWT-shaped values to avoid storing
  obvious junk. This is acceptable because the cookie is not authorization by
  itself; invalid tokens fail at `pandar-hub`.
- The callback has no nonce/state binding in this phase. A user can be made to
  store another valid self-hosted issuer token if they visit a crafted fragment
  URL, but that only switches the browser to the token's signed identity; it
  does not forge a tenant role or bypass hub-side signature and membership
  checks.
- The new dashboard callback route sets the bearer cookie as `HttpOnly`,
  `SameSite=Lax`, `Path=/`, with an explicit max age
  (`APP_AUTH_COOKIE_MAX_AGE_SECONDS`, default 12 hours), and `Secure` whenever
  `APP_BASE_URL` is `https://...`. Reading `APP_BASE_URL` for this cookie
  policy is introduced by this callback route.
- Signup/self-create rate limiting is delegated to the reverse proxy
  (consistent with the Phase 31/33 guidance).
- Secrets (`BETTER_AUTH_SECRET`) are never baked into the image; they come from
  the host secret store. No external JWT signing key secret is required because
  Better Auth generates and stores the keypair in its own database. By default
  Better Auth encrypts the stored JWKS private key with `BETTER_AUTH_SECRET`;
  rotating that secret without clearing or re-encrypting the issuer `jwks` rows
  will make existing signing keys undecryptable and break JWT issuance until the
  key material is repaired.

## 9. Documentation impact

- Update `docs/roadmap.md` to mark Phase 33 completed and record the callback,
  Nix package/module, and issuer security trade-off.
- Update the prior Phase 30–33 Better Auth design note if needed so Better Auth
  1.6.22 is consistently documented with `keyPairConfig.alg = "RS256"` and the
  JWT/JWKS smoke check records the emitted `alg: "RS256"` and `kty: "RSA"`.
- Refresh `docs/deployment/nixos/options.md` from the generated NixOS option
  documentation so `pandar-nixos-options-doc` stays green.
- No Rust API docs are required because `pandar-hub` keeps the existing
  external-auth contract unchanged.

## 10. Verification

1. `packages.pandar-auth` builds; `nix build .#pandar-auth` succeeds.
2. `services.pandar-auth` NixOS module evaluates and starts; the issuer serves
   its handler and a JWKS endpoint returning the RSA public key with a stable
   `kid`.
3. `pandar-auth-migrate` initializes a fresh SQLite database at
   `PANDAR_AUTH_DATABASE_FILE` before the server starts.
4. With the issuer configured, existing `pandar-hub` `GET /readyz` reports
   external-auth JWKS readiness through `checks.external_auth`; this is already
   implemented by `crates/pandar-hub/src/readiness.rs` using
   `verifier.check_ready()`, so no Rust hub change is required.
5. A passkey enrolment against the issuer yields a JWT with
   configured auth issuer as `iss`, `alg=RS256`, `email_verified=true`, and the
   default `exp - iat` is 43,200 seconds.
6. `pandar-hub` `GET /api/v1/me` with that bearer JWT returns the identity and
   `can_self_create_tenant=true`; submitting the existing dashboard onboarding
   tenant-create form creates the tenant and the dashboard loads.
7. `frontend/app/auth/betterauth/callback/route.ts` serves a GET callback at
   `/auth/betterauth/callback#token=<jwt>` that POSTs the token body back to the
   same route, sets the configured `HttpOnly` auth cookie, and redirects to `/`;
   missing or malformed tokens return `400`. The response sets `SameSite=Lax`,
   `Path=/`, honors `APP_AUTH_COOKIE_MAX_AGE_SECONDS`, and sets `Secure` when
   `APP_BASE_URL` is `https://...`.
8. Issuer `/sign-out` signs out of Better Auth and redirects to the dashboard
   sign-out callback; `frontend/app/auth/betterauth/sign-out/route.ts` clears
   the configured auth cookie with matching cookie attributes and redirects to
   `/`.
9. No `better-auth` dependency is added to `pandar-web` (frontend stays
   provider-neutral); `pandar-hub` Rust is unchanged.
10. `nix build .#pandar-nixos-options-doc` and the checked-in
    `docs/deployment/nixos/options.md` include `services.pandar-auth`.

## 11. Out of scope

- Embedding the issuer into `pandar-web` (the alternative Phase 33 form).
- PostgreSQL backend for the issuer (its own sqlite is sufficient; the
  SQLite/PostgreSQL parity rule in `AGENTS.md` applies to pandar-hub data, not
  to Better Auth's independent database).
- Email verification via SMTP / social OAuth (only if policy moves away from
  open passkey signup).
- A `pandar-auth` module option for nginx/vhost (the deployment owns the reverse
  proxy, matching how `pandar` itself leaves nginx to the deployer).
