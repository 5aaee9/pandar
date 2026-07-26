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

  const callbackUrl = new URL(dashboardCallbackUrl);
  const state = callbackUrl.searchParams.get("state");
  if (!state) {
    throw new Error(messages.dashboardTokenFailed);
  }
  const form = document.createElement("form");
  form.method = "POST";
  form.action = callbackUrl.toString();
  for (const [name, value] of [
    ["token", body.token],
    ["state", state],
  ]) {
    const input = document.createElement("input");
    input.type = "hidden";
    input.name = name;
    input.value = value;
    form.append(input);
  }
  document.body.append(form);
  form.submit();
}
