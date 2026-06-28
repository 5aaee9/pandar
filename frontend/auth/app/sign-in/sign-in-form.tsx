"use client";

import { FormEvent, useEffect, useState } from "react";

import { authClient } from "../../lib/auth-client";
import type { SignInMessages } from "../../lib/i18n";

type SignInFormProps = {
  messages: SignInMessages;
};

const RESEND_COOLDOWN_SECONDS = 60;

function formatCooldown(template: string, seconds: number): string {
  return template.replace("{seconds}", String(seconds));
}

export function SignInForm({ messages }: SignInFormProps) {
  const [pending, setPending] = useState(false);
  const [sent, setSent] = useState(false);
  const [email, setEmail] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [cooldown, setCooldown] = useState(0);

  useEffect(() => {
    if (cooldown <= 0) {
      return;
    }

    const timer = window.setTimeout(() => {
      setCooldown((current) => Math.max(0, current - 1));
    }, 1000);

    return () => window.clearTimeout(timer);
  }, [cooldown]);

  async function sendMagicLink(event?: FormEvent<HTMLFormElement>) {
    event?.preventDefault();
    if (pending || (sent && cooldown > 0)) {
      return;
    }

    const normalizedEmail = email.trim().toLowerCase();
    const name = normalizedEmail.split("@", 1)[0] || normalizedEmail;
    setPending(true);
    setError(null);

    try {
      const result = await authClient.signIn.magicLink({
        email: normalizedEmail,
        name,
        callbackURL: "/auth/complete",
        newUserCallbackURL: "/auth/complete",
        errorCallbackURL: "/sign-in",
      });
      if (result.error) {
        throw new Error(result.error.message || messages.magicLinkSendFailed);
      }

      setSent(true);
      setCooldown(RESEND_COOLDOWN_SECONDS);
    } catch {
      setError(messages.unableSignIn);
    } finally {
      setPending(false);
    }
  }

  return (
    <form className="auth-form" onSubmit={sendMagicLink}>
      <label className="auth-field">
        <span>{messages.email}</span>
        <input
          autoComplete="email"
          inputMode="email"
          name="email"
          required
          type="email"
          value={email}
          onChange={(event) => setEmail(event.currentTarget.value)}
        />
      </label>
      {error ? (
        <div className="auth-error" role="alert">
          <span>{messages.magicLinkSendFailed}</span>
          {error}
        </div>
      ) : null}
      {sent ? (
        <div className="auth-status" role="status">
          <strong>{messages.magicLinkEmailSent}</strong>
          <span>{messages.magicLinkSentBody}</span>
          <span>{messages.magicLinkCheckInbox}</span>
        </div>
      ) : null}
      <button
        className="auth-button"
        disabled={pending || (sent && cooldown > 0)}
        type="submit"
      >
        {pending
          ? messages.magicLinkSending
          : sent && cooldown > 0
            ? formatCooldown(messages.magicLinkResendCooldown, cooldown)
            : sent
              ? messages.magicLinkResend
              : messages.magicLinkSubmit}
      </button>
    </form>
  );
}
