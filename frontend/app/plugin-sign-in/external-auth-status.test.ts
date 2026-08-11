import { afterEach, describe, expect, it, vi } from "vitest";

import { fetchExternalAuthStatus } from "./external-auth-status";

describe("plugin sign-in external auth readiness", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reads external auth status from the public Hub API", async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = String(input);
      if (url.endsWith("/readyz")) {
        return Response.json({}, { status: 404 });
      }
      return Response.json({ external_auth: { enabled: true, ready: true } });
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(fetchExternalAuthStatus()).resolves.toEqual({
      externalAuthEnabled: true,
      error: null,
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/auth/status",
      { cache: "no-store" },
    );
  });
});
