import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  Agent,
  AuditEvent,
  Command,
  Job,
  Printer,
  TenantToken,
} from "./dashboard-types";
import { invalidateTenantResources } from "./mutation-invalidation";
import { printerCompatibility } from "./printer-compatibility.test-utils";

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

const printer = (id: string): Printer => ({
  id,
  tenant_id: "t1",
  agent_id: "a1",
  serial_number: `serial-${id}`,
  name: id,
  model: null,
  compatibility: printerCompatibility("unknown"),
  status: "idle",
  last_seen_at: "seen",
  created_at: "created",
  materials: null,
});

const agent = (id: string): Agent => ({
  id,
  tenant_id: "t1",
  name: id,
  status: "online",
  created_at: "created",
});

const command = (id: string, kind: string): Command => ({
  id,
  tenant_id: "t1",
  agent_id: "a1",
  printer_id: null,
  kind,
  status: "sent",
  payload_json: "{}",
  error: null,
  result_json: null,
  created_at: "created",
  updated_at: "updated",
});

const job = (id: string): Job => ({
  id,
  tenant_id: "t1",
  printer_id: "p1",
  agent_id: "a1",
  artifact_id: "artifact-1",
  command_id: "command-1",
  status: "acknowledged",
  error: null,
  created_at: "created",
  updated_at: "updated",
  print: {
    status: "running",
    printer_state: null,
    progress_percent: null,
    remaining_time_minutes: null,
    current_layer: null,
    total_layers: null,
    active_file: null,
    last_progress_percent: null,
    last_layer: null,
    error: null,
    started_at: null,
    finished_at: null,
    updated_at: null,
  },
  command: {
    id: "command-1",
    kind: "print_project_file",
    status: "acknowledged",
  },
  artifact: {
    id: "artifact-1",
    tenant_id: "t1",
    filename: "part.3mf",
    content_type: "model/3mf",
    size_bytes: 1,
    metadata: null,
    created_at: "created",
  },
  material: {
    ams_mapping: null,
    ams_mapping2: null,
    ams_mapping_info: null,
    filament_usage: [],
  },
});

const token: TenantToken = {
  id: "tt1",
  tenant_id: "t1",
  name: "Token",
  scopes: [],
  created_by_user_id: null,
  created_at: "created",
  last_used_at: null,
  expires_at: null,
  revoked_at: null,
};

const auditEvent: AuditEvent = {
  id: "e1",
  tenant_id: "t1",
  actor_type: "user",
  user_id: null,
  action: "read",
  target_type: "printer",
  target_id: null,
  metadata: {},
  created_at: "created",
};

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
      "/printers": { printers: [printer("p1")] },
      "/agents": { agents: [agent("a1")] },
      "/jobs": { jobs: [job("j1")] },
      "/tenant-tokens": { tenant_tokens: [token] },
      "/audit-events": { audit_events: [auditEvent] },
    });

    await expect(
      printersResourceQuery("t1").queryFn!({} as never),
    ).resolves.toEqual([printer("p1")]);
    await expect(
      agentsResourceQuery("t1").queryFn!({} as never),
    ).resolves.toEqual([agent("a1")]);
    await expect(
      jobsResourceQuery("t1").queryFn!({} as never),
    ).resolves.toEqual([job("j1")]);
    await expect(
      tenantTokensResourceQuery("t1").queryFn!({} as never),
    ).resolves.toEqual([token]);
    await expect(
      auditEventsResourceQuery("t1").queryFn!({} as never),
    ).resolves.toEqual([auditEvent]);

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
          JSON.stringify({ printers: [printer(`printer-${generation}`)] }),
          { status: 200 },
        );
      }),
    );
    const queryClient = new QueryClient();
    const [devicesPrinters] = devicesRouteQueries("tenant-1");
    const [, jobsPrinters] = jobsRouteQueries("tenant-1");

    await queryClient.fetchQuery(devicesPrinters);
    expect(queryClient.getQueryData(jobsPrinters.queryKey)).toEqual([
      printer("printer-1"),
    ]);

    await invalidateTenantResources(queryClient, "tenant-1", ["printers"]);
    await queryClient.fetchQuery(jobsPrinters);

    expect(queryClient.getQueryData(devicesPrinters.queryKey)).toEqual([
      printer("printer-2"),
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
      "/agents": { agents: [agent("a1"), agent("a2")] },
    });
    const query = agentSettingsRouteQuery("t1", "a2");
    const agents = await query.queryFn!({} as never);

    expect(query.queryKey).toEqual(resourceDataKeys.agents("t1"));
    expect(query.select(agents)).toEqual(agent("a2"));
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
    const link = command("cmd-2", "link_printer");
    const discovery = command("cmd-1", "discover_printers");
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
