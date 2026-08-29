import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Command } from "./dashboard-types";
import { invalidateTenantResources } from "./mutation-invalidation";

const parseCommandResultMock = vi.hoisted(() =>
  vi.fn(() => ({ parsed: true }) as never),
);

vi.mock("./command-result-parser", () => ({
  parseCommandResult: parseCommandResultMock,
}));

import {
  agentSettingsRouteQuery,
  agentsCommandRouteQuery,
  agentsResourceQuery,
  auditEventsResourceQuery,
  devicesRouteQueries,
  jobsResourceQuery,
  jobsRouteQueries,
  printersResourceQuery,
  resourceDataKeys,
  settingsAdminRouteQueries,
  settingsRouteQueries,
  tenantTokensResourceQuery,
  usersRouteQueries,
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

describe("canonical resource data", () => {
  beforeEach(() => vi.clearAllMocks());
  afterEach(() => vi.unstubAllGlobals());

  it("uses tenant-scoped keys shared by every consuming route", () => {
    expect(resourceDataKeys.printers("t1")).toEqual([
      "tenant",
      "t1",
      "printers",
    ]);
    expect(resourceDataKeys.agents("t1")).toEqual(["tenant", "t1", "agents"]);
    expect(resourceDataKeys.jobs("t1")).toEqual(["tenant", "t1", "jobs"]);
    expect(resourceDataKeys.tenantTokens("t1")).toEqual([
      "tenant",
      "t1",
      "tenant-tokens",
    ]);

    const [devicesPrinters, devicesAgents, devicesJobs] =
      devicesRouteQueries("t1");
    const [jobs, jobsPrinters, jobsAgents] = jobsRouteQueries("t1");
    const [settingsAgents, settingsPrinters] = settingsRouteQueries("t1");

    expect(devicesPrinters.queryKey).toEqual(jobsPrinters.queryKey);
    expect(devicesPrinters.queryKey).toEqual(settingsPrinters.queryKey);
    expect(devicesAgents.queryKey).toEqual(jobsAgents.queryKey);
    expect(devicesAgents.queryKey).toEqual(settingsAgents.queryKey);
    expect(devicesJobs.queryKey).toEqual(jobs.queryKey);
  });

  it("fetches each typed resource through the same-origin Hub proxy", async () => {
    const fetchMock = stubRouteFetch({
      "/printers": { printers: ["p1"] },
      "/agents": { agents: ["a1"] },
      "/jobs": { jobs: ["j1"] },
      "/tenant-tokens": { tenant_tokens: ["tt1"] },
      "/audit-events": { audit_events: ["e1"] },
    });

    await expect(
      printersResourceQuery("t1").queryFn!({} as never),
    ).resolves.toEqual(["p1"]);
    await expect(
      agentsResourceQuery("t1").queryFn!({} as never),
    ).resolves.toEqual(["a1"]);
    await expect(
      jobsResourceQuery("t1").queryFn!({} as never),
    ).resolves.toEqual(["j1"]);
    await expect(
      tenantTokensResourceQuery("t1").queryFn!({} as never),
    ).resolves.toEqual(["tt1"]);
    await expect(
      auditEventsResourceQuery("t1").queryFn!({} as never),
    ).resolves.toEqual(["e1"]);

    expect(fetchMock).toHaveBeenCalledTimes(5);
    expect(
      fetchMock.mock.calls.every(([input]) =>
        String(input).startsWith("/api/tenants/t1/"),
      ),
    ).toBe(true);
  });

  it("keeps cross-route reads fresh through one authoritative cache", async () => {
    let generation = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        generation += 1;
        return new Response(
          JSON.stringify({ printers: [{ id: `printer-${generation}` }] }),
          { status: 200 },
        );
      }),
    );
    const queryClient = new QueryClient();
    const [devicesPrinters] = devicesRouteQueries("tenant-1");
    const [, jobsPrinters] = jobsRouteQueries("tenant-1");

    await queryClient.fetchQuery(devicesPrinters);
    expect(queryClient.getQueryData(jobsPrinters.queryKey)).toEqual([
      { id: "printer-1" },
    ]);

    await invalidateTenantResources(queryClient, "tenant-1", ["printers"]);
    await queryClient.fetchQuery(jobsPrinters);

    expect(queryClient.getQueryData(devicesPrinters.queryKey)).toEqual([
      { id: "printer-2" },
    ]);
    expect(generation).toBe(2);
  });

  it("keeps user, token, and audit composition on canonical resource keys", () => {
    const [users, joinLinks] = usersRouteQueries("t1");
    const [tokens, audit] = settingsAdminRouteQueries("t1");

    expect(users.queryKey).toEqual(resourceDataKeys.users("t1"));
    expect(joinLinks.queryKey).toEqual(resourceDataKeys.joinLinks("t1"));
    expect(tokens.queryKey).toEqual(resourceDataKeys.tenantTokens("t1"));
    expect(audit.queryKey).toEqual(resourceDataKeys.auditEvents("t1"));
  });

  it("selects one agent without creating an agent-settings cache", async () => {
    stubRouteFetch({
      "/agents": { agents: [{ id: "a1" }, { id: "a2" }] },
    });
    const query = agentSettingsRouteQuery("t1", "a2");
    const agents = await query.queryFn!({} as never);

    expect(query.queryKey).toEqual(resourceDataKeys.agents("t1"));
    expect(query.select(agents)).toEqual({ id: "a2" });
  });

  it("rejects resource reads when the proxy responds with an error status", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("nope", { status: 500 })),
    );

    await expect(
      printersResourceQuery("t1").queryFn!({} as never),
    ).rejects.toThrow("Route data error: 500");
  });
});

describe("agents command route composition", () => {
  beforeEach(() => vi.clearAllMocks());
  afterEach(() => vi.unstubAllGlobals());

  it("does not fetch a command when route context has no command id", async () => {
    const fetchMock = stubRouteFetch({});
    await expect(
      agentsCommandRouteQuery("t1", null).queryFn!({} as never),
    ).resolves.toEqual({
      command: null,
      commandData: null,
      discoveryCommand: null,
      discoveryData: null,
    });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("parses selected and discovery commands without owning agent data", async () => {
    const link = { id: "cmd-2", kind: "link_printer" } as Command;
    const discovery = {
      id: "cmd-1",
      kind: "discover_printers",
    } as Command;
    stubRouteFetch({
      "/commands/cmd-2": link,
      "/commands/cmd-1": discovery,
    });

    const data = await agentsCommandRouteQuery("t1", "cmd-2", "cmd-1").queryFn!(
      {} as never,
    );

    expect(data.command).toEqual(link);
    expect(data.discoveryCommand).toEqual(discovery);
    expect(parseCommandResultMock).toHaveBeenCalledWith(link);
    expect(parseCommandResultMock).toHaveBeenCalledWith(discovery);
  });

  it("polls quickly while either tracked command is pending", () => {
    const interval = agentsCommandRouteQuery("t1", "cmd-1")
      .refetchInterval as (query: { state: { data?: unknown } }) => number;

    expect(interval({ state: { data: undefined } })).toBe(60_000);
    expect(
      interval({
        state: {
          data: { command: { status: "sent" }, discoveryCommand: null },
        },
      }),
    ).toBe(2_000);
  });
});
