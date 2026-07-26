export type AuthCookieConfig = {
  name: string;
  maxAgeSeconds: number;
  secure: boolean;
  issuer: string | null;
};

type AuthCookieOptions = {
  httpOnly: true;
  sameSite: "lax";
  path: "/";
  maxAge: number;
  secure: boolean;
};

const defaultCookieName = "pandar_auth_token";
const defaultMaxAgeSeconds = 43200;
const compactJwtPattern = /^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/;

export function readAuthCookieConfig(): AuthCookieConfig {
  return {
    name: process.env.APP_AUTH_COOKIE_NAME || defaultCookieName,
    maxAgeSeconds: maxAgeSeconds(),
    secure:
      process.env.NODE_ENV === "production" ||
      (process.env.APP_BASE_URL?.startsWith("https://") ?? false),
    issuer: issuer(),
  };
}

export function authCookieOptions(): AuthCookieOptions {
  const config = readAuthCookieConfig();
  return {
    httpOnly: true,
    sameSite: "lax",
    path: "/",
    maxAge: config.maxAgeSeconds,
    secure: config.secure,
  };
}

export function clearedAuthCookieOptions(): AuthCookieOptions {
  return {
    ...authCookieOptions(),
    maxAge: 0,
  };
}

export function isCompactJwt(value: string) {
  return compactJwtPattern.test(value);
}

export function isAllowedDashboardJwt(value: string) {
  if (!isCompactJwt(value)) {
    return false;
  }

  const expectedIssuer = issuer();
  if (!expectedIssuer) {
    return true;
  }

  const payload = decodeJwtPayload(value);
  if (!payload) {
    return false;
  }

  return (
    payload.iss === expectedIssuer &&
    (payload.aud === expectedIssuer ||
      (Array.isArray(payload.aud) && payload.aud.includes(expectedIssuer)))
  );
}

function maxAgeSeconds() {
  const parsed = Number.parseInt(
    process.env.APP_AUTH_COOKIE_MAX_AGE_SECONDS ?? "",
    10,
  );
  return Number.isFinite(parsed) && parsed > 0 ? parsed : defaultMaxAgeSeconds;
}

function issuer() {
  const value = process.env.APP_AUTH_BETTER_AUTH_BASE_URL?.trim();
  return value ? value.replace(/\/+$/, "") : null;
}

function decodeJwtPayload(
  value: string,
): { iss?: unknown; aud?: unknown } | null {
  const payload = value.split(".")[1];
  try {
    return JSON.parse(Buffer.from(payload, "base64url").toString("utf8")) as {
      iss?: unknown;
      aud?: unknown;
    };
  } catch {
    return null;
  }
}
