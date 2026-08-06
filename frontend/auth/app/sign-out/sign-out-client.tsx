"use client";

import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";

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

  const clearDashboardSession = useCallback(() => {
    const form = document.createElement("form");
    form.method = "post";
    form.action = dashboardSignOutUrl;
    document.body.append(form);
    form.submit();
  }, [dashboardSignOutUrl]);

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
            void clearDashboardSession();
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
  }, [clearDashboardSession, messages.unableSignOut]);

  if (state === "failed") {
    return (
      <div className="auth-error auth-feedback-enter" role="alert">
        <span>{messages.signOutWarning}</span>
        {error}
        <div className="auth-status-actions">
          <Button
            variant="outline"
            onClick={() => window.location.reload()}
            type="button"
          >
            {messages.retrySignOut}
          </Button>
          <form action={dashboardSignOutUrl} method="post">
            <Button variant="outline" type="submit">
              {messages.returnToDashboard}
            </Button>
          </form>
        </div>
      </div>
    );
  }

  return (
    <output className="auth-status" aria-live="polite">
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
    </output>
  );
}
