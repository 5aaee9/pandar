"use client";

import { useEffect, useState } from "react";

import { authClient } from "../../lib/auth-client";
import type { SignOutMessages } from "../../lib/i18n";

type SignOutClientProps = {
  dashboardSignOutUrl: string;
  messages: SignOutMessages;
};

type SignOutState = "signing-out" | "returning" | "failed";

export function SignOutClient({
  dashboardSignOutUrl,
  messages,
}: SignOutClientProps) {
  const [state, setState] = useState<SignOutState>("signing-out");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let redirectTimer: number | undefined;

    async function signOut() {
      try {
        const result = await authClient.signOut();
        if (result.error) {
          throw new Error(result.error.message || messages.unableSignOut);
        }
        if (!cancelled) {
          setState("returning");
          redirectTimer = window.setTimeout(() => {
            window.location.assign(dashboardSignOutUrl);
          }, 1200);
        }
      } catch (caught) {
        if (!cancelled) {
          setError(
            caught instanceof Error ? caught.message : messages.unableSignOut,
          );
          setState("failed");
        }
      }
    }

    void signOut();

    return () => {
      cancelled = true;
      if (redirectTimer !== undefined) {
        window.clearTimeout(redirectTimer);
      }
    };
  }, [dashboardSignOutUrl]);

  if (state === "failed") {
    return (
      <div className="auth-error" role="alert">
        <span>{messages.signOutWarning}</span>
        {error}
        <div className="auth-status-actions">
          <button
            className="auth-secondary-button"
            onClick={() => window.location.reload()}
            type="button"
          >
            {messages.retrySignOut}
          </button>
          <a className="auth-secondary-button" href={dashboardSignOutUrl}>
            {messages.returnToDashboard}
          </a>
        </div>
      </div>
    );
  }

  return (
    <div className="auth-status" role="status" aria-live="polite">
      <div className="auth-status-row">
        <span className="auth-spinner" aria-hidden="true" />
        <strong>
          {state === "returning"
            ? messages.signedOut
            : messages.clearingIssuerSession}
        </strong>
      </div>
      <span>
        {state === "returning"
          ? messages.returningDashboard
          : messages.signingOutIntro}
      </span>
    </div>
  );
}
