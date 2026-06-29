import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const root = dirname(dirname(fileURLToPath(import.meta.url)));

async function readSource(path: string): Promise<string> {
  return readFile(join(root, path), "utf8");
}

describe("passkey sign-in UI", () => {
  it("exposes a passkey sign-in action that returns through the dashboard token flow", async () => {
    const loginForm = await readSource("components/login-form.tsx");
    const signInPage = await readSource("app/sign-in/page.tsx");
    const i18n = await readSource("lib/i18n.ts");

    expect(loginForm).toMatch(/authClient\.signIn\.passkey\(/);
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
    expect(loginForm).toMatch(
      /redirectWithAuthToken\(dashboardCallbackUrl, messages\)/,
    );
    expect(signInPage).toMatch(
      /dashboardCallbackUrl=\{env\.dashboardCallbackUrl\}/,
    );

    for (const key of [
      "or",
      "passkeySignIn",
      "passkeySigningIn",
      "passkeySignInFailed",
    ]) {
      expect(i18n).toMatch(new RegExp(`${key}:`));
    }
  });
});
