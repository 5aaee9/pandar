# Phase 33 Self-Hosted Better Auth Bundle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the reviewed Phase 33 design: a sibling `frontend/auth/` Next.js Better Auth issuer app, dashboard callback/sign-out routes, Nix package/module wiring, and docs for self-hosted passkey Better Auth deployments.

**Spec Reference:** `docs/superpowers/specs/2026-06-28-phase-33-self-hosted-better-auth-bundle-design.md`

**Architecture:** Keep `pandar-hub` unchanged. `frontend/auth/` owns Better Auth runtime, passkey registration, JWT/JWKS issuance, and its SQLite database. `frontend/` remains provider-neutral and only receives a compact JWT through a fragment-to-POST callback before forwarding the cookie as an API bearer token.

**Tech Stack:** Next.js 16 App Router, React 19, Better Auth 1.6.22, `@better-auth/passkey` 1.6.22, `auth` CLI 1.6.22, `better-sqlite3`, Nix `buildNpmPackage`, NixOS module options, TypeScript.

## Global Constraints

- Do not change Rust hub behavior; existing external-auth verification and `/readyz` readiness must remain the contract.
- Do not add `better-auth` or `@better-auth/passkey` to `frontend/package.json`.
- Use the pinned `auth@1.6.22` CLI package for migrations; `better-auth@1.6.22` is runtime code and has no CLI bin.
- `auth migrate --config ./lib/auth.ts --yes` must be non-interactive under systemd `ExecStartPre`.
- `BETTER_AUTH_SECRET` is required and encrypts Better Auth JWKS private keys by default; docs must warn about rotation.
- Keep the new NixOS service under top-level `services.pandar-auth`, independent of `services.pandar.enable`.
- Refresh `docs/roadmap.md` after implementation.

## File Structure

New `frontend/auth/` files:

- `frontend/auth/package.json`
- `frontend/auth/package-lock.json`
- `frontend/auth/.gitignore`
- `frontend/auth/next.config.ts`
- `frontend/auth/tsconfig.json`
- `frontend/auth/app/layout.tsx`
- `frontend/auth/app/api/auth/[...all]/route.ts`
- `frontend/auth/app/sign-in/page.tsx`
- `frontend/auth/app/sign-in/sign-in-form.tsx`
- `frontend/auth/app/sign-up/page.tsx`
- `frontend/auth/app/sign-up/sign-up-form.tsx`
- `frontend/auth/app/sign-out/page.tsx`
- `frontend/auth/app/sign-out/sign-out-client.tsx`
- `frontend/auth/app/globals.css`
- `frontend/auth/lib/auth.ts`
- `frontend/auth/lib/auth-client.ts`
- `frontend/auth/lib/env.ts`
- `frontend/auth/lib/token.ts`

Frontend files:

- Create `frontend/app/auth/betterauth/callback/route.ts`
- Create `frontend/app/auth/betterauth/sign-out/route.ts`

Nix files:

- Modify `nix/pandar.nix`
- Modify `nix/nixos-module.nix`
- Modify `nix/nixos-tests.nix` only if package threading requires it

Docs:

- Modify `docs/deployment/nixos/options.md`
- Modify `docs/roadmap.md`
- Modify `docs/architecture.md`
- Modify `docs/release-installation.md`
- Modify `docs/development.md`
- Keep spec corrections already made in `docs/superpowers/specs/2026-06-28-phase-33-self-hosted-better-auth-bundle-design.md`

## Task 1: Scaffold The Better Auth Issuer App

**Files:**

- Create: `frontend/auth/package.json`
- Create: `frontend/auth/package-lock.json`
- Create: `frontend/auth/.gitignore`
- Create: `frontend/auth/next.config.ts`
- Create: `frontend/auth/tsconfig.json`
- Create: `frontend/auth/app/layout.tsx`
- Create: `frontend/auth/app/globals.css`

- [ ] **Step 1: Create `frontend/auth/package.json` with pinned dependencies**

Include scripts:

```json
{
  "build": "next build",
  "start": "next start",
  "migrate": "auth migrate --config ./lib/auth.ts --yes"
}
```

Dependencies must include `auth@1.6.22`, `better-auth@1.6.22`, `@better-auth/passkey@1.6.22`, `better-sqlite3`, `next`, `react`, and `react-dom`. Dev dependencies must include TypeScript and relevant React/Node types.

- [ ] **Step 2: Generate `frontend/auth/package-lock.json`**

Run:

```bash
cd frontend/auth && npm install
```

Expected: lockfile pins the same Better Auth major/minor versions and includes `auth` CLI bin metadata.

- [ ] **Step 3: Add `frontend/auth/.gitignore`**

Ignore `node_modules`, `.next`, `out`, and local env files under `frontend/auth/`.

- [ ] **Step 4: Add Next standalone config**

`frontend/auth/next.config.ts` must set:

```ts
const nextConfig = {
  output: "standalone",
};
```

- [ ] **Step 5: Add TypeScript config and minimal global CSS**

Use the same modern module resolution style as `frontend/`, without adding unused aliases.

- [ ] **Step 6: Add root layout**

Create `frontend/auth/app/layout.tsx`, import `./globals.css`, and render a minimal `<html>`/`<body>` shell. This is required for App Router builds.

- [ ] **Step 7: Verify scaffold**

Run:

```bash
cd frontend/auth && npx tsc --noEmit
```

Expected: dependency install, TypeScript config, and root layout compile before Better Auth routes are added.

## Task 2: Implement Issuer Configuration And Routes

**Files:**

- Create: `frontend/auth/lib/env.ts`
- Create: `frontend/auth/lib/auth.ts`
- Create: `frontend/auth/lib/auth-client.ts`
- Create: `frontend/auth/lib/token.ts`
- Create: `frontend/auth/app/api/auth/[...all]/route.ts`

- [ ] **Step 1: Implement typed env mapping**

`frontend/auth/lib/env.ts` must map:

- `PANDAR_AUTH_DATABASE_FILE -> databaseFile`
- `PANDAR_AUTH_BASE_URL -> baseURL`
- `PANDAR_AUTH_TRUSTED_ORIGINS -> trustedOrigins`
- `PANDAR_AUTH_DASHBOARD_CALLBACK_URL -> dashboardCallbackUrl`
- `PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL -> dashboardSignOutUrl`
- `PANDAR_AUTH_JWT_MAX_AGE_SECONDS -> jwtMaxAgeSeconds`
- `BETTER_AUTH_SECRET -> secret`

Use defaults from the spec for local development.

- [ ] **Step 2: Implement Better Auth server config**

`frontend/auth/lib/auth.ts` must:

- create `new Database(env.databaseFile)`
- configure `baseURL`, `trustedOrigins`, and `secret`
- configure `basePath: "/api/auth"` so `/api/auth/jwks` and `/api/auth/token` are guaranteed
- add `passkey({ registration: { requireSession: false, resolveUser } })`
- add `jwt({ jwks: { keyPairConfig: { alg: "RS256" }, jwksPath: "/jwks" }, jwt: { issuer, audience, expirationTime, definePayload } })`

`definePayload` emits `email`, `email_verified`, `name`, and `preferred_username`. Do not emit `sub`; Better Auth supplies `session.user.id`.

- [ ] **Step 3: Implement passkey-first `resolveUser`**

Parse the client context as JSON with email/name, normalize email, reject signup when that email already exists, create a new user with `emailVerified: true`, and return `{ id, name, displayName }`. Existing users must use sign-in; signup must not attach a new passkey to an existing account.

- [ ] **Step 4: Mount Better Auth handler**

`frontend/auth/app/api/auth/[...all]/route.ts` exports `GET` and `POST` from `toNextJsHandler(auth.handler)` with `basePath: "/api/auth"` configured in Better Auth.

- [ ] **Step 5: Verify server types**

Run:

```bash
cd frontend/auth && npm run build
```

Expected: Better Auth imports and route handler compile.

## Task 3: Implement Issuer Sign-In, Sign-Up, And Sign-Out Pages

**Files:**

- Create: `frontend/auth/app/sign-in/page.tsx`
- Create: `frontend/auth/app/sign-in/sign-in-form.tsx`
- Create: `frontend/auth/app/sign-up/page.tsx`
- Create: `frontend/auth/app/sign-up/sign-up-form.tsx`
- Create: `frontend/auth/app/sign-out/page.tsx`
- Create: `frontend/auth/app/sign-out/sign-out-client.tsx`
- Create/modify: `frontend/auth/lib/auth-client.ts`
- Create/modify: `frontend/auth/lib/token.ts`

- [ ] **Step 1: Implement Better Auth client**

Use Better Auth React client with `passkeyClient()`. Keep it scoped to `frontend/auth/`.

- [ ] **Step 2: Implement token retrieval helper**

After a session exists, fetch `/api/auth/token` with `credentials: "include"`, read `{ token }`, require a non-empty token, and redirect to `${dashboardCallbackUrl}#token=${encodeURIComponent(token)}`.
Client code must receive `dashboardCallbackUrl` or `dashboardSignOutUrl` as props from server page components; do not read non-`NEXT_PUBLIC_` env directly in client components.

- [ ] **Step 3: Implement returning-user sign-in**

`frontend/auth/app/sign-in/page.tsx` is a Server Component that reads `env.dashboardCallbackUrl` and renders `sign-in-form.tsx`. The client form calls `authClient.signIn.passkey()`, then the token helper, then redirects to the dashboard callback fragment.

- [ ] **Step 4: Implement open signup**

`frontend/auth/app/sign-up/page.tsx` is a Server Component that reads `env.dashboardCallbackUrl` and renders `sign-up-form.tsx`. The client form collects email/name, calls `authClient.passkey.addPasskey({ context })`, then `authClient.signIn.passkey()`, then the token helper.

- [ ] **Step 5: Implement issuer sign-out**

`frontend/auth/app/sign-out/page.tsx` is a Server Component that reads `env.dashboardSignOutUrl` and renders `sign-out-client.tsx`. The client component calls `authClient.signOut()` and redirects to the provided dashboard sign-out URL.

- [ ] **Step 6: Verify auth app build**

Run:

```bash
cd frontend/auth && npm run build
```

Expected: build passes.

## Task 4: Add Dashboard Callback And Sign-Out Routes

**Files:**

- Create: `frontend/app/auth/betterauth/callback/route.ts`
- Create: `frontend/app/auth/betterauth/sign-out/route.ts`
- Create: `frontend/app/auth/betterauth/cookie.ts`

- [ ] **Step 1: Implement GET fragment callback page**

`GET /auth/betterauth/callback` returns minimal HTML/JS that reads `location.hash`, extracts `token`, POSTs the token body to the same route, and follows the redirect.

- [ ] **Step 2: Implement POST cookie setter**

The POST handler accepts only a non-empty compact-JWT-shaped token, sets `APP_AUTH_COOKIE_NAME` (`pandar_auth_token` default), uses `HttpOnly`, `SameSite=Lax`, `Path=/`, `APP_AUTH_COOKIE_MAX_AGE_SECONDS` default 43200, and `Secure` when `APP_BASE_URL` starts with `https://`.

- [ ] **Step 3: Implement dashboard sign-out callback**

`frontend/app/auth/betterauth/sign-out/route.ts` clears the same cookie with matching path/samesite/secure policy and redirects to `/`.

- [ ] **Step 4: Verify frontend build**

Run:

```bash
cd frontend && npm run build
```

Expected: build passes with no Better Auth dependency added to `frontend/package.json`.

- [ ] **Step 5: Verify callback cookie behavior with a concrete smoke**

Add reusable cookie policy helpers in `frontend/app/auth/betterauth/cookie.ts` so the route behavior can be smoke-checked without duplicating logic. Verify at minimum:

- GET callback response contains script logic that reads `location.hash` and POSTs to the same path.
- POST with `abc.def.ghi` returns a redirect and `Set-Cookie` with configured name, `HttpOnly`, `SameSite=Lax`, `Path=/`, max age, and `Secure` when `APP_BASE_URL=https://...`.
- POST with missing or malformed token returns `400`.
- sign-out route clears the same cookie name/path/samesite/secure policy.

Use a small Node smoke script or direct route-helper test; do not add a broad frontend test framework.

## Task 5: Add Nix Package And NixOS Module

**Files:**

- Modify: `nix/pandar.nix`
- Modify: `nix/nixos-module.nix`
- Modify: `nix/nixos-tests.nix` only if needed

- [ ] **Step 1: Add `pandar-auth` package**

Use `pkgs.buildNpmPackage` over `frontend/auth/`, include native inputs for `better-sqlite3` (`python3`, `pkg-config`, node-gyp-compatible build support, `makeWrapper`) and SQLite build/runtime inputs (`pkgs.sqlite` / its dev output as needed), and install:

- Next standalone output under `$out/share/pandar-auth`
- the runtime `node_modules` entries that standalone does not reliably bundle for native addons, especially `better-sqlite3` and its compiled `.node` binding
- static/public assets if present
- `$out/bin/pandar-auth` wrapper around standalone `server.js`
- `$out/share/pandar-auth/migrate-src` containing `package.json`, `package-lock.json`, `node_modules`, `tsconfig.json`, `lib/auth.ts`, `lib/env.ts`, and every file transitively imported by `lib/auth.ts`
- `$out/bin/pandar-auth-migrate` wrapper that changes to migrate-src and runs `npm run migrate`

The migration wrapper must be tested to ensure it runs from `$out/share/pandar-auth/migrate-src`, resolves the pinned local `auth@1.6.22` binary from `node_modules/.bin/auth`, loads the TypeScript Better Auth config, can load `better-sqlite3`, and initializes a fresh SQLite database.

- [ ] **Step 2: Thread package into module imports**

Pass `pandarAuthPackage = config.packages.pandar-auth` from `moduleWithSystem` and from all local `import ./nixos-module.nix` evaluator calls.

- [ ] **Step 3: Add independent `services.pandar-auth` options**

In `nix/nixos-module.nix`, add `pandarAuthPackage` argument, `cfgAuth = config.services.pandar-auth`, options, and `lib.mkIf cfgAuth.enable` config outside `lib.mkIf cfg.enable`.

Options: `enable`, `package`, `bind`, `baseURL`, `trustedOrigins`, `dashboardCallbackUrl`, `dashboardSignOutUrl`, `databaseFile`, `jwtMaxAgeSeconds`, `environmentFile`, `extraEnvironment`.

- [ ] **Step 4: Add systemd unit**

Parse `bind` into `HOSTNAME` and `PORT`, set all `PANDAR_AUTH_*` env vars, include `EnvironmentFile` when configured, run `ExecStartPre = "${cfgAuth.package}/bin/pandar-auth-migrate"`, and `ExecStart = "${cfgAuth.package}/bin/pandar-auth"`.

- [ ] **Step 5: Extend Nix module check**

Evaluate `services.pandar-auth.enable = true` and assert `ExecStart`, `ExecStartPre`, `HOSTNAME`, `PORT`, core `PANDAR_AUTH_*` env vars, `EnvironmentFile`, and that `services.pandar-auth` does not require `services.pandar.enable`.

- [ ] **Step 6: Extend option docs generator**

`pandarNixosOptionsDoc` must pass both `services.pandar` and `services.pandar-auth` to `pkgs.nixosOptionsDoc`.

- [ ] **Step 7: Verify Nix package/module**

Run:

```bash
nix build .#pandar-auth
nix build .#checks.x86_64-linux.pandar-nixos-module
nix build .#checks.x86_64-linux.pandar-nixos-options-doc
```

Expected: all pass on x86_64-linux. Use `.#checks.${system}.…` when running on another supported system.

- [ ] **Step 8: Verify installed native-addon runtime**

After `nix build .#pandar-auth`, run checks against the built output to confirm both the standalone server tree and migrate source can load `better-sqlite3`, and run `result/bin/pandar-auth-migrate` against a temporary `PANDAR_AUTH_DATABASE_FILE` with `BETTER_AUTH_SECRET` set.

Expected: `require("better-sqlite3")` succeeds in the installed runtime context and the temporary database gains Better Auth tables including `jwks` and `passkey`.

- [ ] **Step 9: Add Nix migration smoke check**

Add a lightweight `pkgs.runCommand` check or NixOS test assertion that executes the installed `pandar-auth-migrate` wrapper against a fresh SQLite file with required env vars. This check must fail if the wrapper cannot find `auth@1.6.22`, cannot load the TypeScript config, cannot load `better-sqlite3`, or does not initialize the database.

## Task 6: Refresh Documentation

**Files:**

- Modify: `docs/deployment/nixos/options.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/architecture.md`
- Modify: `docs/release-installation.md`
- Modify: `docs/development.md`

- [ ] **Step 1: Refresh generated NixOS options doc**

Copy the generated `pandar-nixos-options-doc` output into `docs/deployment/nixos/options.md`.

- [ ] **Step 2: Update operator docs**

Update each operator doc with concrete content:

- `docs/architecture.md`: add the sibling `pandar-auth` issuer to the deployment/authentication architecture and note that `pandar-hub` still verifies only JWKS/JWT.
- `docs/release-installation.md`: document the `pandar-auth` package, `services.pandar-auth` module, required env/secrets, callback/sign-out URLs, and `PANDAR_EXTERNAL_AUTH_*` hub wiring.
- `docs/development.md`: add local development commands for `frontend/auth/`, including `npm install`, `npm run migrate`, `npm run build`, required env vars, and the Better Auth secret/JWKS rotation warning.

All docs should mention required `BETTER_AUTH_SECRET`, `APP_AUTH_COOKIE_MAX_AGE_SECONDS`, and the JWKS private-key encryption rotation warning where operator-relevant.

- [ ] **Step 3: Update roadmap**

Mark Phase 33 complete and list the issuer app, dashboard callback, Nix package/module, and remaining future work.

- [ ] **Step 4: Verify docs references**

Run:

```bash
rg -n "RSA256|services\\.pandar-auth|pandar-auth|BETTER_AUTH_SECRET|APP_AUTH_COOKIE_MAX_AGE_SECONDS" docs frontend frontend/auth nix
```

Expected: `RSA256` appears only in historical correction notes; new service/env references are present.

## Task 7: Final Verification

- [ ] **Step 1: Frontend and auth builds**

Run:

```bash
cd frontend/auth && npm run build
node scripts/smoke-jwt-and-registration.mjs
cd .. && npm run build
```

- [ ] **Step 2: Rust formatting/lint/tests**

No Rust changes are expected, but run the project-required checks:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- --deny warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

- [ ] **Step 3: Nix checks**

Run:

```bash
nix build .#pandar-auth
nix build .#checks.x86_64-linux.pandar-nixos-module
nix build .#checks.x86_64-linux.pandar-nixos-options-doc
```

- [ ] **Step 4: Callback route smoke**

Run the Task 4 smoke verification for callback and sign-out cookie behavior with both `APP_BASE_URL=http://...` and `APP_BASE_URL=https://...`.

- [ ] **Step 5: Manual contract checks**

Confirm from files/build output:

- `frontend/package.json` has no Better Auth dependencies.
- `frontend/auth/package-lock.json` includes `auth@1.6.22`.
- `pandar-hub` Rust files are unchanged unless a build-required mechanical update was unavoidable.
- `/api/auth/jwks` and `/api/auth/token` remain under Better Auth `basePath: "/api/auth"`.

## Task 8: Commit And Push

- [ ] **Step 1: Review diff**

Run:

```bash
git status --short
git diff --stat
git diff --check
```

- [ ] **Step 2: Commit with Lore protocol**

Use a Lore-format commit message. Include `Tested:` trailers for every verification command that passed and `Not-tested:` for any gaps.

- [ ] **Step 3: Push current branch**

Run:

```bash
git push
```

If push needs an upstream, use the current branch name and set upstream.
