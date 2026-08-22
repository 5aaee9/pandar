import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const apiHeadersMock = vi.hoisted(() =>
  vi.fn(async () => ({ authorization: "Bearer server-secret" })),
);

vi.mock("./api-auth", () => ({
  apiHeaders: apiHeadersMock,
}));

const hubBase = "https://hub.internal.example/base";

async function loadHubProxy() {
  vi.resetModules();
  vi.stubEnv("APP_API_URL", hubBase);
  return import("./hub-proxy");
}

function mutationRequest(url: string, init?: RequestInit) {
  const headers = new Headers(init?.headers);
  headers.set("origin", "https://web.example");
  headers.set("sec-fetch-site", "same-origin");
  return new Request(url, { ...init, headers });
}

type UpstreamInit = RequestInit & { duplex?: "half" };

type FetchMock = ReturnType<
  typeof vi.fn<
    (input: RequestInfo | URL, init?: UpstreamInit) => Promise<Response>
  >
>;

function stubUpstream(response: Response): FetchMock {
  const fetchMock = vi.fn<
    (input: RequestInfo | URL, init?: UpstreamInit) => Promise<Response>
  >(async () => response);
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

function firstCall(fetchMock: FetchMock): [RequestInfo | URL, UpstreamInit] {
  const call = fetchMock.mock.calls[0];
  if (!call) throw new Error("expected an upstream fetch call");
  return [call[0], call[1] ?? {}];
}

function jsonUpstream(init?: { status?: number; headers?: HeadersInit }) {
  return stubUpstream(
    new Response("{}", {
      status: init?.status ?? 200,
      headers: { "content-type": "application/json", ...init?.headers },
    }),
  );
}

describe("hubProxy", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.unstubAllEnvs();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("rejects cross-origin mutations without calling upstream", async () => {
    const { hubProxy } = await loadHubProxy();
    const fetchMock = stubUpstream(new Response());
    const handler = hubProxy({ method: "DELETE", path: "/jobs" });

    const response = await handler(
      new Request("https://web.example/api/tenants/tenant-1/jobs", {
        headers: {
          origin: "https://evil.example",
          "sec-fetch-site": "cross-site",
        },
      }),
      { params: Promise.resolve({ tenantId: "tenant-1" }) },
    );

    expect(response.status).toBe(403);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("rejects invalid tenant and path ids without calling upstream", async () => {
    const { hubProxy } = await loadHubProxy();
    const fetchMock = stubUpstream(new Response());
    const handler = hubProxy({ method: "DELETE", path: "/jobs/{jobId}" });

    const badTenant = await handler(
      mutationRequest("https://web.example/api/tenants/bad tenant/jobs/job-1"),
      { params: Promise.resolve({ tenantId: "bad tenant", jobId: "job-1" }) },
    );
    const badJob = await handler(
      mutationRequest(
        "https://web.example/api/tenants/tenant-1/jobs/bad%20job",
      ),
      { params: Promise.resolve({ tenantId: "tenant-1", jobId: "bad job" }) },
    );

    expect(badTenant.status).toBe(400);
    expect(await badTenant.json()).toEqual({ error: "invalid_id" });
    expect(badJob.status).toBe(400);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("forwards mutations with auth headers, resolved path, and request signal", async () => {
    const { hubProxy } = await loadHubProxy();
    const fetchMock = jsonUpstream();
    const handler = hubProxy({ method: "DELETE", path: "/jobs/{jobId}" });
    const request = mutationRequest(
      "https://web.example/api/tenants/tenant-1/jobs/job-9",
    );

    const response = await handler(request, {
      params: Promise.resolve({ tenantId: "tenant-1", jobId: "job-9" }),
    });

    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, init] = firstCall(fetchMock);
    expect(url).toBe(`${hubBase}/api/v1/tenants/tenant-1/jobs/job-9`);
    expect(init.method).toBe("DELETE");
    expect(init.cache).toBe("no-store");
    expect(init.signal).toBe(request.signal);
    expect(new Headers(init.headers).get("authorization")).toBe(
      "Bearer server-secret",
    );
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(response.headers.get("content-type")).toBe("application/json");
  });

  it("applies no cross-origin check to GET", async () => {
    const { hubProxy } = await loadHubProxy();
    const fetchMock = jsonUpstream();
    const handler = hubProxy({ method: "GET", path: "/printers" });

    const response = await handler(
      new Request("https://web.example/api/tenants/tenant-1/printers"),
      { params: Promise.resolve({ tenantId: "tenant-1" }) },
    );

    expect(fetchMock).toHaveBeenCalledOnce();
    expect(response.status).toBe(200);
  });

  it("appends the declared query string to the upstream URL", async () => {
    const { hubProxy } = await loadHubProxy();
    const fetchMock = jsonUpstream();
    const handler = hubProxy({
      method: "GET",
      path: "/audit-events",
      query: "limit=20",
    });

    await handler(
      new Request("https://web.example/api/tenants/tenant-1/audit-events"),
      { params: Promise.resolve({ tenantId: "tenant-1" }) },
    );

    const [url] = firstCall(fetchMock);
    expect(url).toBe(
      `${hubBase}/api/v1/tenants/tenant-1/audit-events?limit=20`,
    );
  });

  it("streams request bodies with forced JSON content-type", async () => {
    const { hubProxy } = await loadHubProxy();
    const fetchMock = jsonUpstream();
    const handler = hubProxy({
      method: "POST",
      path: "/jobs/{jobId}/reprint",
      body: "stream",
      contentType: "json",
    });
    const request = mutationRequest(
      "https://web.example/api/tenants/tenant-1/jobs/job-9/reprint",
      {
        method: "POST",
        body: "{}",
        headers: { "content-type": "text/plain" },
      },
    );

    await handler(request, {
      params: Promise.resolve({ tenantId: "tenant-1", jobId: "job-9" }),
    });

    const [, init] = firstCall(fetchMock);
    expect(init.body).toBe(request.body);
    expect(init.duplex).toBe("half");
    expect(new Headers(init.headers).get("content-type")).toBe(
      "application/json",
    );
  });

  it("streams request bodies with the request content-type forwarded", async () => {
    const { hubProxy } = await loadHubProxy();
    const fetchMock = jsonUpstream();
    const handler = hubProxy({
      method: "POST",
      path: "/printers/{printerId}/jobs",
      body: "stream",
      contentType: "forward",
    });
    const request = mutationRequest(
      "https://web.example/api/tenants/tenant-1/printers/printer-1/jobs",
      {
        method: "POST",
        body: "gcode",
        headers: { "content-type": "text/csv" },
      },
    );

    await handler(request, {
      params: Promise.resolve({ tenantId: "tenant-1", printerId: "printer-1" }),
    });

    const [, init] = firstCall(fetchMock);
    expect(new Headers(init.headers).get("content-type")).toBe("text/csv");
  });

  it("omits content-type for streamed bodies when the request has none", async () => {
    const { hubProxy } = await loadHubProxy();
    const fetchMock = jsonUpstream();
    const handler = hubProxy({
      method: "POST",
      path: "/artifact-metadata-preview",
      body: "stream",
      contentType: "forward",
    });
    const request = mutationRequest(
      "https://web.example/api/tenants/tenant-1/artifact-metadata-preview",
      { method: "POST", body: "bytes" },
    );
    request.headers.delete("content-type");

    await handler(request, {
      params: Promise.resolve({ tenantId: "tenant-1" }),
    });

    const [, init] = firstCall(fetchMock);
    expect(new Headers(init.headers).get("content-type")).toBeNull();
  });

  it("passes status and statusText through and strips unsafe upstream headers", async () => {
    const { hubProxy } = await loadHubProxy();
    stubUpstream(
      new Response("payload", {
        status: 200,
        statusText: "OK-custom",
        headers: {
          "content-type": "application/json",
          location: "https://hub.internal.example/private",
        },
      }),
    );
    const handler = hubProxy({ method: "GET", path: "/printers" });

    const response = await handler(
      new Request("https://web.example/api/tenants/tenant-1/printers"),
      { params: Promise.resolve({ tenantId: "tenant-1" }) },
    );

    expect(response.status).toBe(200);
    expect(response.statusText).toBe("OK-custom");
    expect([...response.headers].map(([name]) => name).sort()).toEqual([
      "cache-control",
      "content-type",
    ]);
    expect(await response.text()).toBe("payload");
  });

  it("applies the configured content-type fallback when the upstream omits it", async () => {
    const { hubProxy } = await loadHubProxy();
    stubUpstream(new Response(null, { status: 200 }));
    const handler = hubProxy({
      method: "GET",
      path: "/printers/{printerId}/camera.mp4",
      contentTypeFallback: "video/mp4",
    });

    const response = await handler(
      new Request(
        "https://web.example/api/tenants/tenant-1/printers/p1/camera.mp4",
      ),
      { params: Promise.resolve({ tenantId: "tenant-1", printerId: "p1" }) },
    );

    expect(response.headers.get("content-type")).toBe("video/mp4");
  });

  it("omits response content-type when the upstream omits it and no fallback is configured", async () => {
    const { hubProxy } = await loadHubProxy();
    stubUpstream(new Response(null, { status: 204 }));
    const handler = hubProxy({ method: "DELETE", path: "/jobs" });

    const response = await handler(
      mutationRequest("https://web.example/api/tenants/tenant-1/jobs"),
      { params: Promise.resolve({ tenantId: "tenant-1" }) },
    );

    expect(response.status).toBe(204);
    expect(response.headers.get("content-type")).toBeNull();
    expect(response.headers.get("cache-control")).toBe("no-store");
  });
});
