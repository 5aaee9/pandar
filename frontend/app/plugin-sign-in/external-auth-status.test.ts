import { afterEach, describe, expect, it, vi } from "vitest";

import { fetchExternalAuthStatus } from "./external-auth-status";

describe("plugin sign-in external auth readiness", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reads the Hub readiness check instead of treating health as disabled auth", async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/healthz")) {
        return Response.json({ status: "ok" });
      }
      return Response.json({
        status: "ready",
        checks: {
          external_auth: { ready: true, detail: "configured" },
        },
      });
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(fetchExternalAuthStatus()).resolves.toEqual({
      externalAuthEnabled: true,
      error: null,
    });
    expect(fetchMock).toHaveBeenCalledWith("http://localhost:8080/readyz", {
      cache: "no-store",
    });
  });
});
