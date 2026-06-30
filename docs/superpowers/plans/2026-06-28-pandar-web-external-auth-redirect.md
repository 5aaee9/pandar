# Pandar Web External Auth Redirect Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redirect unauthenticated external-auth dashboard requests to the configured provider sign-in URL instead of rendering a broken dashboard.

**Architecture:** Add pure redirect decision helpers that are testable under Node 24 type stripping, then wire `frontend/app/page.tsx` to call `next/navigation` `redirect()` before dashboard rendering. Keep stale-cookie cleanup in the existing local sign-out route, but make its `next` target exact-match allowlisted to prevent open redirects.

**Tech Stack:** Next.js App Router, TypeScript, Node 24 `--experimental-strip-types` smoke tests, Nix flake checks.

---

## File Structure

- Create `frontend/app/auth-redirect.ts`: pure dashboard auth redirect decision helper. No value imports from `api-auth.ts` or any Next runtime module.
- Create `frontend/app/auth/betterauth/sign-out-redirect.ts`: pure sign-out `next` allowlist helper. No Next runtime imports.
- Create `frontend/scripts/auth-redirect.smoke.mjs`: Node smoke test importing the pure TypeScript helpers with `pathToFileURL`.
- Modify `frontend/app/dashboard-types.ts`: add optional `status?: number` to each `FetchResult` variant.
- Modify `frontend/app/page.tsx`: add redirect wiring and propagate `fetchJson` status.
- Modify `frontend/app/auth/betterauth/sign-out/route.ts`: delegate safe post-sign-out target selection to the new pure helper.
- Modify `nix/pandar.nix`: add `pandar-web-auth-redirect-smoke` check using Node 24 `--experimental-strip-types`.
- Modify `docs/roadmap.md`: record the auth redirect hardening and Nix smoke check.

## Task 1: Pure Redirect Helpers And Red Smoke Test

**Files:**

- Create: `frontend/app/auth-redirect.ts`
- Create: `frontend/app/auth/betterauth/sign-out-redirect.ts`
- Create: `frontend/scripts/auth-redirect.smoke.mjs`

- [ ] **Step 1: Add the red smoke test**

Create `frontend/scripts/auth-redirect.smoke.mjs`:

```js
import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

const authRedirectModuleUrl = pathToFileURL(
  new URL("./auth-redirect.ts", import.meta.url).pathname,
);
const signOutRedirectModuleUrl = pathToFileURL(
  new URL("./auth/betterauth/sign-out-redirect.ts", import.meta.url).pathname,
);

const { dashboardAuthRedirectTarget } = await import(
  authRedirectModuleUrl.href
);
const { safeSignOutRedirectTarget } = await import(
  signOutRedirectModuleUrl.href
);

const betterAuth = {
  provider: "betterauth",
  signInUrl: "https://auth.example/sign-in",
};
const logto = {
  provider: "logto",
  signInUrl: "https://logto.example/sign-in",
};
const clerk = {
  provider: "clerk",
  signInUrl: "/sign-in",
};

assert.equal(
  dashboardAuthRedirectTarget({ source: "none", provider: betterAuth }),
  "https://auth.example/sign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({ source: "none", provider: logto }),
  "https://logto.example/sign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({ source: "none", provider: clerk }),
  "/sign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "none",
    provider: { provider: "none", signInUrl: null },
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "none",
    provider: { provider: "betterauth", signInUrl: null },
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "app_auth_bearer_token",
    provider: betterAuth,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "app_api_token",
    provider: betterAuth,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "app_auth_bearer_token",
    provider: betterAuth,
    meStatus: 401,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "app_api_token",
    provider: betterAuth,
    meStatus: 401,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: betterAuth,
    meStatus: 401,
  }),
  "/auth/betterauth/sign-out?next=https%3A%2F%2Fauth.example%2Fsign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: logto,
    meStatus: 401,
  }),
  "/auth/betterauth/sign-out?next=https%3A%2F%2Flogto.example%2Fsign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: clerk,
    meStatus: 401,
  }),
  "/auth/betterauth/sign-out?next=%2Fsign-in",
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: betterAuth,
    meStatus: 200,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: betterAuth,
    meStatus: 500,
  }),
  null,
);
assert.equal(
  dashboardAuthRedirectTarget({
    source: "request_cookie",
    provider: betterAuth,
  }),
  null,
);

assert.equal(
  safeSignOutRedirectTarget(
    "https://auth.example/sign-in",
    "https://auth.example/sign-in",
  ),
  "https://auth.example/sign-in",
);
assert.equal(
  safeSignOutRedirectTarget(
    "https://evil.example",
    "https://auth.example/sign-in",
  ),
  "/",
);
assert.equal(
  safeSignOutRedirectTarget(null, "https://auth.example/sign-in"),
  "/",
);
assert.equal(
  safeSignOutRedirectTarget("not a url", "https://auth.example/sign-in"),
  "/",
);
assert.equal(
  safeSignOutRedirectTarget(
    "https://auth.example/sign-out",
    "https://auth.example/sign-in",
  ),
  "/",
);
assert.equal(safeSignOutRedirectTarget("/sign-in", "/sign-in"), "/sign-in");
assert.equal(safeSignOutRedirectTarget("/other", "/sign-in"), "/");
```

- [ ] **Step 2: Run the smoke test and confirm it fails**

Run:

```bash
cd frontend
node --experimental-strip-types scripts/auth-redirect.smoke.mjs
```

Expected: FAIL with a module-not-found error for `auth-redirect.ts` or `sign-out-redirect.ts`.

- [ ] **Step 3: Add the pure redirect helpers**

Create `frontend/app/auth-redirect.ts`:

```ts
type AuthSource =
  | "request_cookie"
  | "app_auth_bearer_token"
  | "app_api_token"
  | "none";

type AuthProviderForRedirect = {
  provider: "clerk" | "logto" | "betterauth" | "none";
  signInUrl: string | null;
};

export function dashboardAuthRedirectTarget({
  source,
  provider,
  meStatus,
}: {
  source: AuthSource;
  provider: AuthProviderForRedirect;
  meStatus?: number;
}) {
  if (provider.provider === "none" || !provider.signInUrl) {
    return null;
  }
  if (source === "none") {
    return provider.signInUrl;
  }
  if (source === "request_cookie" && meStatus === 401) {
    return `/auth/betterauth/sign-out?next=${encodeURIComponent(provider.signInUrl)}`;
  }
  return null;
}
```

Create `frontend/app/auth/betterauth/sign-out-redirect.ts`:

```ts
export function safeSignOutRedirectTarget(
  next: string | null,
  signInUrl: string | null,
) {
  if (!next || !signInUrl) {
    return "/";
  }
  return next === signInUrl ? next : "/";
}
```

- [ ] **Step 4: Run the smoke test and confirm it passes**

Run:

```bash
cd frontend
node --experimental-strip-types scripts/auth-redirect.smoke.mjs
```

Expected: PASS with exit code 0.

## Task 2: Wire Dashboard And Sign-Out Redirects

**Files:**

- Modify: `frontend/app/dashboard-types.ts`
- Modify: `frontend/app/page.tsx`
- Modify: `frontend/app/auth/betterauth/sign-out/route.ts`
- Test: `frontend/scripts/auth-redirect.smoke.mjs`

- [ ] **Step 1: Add optional HTTP status to `FetchResult`**

Modify `frontend/app/dashboard-types.ts`:

```ts
export type FetchResult<T> =
  | { data: T; error: null; status?: number }
  | { data: null; error: null; status?: number }
  | { data: null; error: string; status?: number };
```

- [ ] **Step 2: Wire the dashboard redirect before rendering**

Modify `frontend/app/page.tsx`:

```ts
import { redirect } from "next/navigation";

import { apiHeaders, authSource } from "./api-auth";
import { dashboardAuthRedirectTarget } from "./auth-redirect";
import { authProviderConfig } from "./auth-provider";
```

Update `fetchJson` so non-OK responses and successful responses carry status:

```ts
if (!response.ok) {
  return {
    data: null,
    error: `${label} returned ${response.status}`,
    status: response.status,
  };
}

return {
  data: (await response.json()) as T,
  error: null,
  status: response.status,
};
```

Update the catch result:

```ts
return {
  data: null,
  error: `${label} failed: ${error instanceof Error ? error.message : "unknown error"}`,
};
```

At the start of `Page`, compute the provider config and short-circuit source-less requests before `Promise.all`:

```ts
const auth = await authSource();
const authProvider = authProviderConfig();
const initialRedirect = dashboardAuthRedirectTarget({
  source: auth.source,
  provider: authProvider,
});
if (initialRedirect) {
  redirect(initialRedirect);
}
```

After the `Promise.all` resolving `meResult`, redirect stale request cookies before deriving tenants:

```ts
const meRedirect = dashboardAuthRedirectTarget({
  source: auth.source,
  provider: authProvider,
  meStatus: meResult.status,
});
if (meRedirect) {
  redirect(meRedirect);
}
```

Keep the existing dashboard and onboarding rendering unchanged after those gates.

- [ ] **Step 3: Wire safe sign-out `next` handling**

Modify `frontend/app/auth/betterauth/sign-out/route.ts`:

```ts
import { NextResponse } from "next/server";

import { authProviderConfig } from "../../../auth-provider";
import { clearedAuthCookieOptions, readAuthCookieConfig } from "../cookie";
import { safeSignOutRedirectTarget } from "../sign-out-redirect";

export function GET(request: Request) {
  const requestUrl = new URL(request.url);
  const target = safeSignOutRedirectTarget(
    requestUrl.searchParams.get("next"),
    authProviderConfig().signInUrl,
  );
  const response = NextResponse.redirect(new URL(target, request.url));
  response.cookies.set(
    readAuthCookieConfig().name,
    "",
    clearedAuthCookieOptions(),
  );
  return response;
}
```

- [ ] **Step 4: Run the smoke test and frontend build**

Run:

```bash
cd frontend
node --experimental-strip-types scripts/auth-redirect.smoke.mjs
npm run build
```

Expected: both commands exit 0.

## Task 3: Add Nix Check And Roadmap Entry

**Files:**

- Modify: `nix/pandar.nix`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Add the Nix smoke check**

In `nix/pandar.nix`, after `pandarAuthCookieSmokeCheck`, add:

```nix
pandarWebAuthRedirectSmokeCheck = pkgs.runCommand "pandar-web-auth-redirect-smoke-check" { } ''
  cd ${frontendSource}
  ${pkgs.nodejs_24}/bin/node \
    --experimental-strip-types \
    scripts/auth-redirect.smoke.mjs
  touch "$out"
'';
```

In the `checks` attrset, add:

```nix
pandar-web-auth-redirect-smoke = pandarWebAuthRedirectSmokeCheck;
```

- [ ] **Step 2: Update roadmap**

Add a Completed bullet near the Phase 33 auth entries in `docs/roadmap.md`:

```md
- Hardened `pandar-web` external-auth entry so source-less Clerk/Logto/Better Auth dashboard requests redirect to the configured sign-in URL, stale dashboard cookies are cleared before provider sign-in, and a Nix `pandar-web-auth-redirect-smoke` check locks the redirect/open-redirect behavior.
```

- [ ] **Step 3: Run Nix and formatting checks**

Run:

```bash
nix fmt
nix build --show-trace .#checks.x86_64-linux.pandar-web-auth-redirect-smoke
nix flake check --no-build --show-trace
git diff --check
```

Expected: all commands exit 0 and `nix flake check --no-build --show-trace` lists `checks.x86_64-linux.pandar-web-auth-redirect-smoke`.

## Final Verification

- [ ] Run the targeted smoke:

```bash
cd frontend
node --experimental-strip-types scripts/auth-redirect.smoke.mjs
```

- [ ] Run the frontend build:

```bash
cd frontend
npm run build
```

- [ ] Run the Nix smoke check:

```bash
nix build --show-trace .#checks.x86_64-linux.pandar-web-auth-redirect-smoke
```

- [ ] Run flake check evaluation:

```bash
nix flake check --no-build --show-trace
```

- [ ] Run repository hygiene:

```bash
nix fmt
git diff --check
git status --short
```
