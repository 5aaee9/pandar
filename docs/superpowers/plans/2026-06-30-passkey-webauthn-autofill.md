# Passkey WebAuthn Autofill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable browser/password-manager passkey suggestions on the standalone Better Auth sign-in page while preserving the existing magic-link and manual passkey flows.

**Architecture:** Keep the change localized to `LoginForm`. Add a shared passkey redirect helper guarded by a `useRef` flag, start Better Auth conditional WebAuthn UI from a mount effect when browser support is present, and source-pin the expected behavior in the existing Vitest UI test.

**Tech Stack:** Next.js App Router, React 19 client component, Better Auth passkey client, TypeScript, Vitest source-regex test.

## Global Constraints

- Scope is limited to `frontend/auth/components/login-form.tsx`, `frontend/auth/tests/passkey-sign-in-ui.test.ts`, and `docs/roadmap.md`.
- Do not change server-side passkey registration or verification.
- Do not add new auth UI screens, settings, feature flags, or provider configuration.
- The email input must use `autoComplete="username webauthn"` with `webauthn` last.
- Conditional passkey UI must call `authClient.signIn.passkey({ autoFill: true })` only after `PublicKeyCredential.isConditionalMediationAvailable()` resolves true.
- Conditional passkey sign-in must not set the visible `pending` state or visible form error.
- Conditional passkey failures must log the underlying error object.
- Manual passkey sign-in must continue to call `authClient.signIn.passkey()` without `autoFill` and must keep its pending/error behavior.
- Conditional and manual passkey success paths must share a duplicate-redirect guard; the guard must no-op when already held and must release before rethrowing if `redirectWithAuthToken` rejects.
- Update `docs/roadmap.md` after the implementation.

---

## File Structure

- `frontend/auth/components/login-form.tsx`: Owns all sign-in UI state and both passkey flows. Add `useRef`, a shared `redirectAfterPasskeySignIn` helper, and the conditional mediation `useEffect`.
- `frontend/auth/tests/passkey-sign-in-ui.test.ts`: Existing source-level Vitest coverage for the passkey sign-in UI. Extend it with regex/string assertions for the conditional WebAuthn behavior and guard semantics.
- `docs/roadmap.md`: Append one bullet under the main `## Completed` list describing passkey WebAuthn autofill on the Better Auth issuer sign-in page.

---

### Task 1: Pin Passkey Autofill Behavior In The Existing UI Test

**Files:**

- Modify: `frontend/auth/tests/passkey-sign-in-ui.test.ts`

**Interfaces:**

- Consumes: Current `readSource` helper and `loginForm` source string.
- Produces: Failing test assertions for the code changes in Task 2.

- [ ] **Step 1: Add source assertions for autocomplete, conditional mediation, autofill, hidden failure handling, and redirect guard semantics**

Modify the existing test body in `frontend/auth/tests/passkey-sign-in-ui.test.ts` so the passkey UI test includes these assertions after the existing `authClient.signIn.passkey()` assertion:

```ts
const passkeyAutofill =
  loginForm.match(
    /async function preloadPasskeyAutofill\(\) \{[\s\S]*?void preloadPasskeyAutofill\(\);/,
  )?.[0] ?? "";

expect(loginForm).toMatch(/autoComplete="username webauthn"/);
expect(loginForm).toMatch(/PublicKeyCredential/);
expect(loginForm).toMatch(/isConditionalMediationAvailable/);
expect(passkeyAutofill).toMatch(
  /authClient\.signIn\.passkey\(\{\s*autoFill: true,?\s*\}\)/,
);
expect(passkeyAutofill).toMatch(/throw result\.error/);
expect(passkeyAutofill).toMatch(/console\.warn\([^)]*passkey[^)]*error/i);
expect(passkeyAutofill).not.toMatch(/setPending/);
expect(passkeyAutofill).not.toMatch(/setError/);
expect(loginForm).toMatch(/redirectStartedRef\.current/);
expect(loginForm).toMatch(/redirectStartedRef\.current = false/);
expect(loginForm).toMatch(
  /if \(redirectStartedRef\.current\) \{\s*return;\s*\}/,
);
expect(loginForm).toMatch(/let active = true/);
```

Keep the existing assertions for `redirectWithAuthToken(dashboardCallbackUrl, messages)`, `dashboardCallbackUrl={env.dashboardCallbackUrl}`, and i18n keys unchanged.

- [ ] **Step 2: Run the focused UI test and confirm it fails for the missing behavior**

Run:

```bash
npm --prefix frontend/auth test -- passkey-sign-in-ui
```

Expected before implementation: FAIL. The failure should mention at least the missing `autoComplete="username webauthn"` or missing `autoFill: true` assertion.

---

### Task 2: Implement Conditional WebAuthn Autofill In LoginForm

**Files:**

- Modify: `frontend/auth/components/login-form.tsx`

**Interfaces:**

- Consumes: `authClient.signIn.passkey`, `redirectWithAuthToken(dashboardCallbackUrl, messages)`, existing `pending` state for manual controls.
- Produces: Shared helper `redirectAfterPasskeySignIn(): Promise<void>` and mount effect that starts conditional passkey UI when supported.

- [ ] **Step 1: Import `useRef`**

Change the React import at the top of `frontend/auth/components/login-form.tsx` from:

```ts
import { FormEvent, useEffect, useState } from "react";
```

to:

```ts
import { FormEvent, useEffect, useRef, useState } from "react";
```

- [ ] **Step 2: Add the duplicate-redirect guard ref and shared redirect helper**

Inside `LoginForm`, after the existing state declarations, add:

```ts
const redirectStartedRef = useRef(false);

async function redirectAfterPasskeySignIn() {
  if (redirectStartedRef.current) {
    return;
  }

  redirectStartedRef.current = true;
  try {
    await redirectWithAuthToken(dashboardCallbackUrl, messages);
  } catch (error) {
    redirectStartedRef.current = false;
    throw error;
  }
}
```

This helper is intentionally local because it depends on component props and the guard ref.

- [ ] **Step 3: Add the conditional WebAuthn mount effect**

After the existing cooldown `useEffect`, add this effect:

```ts
useEffect(() => {
  let active = true;

  async function preloadPasskeyAutofill() {
    const publicKeyCredential = window.PublicKeyCredential;
    if (!publicKeyCredential?.isConditionalMediationAvailable) {
      return;
    }

    try {
      const available =
        await publicKeyCredential.isConditionalMediationAvailable();
      if (!available || !active) {
        return;
      }

      const result = await authClient.signIn.passkey({ autoFill: true });
      if (!active) {
        return;
      }
      if (result.error) {
        throw result.error;
      }

      await redirectAfterPasskeySignIn();
    } catch (error) {
      if (active) {
        console.warn("Passkey autofill sign-in failed", error);
      }
    }
  }

  void preloadPasskeyAutofill();

  return () => {
    active = false;
  };
}, [dashboardCallbackUrl, messages]);
```

Do not call `setPending` or `setError` in this effect.

- [ ] **Step 4: Route the manual passkey button through the shared helper**

In `signInWithPasskey`, replace:

```ts
await redirectWithAuthToken(dashboardCallbackUrl, messages);
```

with:

```ts
await redirectAfterPasskeySignIn();
```

Keep the existing `if (pending) return`, `setPending("passkey")`, `setError(null)`, catch `setError(messages.passkeySignInFailed)`, and `setPending(null)` behavior unchanged.

- [ ] **Step 5: Change the email input autocomplete token**

In the `<Input id="email" ... />`, replace:

```tsx
autoComplete = "email";
```

with:

```tsx
autoComplete = "username webauthn";
```

Keep `type="email"` and `inputMode="email"` unchanged.

- [ ] **Step 6: Run the focused UI test and confirm it passes**

Run:

```bash
npm --prefix frontend/auth test -- passkey-sign-in-ui
```

Expected after implementation: PASS.

---

### Task 3: Update Roadmap And Run Final Verification

**Files:**

- Modify: `docs/roadmap.md`

**Interfaces:**

- Consumes: Implemented `LoginForm` and passing `passkey-sign-in-ui` test.
- Produces: Documentation note and final verification evidence.

- [ ] **Step 1: Add the roadmap entry**

In `docs/roadmap.md`, append this bullet near the end of the main `## Completed` list, after the current latest auth/frontend completion bullet:

```md
- Added WebAuthn conditional passkey autofill to the standalone Better Auth issuer sign-in page: the identifier field now advertises `username webauthn`, the page preloads Better Auth passkey autofill when conditional mediation is available, and the existing manual passkey button remains the visible fallback.
```

- [ ] **Step 2: Run focused verification**

Run:

```bash
npm --prefix frontend/auth test -- passkey-sign-in-ui
```

Expected: PASS.

- [ ] **Step 3: Run the auth package test script**

Run:

```bash
npm --prefix frontend/auth test
```

Expected: PASS.

- [ ] **Step 4: Inspect the final diff**

Run:

```bash
git diff -- frontend/auth/components/login-form.tsx frontend/auth/tests/passkey-sign-in-ui.test.ts docs/roadmap.md docs/superpowers/specs/2026-06-30-passkey-webauthn-autofill-design.md docs/superpowers/plans/2026-06-30-passkey-webauthn-autofill.md
```

Expected: Diff is limited to the scoped implementation, source test, roadmap update, and SDD artifacts.

---

## Self-Review

- Spec coverage: Task 1 pins acceptance criteria in the existing test; Task 2 implements WebAuthn autocomplete, conditional mediation gating, `autoFill: true`, hidden failure behavior with underlying error logging, no conditional pending state, manual fallback preservation, and shared redirect guard with reset on rejection; Task 3 updates roadmap and verifies.
- Placeholder scan: No TBD/TODO placeholders remain.
- Type consistency: `redirectAfterPasskeySignIn(): Promise<void>` is local to `LoginForm`; `redirectStartedRef.current` is a boolean guard; the Better Auth conditional call uses `authClient.signIn.passkey({ autoFill: true })`; the manual call remains `authClient.signIn.passkey()`.
