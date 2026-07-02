export type BetterAuthCallbackRedirect =
  | { ok: true; token: string; target: string; status: 303 }
  | { ok: false; body: string; status: 400 };

export function betterAuthCallbackRedirect(
  requestUrl: string,
  isAllowedToken: (token: string) => boolean,
): BetterAuthCallbackRedirect {
  const token = new URL(requestUrl).searchParams.get("token")?.trim() ?? "";
  if (!token || !isAllowedToken(token)) {
    return { ok: false, body: "malformed token", status: 400 };
  }

  return { ok: true, token, target: "/", status: 303 };
}

export function dashboardCallbackRedirectUrl(
  target: string,
  requestUrl: string,
  appBaseUrl = process.env.APP_BASE_URL,
): URL {
  const publicBaseUrl = appBaseUrl?.trim();
  return new URL(target, publicBaseUrl || requestUrl);
}
