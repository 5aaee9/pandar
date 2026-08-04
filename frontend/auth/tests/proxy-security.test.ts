import { NextRequest } from "next/server";
import { afterEach, describe, expect, it } from "vitest";

import { proxy } from "../proxy";

describe("Auth security proxy", () => {
  afterEach(() => {
    delete process.env.PANDAR_AUTH_DASHBOARD_CALLBACK_URL;
  });

  it("nonces scripts and restricts forms to the configured Dashboard", () => {
    process.env.PANDAR_AUTH_DASHBOARD_CALLBACK_URL =
      "https://pandar.example/auth/betterauth/callback";
    const response = proxy(new NextRequest("https://auth.example/sign-in"));
    const policy = response.headers.get("content-security-policy");

    expect(policy).toMatch(
      /script-src 'self' 'nonce-[a-f0-9]{32}' 'strict-dynamic'/,
    );
    expect(policy).not.toContain("script-src 'self' 'unsafe-inline'");
    expect(policy).toContain("form-action 'self' https://pandar.example");
    expect(policy).toContain("frame-ancestors 'none'");
    expect(response.headers.get("x-frame-options")).toBe("DENY");
  });

  it("rejects a malformed Dashboard callback URL", () => {
    process.env.PANDAR_AUTH_DASHBOARD_CALLBACK_URL = "not a valid URL";

    expect(() => proxy(new NextRequest("https://auth.example/sign-in"))).toThrow(
      "PANDAR_AUTH_DASHBOARD_CALLBACK_URL must be a valid URL",
    );
  });
});
