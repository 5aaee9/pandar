const DEFAULT_BASE_URL = "http://127.0.0.1:3001";
const DEFAULT_DASHBOARD_URL = "http://127.0.0.1:3000";
const DEFAULT_JWT_MAX_AGE_SECONDS = 43_200;
const DEFAULT_MAGIC_LINK_TTL_SECONDS = 1_800;
const DEVELOPMENT_SECRET =
  "pandar-auth-development-secret-change-before-production";

type EmailProvider = "resend" | "smtp";
type SmtpTlsMode = "starttls" | "tls" | "none";

type ResendEmailConfig = {
  provider: "resend";
  from: string;
  brandName: string;
  apiKey: string;
};

type SmtpEmailConfig = {
  provider: "smtp";
  from: string;
  brandName: string;
  host: string;
  port: number;
  username: string;
  password: string;
  tls: SmtpTlsMode;
};

type Env = {
  databaseFile: string;
  baseURL: string;
  trustedOrigins: string[];
  dashboardCallbackUrl: string;
  dashboardSignOutUrl: string;
  jwtMaxAgeSeconds: number;
  magicLinkTtlSeconds: number;
  secret: string;
  email: ResendEmailConfig | SmtpEmailConfig;
};

function trimTrailingSlash(value: string): string {
  return value.replace(/\/+$/, "");
}

function readString(name: string, fallback: string): string {
  const value = process.env[name]?.trim();
  return value ? value : fallback;
}

function isBuild(): boolean {
  return process.env.npm_lifecycle_event === "build";
}

function readSecret(): string {
  const value = process.env.BETTER_AUTH_SECRET?.trim();
  if (value) {
    return value;
  }

  if (process.env.NODE_ENV === "production" && !isBuild()) {
    throw new Error("BETTER_AUTH_SECRET is required in production");
  }

  return DEVELOPMENT_SECRET;
}

function readTrustedOrigins(baseURL: string): string[] {
  const value = process.env.PANDAR_AUTH_TRUSTED_ORIGINS?.trim();
  if (!value) {
    return [baseURL, DEFAULT_DASHBOARD_URL];
  }

  return [
    ...new Set([
      baseURL,
      ...value
        .split(",")
        .map((origin) => origin.trim())
        .filter((origin) => origin.length > 0)
        .map(trimTrailingSlash),
    ]),
  ];
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

function readRequiredRuntimeString(name: string): string {
  const value = process.env[name]?.trim();
  if (value) {
    return value;
  }

  if (isBuild()) {
    return `build-${name.toLowerCase()}`;
  }

  throw new Error(`${name} is required`);
}

function readRequiredRuntimePositiveInteger(name: string): number {
  const value = process.env[name]?.trim();
  if (!value && isBuild()) {
    return 1;
  }

  if (!value) {
    throw new Error(`${name} is required`);
  }

  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }

  return parsed;
}

function readEmailProvider(): EmailProvider | undefined {
  const provider = process.env.PANDAR_AUTH_EMAIL_PROVIDER?.trim();
  if (!provider) {
    return undefined;
  }

  if (provider === "resend" || provider === "smtp") {
    return provider;
  }

  throw new Error("PANDAR_AUTH_EMAIL_PROVIDER must be resend or smtp");
}

function readSmtpTlsMode(): SmtpTlsMode {
  const tls = readString("PANDAR_AUTH_SMTP_TLS", "starttls");
  if (tls === "starttls" || tls === "tls" || tls === "none") {
    return tls;
  }

  throw new Error("PANDAR_AUTH_SMTP_TLS must be starttls, tls, or none");
}

function readEmailConfig(): Env["email"] {
  const provider = readEmailProvider();
  const brandName = readString("PANDAR_AUTH_EMAIL_BRAND_NAME", "Pandar");

  if (!provider && isBuild()) {
    return {
      provider: "resend",
      from: "Pandar <build@example.invalid>",
      brandName,
      apiKey: "build-resend-api-key",
    };
  }

  if (provider === "resend") {
    return {
      provider,
      from: readRequiredRuntimeString("PANDAR_AUTH_EMAIL_FROM"),
      brandName,
      apiKey: readRequiredRuntimeString("RESEND_API_KEY"),
    };
  }

  if (provider === "smtp") {
    return {
      provider,
      from: readRequiredRuntimeString("PANDAR_AUTH_EMAIL_FROM"),
      brandName,
      host: readRequiredRuntimeString("PANDAR_AUTH_SMTP_HOST"),
      port: readRequiredRuntimePositiveInteger("PANDAR_AUTH_SMTP_PORT"),
      username: readRequiredRuntimeString("PANDAR_AUTH_SMTP_USERNAME"),
      password: readRequiredRuntimeString("PANDAR_AUTH_SMTP_PASSWORD"),
      tls: readSmtpTlsMode(),
    };
  }

  throw new Error("PANDAR_AUTH_EMAIL_PROVIDER must be resend or smtp");
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
  magicLinkTtlSeconds: readPositiveInteger(
    "PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS",
    DEFAULT_MAGIC_LINK_TTL_SECONDS,
  ),
  secret: readSecret(),
  email: readEmailConfig(),
};
