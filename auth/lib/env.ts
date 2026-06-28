const DEFAULT_BASE_URL = "http://127.0.0.1:3001";
const DEFAULT_DASHBOARD_URL = "http://127.0.0.1:3000";
const DEFAULT_JWT_MAX_AGE_SECONDS = 43_200;
const DEVELOPMENT_SECRET = "pandar-auth-development-secret-change-before-production";

type Env = {
  databaseFile: string;
  baseURL: string;
  trustedOrigins: string[];
  dashboardCallbackUrl: string;
  dashboardSignOutUrl: string;
  jwtMaxAgeSeconds: number;
  secret: string;
};

function trimTrailingSlash(value: string): string {
  return value.replace(/\/+$/, "");
}

function readString(name: string, fallback: string): string {
  const value = process.env[name]?.trim();
  return value ? value : fallback;
}

function readSecret(): string {
  const value = process.env.BETTER_AUTH_SECRET?.trim();
  if (value) {
    return value;
  }

  const isBuild = process.env.npm_lifecycle_event === "build";
  if (process.env.NODE_ENV === "production" && !isBuild) {
    throw new Error("BETTER_AUTH_SECRET is required in production");
  }

  return DEVELOPMENT_SECRET;
}

function readTrustedOrigins(baseURL: string): string[] {
  const value = process.env.PANDAR_AUTH_TRUSTED_ORIGINS?.trim();
  if (!value) {
    return [DEFAULT_DASHBOARD_URL];
  }

  return value
    .split(",")
    .map((origin) => origin.trim())
    .filter((origin) => origin.length > 0)
    .map(trimTrailingSlash);
}

function readPositiveInteger(name: string, fallback: number): number {
  const value = process.env[name]?.trim();
  if (!value) {
    return fallback;
  }

  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }

  return parsed;
}

const baseURL = trimTrailingSlash(
  readString("PANDAR_AUTH_BASE_URL", DEFAULT_BASE_URL),
);

export const env: Env = {
  databaseFile: readString("PANDAR_AUTH_DATABASE_FILE", "./pandar-auth.db"),
  baseURL,
  trustedOrigins: readTrustedOrigins(baseURL),
  dashboardCallbackUrl: readString(
    "PANDAR_AUTH_DASHBOARD_CALLBACK_URL",
    `${DEFAULT_DASHBOARD_URL}/auth/betterauth/callback`,
  ),
  dashboardSignOutUrl: readString(
    "PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL",
    `${DEFAULT_DASHBOARD_URL}/auth/betterauth/sign-out`,
  ),
  jwtMaxAgeSeconds: readPositiveInteger(
    "PANDAR_AUTH_JWT_MAX_AGE_SECONDS",
    DEFAULT_JWT_MAX_AGE_SECONDS,
  ),
  secret: readSecret(),
};
