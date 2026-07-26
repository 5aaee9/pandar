import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const apiHeadersMock = vi.hoisted(() =>
  vi.fn(async () => ({ authorization: "Bearer server-secret" })),
);

vi.mock("../../../../../../api-auth", () => ({
  apiHeaders: apiHeadersMock,
}));

async function loadRoute() {
  vi.resetModules();
  vi.stubEnv("APP_API_URL", "https://hub.internal.example/base");
  return import("./route");
}

describe("tenant job reprint proxy", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.unstubAllEnvs();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("forwards the editable print options as authenticated JSON", async () => {
    let forwardedBody: unknown;
    const upstreamFetch = vi.fn(async (_url: string, init?: RequestInit) => {
      forwardedBody = await new Response(init?.body).json();
      return Response.json({ id: "reprint-1" }, { status: 201 });
    });
    vi.stubGlobal("fetch", upstreamFetch);
    const { POST, dynamic } = await loadRoute();
    const body = {
      printer_id: "printer-2",
      plate_id: 2,
      use_ams: true,
      ams_mapping: [4, 0],
    };
    const response = await POST(
      new Request("https://web.example/proxy", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          origin: "https://web.example",
          "sec-fetch-site": "same-origin",
        },
        body: JSON.stringify(body),
      }),
      { params: Promise.resolve({ tenantId: "tenant-1", jobId: "job-1" }) },
    );

    expect(dynamic).toBe("force-dynamic");
    expect(apiHeadersMock).toHaveBeenCalledOnce();
    expect(upstreamFetch).toHaveBeenCalledWith(
      "https://hub.internal.example/base/api/v1/tenants/tenant-1/jobs/job-1/reprint",
      expect.objectContaining({
        method: "POST",
        cache: "no-store",
        headers: expect.any(Headers),
        duplex: "half",
      }),
    );
    const headers = upstreamFetch.mock.calls[0]?.[1]?.headers as Headers;
    expect(headers.get("authorization")).toBe("Bearer server-secret");
    expect(headers.get("content-type")).toBe("application/json");
    expect(forwardedBody).toEqual(body);
    expect(response.status).toBe(201);
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(await response.json()).toEqual({ id: "reprint-1" });
  });

  it("rejects invalid path identifiers before contacting the Hub", async () => {
    const upstreamFetch = vi.fn();
    vi.stubGlobal("fetch", upstreamFetch);
    const { POST } = await loadRoute();

    const response = await POST(
      new Request("https://web.example/proxy", {
        method: "POST",
        headers: {
          origin: "https://web.example",
          "sec-fetch-site": "same-origin",
        },
        body: "{}",
      }),
      { params: Promise.resolve({ tenantId: "../tenant", jobId: "job-1" }) },
    );

    expect(response.status).toBe(400);
    expect(upstreamFetch).not.toHaveBeenCalled();
  });
});
