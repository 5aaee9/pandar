import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  createTenantToken,
  revokeTenantToken,
  rotateTenantToken,
} from "./admin-actions";
import { deleteAgent, refreshPrinters } from "./actions";
import {
  duplicateJob,
  retryDispatchJob,
  retryDispatchJobs,
} from "./job-actions";

const redirectMock = vi.hoisted(() =>
  vi.fn((url: string) => {
    throw new Error(`NEXT_REDIRECT:${url}`);
  }),
);
const refreshMock = vi.hoisted(() => vi.fn());

vi.mock("next/cache", () => ({
  refresh: refreshMock,
}));

vi.mock("next/navigation", () => ({
  redirect: redirectMock,
}));

vi.mock("./api-auth", () => ({
  requireAuth: vi.fn(async () => undefined),
  apiHeaders: vi.fn(async () => ({ "content-type": "application/json" })),
}));

describe("deleteAgent", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ id: "agent-1" }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  it("returns completion for client-side resource invalidation", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("agent_id", "agent-1");

    await expect(deleteAgent(formData)).resolves.toEqual({
      ok: true,
      redirectUrl: "/agents?status=agent_deleted",
    });
  });
});

describe("job action redirects", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ id: "command-1" }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  it.each([
    [
      "refreshPrinters",
      refreshPrinters,
      [["agent_id", "agent-1"]],
      "refresh_queued",
    ],
    [
      "retryDispatchJobs",
      retryDispatchJobs,
      [["job_id", "job-1"]],
      "retry_queued",
    ],
    ["duplicateJob", duplicateJob, [["job_id", "job-1"]], "duplicate_queued"],
  ] as const)(
    "redirects %s back to jobs when submitted from jobs",
    async (_name, action, fields, status) => {
      const formData = new FormData();
      formData.set("tenant_id", "tenant-1");
      formData.set("return_to", "jobs");
      for (const [name, value] of fields) {
        formData.append(name, value);
      }

      await expect(action(formData)).rejects.toThrow(
        `NEXT_REDIRECT:/jobs?status=${status}`,
      );
    },
  );

  it("redirects agent refresh back to Agents", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("agent_id", "agent-1");
    formData.set("return_to", "agents");

    await expect(refreshPrinters(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/agents?status=refresh_queued",
    );
  });

  it("returns the canonical retry completion URL for client invalidation", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("job_id", "job-1");

    await expect(retryDispatchJob(formData)).resolves.toEqual({
      ok: true,
      redirectUrl: "/devices?status=retry_queued",
    });

    formData.set("return_to", "jobs");
    await expect(retryDispatchJob(formData)).resolves.toEqual({
      ok: true,
      redirectUrl: "/jobs?status=retry_queued",
    });
  });
});

describe("revokeTenantToken", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ tenant_token: { id: "token-1" } }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  it("returns mutation completion after revoking a token", async () => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("token_id", "token-1");

    await expect(revokeTenantToken(null, formData)).resolves.toEqual({
      ok: true,
    });
    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/tenant-tokens/token-1",
      {
        method: "DELETE",
        headers: { "content-type": "application/json" },
      },
    );
  });

  it("returns the Hub error when token revocation fails", async () => {
    vi.mocked(fetch).mockResolvedValueOnce(
      new Response(JSON.stringify({ error: "token_not_found" }), {
        status: 404,
        headers: { "content-type": "application/json" },
      }),
    );
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("token_id", "missing-token");

    await expect(revokeTenantToken(null, formData)).resolves.toEqual({
      ok: false,
      error: "token_not_found",
    });
  });
});

describe("createTenantToken", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("posts normalized token fields, returns the one-time secret, and refreshes token management", async () => {
    const tenantToken = {
      id: "token-created",
      tenant_id: "tenant-1",
      name: "Studio automation",
      scopes: ["plugin:studio", "agent:register"],
      created_by_user_id: null,
      created_at: "2026-07-17T01:00:00Z",
      last_used_at: null,
      expires_at: "2026-12-31T00:00:00Z",
      revoked_at: null,
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              tenant_token: tenantToken,
              token: "pandar_tenant_created-secret",
            }),
            {
              status: 201,
              headers: { "content-type": "application/json" },
            },
          ),
      ),
    );
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("name", "Studio automation");
    formData.set("scopes", " plugin:studio, agent:register, ");
    formData.set("expires_at", "2026-12-31T00:00:00Z");

    await expect(createTenantToken(null, formData)).resolves.toEqual({
      ok: true,
      kind: "tenant_token",
      operation: "created",
      tenantToken,
      token: "pandar_tenant_created-secret",
    });
    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/tenant-tokens",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          name: "Studio automation",
          scopes: ["plugin:studio", "agent:register"],
          expires_at: "2026-12-31T00:00:00Z",
        }),
      },
    );
    expect(refreshMock).toHaveBeenCalledTimes(1);
  });

  it("returns the API error without refreshing token management", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ error: "invalid_scope" }), {
            status: 400,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("name", "Broken token");
    formData.set("scopes", "unknown:scope");

    await expect(createTenantToken(null, formData)).resolves.toEqual({
      ok: false,
      error: "invalid_scope",
    });
    expect(refreshMock).not.toHaveBeenCalled();
  });
});

describe("rotateTenantToken", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("posts the replacement expiration, returns the one-time secret, and refreshes token management", async () => {
    const tenantToken = {
      id: "token-rotated",
      tenant_id: "tenant-1",
      name: "Studio automation",
      scopes: ["plugin:studio"],
      created_by_user_id: null,
      created_at: "2026-07-17T02:00:00Z",
      last_used_at: null,
      expires_at: "2027-01-01T00:00:00Z",
      revoked_at: null,
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              tenant_token: tenantToken,
              token: "pandar_tenant_rotated-secret",
            }),
            {
              status: 201,
              headers: { "content-type": "application/json" },
            },
          ),
      ),
    );
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("token_id", "token-old");
    formData.set("expires_at", "2027-01-01T00:00:00Z");

    await expect(rotateTenantToken(null, formData)).resolves.toEqual({
      ok: true,
      kind: "tenant_token",
      operation: "rotated",
      tenantToken,
      token: "pandar_tenant_rotated-secret",
    });
    expect(fetch).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants/tenant-1/tenant-tokens/token-old/rotate",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ expires_at: "2027-01-01T00:00:00Z" }),
      },
    );
    expect(refreshMock).toHaveBeenCalledTimes(1);
  });

  it("returns the API error without refreshing token management", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ error: "invalid_expires_at" }), {
            status: 400,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("token_id", "token-old");
    formData.set("expires_at", "not-a-date");

    await expect(rotateTenantToken(null, formData)).resolves.toEqual({
      ok: false,
      error: "invalid_expires_at",
    });
    expect(refreshMock).not.toHaveBeenCalled();
  });
});
