"use client";

import { useState } from "react";

import { authClient } from "../../../lib/auth-client";
import type { CompleteAuthMessages } from "../../../lib/i18n";
import { redirectWithAuthToken } from "../../../lib/token";

type CompleteAuthProps = {
  dashboardCallbackUrl: string;
  messages: CompleteAuthMessages;
};

export function CompleteAuth({
  dashboardCallbackUrl,
  messages,
}: CompleteAuthProps) {
  const [pending, setPending] = useState<"passkey" | "redirect" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [added, setAdded] = useState(false);

  async function continueToDashboard() {
    setPending("redirect");
    setError(null);

    try {
      await redirectWithAuthToken(dashboardCallbackUrl, messages);
    } catch (caught) {
      setError(
        caught instanceof Error ? caught.message : messages.dashboardTokenFailed,
      );
      setPending(null);
    }
  }

  async function addPasskey() {
    setPending("passkey");
    setError(null);

    try {
      const result = await authClient.passkey.addPasskey();
      if (result.error) {
        throw new Error(result.error.message || messages.passkeyAddFailed);
      }

      setAdded(true);
      await continueToDashboard();
    } catch {
      setError(messages.passkeyAddFailed);
      setPending(null);
    }
  }

  return (
    <div className="auth-form">
      {added ? (
        <output className="auth-feedback-enter auth-status">
          <strong>{messages.passkeyAdded}</strong>
          <span>{messages.returningDashboard}</span>
        </output>
      ) : null}
      {error ? (
        <div className="auth-error auth-feedback-enter" role="alert">
          <span>{messages.passkeyAddFailed}</span>
          {error}
        </div>
      ) : null}
      <div className="auth-actions">
        <button
          className="auth-button"
          disabled={pending !== null}
          type="button"
          onClick={addPasskey}
        >
          {pending === "passkey" ? messages.addingPasskey : messages.addPasskey}
        </button>
        <button
          className="auth-secondary-button"
          disabled={pending === "redirect"}
          type="button"
          onClick={continueToDashboard}
        >
          {added ? messages.continueDashboard : messages.skipPasskey}
        </button>
      </div>
    </div>
  );
}
