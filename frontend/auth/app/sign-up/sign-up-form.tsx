"use client";

import { FormEvent, useState } from "react";

import { authClient } from "../../lib/auth-client";
import { redirectWithAuthToken } from "../../lib/token";

type SignUpFormProps = {
  dashboardCallbackUrl: string;
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Unable to create account";
}

export function SignUpForm({ dashboardCallbackUrl }: SignUpFormProps) {
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
        throw new Error(addPasskey.error.message || "Passkey registration failed");
      }

      const signIn = await authClient.signIn.passkey();
      if (signIn.error) {
        throw new Error(signIn.error.message || "Passkey sign-in failed");
      }

      await redirectWithAuthToken(dashboardCallbackUrl);
    } catch (caught) {
      setError(errorMessage(caught));
      setPending(false);
    }
  }

  return (
    <form className="auth-form" onSubmit={signUp}>
      <label className="auth-field">
        <span>Email</span>
        <input
          autoComplete="email webauthn"
          inputMode="email"
          name="email"
          required
          type="email"
        />
      </label>
      <label className="auth-field">
        <span>Name</span>
        <input autoComplete="name" name="name" required type="text" />
      </label>
      {error ? <div className="auth-error">{error}</div> : null}
      <button className="auth-button" disabled={pending} type="submit">
        {pending ? "Creating account..." : "Create account with passkey"}
      </button>
    </form>
  );
}
