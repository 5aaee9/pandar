import { afterEach, describe, expect, it, vi } from "vitest";

import { validateAuthConfiguration } from "./auth-provider";

describe("external authentication transport", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it("rejects cleartext provider endpoints in production", () => {
    vi.stubEnv("NODE_ENV", "production");
    vi.stubEnv("APP_BASE_URL", "https://dashboard.example.test");
    vi.stubEnv("APP_AUTH_PROVIDER", "betterauth");
    vi.stubEnv("APP_AUTH_BETTER_AUTH_BASE_URL", "http://auth.example.test");
    expect(() => validateAuthConfiguration()).toThrow(
      /APP_AUTH_BETTER_AUTH_BASE_URL must use https/,
    );

    vi.stubEnv("APP_AUTH_PROVIDER", "logto");
    vi.stubEnv("APP_AUTH_LOGTO_ENDPOINT", "http://logto.example.test");
    expect(() => validateAuthConfiguration()).toThrow(
      /APP_AUTH_LOGTO_ENDPOINT must use https/,
    );
  });

  it("requires an HTTPS dashboard origin in production", () => {
    vi.stubEnv("NODE_ENV", "production");
    vi.stubEnv("APP_AUTH_PROVIDER", "clerk");
    vi.stubEnv("APP_BASE_URL", "http://dashboard.example.test");

    expect(() => validateAuthConfiguration()).toThrow(/must use https/);

    vi.stubEnv("APP_BASE_URL", "https://dashboard.example.test");
    expect(validateAuthConfiguration()).toBe("clerk");
  });
});
