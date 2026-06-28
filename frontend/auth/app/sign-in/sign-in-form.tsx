"use client";

import { useState } from "react";

import { authClient } from "../../lib/auth-client";
import type { SignInMessages } from "../../lib/i18n";
import { redirectWithAuthToken } from "../../lib/token";

type SignInFormProps = {
  dashboardCallbackUrl: string;
  messages: SignInMessages;
};

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

export function SignInForm({ dashboardCallbackUrl, messages }: SignInFormProps) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function signIn() {
    setPending(true);
    setError(null);

    try {
      const result = await authClient.signIn.passkey();
      if (result.error) {
        throw new Error(result.error.message || messages.passkeySignInFailed);
      }

      await redirectWithAuthToken(dashboardCallbackUrl, messages);
    } catch (caught) {
      setError(errorMessage(caught, messages.unableSignIn));
      setPending(false);
    }
  }

  return (
    <div className="auth-form">
      {error ? (
        <div className="auth-error" role="alert">
          <span>{messages.signInFailed}</span>
          {error}
        </div>
      ) : null}
      <button
        className="auth-button"
        disabled={pending}
        type="button"
        onClick={signIn}
      >
        {pending ? messages.signingIn : messages.signInWithPasskey}
      </button>
    </div>
  );
}
