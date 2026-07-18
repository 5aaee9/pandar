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

describe("tenant printer list proxy", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.unstubAllEnvs();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("forwards the authenticated request, status, body, signal, and no-store policy", async () => {
    const upstreamFetch = vi.fn(async () =>
      new Response(JSON.stringify({ printers: [{ id: "printer-1" }] }), {
        status: 503,
        headers: {
          "content-type": "application/json; charset=utf-8",
          location: "https://hub.internal.example/private",
          "x-internal-api-url": "https://hub.internal.example/base",
        },
      }),
    );
    vi.stubGlobal("fetch", upstreamFetch);
    const { GET, dynamic } = await loadRoute();
    const controller = new AbortController();
    const request = new Request(
      "https://web.example/api/tenants/tenant/printers",
      { signal: controller.signal },
    );

    const response = await GET(request, {
      params: Promise.resolve({ tenantId: "tenant-1" }),
    });

    expect(dynamic).toBe("force-dynamic");
    expect(apiHeadersMock).toHaveBeenCalledOnce();
    expect(upstreamFetch).toHaveBeenCalledWith(
      "https://hub.internal.example/base/api/v1/tenants/tenant-1/printers",
      {
        cache: "no-store",
        headers: { authorization: "Bearer server-secret" },
        signal: request.signal,
      },
    );
    expect(response.status).toBe(503);
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(response.headers.get("content-type")).toBe(
      "application/json; charset=utf-8",
    );
    const responseText = await response.text();
    expect(JSON.parse(responseText)).toEqual({
      printers: [{ id: "printer-1" }],
    });
    expect([...response.headers].join(" ")).not.toContain("server-secret");
    expect([...response.headers].join(" ")).not.toContain(
      "hub.internal.example",
    );
    expect(responseText).not.toContain("hub.internal.example");
  });

  it("propagates browser aborts to the upstream request", async () => {
    let upstreamSignal: AbortSignal | undefined;
    let markStarted!: () => void;
    const started = new Promise<void>((resolve) => {
      markStarted = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn((_input: RequestInfo | URL, init?: RequestInit) => {
        upstreamSignal = init?.signal ?? undefined;
        markStarted();
        return new Promise<Response>((_resolve, reject) => {
          upstreamSignal?.addEventListener("abort", () => {
            reject(new DOMException("aborted", "AbortError"));
          });
        });
      }),
    );
    const { GET } = await loadRoute();
    const controller = new AbortController();
    const request = new Request("https://web.example/proxy", {
      signal: controller.signal,
    });

    const pending = GET(request, {
      params: Promise.resolve({ tenantId: "tenant-1" }),
    });
    await started;
    controller.abort();

    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
    expect(upstreamSignal?.aborted).toBe(true);
  });
});
