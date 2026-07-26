# Android Hub Browser Login Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Android direct OIDC configuration with a Hub browser login flow that returns a one-use ticket to Android.

**Architecture:** Add mobile login-ticket routes that mirror the network-plugin ticket shape without loosening plugin localhost validation. Add a `/mobile-sign-in` frontend page/action that redirects to the Android custom scheme. Update Android to collect only Hub URL, open the mobile sign-in page, receive the callback intent, exchange the ticket, and store the tenant token.

**Tech Stack:** Rust axum/SeaORM, Next.js server actions, Kotlin Android Compose, Retrofit, DataStore.

## Global Constraints

- Keep network-plugin callback validation loopback-only.
- Android callback URL is exactly `zip.iptables.pandar.android://auth/callback`.
- Android settings UI must not expose OIDC discovery/client/scopes/redirect fields.
- Persistent Hub data access must remain backend-neutral and covered by existing SQLite/PostgreSQL test paths when database behavior changes.
- Update `docs/roadmap.md` after code changes.
- Run `cargo fmt`, `cargo clippy`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace` after code changes.

---

## File Structure

- Modify `crates/pandar-hub/src/repositories/auth/plugin_tickets.rs`: add a mobile callback validator and reusable exchange helper if needed.
- Modify `crates/pandar-hub/src/repositories/auth/tenant_tokens.rs`: add a mobile token creation helper with normal tenant scope.
- Modify `crates/pandar-hub/src/routes/plugin.rs`: add mobile ticket request/response handlers close to plugin ticket handlers.
- Modify `crates/pandar-hub/src/routes.rs`: register mobile routes.
- Modify `crates/pandar-hub/src/routes/tests/plugin.rs`: add route-level mobile login-ticket tests.
- Modify `crates/pandar-hub/src/repositories/tests/tenant_tokens.rs`: add repository-level mobile callback/token scope tests.
- Modify `frontend/app/actions.ts`: add `createMobileTicket`.
- Create `frontend/app/mobile-sign-in/page.tsx`: mobile sign-in page.
- Create `frontend/app/mobile-sign-in/mobile-ticket-form.tsx`: mobile tenant/ticket form.
- Modify or add frontend tests in `frontend/app/actions.test.ts`.
- Modify `mobile/android/app/src/main/AndroidManifest.xml`: add Android callback intent filter to `MainActivity`.
- Modify `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/PandarApi.kt`: add mobile ticket exchange endpoint.
- Add Android DTOs under `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/dto`.
- Modify `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/auth/AuthRepository.kt`: replace AppAuth sign-in with Hub mobile browser sign-in and ticket exchange.
- Modify `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/settings/*`: remove OIDC config from active state and keep Hub URL/token/tenant mapping.
- Modify `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/settings/*`: simplify settings UI to Hub URL plus account actions.
- Update Android unit tests under `mobile/android/app/src/test/kotlin`.
- Modify `docs/roadmap.md`.

### Task 1: Hub Mobile Ticket API

**Files:**

- Modify: `crates/pandar-hub/src/repositories/auth/plugin_tickets.rs`
- Modify: `crates/pandar-hub/src/repositories/auth/tenant_tokens.rs`
- Modify: `crates/pandar-hub/src/routes/plugin.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Test: `crates/pandar-hub/src/routes/tests/plugin.rs`
- Test: `crates/pandar-hub/src/repositories/tests/tenant_tokens.rs`

**Interfaces:**

- Produces: `POST /api/v1/tenants/{tenant_id}/mobile/login-tickets`
- Produces: `POST /api/v1/mobile/login-tickets/exchange`
- Produces: exchange response `{ token, expires_at, profile: { user_id, user_name, tenant_id, tenant_name } }`

- [ ] Add failing repository tests for `validate_mobile_redirect_url("zip.iptables.pandar.android://auth/callback")` success and non-matching callback rejection.
- [ ] Add failing route test that creates a mobile ticket with the Android callback, exchanges it once, receives a token, and sees second exchange rejected.
- [ ] Implement mobile callback validation without changing `validate_plugin_redirect_url`.
- [ ] Add a mobile token creation path that creates a normal tenant token usable by tenant APIs.
- [ ] Add axum handlers and router registrations.
- [ ] Run `cargo test -p pandar-hub plugin_login_ticket`.

### Task 2: Frontend Mobile Sign-In

**Files:**

- Modify: `frontend/app/actions.ts`
- Create: `frontend/app/mobile-sign-in/page.tsx`
- Create: `frontend/app/mobile-sign-in/mobile-ticket-form.tsx`
- Test: `frontend/app/actions.test.ts`

**Interfaces:**

- Consumes: `POST /api/v1/tenants/{tenant_id}/mobile/login-tickets`
- Produces: browser redirect to `zip.iptables.pandar.android://auth/callback?ticket=<ticket>&redirect_url=<callback>`

- [ ] Add a failing action test for `createMobileTicket` redirecting to the callback with `ticket` and `redirect_url`.
- [ ] Implement `createMobileTicket` next to `createPluginTicket`.
- [ ] Add `/mobile-sign-in` page by reusing the plugin sign-in tenant/readiness pattern but without Studio callback discovery.
- [ ] Add a small mobile ticket form that submits `tenant_id` and `redirect_url`.
- [ ] Run the existing frontend action test command from the repo if available; otherwise run the closest package test command discovered in `frontend/package.json`.

### Task 3: Android Browser Login

**Files:**

- Modify: `mobile/android/app/src/main/AndroidManifest.xml`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/PandarApi.kt`
- Add: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/dto/MobileAuthDto.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/auth/AuthRepository.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/settings/SettingsSnapshot.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/settings/SettingsMapping.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/settings/SettingsRepository.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/settings/SettingsScreen.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/settings/SettingsViewModel.kt`
- Test: `mobile/android/app/src/test/kotlin/zip/iptables/pandar/android/data/settings/SettingsMappingTest.kt`

**Interfaces:**

- Consumes: `/api/v1/mobile/login-tickets/exchange`
- Produces: Android callback handling for `zip.iptables.pandar.android://auth/callback`

- [ ] Add failing Android unit tests for settings readiness with Hub URL only.
- [ ] Add Retrofit DTOs and API method for mobile ticket exchange.
- [ ] Register the custom scheme callback intent filter on `MainActivity`.
- [ ] Replace AppAuth sign-in with `Intent.ACTION_VIEW` to `{hub}/mobile-sign-in?redirect_url=zip.iptables.pandar.android://auth/callback`.
- [ ] Parse callback `ticket`, exchange it, and store token, expiry, and tenant id.
- [ ] Remove OIDC fields from the settings UI and messages.
- [ ] Run `.\gradlew.bat :app:testDebugUnitTest` and `.\gradlew.bat :app:assembleDebug` from `mobile/android`.

### Task 4: Verification And Documentation

**Files:**

- Modify: `docs/roadmap.md`

**Interfaces:**

- Consumes: completed Hub, frontend, and Android changes.
- Produces: verified worktree with roadmap entry.

- [ ] Update `docs/roadmap.md` with the Android Hub browser-login completion.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo clippy`.
- [ ] Run `cargo nextest run --manifest-path "Cargo.toml" --workspace`.
- [ ] Run Android tests/build again if Rust changes forced any regenerated files or docs-only edits did not affect Android.
