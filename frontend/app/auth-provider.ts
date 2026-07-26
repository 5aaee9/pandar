export type AuthProvider = "clerk" | "logto" | "betterauth" | "none";

export function authProviderConfig() {
  const provider = validateAuthConfiguration();
  const clerkPublishableKey =
    process.env.APP_AUTH_CLERK_PUBLISHABLE_KEY ?? null;
  const logtoEndpoint = process.env.APP_AUTH_LOGTO_ENDPOINT ?? null;
  const logtoAppId = process.env.APP_AUTH_LOGTO_APP_ID ?? null;
  const betterAuthBaseUrl = process.env.APP_AUTH_BETTER_AUTH_BASE_URL ?? null;
  return {
    provider,
    cookieName: process.env.APP_AUTH_COOKIE_NAME ?? "pandar_auth_token",
    clerkPublishableKey,
    logtoEndpoint,
    logtoAppId,
    betterAuthBaseUrl,
    signInUrl: signInUrl(provider, { logtoEndpoint, betterAuthBaseUrl }),
    signOutUrl: signOutUrl(provider, { logtoEndpoint, betterAuthBaseUrl }),
  };
}

export function validateAuthConfiguration(): AuthProvider {
  const provider = providerValue(process.env.APP_AUTH_PROVIDER);
  const hasApiToken = Boolean(process.env.APP_API_TOKEN);
  const hasStaticAuthToken = Boolean(process.env.APP_AUTH_BEARER_TOKEN);
  if (provider !== "none" && (hasApiToken || hasStaticAuthToken)) {
    throw new Error(
      "Static API tokens cannot be combined with external authentication",
    );
  }
  if (hasApiToken && hasStaticAuthToken) {
    throw new Error(
      "APP_API_TOKEN and APP_AUTH_BEARER_TOKEN are mutually exclusive",
    );
  }
  if (process.env.NODE_ENV === "production" && provider !== "none") {
    if (!process.env.APP_BASE_URL?.trim().startsWith("https://")) {
      throw new Error(
        "APP_BASE_URL must use https when external authentication is enabled in production",
      );
    }
    if (provider === "logto") {
      requireHttpsUrl(
        "APP_AUTH_LOGTO_ENDPOINT",
        process.env.APP_AUTH_LOGTO_ENDPOINT,
      );
    }
    if (provider === "betterauth") {
      requireHttpsUrl(
        "APP_AUTH_BETTER_AUTH_BASE_URL",
        process.env.APP_AUTH_BETTER_AUTH_BASE_URL,
      );
    }
  }
  return provider;
}

function requireHttpsUrl(name: string, value: string | undefined) {
  if (!value) {
    return;
  }
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error(`${name} must be a valid https URL in production`);
  }
  if (url.protocol !== "https:") {
    throw new Error(`${name} must use https in production`);
  }
}

function providerValue(value: string | undefined): AuthProvider {
  if (!value || value === "none") {
    return "none";
  }
  if (value === "clerk" || value === "logto" || value === "betterauth") {
    return value;
  }
  throw new Error(`Unsupported APP_AUTH_PROVIDER: ${value}`);
}

function signInUrl(
  provider: AuthProvider,
  config: { logtoEndpoint: string | null; betterAuthBaseUrl: string | null },
) {
  if (provider === "logto" && config.logtoEndpoint) {
    return `${config.logtoEndpoint.replace(/\/$/, "")}/sign-in`;
  }
  if (provider === "betterauth" && config.betterAuthBaseUrl) {
    return "/auth/betterauth/start";
  }
  return provider === "clerk" ? "/sign-in" : null;
}

function signOutUrl(
  provider: AuthProvider,
  config: { logtoEndpoint: string | null; betterAuthBaseUrl: string | null },
) {
  if (provider === "logto" && config.logtoEndpoint) {
    return `${config.logtoEndpoint.replace(/\/$/, "")}/sign-out`;
  }
  if (provider === "betterauth" && config.betterAuthBaseUrl) {
    return `${config.betterAuthBaseUrl.replace(/\/$/, "")}/sign-out`;
  }
  return provider === "clerk" ? "/sign-out" : null;
}
