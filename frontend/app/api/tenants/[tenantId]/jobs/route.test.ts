import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const apiHeadersMock = vi.hoisted(() =>
  vi.fn(async () => ({ authorization: "Bearer server-secret" })),
);

vi.mock("../../../../api-auth", () => ({
  apiHeaders: apiHeadersMock,
}));

async function loadRoute() {
  vi.resetModules();
  vi.stubEnv("APP_API_URL", "https://hub.internal.example/base");
  return import("./route");
}

describe("tenant jobs clear proxy", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.unstubAllEnvs();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("forwards authenticated DELETE and returns only safe response headers", async () => {
    const upstreamFetch = vi.fn(
      async () =>
        new Response(JSON.stringify({ deleted: 2 }), {
          status: 200,
          headers: {
            "content-type": "application/json; charset=utf-8",
            location: "https://hub.internal.example/private",
          },
        }),
    );
    vi.stubGlobal("fetch", upstreamFetch);
    const { DELETE, dynamic } = await loadRoute();
    const request = new Request("https://web.example/api/tenants/tenant/jobs", {
      headers: {
        origin: "https://web.example",
        "sec-fetch-site": "same-origin",
      },
    });

    const response = await DELETE(request, {
      params: Promise.resolve({ tenantId: "tenant-1" }),
    });

    expect(dynamic).toBe("force-dynamic");
    expect(apiHeadersMock).toHaveBeenCalledOnce();
    expect(upstreamFetch).toHaveBeenCalledWith(
      "https://hub.internal.example/base/api/v1/tenants/tenant-1/jobs",
      {
        method: "DELETE",
        cache: "no-store",
        headers: { authorization: "Bearer server-secret" },
        signal: request.signal,
      },
    );
    expect(response.status).toBe(200);
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(response.headers.get("content-type")).toBe(
      "application/json; charset=utf-8",
    );
    expect(await response.json()).toEqual({ deleted: 2 });
    expect([...response.headers].join(" ")).not.toContain(
      "hub.internal.example",
    );
  });
});
