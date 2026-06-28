# Pandar Web External Auth Redirect Design

## Problem

`pandar-web` currently renders the dashboard shell when external authentication is enabled but the browser has no valid bearer token. In the reported Better Auth deployment, `APP_AUTH_PROVIDER=betterauth` and `APP_AUTH_BETTER_AUTH_BASE_URL=https://auth.example` produce a configured sign-in URL, but a fresh browser request to `/` calls `/api/v1/me` without an `Authorization` header, receives 401, and displays "Current identity returned 401" instead of redirecting to the issuer.

The same control-flow bug applies to Clerk and Logto when they have a configured sign-in URL and no trusted bearer source is available.

## Goals

- Redirect unauthenticated browser requests to the configured external provider sign-in URL before rendering the dashboard.
- Preserve static bearer-token and API-token deployments that intentionally run `pandar-web` with server-side credentials.
- Avoid redirect loops when an expired or rejected dashboard cookie causes `/api/v1/me` to return 401.
- Add a small smoke test that locks the redirect decision for Better Auth, Logto, Clerk, static bearer, API token, and no-provider modes.
- Add the smoke test to Nix checks so the behavior is enforced with the packaged `pandar-web` source.

## Non-Goals

- Do not change `pandar-hub` authentication or authorization.
- Do not add a client-side login button as the primary fix for unauthenticated dashboard entry.
- Do not change the Better Auth callback token validation contract.
- Do not introduce a full browser/e2e test harness for this fix.

## Design

Create a small pure helper in `frontend/app/auth-redirect.ts` that decides whether `/` should redirect before dashboard rendering. It will accept the current auth source, a pure provider shape `{ provider, signInUrl }`, and optional `/api/v1/me` status. It returns either `null` or a redirect target string.

The helper must not value-import `frontend/app/api-auth.ts`, because that module imports `next/headers` and would break the lightweight Node smoke test. If it needs shared types, use `import type` only or define the small auth-source union locally.

Rules:

- If provider is `none`, return `null`.
- If provider has no `signInUrl`, return `null` so misconfigured deployments continue to surface existing dashboard/API errors instead of redirecting to nowhere.
- If auth source is `none`, return `signInUrl`. This covers the fresh-browser case without calling hub endpoints.
- If `/api/v1/me` returns 401 and auth source is `request_cookie`, return `/auth/betterauth/sign-out?next=<encoded signInUrl>` so the dashboard cookie is cleared before leaving the site. The route path is reused for Clerk, Logto, and Better Auth because it clears Pandar Web's shared dashboard bearer cookie; it is not a Better Auth SDK operation.
- If `/api/v1/me` returns 401 and auth source is `app_auth_bearer_token` or `app_api_token`, return `null`; a server-configured credential failure should remain visible as an operational error, not an infinite login redirect.
- For non-401 `/api/v1/me` failures, return `null`; the existing dashboard error display remains the diagnostic path.

Update `frontend/app/page.tsx` to:

- Import `redirect` from `next/navigation`.
- Compute `cfg = authProviderConfig()` once.
- Immediately `redirect(signInUrl)` when the helper says a source-less external auth request must sign in. This branch must run before the existing `Promise.all` fetch block so a fresh unauthenticated request skips `/api/v1/me` entirely.
- Keep `/api/v1/me` fetching for cookie/static-token requests.
- After `/api/v1/me`, call the helper with its response status and redirect before selecting tenants or rendering dashboard/onboarding.
- Clerk currently resolves `signInUrl` to the local `/sign-in` page, which renders the existing plugin sign-in surface and does not redirect back to `/`; this avoids a root-page redirect loop while preserving the existing Clerk URL contract.

Create a pure helper in `frontend/app/auth/betterauth/sign-out-redirect.ts` that returns a safe post-sign-out target. It accepts the decoded `next` value from `URLSearchParams.get("next")` and the configured provider `signInUrl`, returning `next` only when it exactly matches `signInUrl`; any missing, malformed, or untrusted value returns `/`.

Update `frontend/app/auth/betterauth/sign-out/route.ts` to accept an optional `next` query parameter. The route will still clear the dashboard auth cookie, then redirect to the pure helper result. This is a local cleanup hop for stale cookies and must not become an open redirect.

Update `fetchJson` in `page.tsx` to include optional `status?: number` on `FetchResult` results. Existing synthetic empty results can omit `status`, and network-throw failures leave it undefined; the redirect helper treats undefined as non-401.

## Test Plan

Add `frontend/app/auth-redirect.smoke.mjs`, run with Node 24 `--experimental-strip-types`, importing the TypeScript helpers directly. The smoke script must avoid importing Next route handlers so it can run in the lightweight Nix check without a Next server runtime.

Assertions for `frontend/app/auth-redirect.ts`:

- Better Auth with `source: "none"` redirects to `https://auth.example/sign-in`.
- Logto with `source: "none"` redirects to `<endpoint>/sign-in`.
- Clerk with `source: "none"` redirects to `/sign-in`.
- Provider `none` does not redirect.
- Better Auth with `source: "none"` and no configured Better Auth base URL does not redirect.
- Better Auth with static bearer or API token does not redirect before `/me`.
- Better Auth with request cookie and `/me` 401 redirects to `/auth/betterauth/sign-out?next=https%3A%2F%2Fauth.example%2Fsign-in`.
- Logto with request cookie and `/me` 401 redirects to `/auth/betterauth/sign-out?next=<encoded logto sign-in URL>`.
- Clerk with request cookie and `/me` 401 redirects to `/auth/betterauth/sign-out?next=%2Fsign-in`.
- Better Auth with request cookie and `/me` 200 does not redirect.
- Better Auth with request cookie and `/me` 500 does not redirect.

Assertions for `frontend/app/auth/betterauth/sign-out-redirect.ts`:

- The sign-out redirect helper returns the configured sign-in URL when `next` matches exactly.
- The sign-out redirect helper rejects `next=https://evil.example` and returns `/`.
- The sign-out redirect helper rejects missing `next`, malformed URLs, and same-host-but-different-path values that do not exactly equal `signInUrl`.

Add a Nix check `pandar-web-auth-redirect-smoke` that runs the smoke script against `frontendSource` using Node 24 `--experimental-strip-types`.

## Acceptance Criteria

- Fresh browser request to `/` with `APP_AUTH_PROVIDER=betterauth`, `APP_AUTH_BETTER_AUTH_BASE_URL=https://auth.example`, no auth cookie, no `APP_API_TOKEN`, and no `APP_AUTH_BEARER_TOKEN` returns a server redirect to `https://auth.example/sign-in`.
- Equivalent source-less Clerk and Logto provider configurations redirect to their configured sign-in URL.
- Stale dashboard cookie causing `/api/v1/me` 401 clears the shared dashboard bearer cookie through the existing local sign-out route before provider sign-in, for Clerk, Logto, and Better Auth.
- The stale-cookie cleanup route does not accept arbitrary external redirect targets.
- Static credential deployments do not start redirecting just because `/api/v1/me` is unavailable or rejected.
- Nix exposes and can build the new `pandar-web-auth-redirect-smoke` check.

## Documentation Impact

Update `docs/roadmap.md` Completed section with the dashboard external-auth redirect hardening and the new Nix smoke check.
