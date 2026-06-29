# Better Auth Direct Callback Redirect Design

## Goal

Fix the Better Auth return-to-Pandar flow so a successful sign-in does not leave the browser on a blank `/auth/betterauth/callback` page and does not require the dashboard callback page to issue a second browser `POST` request.

## Root Cause

`frontend/auth/lib/token.ts` currently redirects to `PANDAR_AUTH_DASHBOARD_CALLBACK_URL#token=<jwt>`. URL fragments are never sent to the server, so `frontend/app/auth/betterauth/callback/route.ts` cannot set the HTTP-only dashboard bearer cookie from its `GET` handler. The route works around that by returning an otherwise blank HTML page whose script reads `location.hash`, `POST`s the token back to the same route, then follows the redirect.

That workaround makes the final redirect dependent on client-side JavaScript and fetch redirect behavior. When that script path does not complete the navigation, the browser remains on `/auth/betterauth/callback` with a blank page after the `POST` finishes.

## Scope

In scope:

- Change the self-hosted `pandar-auth` issuer redirect helper to append the dashboard JWT as a `token` query parameter instead of a URL fragment.
- Change the `pandar-web` Better Auth callback `GET` handler to read `token` from the query string, validate it with the existing compact-JWT and issuer/audience checks, set the existing HTTP-only bearer cookie, and redirect directly to `/` with `303 See Other`.
- Add a framework-free `frontend/app/auth/betterauth/callback-redirect.ts` helper so the callback redirect decision can be tested under bare Node, following the existing `sign-out-redirect.ts` smoke-test pattern.
- Remove the callback page HTML/script and remove the callback `POST` handler because the callback no longer needs a browser bridge.
- Add `frontend/app/auth/betterauth/callback.smoke.mjs` to exercise the callback helper and route-wrapper source contract.
- Add `frontend/auth/scripts/smoke-dashboard-token-redirect.mjs` to exercise the issuer redirect helper directly.
- Update `docs/architecture.md` and `docs/roadmap.md` to describe the direct callback redirect contract. Leave existing `docs/superpowers/` specs and plans as historical records.

Out of scope:

- Adding Better Auth runtime dependencies to `pandar-web`.
- Changing hub JWT verification, tenant membership, or join-link behavior.
- Implementing a server-side one-time authorization code exchange.
- Changing cookie name, max-age, SameSite, or secure-flag semantics except as required by the existing cookie helper.

## Design

### Issuer Token Redirect

`frontend/auth/lib/token.ts` fetches `/api/auth/token` once with `credentials: "include"` after Better Auth has established a session. After receiving a non-empty token, it builds a URL from `dashboardCallbackUrl`, sets `searchParams.set("token", token)`, and assigns `window.location.href` to that URL.

`PANDAR_AUTH_DASHBOARD_CALLBACK_URL` is documented and deployed as an absolute dashboard callback URL, for example `https://pandar.example.com/auth/betterauth/callback`. Using the URL API preserves any existing query parameters on that configured URL and correctly escapes the JWT. For example, a callback configured as `https://pandar.example.com/auth/betterauth/callback?source=issuer` becomes `https://pandar.example.com/auth/betterauth/callback?source=issuer&token=<jwt>`. The redirect helper must not use `#token=`.

### Dashboard Callback

`frontend/app/auth/betterauth/callback-redirect.ts` defines the framework-free callback decision helper:

```ts
export type BetterAuthCallbackRedirect =
  | { ok: true; token: string; target: string; status: 303 }
  | { ok: false; body: string; status: 400 };

export function betterAuthCallbackRedirect(
  requestUrl: string,
  isAllowedToken: (token: string) => boolean,
): BetterAuthCallbackRedirect;
```

The helper reads `token` from the URL query string, trims it, validates it with the provided `isAllowedToken` function, returns `{ ok: false, body: "malformed token", status: 400 }` for missing or invalid tokens, and returns `{ ok: true, token, target: "/", status: 303 }` for valid tokens. The route passes the existing `isAllowedDashboardJwt` function; the injected validator keeps the helper framework-free and directly testable under bare Node.

`frontend/app/auth/betterauth/callback/route.ts` becomes a thin server-only callback wrapper:

1. `GET(request)` calls `betterAuthCallbackRedirect(request.url, isAllowedDashboardJwt)`.
2. If the helper returns invalid, the route returns `400 malformed token` with `cache-control: no-store`.
3. If the helper returns valid, the route returns `NextResponse.redirect(new URL(result.target, request.url), result.status)`, sets the configured auth cookie on that response, and applies `cache-control: no-store` plus `referrer-policy: no-referrer`.

The callback route does not render a page and does not expose `POST`. A successful callback should be observable as one `GET /auth/betterauth/callback?token=...` followed by a redirect to `/` with `Set-Cookie`.

## Security / Trade-offs

The previous fragment transport kept the JWT out of HTTP requests, server logs, reverse-proxy logs, and browser `Referer` headers. A direct server redirect requires the dashboard callback server to receive the JWT, so this design moves the JWT into a query parameter instead of using the fragment-plus-POST bridge.

That is an intentional trade-off for this bug fix. The dashboard callback must mitigate the increased exposure by:

- returning `cache-control: no-store` for both success and error responses;
- returning `referrer-policy: no-referrer` on the successful redirect so the callback URL is not propagated as the referrer for the subsequent dashboard request;
- immediately redirecting to `/` instead of rendering any page from the token-bearing URL;
- keeping the existing short cookie lifetime, issuer/audience validation, and hub-side JWT verification unchanged.

Operators should still treat access logs for `pandar-web` and any reverse proxy in front of it as sensitive because the callback request line can contain a bearer JWT. A one-time authorization-code exchange would reduce that exposure, but it is out of scope for this focused fix.

The token-bearing callback URL can also remain in browser history after the redirect. That is a known consequence of choosing direct `GET` callback handling over the previous fragment bridge.

The issuer redirect helper and dashboard callback handler must deploy together. An issuer that still emits `#token=` against the new server-only callback will receive `400 malformed token`, and an issuer that emits `?token=` against the old callback will render the old bridge page instead of completing a direct redirect.

## Acceptance Criteria

- Successful Better Auth magic-link completion and passkey sign-in redirect with a query parameter named `token` set to the JWT instead of `#token=<jwt>`, preserving any existing query parameters on `PANDAR_AUTH_DASHBOARD_CALLBACK_URL`.
- `GET /auth/betterauth/callback?token=<valid jwt>` returns a `303` redirect to `/`, sets the existing configured auth cookie, includes `cache-control: no-store`, and includes `referrer-policy: no-referrer`.
- `GET /auth/betterauth/callback` and `GET /auth/betterauth/callback?token=<malformed>` return `400 malformed token`, include `cache-control: no-store`, and do not set an auth cookie.
- `frontend/app/auth/betterauth/callback/route.ts` does not return callback HTML and does not define `POST`.
- Existing issuer/audience validation for Better Auth JWTs remains unchanged.
- `docs/architecture.md` no longer describes the callback fragment as the current Better Auth dashboard return flow.
- `docs/roadmap.md` records the completed direct callback redirect fix.

## Validation

- From `frontend`, run `node --experimental-strip-types app/auth/betterauth/callback.smoke.mjs`. This smoke check imports `frontend/app/auth/betterauth/callback-redirect.ts`, calls the helper with valid, missing, and malformed tokens, and asserts the status, target, token, and error body. It also reads `frontend/app/auth/betterauth/callback/route.ts` as source text to assert the route calls `NextResponse.redirect`, sets the auth cookie, sets `cache-control: no-store`, sets `referrer-policy: no-referrer`, and does not export `POST` or `callbackHtml`.
- From `frontend/auth`, run `node --experimental-strip-types scripts/smoke-dashboard-token-redirect.mjs`. This smoke check imports `frontend/auth/lib/token.ts`, stubs `fetch` and `window.location`, and asserts the helper uses `URL.searchParams` to set a query parameter named `token`, preserves an existing query parameter, and does not use `#token=`.
- From `frontend/auth`, run `npm test` for the existing Vitest suite.
- Run `npm run build` in `frontend/auth` and `frontend` if dependency and environment availability allow it.
- Run repo-required `cargo fmt`, `cargo clippy`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace` before commit.
