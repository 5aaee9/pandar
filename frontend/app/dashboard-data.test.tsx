import { beforeEach, describe, expect, it, vi } from "vitest";

import { renderDashboardView } from "./dashboard-data";

vi.mock("next/navigation", () => ({ redirect: vi.fn() }));
vi.mock("./api-auth", () => ({
  apiHeaders: vi.fn(async () => undefined),
  authSource: vi.fn(async () => ({
    source: "none",
    cookieName: "pandar_auth_token",
    provider: "none",
  })),
}));
vi.mock("./auth-provider", () => ({
  authProviderConfig: vi.fn(() => ({
    provider: "none",
    signInUrl: null,
    signOutUrl: null,
  })),
}));

const tenant = {
  id: "tenant-1",
  slug: "tenant-one",
  display_name: "Tenant One",
  created_at: "2026-01-01T00:00:00Z",
};

function responseBody(path: string) {
  if (path === "/api/v1/tenants") {
    return { tenants: [tenant] };
  }
  if (path.endsWith("/printers")) return { printers: [] };
  if (path.endsWith("/agents")) return { agents: [] };
  if (path.endsWith("/jobs")) return { jobs: [] };
  if (path.endsWith("/users")) {
    return {
      users: [
        {
          id: "user-1",
          tenant_id: tenant.id,
          email: "one@example.com",
          display_name: "One",
          role: "tenant_admin",
          created_at: "2026-01-01T00:00:00Z",
        },
        {
          id: "user-2",
          tenant_id: tenant.id,
          email: "two@example.com",
          display_name: "Two",
          role: "viewer",
          created_at: "2026-01-01T00:00:00Z",
        },
      ],
      identities: [],
    };
  }
  if (path.includes("/identities")) return { identities: [] };
  if (path.endsWith("/tenant-tokens")) return { tenant_tokens: [] };
  if (path.endsWith("/join-links")) return { join_links: [] };
  if (path.includes("/audit-events")) return { audit_events: [] };
  if (path.includes("/commands/")) {
    return {
      id: "command-1",
      tenant_id: tenant.id,
      agent_id: "agent-1",
      printer_id: null,
      kind: "discover_printers",
      status: "succeeded",
      payload_json: "{}",
      error: null,
      result_json: null,
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    };
  }
  throw new Error(`Unexpected dashboard request: ${path}`);
}

function requestedPaths() {
  return vi.mocked(fetch).mock.calls.map(([input]) => {
    const url = new URL(String(input));
    return `${url.pathname}${url.search}`;
  });
}

describe("renderDashboardView data loading", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: string | URL | Request) => {
        const url = new URL(String(input));
        return Response.json(responseBody(`${url.pathname}${url.search}`));
      }),
    );
  });

  it.each([
    [
      "devices",
      [
        "/api/v1/tenants",
        "/api/v1/tenants/tenant-1/printers",
        "/api/v1/tenants/tenant-1/agents",
        "/api/v1/tenants/tenant-1/jobs",
      ],
    ],
    [
      "jobs",
      [
        "/api/v1/tenants",
        "/api/v1/tenants/tenant-1/printers",
        "/api/v1/tenants/tenant-1/agents",
        "/api/v1/tenants/tenant-1/jobs",
      ],
    ],
    [
      "agents",
      [
        "/api/v1/tenants",
        "/api/v1/tenants/tenant-1/printers",
        "/api/v1/tenants/tenant-1/agents",
      ],
    ],
    [
      "users",
      [
        "/api/v1/tenants",
        "/api/v1/tenants/tenant-1/users",
        "/api/v1/tenants/tenant-1/join-links",
      ],
    ],
    [
      "settings",
      [
        "/api/v1/tenants",
        "/api/v1/tenants/tenant-1/printers",
        "/api/v1/tenants/tenant-1/agents",
        "/api/v1/tenants/tenant-1/tenant-tokens",
        "/api/v1/tenants/tenant-1/audit-events?limit=20",
      ],
    ],
  ] as const)("loads only data used by the %s view", async (view, expected) => {
    await renderDashboardView(view, {
      searchParams: Promise.resolve({ tenant: tenant.id }),
    });

    expect(requestedPaths().sort()).toEqual([...expected].sort());
  });

  it("loads all user identities with the users request instead of N+1 requests", async () => {
    await renderDashboardView("users", {
      searchParams: Promise.resolve({ tenant: tenant.id }),
    });

    expect(
      requestedPaths().filter((path) => path.includes("/identities")),
    ).toEqual([]);
    expect(
      requestedPaths().filter((path) => path.endsWith("/users")),
    ).toHaveLength(1);
  });

  it.each(["devices", "jobs", "users", "settings"] as const)(
    "does not load command details for the %s view",
    async (view) => {
      await renderDashboardView(view, {
        searchParams: Promise.resolve({
          tenant: tenant.id,
          command: "command-1",
        }),
      });

      expect(
        requestedPaths().filter((path) => path.includes("/commands/")),
      ).toEqual([]);
    },
  );

  it("loads command details for the agents view", async () => {
    await renderDashboardView("agents", {
      searchParams: Promise.resolve({
        tenant: tenant.id,
        command: "command-1",
      }),
    });

    expect(
      requestedPaths().filter((path) => path.includes("/commands/")),
    ).toEqual(["/api/v1/tenants/tenant-1/commands/command-1"]);
  });
});
