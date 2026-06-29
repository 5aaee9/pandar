# Better Auth Direct Callback Redirect Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Better Auth callback fragment-plus-POST bridge with a direct server-side callback redirect that sets the Pandar dashboard auth cookie and redirects to `/`.

**Architecture:** `pandar-auth` still fetches a Better Auth session JWT from `/api/auth/token`, but now appends it as a `token` query parameter on the configured absolute dashboard callback URL. `pandar-web` uses a framework-free callback decision helper plus a thin Next route wrapper that validates the token with the existing `isAllowedDashboardJwt`, sets the existing HTTP-only cookie, applies no-store/no-referrer headers, and redirects directly to `/`.

**Tech Stack:** Next.js route handlers, TypeScript, Node 24 `--experimental-strip-types` smoke tests, existing Better Auth JWT/cookie helpers.

## Global Constraints

- Keep `better-auth` runtime dependencies out of `frontend/` (`pandar-web`).
- Keep the fix focused on the Better Auth callback return flow; do not change hub JWT verification, tenant membership, join links, cookie name, cookie max age, SameSite, or Secure semantics.
- `PANDAR_AUTH_DASHBOARD_CALLBACK_URL` is an absolute dashboard callback URL and existing query parameters must be preserved when adding `token`.
- Successful callback responses must include `cache-control: no-store` and `referrer-policy: no-referrer`; error responses must include `cache-control: no-store`.
- The query-string JWT trade-off is accepted for this fix, but docs must mention that callback request logs/history can contain bearer JWTs.
- Update `docs/architecture.md` and `docs/roadmap.md`; leave existing `docs/superpowers/` specs and plans as historical records.
- Do not commit until the final `$sdd-workflow` verification and review gates pass.

---

### Task 1: Direct Better Auth Callback Redirect

**Files:**

- Create: `frontend/app/auth/betterauth/callback-redirect.ts`
- Create: `frontend/app/auth/betterauth/callback.smoke.mjs`
- Create: `frontend/auth/scripts/smoke-dashboard-token-redirect.mjs`
- Modify: `frontend/app/auth/betterauth/callback/route.ts`
- Modify: `frontend/auth/lib/token.ts`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

**Interfaces:**

- Consumes: `isAllowedDashboardJwt(token: string): boolean`, `readAuthCookieConfig()`, and `authCookieOptions()` from `frontend/app/auth/betterauth/cookie.ts`.
- Produces: `betterAuthCallbackRedirect(requestUrl: string, isAllowedToken: (token: string) => boolean): BetterAuthCallbackRedirect` in `frontend/app/auth/betterauth/callback-redirect.ts`.
- Produces: `redirectWithAuthToken(dashboardCallbackUrl, messages)` behavior that redirects to the configured callback URL with a `token` query parameter.

- [ ] **Step 1: Write the failing dashboard callback smoke test**

Create `frontend/app/auth/betterauth/callback.smoke.mjs`:

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

const helperModuleUrl = pathToFileURL(
  new URL("./callback-redirect.ts", import.meta.url).pathname,
);
const { betterAuthCallbackRedirect } = await import(helperModuleUrl.href);

const validToken = "header.payload.signature";
const acceptsOnlyValidToken = (token) => token === validToken;

assert.deepEqual(
  betterAuthCallbackRedirect(
    `https://pandar.example/auth/betterauth/callback?token=${validToken}`,
    acceptsOnlyValidToken,
  ),
  { ok: true, token: validToken, target: "/", status: 303 },
);

assert.deepEqual(
  betterAuthCallbackRedirect(
    `https://pandar.example/auth/betterauth/callback?token=%20${validToken}%20`,
    acceptsOnlyValidToken,
  ),
  { ok: true, token: validToken, target: "/", status: 303 },
);

assert.deepEqual(
  betterAuthCallbackRedirect(
    "https://pandar.example/auth/betterauth/callback",
    acceptsOnlyValidToken,
  ),
  { ok: false, body: "malformed token", status: 400 },
);

assert.deepEqual(
  betterAuthCallbackRedirect(
    "https://pandar.example/auth/betterauth/callback?token=%20%20",
    () => true,
  ),
  { ok: false, body: "malformed token", status: 400 },
);

assert.deepEqual(
  betterAuthCallbackRedirect(
    "https://pandar.example/auth/betterauth/callback?token=bad-token",
    acceptsOnlyValidToken,
  ),
  { ok: false, body: "malformed token", status: 400 },
);

const routeSource = await readFile(
  new URL("./callback/route.ts", import.meta.url),
  "utf8",
);

assert.match(
  routeSource,
  /betterAuthCallbackRedirect\(request\.url,\s*isAllowedDashboardJwt\)/s,
);
assert.match(
  routeSource,
  /NextResponse\.redirect\([\s\S]*new URL\(result\.target, request\.url\),[\s\S]*result\.status,[\s\S]*\)/,
);
assert.match(
  routeSource,
  /response\.cookies\.set\(\s*readAuthCookieConfig\(\)\.name,\s*result\.token,\s*authCookieOptions\(\)/s,
);
assert.match(
  routeSource,
  /response\.headers\.set\("cache-control", "no-store"\)/,
);
assert.match(
  routeSource,
  /response\.headers\.set\("referrer-policy", "no-referrer"\)/,
);
assert.doesNotMatch(
  routeSource,
  /export\s+async\s+function\s+POST|export\s+function\s+POST/,
);
assert.doesNotMatch(
  routeSource,
  /callbackHtml|location\.hash|method:\s*"POST"|method:\s*'POST'/,
);
```

- [ ] **Step 2: Run dashboard callback smoke to verify it fails**

Run: `cd frontend && node --experimental-strip-types app/auth/betterauth/callback.smoke.mjs`

Expected: FAIL because `callback-redirect.ts` does not exist yet, or because the current route still contains the HTML/POST bridge.

- [ ] **Step 3: Write the failing issuer redirect smoke test**

Create `frontend/auth/scripts/smoke-dashboard-token-redirect.mjs`:

```js
import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

const tokenModuleUrl = pathToFileURL(
  new URL("../lib/token.ts", import.meta.url).pathname,
);
const { redirectWithAuthToken } = await import(tokenModuleUrl.href);

const originalFetch = globalThis.fetch;
const originalWindow = globalThis.window;

async function captureRedirect({
  callbackUrl,
  token = "aaa.bbb.ccc",
  responseOk = true,
  body = { token },
}) {
  let redirectedTo = null;
  globalThis.fetch = async (input, init) => {
    assert.equal(input, "/api/auth/token");
    assert.deepEqual(init, { credentials: "include" });
    return {
      ok: responseOk,
      json: async () => body,
    };
  };
  globalThis.window = {
    location: {
      set href(value) {
        redirectedTo = value;
      },
    },
  };

  await redirectWithAuthToken(callbackUrl, {
    dashboardTokenEmpty: "empty token",
    dashboardTokenFailed: "token failed",
  });

  return redirectedTo;
}

try {
  assert.equal(
    await captureRedirect({
      callbackUrl: "https://pandar.example/auth/betterauth/callback",
    }),
    "https://pandar.example/auth/betterauth/callback?token=aaa.bbb.ccc",
  );

  assert.equal(
    await captureRedirect({
      callbackUrl:
        "https://pandar.example/auth/betterauth/callback?source=issuer",
      token: "header.payload.signature",
    }),
    "https://pandar.example/auth/betterauth/callback?source=issuer&token=header.payload.signature",
  );

  await assert.rejects(
    () =>
      captureRedirect({
        callbackUrl: "https://pandar.example/auth/betterauth/callback",
        responseOk: false,
      }),
    /token failed/,
  );

  await assert.rejects(
    () =>
      captureRedirect({
        callbackUrl: "https://pandar.example/auth/betterauth/callback",
        body: { token: "" },
      }),
    /empty token/,
  );
} finally {
  globalThis.fetch = originalFetch;
  globalThis.window = originalWindow;
}
```

- [ ] **Step 4: Run issuer redirect smoke to verify it fails**

Run: `cd frontend/auth && node --experimental-strip-types scripts/smoke-dashboard-token-redirect.mjs`

Expected: FAIL because the current helper redirects to `#token=...`.

- [ ] **Step 5: Implement the callback helper**

Create `frontend/app/auth/betterauth/callback-redirect.ts`:

```ts
export type BetterAuthCallbackRedirect =
  | { ok: true; token: string; target: string; status: 303 }
  | { ok: false; body: string; status: 400 };

export function betterAuthCallbackRedirect(
  requestUrl: string,
  isAllowedToken: (token: string) => boolean,
): BetterAuthCallbackRedirect {
  const token = new URL(requestUrl).searchParams.get("token")?.trim() ?? "";
  if (!token || !isAllowedToken(token)) {
    return { ok: false, body: "malformed token", status: 400 };
  }

  return { ok: true, token, target: "/", status: 303 };
}
```

- [ ] **Step 6: Replace the callback route bridge with direct redirect**

Replace `frontend/app/auth/betterauth/callback/route.ts` with:

```ts
import { NextResponse } from "next/server";

import {
  authCookieOptions,
  isAllowedDashboardJwt,
  readAuthCookieConfig,
} from "../cookie";
import { betterAuthCallbackRedirect } from "../callback-redirect";

export function GET(request: Request) {
  const result = betterAuthCallbackRedirect(request.url, isAllowedDashboardJwt);
  if (!result.ok) {
    return new NextResponse(result.body, {
      status: result.status,
      headers: {
        "cache-control": "no-store",
      },
    });
  }

  const response = NextResponse.redirect(
    new URL(result.target, request.url),
    result.status,
  );
  response.cookies.set(
    readAuthCookieConfig().name,
    result.token,
    authCookieOptions(),
  );
  response.headers.set("cache-control", "no-store");
  response.headers.set("referrer-policy", "no-referrer");
  return response;
}
```

- [ ] **Step 7: Implement issuer query redirect**

Update `frontend/auth/lib/token.ts` so the final redirect uses the URL API:

```ts
const callbackUrl = new URL(dashboardCallbackUrl);
callbackUrl.searchParams.set("token", body.token);
window.location.href = callbackUrl.toString();
```

Remove the old `#token=${encodeURIComponent(...)}` construction.

- [ ] **Step 8: Run focused smoke tests and verify they pass**

Run:

```bash
(cd frontend && node --experimental-strip-types app/auth/betterauth/callback.smoke.mjs)
(cd frontend/auth && node --experimental-strip-types scripts/smoke-dashboard-token-redirect.mjs)
```

Expected: both commands exit 0.

- [ ] **Step 9: Update architecture docs**

Update `docs/architecture.md` line describing Better Auth callback fragments so it says successful sign-ins redirect to `pandar-web`'s callback with a `token` query parameter, the dashboard callback validates issuer/audience shape, sets the HTTP-only bearer cookie, and immediately redirects to `/` with no-store/no-referrer headers. Mention that callback request logs/history can contain a bearer JWT and should be treated as sensitive.

- [ ] **Step 10: Update roadmap**

Add a completed entry near the current auth completion bullets in `docs/roadmap.md` saying the Better Auth dashboard return flow now uses a direct `GET /auth/betterauth/callback?token=...` server redirect, removing the blank HTML/POST bridge while preserving existing JWT validation and cookie semantics.

- [ ] **Step 11: Run broader frontend checks**

Run:

```bash
(cd frontend/auth && npm test)
(cd frontend/auth && npm run build)
(cd frontend && npm run build)
```

Expected: commands exit 0. If a build is blocked by unavailable environment/dependencies, capture the exact error and continue with the focused smoke and available test evidence.

- [ ] **Step 12: Run repo-required Rust checks**

Run from repo root:

```bash
cargo fmt
cargo clippy
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: commands exit 0.

- [ ] **Step 13: Final diff audit before review**

Run:

```bash
git diff -- frontend/app/auth/betterauth frontend/auth/lib/token.ts frontend/auth/scripts/smoke-dashboard-token-redirect.mjs docs/architecture.md docs/roadmap.md
```

Expected: only the direct callback redirect implementation, focused smoke tests, and required docs changed.
