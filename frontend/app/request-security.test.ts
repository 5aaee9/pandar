import { afterEach, describe, expect, it, vi } from "vitest";

import {
  rejectCrossOriginMutation,
  rejectTrustedAuthSignOutMutation,
} from "./request-security";

describe("mutation request origin validation", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it("allows only the configured Auth origin for cross-site sign-out", () => {
    vi.stubEnv("APP_BASE_URL", "https://dashboard.example.test");
    vi.stubEnv("APP_AUTH_BETTER_AUTH_BASE_URL", "https://auth.example.test");
    const trusted = new Request(
      "https://dashboard.example.test/auth/betterauth/session",
      {
        method: "POST",
        headers: {
          origin: "https://auth.example.test",
          "sec-fetch-site": "same-site",
        },
      },
    );
    const attacker = new Request(
      "https://dashboard.example.test/auth/betterauth/session",
      {
        method: "POST",
        headers: { origin: "https://attacker.example" },
      },
    );

    expect(rejectTrustedAuthSignOutMutation(trusted)).toBeNull();
    expect(rejectTrustedAuthSignOutMutation(attacker)?.status).toBe(403);
  });

  it("accepts same-origin browser mutations and rejects missing or cross-site origins", () => {
    vi.stubEnv("APP_BASE_URL", "https://dashboard.example.test");
    const sameOrigin = new Request("https://dashboard.example.test/api/jobs", {
      method: "DELETE",
      headers: {
        origin: "https://dashboard.example.test",
        "sec-fetch-site": "same-origin",
      },
    });
    const crossSite = new Request("https://dashboard.example.test/api/jobs", {
      method: "DELETE",
      headers: {
        origin: "https://attacker.example",
        "sec-fetch-site": "cross-site",
      },
    });
    const missingOrigin = new Request(
      "https://dashboard.example.test/api/jobs",
      { method: "DELETE" },
    );

    expect(rejectCrossOriginMutation(sameOrigin)).toBeNull();
    expect(rejectCrossOriginMutation(crossSite)?.status).toBe(403);
    expect(rejectCrossOriginMutation(missingOrigin)?.status).toBe(403);
  });

  it("fails closed when the configured Dashboard URL is malformed", () => {
    vi.stubEnv("APP_BASE_URL", "not a valid URL");
    const request = new Request("https://dashboard.example.test/api/jobs", {
      method: "DELETE",
      headers: {
        origin: "https://dashboard.example.test",
        "sec-fetch-site": "same-origin",
      },
    });

    expect(rejectCrossOriginMutation(request)?.status).toBe(403);
  });
});
