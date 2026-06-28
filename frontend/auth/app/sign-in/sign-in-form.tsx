"use client";

import { useState } from "react";

import { authClient } from "../../lib/auth-client";
import { redirectWithAuthToken } from "../../lib/token";

type SignInFormProps = {
  dashboardCallbackUrl: string;
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Unable to sign in";
}

export function SignInForm({ dashboardCallbackUrl }: SignInFormProps) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function signIn() {
    setPending(true);
    setError(null);

    try {
      const result = await authClient.signIn.passkey();
      if (result.error) {
        throw new Error(result.error.message || "Passkey sign-in failed");
      }

      await redirectWithAuthToken(dashboardCallbackUrl);
    } catch (caught) {
      setError(errorMessage(caught));
      setPending(false);
    }
  }

  return (
    <div className="auth-form">
      {error ? <div className="auth-error">{error}</div> : null}
      <button
        className="auth-button"
        disabled={pending}
        type="button"
        onClick={signIn}
      >
        {pending ? "Signing in..." : "Sign in with passkey"}
      </button>
    </div>
  );
}
