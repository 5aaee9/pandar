import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const apiHeadersMock = vi.hoisted(() =>
  vi.fn(async () => ({ authorization: "Bearer server-secret" })),
);

vi.mock("@/app/api-auth", () => ({
  apiHeaders: apiHeadersMock,
}));

async function loadRoute() {
  vi.resetModules();
  vi.stubEnv("APP_API_URL", "https://hub.internal.example/base");
  return import("./route");
}

describe("printer camera proxy", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.unstubAllEnvs();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("rejects path segments that could normalize into another endpoint", async () => {
    const upstreamFetch = vi.fn(
      async () => new Response(null, { status: 200 }),
    );
    vi.stubGlobal("fetch", upstreamFetch);
    const { GET } = await loadRoute();

    const response = await GET(
      new Request("https://web.example/camera") as never,
      {
        params: Promise.resolve({
          tenantId: "tenant-1",
          printerId: "../jobs/00000000-0000-0000-0000-000000000001",
        }),
      },
    );

    expect(response.status).toBe(400);
    expect(upstreamFetch).not.toHaveBeenCalled();
  });
});
