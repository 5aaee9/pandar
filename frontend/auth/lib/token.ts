"use client";

type TokenResponse = {
  token?: unknown;
};

export async function redirectWithAuthToken(
  dashboardCallbackUrl: string,
  messages: {
    dashboardTokenEmpty: string;
    dashboardTokenFailed: string;
  },
): Promise<void> {
  const response = await fetch("/api/auth/token", {
    credentials: "include",
  });

  if (!response.ok) {
    throw new Error(messages.dashboardTokenFailed);
  }

  const body = (await response.json()) as TokenResponse;
  if (typeof body.token !== "string" || body.token.length === 0) {
    throw new Error(messages.dashboardTokenEmpty);
  }

  window.location.href = `${dashboardCallbackUrl}#token=${encodeURIComponent(
    body.token,
  )}`;
}
