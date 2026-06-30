type AuthSource =
  | "request_cookie"
  | "app_auth_bearer_token"
  | "app_api_token"
  | "none";

type AuthProviderForRedirect = {
  provider: "clerk" | "logto" | "betterauth" | "none";
  signInUrl: string | null;
};

export function dashboardAuthRedirectTarget({
  source,
  provider,
  meStatus,
}: {
  source: AuthSource;
  provider: AuthProviderForRedirect;
  meStatus?: number;
}) {
  if (provider.provider === "none" || !provider.signInUrl) {
    return null;
  }
  if (source === "none") {
    return provider.signInUrl;
  }
  if (source === "request_cookie" && meStatus === 401) {
    return provider.signInUrl;
  }
  return null;
}
