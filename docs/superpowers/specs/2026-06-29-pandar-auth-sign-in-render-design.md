# Pandar Auth Sign-In Render Fix Design

## Goal

Fix the production `/sign-in` render failure in `pandar-auth` while preserving the existing Better Auth email magic-link sign-in behavior.

## Evidence

Production request:

- `https://pandar-auth.nix.ac.cn/sign-in` returns HTTP 500.
- `journalctl -u pandar-auth.service` reports: `Functions cannot be passed directly to Client Components`.
- The offending prop is `magicLinkResendCooldown: function magicLinkResendCooldown` passed from `frontend/auth/app/sign-in/page.tsx` into the client component `SignInForm`.

## Scope

Change only the sign-in cooldown message boundary so all props passed to `SignInForm` are serializable across the Next.js Server Component to Client Component boundary.

Out of scope:

- Changing Better Auth provider behavior.
- Changing email delivery behavior.
- Changing production NixOS configuration or restarting production services. The user explicitly authorized commit and push; production update remains operator-owned after push.
- Redesigning the sign-in UI.

## Design

Represent the cooldown copy as a serializable string template containing `{seconds}` instead of a JavaScript formatter function. `SignInForm`, which already owns the live countdown state, formats that template on the client by replacing `{seconds}` with the current cooldown value.

This keeps localization in `frontend/auth/lib/i18n.ts`, keeps dynamic countdown rendering in `frontend/auth/app/sign-in/sign-in-form.tsx`, and avoids passing functions through the Server Component payload.

## Acceptance Criteria

- `frontend/auth` production build succeeds.
- Local `pandar-auth` can run with these production-shaped environment values: `PANDAR_AUTH_BASE_URL`, `PANDAR_AUTH_TRUSTED_ORIGINS`, `PANDAR_AUTH_DASHBOARD_CALLBACK_URL`, `PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL`, `PANDAR_AUTH_DATABASE_FILE`, `PANDAR_AUTH_EMAIL_PROVIDER=resend`, `PANDAR_AUTH_EMAIL_FROM`, `PANDAR_AUTH_EMAIL_BRAND_NAME`, `PANDAR_AUTH_JWT_MAX_AGE_SECONDS`, `PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS`, `BETTER_AUTH_SECRET`, `RESEND_API_KEY`, `HOSTNAME`, and `PORT`.
- A temporary Playwright smoke from an isolated `/tmp` npm project can open local `/sign-in`, receive a non-500 response, find the `Sign in to Pandar` heading and email input, and observe no browser console error containing `Functions cannot be passed directly to Client Components`. On NixOS, use `playwright-core` with `nixpkgs#playwright-driver.browsers` so Chromium has its runtime libraries.
- Local `/sign-in` returns HTTP 200.
- The fix is committed and pushed for deployment by the operator.

## Documentation Impact

Update `docs/roadmap.md` to record the auth issuer sign-in render fix.
