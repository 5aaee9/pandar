export type AuthCookieConfig = {
  name: string;
  maxAgeSeconds: number;
  secure: boolean;
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
    secure: process.env.APP_BASE_URL?.startsWith("https://") ?? false,
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

function maxAgeSeconds() {
  const parsed = Number.parseInt(
    process.env.APP_AUTH_COOKIE_MAX_AGE_SECONDS ?? "",
    10,
  );
  return Number.isFinite(parsed) && parsed > 0 ? parsed : defaultMaxAgeSeconds;
}
