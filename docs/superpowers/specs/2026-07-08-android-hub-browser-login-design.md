# Android Hub Browser Login Design

## Goal

Make the Android app match the Bambu Studio network plugin login shape: the user enters only the Pandar Hub URL, the app opens the Hub-backed browser sign-in page, and the completed browser login returns a one-use ticket to Android for token exchange.

## Current State

The Android app currently exposes Hub URL, tenant ID, OIDC discovery URL, client ID, scopes, and redirect URI in settings. `AuthRepository` drives AppAuth Authorization Code + PKCE directly against an OIDC provider. This makes Android responsible for identity-provider configuration and differs from the network plugin flow.

The network plugin flow keeps identity handling in the web/Hub layer. The local plugin page opens `/plugin-sign-in`, the frontend creates a Hub plugin login ticket for a selected tenant, and the plugin exchanges the ticket at `/api/v1/plugin/login-tickets/exchange`.

Existing plugin callback validation only accepts loopback `http://localhost|127.0.0.1|::1:<port>/...` URLs. That constraint should remain unchanged because it protects the Bambu Studio local callback contract.

## Proposed Flow

1. Android settings keeps `Hub base URL` as the only user-entered connection value.
2. Android sign-in builds a browser URL:

   ```text
   {hubBaseUrl}/mobile-sign-in?redirect_url=zip.iptables.pandar.android:/auth/callback
   ```

3. The mobile sign-in frontend page reuses the authenticated dashboard session and tenant selection behavior from `/plugin-sign-in`.
4. On submit, the frontend calls a Hub mobile login-ticket creation endpoint for the selected tenant.
5. The frontend redirects the browser to the provided Android callback URL with `ticket=<one-use ticket>` and `redirect_url=<callback URL>`.
6. `MainActivity` receives `zip.iptables.pandar.android:/auth/callback?...`, forwards it to `AuthRepository`, and `AuthRepository` exchanges the ticket with Hub.
7. The exchange response stores the returned tenant token, token expiry, and tenant id in Android settings.
8. Existing tenant-scoped Android API and WebSocket calls continue to use `SettingsRepository.currentToken()` and `currentTenant()`.

## Hub Boundary

Add mobile-specific routes instead of loosening plugin routes:

```text
POST /api/v1/tenants/{tenant_id}/mobile/login-tickets
POST /api/v1/mobile/login-tickets/exchange
```

The mobile callback validator accepts only:

```text
zip.iptables.pandar.android:/auth/callback
```

without fragments or userinfo. Plugin redirect validation remains loopback-only.

The mobile exchange can reuse the plugin ticket repository shape and create a tenant token suitable for normal tenant APIs. The token must not be restricted to the `plugin:studio` scope, because Android calls `/api/v1/tenants/{tenant_id}/...` endpoints that use normal tenant authorization.

## Frontend Boundary

Add `/mobile-sign-in` beside `/plugin-sign-in`. It should:

- Read `redirect_url` from the query string.
- Require the same browser auth state used by the dashboard.
- Let the user select a tenant when multiple tenants exist.
- Submit to a mobile ticket server action.
- Redirect to the callback URL with `ticket` and `redirect_url`.

## Android Boundary

Remove AppAuth-driven OIDC configuration from the user-facing Android settings and login flow. Keep DataStore keys harmlessly readable for existing installs, but new settings UI should not edit OIDC fields.

Register `MainActivity` for the Android callback scheme:

```text
scheme = zip.iptables.pandar.android
path = /auth/callback
```

`AuthRepository.signIn()` opens the Hub mobile sign-in URL. `AuthRepository.handleAuthorizationResponse()` parses the callback ticket and exchanges it through Retrofit. Refresh-token handling is not part of this ticket flow unless the Hub mobile exchange later returns refresh credentials; token expiry should still be stored when returned.

## Non-Goals

- Do not expose OIDC provider fields in Android settings.
- Do not make Android host a local HTTP server.
- Do not relax network-plugin localhost callback validation.
- Do not change the Bambu Studio plugin sign-in UX.

## Verification

- Hub route tests cover mobile callback acceptance, non-mobile callback rejection, one-use exchange, and tenant-token scope.
- Frontend action tests cover mobile ticket redirect URL construction.
- Android unit tests cover settings mapping and mobile callback URL construction/parsing.
- Run Android debug unit tests and debug assemble.
- Run root `cargo fmt`, `cargo clippy`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace`.
- Update `docs/roadmap.md`.
