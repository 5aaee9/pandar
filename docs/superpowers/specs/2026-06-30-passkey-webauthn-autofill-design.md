# Passkey WebAuthn Autofill Design

## Goal

Enable browser and password-manager passkey suggestions on the standalone Better Auth sign-in page when a saved passkey is available, without removing the existing explicit passkey sign-in button.

## Scope

In scope:

- `frontend/auth/components/login-form.tsx`
- `frontend/auth/tests/passkey-sign-in-ui.test.ts`
- `docs/roadmap.md`

Out of scope:

- Server-side passkey registration or verification changes.
- New auth UI screens, settings, feature flags, or provider configuration.
- Changes to the dashboard auth callback/token bridge beyond reusing the existing success redirect.

## Current Behavior

The sign-in form has an email input using `autoComplete="email"` and a separate passkey button. The passkey flow starts only when the user clicks the button, via `authClient.signIn.passkey()`. This means browsers do not get a conditional WebAuthn request to anchor password-manager passkey suggestions to the input.

## Design

Update the email input autocomplete value to include the WebAuthn token with `webauthn` last, using `autoComplete="username webauthn"` per Better Auth passkey documentation for conditional UI. This intentionally replaces the current `email` autocomplete token. The input remains `type="email"` with `inputMode="email"`, so browser validation and mobile email keyboards remain intact while the identifier field becomes eligible for WebAuthn conditional UI.

When the login form mounts, check whether the browser supports WebAuthn conditional mediation:

- `window.PublicKeyCredential` exists.
- `PublicKeyCredential.isConditionalMediationAvailable` exists.
- `await PublicKeyCredential.isConditionalMediationAvailable()` returns true.

If supported, call `authClient.signIn.passkey({ autoFill: true })`. This preloads Better Auth's conditional UI so available passkeys can appear from the password manager while the user is focused on the sign-in input. Conditional sign-in must not set the visible `pending` state, so it does not disable the magic-link or manual passkey controls while the browser-managed suggestion UI is idle.

On successful conditional passkey sign-in, reuse the existing `redirectWithAuthToken(dashboardCallbackUrl, messages)` path. If conditional passkey preload or sign-in fails, log the failure with the underlying error object for debugging, but do not show a form error. Autofill is opportunistic and the explicit magic-link and passkey controls remain available.

Keep the existing passkey button as manual fallback. The button must continue to use the visible pending state and existing error message behavior. It must continue calling `authClient.signIn.passkey()` without `autoFill` so manual sign-in stays an explicit action.

Use one shared redirect helper/guard for both conditional and manual passkey success paths. A `redirectStartedRef` or equivalent should be set before calling `redirectWithAuthToken` so a race between conditional UI completion and the manual button cannot trigger duplicate token redirects. If `redirectWithAuthToken(dashboardCallbackUrl, messages)` rejects, the helper must release the guard and rethrow so manual passkey sign-in keeps the existing visible error behavior and later attempts can retry the redirect. The conditional effect must also use a mounted/active flag with cleanup so React Strict Mode remounts or normal unmounts abandon the pending promise before redirecting or logging stale failures. The mounted flag is for cleanup safety; the shared redirect guard is what prevents duplicate redirects.

## Safety and UX

- Do not start conditional UI on unsupported browsers.
- Do not surface preload failures as form errors because conditional UI may fail due to browser support, cancellation, or password-manager state while normal sign-in remains usable.
- Preserve failure context by logging conditional passkey failures with the error object.
- Avoid duplicate redirects after a successful passkey sign-in by sharing a redirect-started guard across conditional and manual passkey flows, and release that guard when the redirect helper rejects.
- Avoid acting on conditional passkey promises after unmount or Strict Mode cleanup.
- Do not add new visible UI. Password-manager suggestions are browser-managed.

## Acceptance Criteria

- The email/sign-in identifier input includes a WebAuthn autocomplete token with `webauthn` last.
- The sign-in form source guards Better Auth conditional passkey UI behind `PublicKeyCredential.isConditionalMediationAvailable()` and calls `authClient.signIn.passkey({ autoFill: true })` from the mount effect only when supported.
- A successful conditional passkey sign-in redirects through `redirectWithAuthToken(dashboardCallbackUrl, messages)`.
- Conditional passkey failure logs the underlying error object and does not set the visible form error.
- Conditional passkey sign-in does not set the visible `pending` state.
- Conditional and manual passkey success paths share a duplicate-redirect guard that releases on `redirectWithAuthToken` rejection.
- The existing explicit passkey button still works and still shows the existing pending/error behavior.
- The existing auth UI source test is extended to pin the WebAuthn autocomplete, conditional-mediation guard, `autoFill: true` behavior, hidden conditional failure handling, no conditional `pending` state, shared duplicate-redirect guard, and guard release on redirect failure.
- `docs/roadmap.md` records the completed auth autofill change.

## Verification

Run at least:

- `npm --prefix frontend/auth test -- passkey-sign-in-ui`

Because this is a frontend-only scoped change in the auth package, Rust clippy/nextest are not expected to cover the changed behavior. If time allows, run the full auth package test script as an additional check.
