import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Command } from "./dashboard-types";

const parseCommandResultMock = vi.hoisted(() =>
  vi.fn(() => ({ parsed: true }) as never),
);

vi.mock("./command-result-parser", () => ({
  parseCommandResult: parseCommandResultMock,
}));

import {
  agentsRouteQuery,
  agentSettingsRouteQuery,
  devicesRouteQuery,
  jobsRouteQuery,
  routeDataKeys,
  settingsAdminRouteQuery,
  settingsRouteQuery,
  usersRouteQuery,
} from "./route-data";

type FetchMock = ReturnType<
  typeof vi.fn<(input: RequestInfo | URL) => Promise<Response>>
>;

function stubRouteFetch(routes: Record<string, unknown>): FetchMock {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const path = String(input);
    for (const [route, payload] of Object.entries(routes)) {
      if (path.endsWith(route)) {
        return new Response(JSON.stringify(payload), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
    }
    throw new Error(`unexpected fetch: ${path}`);
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

function fetchedPaths(fetchMock: FetchMock): string[] {
  return fetchMock.mock.calls.map(([input]) => String(input));
}

describe("routeDataKeys", () => {
  it("builds per-view prefix keys scoped to the tenant", () => {
    expect(routeDataKeys.devices("t1")).toEqual(["route", "devices", "t1"]);
    expect(routeDataKeys.jobs("t1")).toEqual(["route", "jobs", "t1"]);
    expect(routeDataKeys.agents("t1")).toEqual(["route", "agents", "t1"]);
    expect(routeDataKeys.users("t1")).toEqual(["route", "users", "t1"]);
    expect(routeDataKeys.settings("t1")).toEqual(["route", "settings", "t1"]);
    expect(routeDataKeys.settingsAdmin("t1")).toEqual([
      "route",
      "settings-admin",
      "t1",
    ]);
    expect(routeDataKeys.agentSettings("t1", "a1")).toEqual([
      "route",
      "agent-settings",
      "t1",
      "a1",
    ]);
  });

  it("keeps command-scoped query keys under the view prefix", () => {
    expect(agentsRouteQuery("t1", "cmd-1").queryKey).toEqual([
      "route",
      "agents",
      "t1",
      "cmd-1",
      null,
    ]);
    expect(agentsRouteQuery("t1", "cmd-1", "cmd-0").queryKey).toEqual([
      "route",
      "agents",
      "t1",
      "cmd-1",
      "cmd-0",
    ]);
    expect(agentSettingsRouteQuery("t1", "a1").queryKey).toEqual([
      "route",
      "agent-settings",
      "t1",
      "a1",
    ]);
  });
});

describe("route query cache policies", () => {
  it("locks the per-view staleTime and refetchInterval", () => {
    expect(devicesRouteQuery("t1").staleTime).toBe(10_000);
    expect(devicesRouteQuery("t1").refetchInterval).toBe(30_000);
    expect(jobsRouteQuery("t1").staleTime).toBe(10_000);
    expect(jobsRouteQuery("t1").refetchInterval).toBe(30_000);
    expect(agentsRouteQuery("t1", null).staleTime).toBe(30_000);
    expect(usersRouteQuery("t1").staleTime).toBe(60_000);
    expect(settingsRouteQuery("t1").staleTime).toBe(60_000);
    expect(settingsAdminRouteQuery("t1").staleTime).toBe(60_000);
    expect(agentSettingsRouteQuery("t1", "a1").staleTime).toBe(60_000);
  });

  it("polls the agents view quickly while a tracked command is pending", () => {
    const refetchInterval = agentsRouteQuery("t1", "cmd-1")
      .refetchInterval as (query: {
      state: { data?: unknown };
    }) => number;

    expect(refetchInterval({ state: { data: undefined } })).toBe(60_000);
    expect(
      refetchInterval({
        state: {
          data: {
            command: { status: "succeeded" },
            discoveryCommand: null,
          },
        },
      }),
    ).toBe(60_000);
    expect(
      refetchInterval({
        state: {
          data: {
            command: { status: "sent" },
            discoveryCommand: null,
          },
        },
      }),
    ).toBe(2_000);
    expect(
      refetchInterval({
        state: {
          data: {
            command: { status: "succeeded" },
            discoveryCommand: { status: "queued" },
          },
        },
      }),
    ).toBe(2_000);
  });
});

describe("route query functions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("composes devices data through the hub proxy", async () => {
    const fetchMock = stubRouteFetch({
      "/printers": { printers: ["p1"] },
      "/agents": { agents: ["a1"] },
      "/jobs": { jobs: ["j1"] },
    });

    const data = await devicesRouteQuery("t1").queryFn!({} as never);

    expect(fetchedPaths(fetchMock).sort()).toEqual([
      "/api/tenants/t1/agents",
      "/api/tenants/t1/jobs",
      "/api/tenants/t1/printers",
    ]);
    expect(data).toEqual({ printers: ["p1"], agents: ["a1"], jobs: ["j1"] });
  });

  it("composes jobs data from jobs, printers, and agents", async () => {
    stubRouteFetch({
      "/jobs": { jobs: ["j1"] },
      "/printers": { printers: ["p1"] },
      "/agents": { agents: ["a1"] },
    });

    const data = await jobsRouteQuery("t1").queryFn!({} as never);

    expect(data).toEqual({ jobs: ["j1"], printers: ["p1"], agents: ["a1"] });
  });

  it("omits the command fetch for the agents view without a commandId", async () => {
    const fetchMock = stubRouteFetch({
      "/agents": { agents: ["a1"] },
      "/printers": { printers: ["p1"] },
    });

    const data = await agentsRouteQuery("t1", null).queryFn!({} as never);

    expect(
      fetchedPaths(fetchMock).some((path) => path.includes("/commands/")),
    ).toBe(false);
    expect(data).toEqual({
      agents: ["a1"],
      printers: ["p1"],
      command: null,
      commandData: null,
      discoveryCommand: null,
      discoveryData: null,
    });
  });

  it("fetches and parses the selected command for the agents view", async () => {
    const command = { id: "cmd-1" } as Command;
    const fetchMock = stubRouteFetch({
      "/agents": { agents: [] },
      "/printers": { printers: [] },
      "/commands/cmd-1": command,
    });

    const data = await agentsRouteQuery("t1", "cmd-1").queryFn!({} as never);

    expect(fetchedPaths(fetchMock)).toContain("/api/tenants/t1/commands/cmd-1");
    expect(parseCommandResultMock).toHaveBeenCalledWith(command);
    expect(data.command).toEqual(command);
    expect(data.commandData).toEqual({ parsed: true });
    expect(data.discoveryCommand).toBeNull();
    expect(data.discoveryData).toBeNull();
  });

  it("treats a selected discovery command as the discovery context", async () => {
    const command = { id: "cmd-1", kind: "discover_printers" } as Command;
    stubRouteFetch({
      "/agents": { agents: [] },
      "/printers": { printers: [] },
      "/commands/cmd-1": command,
    });

    const data = await agentsRouteQuery("t1", "cmd-1").queryFn!({} as never);

    expect(data.command).toEqual(command);
    expect(data.discoveryCommand).toEqual(command);
    expect(data.discoveryData).toBeNull();
  });

  it("fetches the listed discovery command alongside a link command", async () => {
    const linkCommand = { id: "cmd-2", kind: "link_printer" } as Command;
    const discoveryCommand = {
      id: "cmd-1",
      kind: "discover_printers",
    } as Command;
    const fetchMock = stubRouteFetch({
      "/agents": { agents: [] },
      "/printers": { printers: [] },
      "/commands/cmd-2": linkCommand,
      "/commands/cmd-1": discoveryCommand,
    });

    const data = await agentsRouteQuery("t1", "cmd-2", "cmd-1").queryFn!(
      {} as never,
    );

    expect(fetchedPaths(fetchMock).sort()).toEqual([
      "/api/tenants/t1/agents",
      "/api/tenants/t1/commands/cmd-1",
      "/api/tenants/t1/commands/cmd-2",
      "/api/tenants/t1/printers",
    ]);
    expect(data.command).toEqual(linkCommand);
    expect(data.discoveryCommand).toEqual(discoveryCommand);
  });

  it("ignores a listed discovery command that is not a discovery", async () => {
    const linkCommand = { id: "cmd-2", kind: "link_printer" } as Command;
    const otherCommand = { id: "cmd-3", kind: "refresh_printers" } as Command;
    stubRouteFetch({
      "/agents": { agents: [] },
      "/printers": { printers: [] },
      "/commands/cmd-2": linkCommand,
      "/commands/cmd-3": otherCommand,
    });

    const data = await agentsRouteQuery("t1", "cmd-2", "cmd-3").queryFn!(
      {} as never,
    );

    expect(data.command).toEqual(linkCommand);
    expect(data.discoveryCommand).toBeNull();
    expect(data.discoveryData).toBeNull();
  });

  it("composes users data from users and join links", async () => {
    const fetchMock = stubRouteFetch({
      "/users": { users: ["u1"], identities: ["i1"] },
      "/join-links": { join_links: ["l1"] },
    });

    const data = await usersRouteQuery("t1").queryFn!({} as never);

    expect(fetchedPaths(fetchMock).sort()).toEqual([
      "/api/tenants/t1/join-links",
      "/api/tenants/t1/users",
    ]);
    expect(data).toEqual({
      users: ["u1"],
      identities: ["i1"],
      joinLinks: ["l1"],
    });
  });

  it("composes settings workspace data from agents and printers", async () => {
    const fetchMock = stubRouteFetch({
      "/agents": { agents: ["a1"] },
      "/printers": { printers: ["p1"] },
    });

    const data = await settingsRouteQuery("t1").queryFn!({} as never);

    expect(fetchedPaths(fetchMock).sort()).toEqual([
      "/api/tenants/t1/agents",
      "/api/tenants/t1/printers",
    ]);
    expect(data).toEqual({
      agents: ["a1"],
      printers: ["p1"],
    });
  });

  it("keeps admin-only settings data in a separate query", async () => {
    const fetchMock = stubRouteFetch({
      "/tenant-tokens": { tenant_tokens: ["tt1"] },
      "/audit-events": { audit_events: ["e1"] },
    });

    const data = await settingsAdminRouteQuery("t1").queryFn!({} as never);

    expect(fetchedPaths(fetchMock).sort()).toEqual([
      "/api/tenants/t1/audit-events",
      "/api/tenants/t1/tenant-tokens",
    ]);
    expect(data).toEqual({
      tenantTokens: ["tt1"],
      auditEvents: ["e1"],
    });
  });

  it("selects the agent for the agent settings view", async () => {
    stubRouteFetch({
      "/agents": { agents: [{ id: "a1" }, { id: "a2" }] },
    });

    const data = await agentSettingsRouteQuery("t1", "a1").queryFn!(
      {} as never,
    );

    expect(data.agent).toEqual({ id: "a1" });
  });

  it("returns a null agent when the agent is missing", async () => {
    stubRouteFetch({
      "/agents": { agents: [] },
    });

    const data = await agentSettingsRouteQuery("t1", "a1").queryFn!(
      {} as never,
    );

    expect(data.agent).toBeNull();
  });

  it("rejects the query when the proxy responds with an error status", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("nope", { status: 500 })),
    );

    await expect(devicesRouteQuery("t1").queryFn!({} as never)).rejects.toThrow(
      "Route data error: 500",
    );
  });
});
