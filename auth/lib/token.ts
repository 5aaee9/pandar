"use client";

type TokenResponse = {
  token?: unknown;
};

export async function redirectWithAuthToken(
  dashboardCallbackUrl: string,
): Promise<void> {
  const response = await fetch("/api/auth/token", {
    credentials: "include",
  });

  if (!response.ok) {
    throw new Error("Unable to create dashboard token");
  }

  const body = (await response.json()) as TokenResponse;
  if (typeof body.token !== "string" || body.token.length === 0) {
    throw new Error("Dashboard token response was empty");
  }

  window.location.href = `${dashboardCallbackUrl}#token=${encodeURIComponent(
    body.token,
  )}`;
}
