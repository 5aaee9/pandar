"use client";

import { useEffect, useState } from "react";

import { authClient } from "../../lib/auth-client";

type SignOutClientProps = {
  dashboardSignOutUrl: string;
};

export function SignOutClient({ dashboardSignOutUrl }: SignOutClientProps) {
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function signOut() {
      try {
        await authClient.signOut();
      } catch (caught) {
        if (!cancelled) {
          setError(caught instanceof Error ? caught.message : "Unable to sign out");
        }
      } finally {
        if (!cancelled) {
          window.location.href = dashboardSignOutUrl;
        }
      }
    }

    void signOut();

    return () => {
      cancelled = true;
    };
  }, [dashboardSignOutUrl]);

  return error ? <div className="auth-error">{error}</div> : null;
}
