"use client";

import { FormEvent, useState } from "react";

import { authClient } from "../../lib/auth-client";
import type { SignUpMessages } from "../../lib/i18n";
import { redirectWithAuthToken } from "../../lib/token";

type SignUpFormProps = {
  dashboardCallbackUrl: string;
  messages: SignUpMessages;
};

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

export function SignUpForm({ dashboardCallbackUrl, messages }: SignUpFormProps) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function signUp(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    setError(null);

    const form = new FormData(event.currentTarget);
    const email = String(form.get("email") ?? "").trim();
    const name = String(form.get("name") ?? "").trim();

    try {
      const addPasskey = await authClient.passkey.addPasskey({
        context: JSON.stringify({ email, name }),
        name,
      });
      if (addPasskey.error) {
        throw new Error(
          addPasskey.error.message || messages.passkeyRegistrationFailed,
        );
      }

      const signIn = await authClient.signIn.passkey();
      if (signIn.error) {
        throw new Error(signIn.error.message || messages.passkeySignInFailed);
      }

      await redirectWithAuthToken(dashboardCallbackUrl, messages);
    } catch (caught) {
      setError(errorMessage(caught, messages.unableCreateAccount));
      setPending(false);
    }
  }

  return (
    <form className="auth-form" onSubmit={signUp}>
      <label className="auth-field">
        <span>{messages.email}</span>
        <input
          autoComplete="email webauthn"
          inputMode="email"
          name="email"
          required
          type="email"
        />
      </label>
      <label className="auth-field">
        <span>{messages.name}</span>
        <input autoComplete="name" name="name" required type="text" />
      </label>
      {error ? (
        <div className="auth-error" role="alert">
          <span>{messages.registerFailed}</span>
          {error}
        </div>
      ) : null}
      <p className="auth-note">{messages.deviceConfirmation}</p>
      <button className="auth-button" disabled={pending} type="submit">
        {pending ? messages.signingUp : messages.createAccountWithPasskey}
      </button>
    </form>
  );
}
