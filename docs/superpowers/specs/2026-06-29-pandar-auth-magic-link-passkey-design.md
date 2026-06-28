# Pandar Auth Magic Link And Optional Passkey Design

## Goal

Replace the self-hosted `pandar-auth` passkey-first login with an email magic-link-first login, keep optional passkey binding immediately after login, and update deployment surfaces so operators can send auth email through Resend or SMTP.

## Decisions From Grilling

- Keep the auth authority in the existing `frontend/auth` Better Auth app.
- Keep the dashboard JWT callback contract unchanged: `pandar-auth` mints the existing JWT and redirects to `PANDAR_AUTH_DASHBOARD_CALLBACK_URL`.
- Make email magic link the default login and account creation path. First-time users are created by Better Auth magic-link verification.
- Remove the old pre-auth passkey registration path. Passkey binding requires an authenticated Better Auth session and is optional.
- After magic-link verification, route users to `/auth/complete`, where they can add a passkey or skip. Passkey errors do not block dashboard entry.
- Redirect `/sign-up` to `/sign-in`; do not keep a separate signup experience.
- Do not add special legacy fallback for existing passkey-only users.
- Use the shadcn `login-05` template inside `frontend/auth`, not the main `frontend` dashboard app.
- Keep auth UI localization through `frontend/auth/lib/i18n.ts`.
- Email provider selection is explicit with one active provider: `PANDAR_AUTH_EMAIL_PROVIDER=resend|smtp`.
- Resend uses the official HTTP API with `RESEND_API_KEY` plus shared `PANDAR_AUTH_EMAIL_FROM`.
- SMTP uses explicit host, port, username, password, from address, and `PANDAR_AUTH_SMTP_TLS=starttls|tls|none`, defaulting to `starttls`.
- Magic links expire after 30 minutes by default via `PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS=1800`.
- The “check your email” state does not reveal whether an account existed.
- Add a resend button with client-side cooldown only. The default cooldown is 60 seconds and does not add persisted server-side rate limiting.
- Add `PANDAR_AUTH_EMAIL_BRAND_NAME`, defaulting to `Pandar`.
- Send both plain-text and minimal HTML email bodies.
- Update deployment docs and NixOS options for the new auth email environment.
- Dashboard user management is deferred to a later phase.

## Current Architecture

`frontend/auth` is a separate Next.js Better Auth issuer. It currently uses `@better-auth/passkey` for unauthenticated passkey registration and passkey sign-in, plus Better Auth's JWT plugin for RS256 JWT/JWKS issuance. `frontend` receives that JWT through `/auth/betterauth/callback` and stores the bearer cookie. `pandar-hub` verifies the JWT using the existing external-auth contract and remains authoritative for tenant membership and roles.

This change must not add Better Auth runtime dependencies to `frontend/`, must not change hub JWT verification, and must not change the dashboard callback token shape.

## Auth Flow

### Sign In

1. User opens `pandar-auth` `/sign-in`.
2. The page renders the shadcn `login-05` layout adapted for Pandar copy, localization, and the existing issuer/session context.
3. User submits an email address.
4. The client calls Better Auth `authClient.signIn.magicLink` with:
   - `email`
   - `name` set to the normalized email local part before `@`
   - `callbackURL: "/auth/complete"`
   - `newUserCallbackURL: "/auth/complete"`
   - `errorCallbackURL: "/sign-in"`
5. The page shows a neutral “check your email” state regardless of whether the account existed.
6. The sent state includes a resend button. The button is disabled during pending sends and during the 60-second client cooldown.

### Magic Link Verification And Completion

1. Better Auth verifies the magic link and establishes a Better Auth session.
2. The browser lands on `/auth/complete`.
3. The completion page offers:
   - primary action: add a passkey,
   - secondary action: skip and continue,
   - retry action when passkey registration fails.
4. Add passkey calls `authClient.passkey.addPasskey()` as an authenticated action. It must not pass unauthenticated registration context or create a user.
5. Skip and successful passkey registration both call the existing JWT token helper and redirect to `PANDAR_AUTH_DASHBOARD_CALLBACK_URL#token=...`.
6. If passkey registration fails or WebAuthn is unsupported, the page shows a localized error and keeps skip/continue available.

### Sign Up

`/sign-up` redirects to `/sign-in`. The old email/name pre-auth passkey registration form is removed from the user flow.

## Better Auth Configuration

`frontend/auth/lib/auth.ts` must:

- keep `betterAuth`, SQLite database, base URL, trusted origins, secret, JWT plugin, and passkey plugin;
- add Better Auth's `magicLink` plugin from `better-auth/plugins`;
- configure `magicLink({ expiresIn, sendMagicLink })`;
- remove `passkey({ registration: { requireSession: false, resolveUser } })` and use session-required passkey registration;
- keep JWT payload claims: `email`, `email_verified`, `name`, and `preferred_username`;
- keep RS256 JWKS behavior unchanged.

`expiresIn` uses `env.magicLinkTtlSeconds`, default `1800`.

The `sendMagicLink` callback calls a repo-local mailer module with the email address, generated URL, brand name, and TTL. Mailer errors must propagate so send failures are visible to Better Auth and the user can retry.

`frontend/auth/lib/auth-client.ts` must register Better Auth's magic-link client plugin alongside `passkeyClient`, so `authClient.signIn.magicLink` is typed and available.

First-time magic-link users use the normalized email local part before `@` as their Better Auth `name`. JWT `preferred_username` continues to derive from the normalized email local part. Existing users keep their stored Better Auth name.

## Email Configuration

`frontend/auth/lib/env.ts` owns validation. It must fail fast at runtime boot for missing or invalid provider-specific settings, while still permitting `next build` without production email secrets when existing build behavior requires it. Runtime boot means normal `next start`, local development server startup, and migration/smoke execution paths that import `auth.ts` outside a build. Those paths need dummy but valid email env when they initialize Better Auth without actually sending mail.

Shared:

- `PANDAR_AUTH_EMAIL_PROVIDER`: required for runtime email sending; allowed values `resend` or `smtp`.
- `PANDAR_AUTH_EMAIL_FROM`: required for both providers.
- `PANDAR_AUTH_EMAIL_BRAND_NAME`: optional, default `Pandar`.
- `PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS`: optional positive integer, default `1800`.

Resend:

- `RESEND_API_KEY`: required.
- Use `https://api.resend.com/emails`.
- Send JSON body with `from`, `to`, `subject`, `text`, and `html`.
- Treat non-2xx responses as send failures with response status and body context.

SMTP:

- `PANDAR_AUTH_SMTP_HOST`: required.
- `PANDAR_AUTH_SMTP_PORT`: required positive integer.
- `PANDAR_AUTH_SMTP_USERNAME`: required.
- `PANDAR_AUTH_SMTP_PASSWORD`: required.
- `PANDAR_AUTH_SMTP_TLS`: optional `starttls|tls|none`, default `starttls`.
- Use a maintained Node mail package that supports plain text, HTML, STARTTLS, direct TLS, and no TLS. `nodemailer` is acceptable for this scope.

No provider fallback is allowed. If Resend fails, do not fall back to SMTP, and vice versa.

## Email Content

Subject:

```text
Sign in to <brand>
```

Plain text:

```text
Use this link to sign in to <brand>:

<url>

This link expires in <duration>.
If you did not request this email, you can ignore it.
```

HTML:

- minimal HTML document;
- escaped brand and URL text;
- visible button/link to the magic link URL;
- expiry and ignore-if-unrequested copy.

Both bodies are generated from the same inputs so they cannot drift.

## UI Requirements

The auth app should be initialized or adapted with:

```bash
cd frontend/auth
npx shadcn@latest add login-05
```

If the command cannot run cleanly in this repo, manually adapt the generated `login-05` structure while preserving the intent: a polished sign-in page using local shadcn-style primitives. Dependencies and generated UI primitives stay under `frontend/auth`.

The UI must:

- use localized strings from `frontend/auth/lib/i18n.ts`;
- render the email field, send button, sent state, resend button/cooldown, and errors;
- preserve the existing issuer/return-target/session-lifetime context or equivalent information;
- keep text fitting at mobile and desktop widths;
- not add account management to the dashboard.

## Nix And Docs

Update `nix/nixos-module.nix` with options for:

- `services.pandar-auth.email.provider`
- `services.pandar-auth.email.from`
- `services.pandar-auth.email.brandName`
- `services.pandar-auth.email.magicLinkTtlSeconds`
- `services.pandar-auth.email.smtp.host`
- `services.pandar-auth.email.smtp.port`
- `services.pandar-auth.email.smtp.username`
- `services.pandar-auth.email.smtp.tls`

Secrets such as `BETTER_AUTH_SECRET`, `RESEND_API_KEY`, and `PANDAR_AUTH_SMTP_PASSWORD` should remain supplied through `environmentFile` or `extraEnvironment`; do not add typed Nix options for secret values unless the module already stores secrets that way.

Update Nix packaging checks in `nix/pandar.nix` for new option environment variables and migration/smoke build environments. Regenerate or update `docs/deployment/nixos/options.md`. Update `docs/development.md`, `docs/architecture.md` if needed, and `docs/roadmap.md`.

Also inspect `docs/release-installation.md`. Update it if it documents self-hosted `pandar-auth` setup or auth environment variables; otherwise leave it unchanged and note that it is unaffected by this phase.

## Testing And Verification

Expected focused checks:

- `frontend/auth` build passes.
- `frontend/auth` smoke script verifies JWT/JWKS behavior still works after plugin changes.
- Add unit-level or script-level coverage for:
  - environment parsing for Resend and SMTP,
  - missing provider-specific env fails at runtime,
  - email body generation includes text and HTML,
  - `/sign-up` redirects to `/sign-in` if testable without full browser.
- Rewrite `frontend/auth/scripts/smoke-jwt-and-registration.mjs` so it no longer tests the removed unauthenticated passkey `resolveUser` path. It must retain JWT/JWKS payload checks, assert `name` and `preferred_username` behavior for email-created users, and cover that passkey registration is session-required through the current Better Auth configuration or a focused config assertion.
- Nix checks covering `pandar-auth` module environment still pass where available.
- Repo-level verification remains:
  - `cargo fmt`
  - `cargo clippy`
  - `cargo nextest run --manifest-path "Cargo.toml" --workspace`

If full repo verification cannot complete because of unrelated environment limits, report the exact command and failure.

## Out Of Scope

- Dashboard user management.
- Server-side persisted resend/rate limiting.
- Migrating legacy passkey-only accounts.
- Changing hub external-auth verification.
- Adding social login or password login.
- Adding template customization beyond `PANDAR_AUTH_EMAIL_BRAND_NAME`.

## Acceptance Criteria

1. `/sign-in` uses the adapted shadcn `login-05` experience and starts magic-link login by email.
2. Magic-link email sends through exactly one configured provider, Resend or SMTP.
3. Email messages include plain text and HTML bodies, use a configurable brand name, and default link expiry to 30 minutes.
4. Sent state hides account existence and supports resend with a 60-second client cooldown.
5. Magic-link callback reaches `/auth/complete`, where passkey binding is optional and non-blocking.
6. Successful completion or skip redirects through the existing dashboard JWT callback contract.
7. `/sign-up` redirects to `/sign-in`.
8. Deployment docs and NixOS options describe the new email configuration.
9. Existing dashboard and hub auth contracts remain unchanged.
