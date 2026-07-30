import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Agent, Command, Printer } from "./dashboard-types";

const apiClientMock = vi.hoisted(() => ({
  printers: { list: vi.fn() },
  agents: { list: vi.fn() },
  jobs: { list: vi.fn() },
  users: { list: vi.fn(), joinLinks: vi.fn() },
  settings: { tenantTokens: vi.fn(), auditEvents: vi.fn() },
  commands: { get: vi.fn() },
}));

vi.mock("./api-client", () => ({
  apiClient: apiClientMock,
}));

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
  settingsRouteQuery,
  usersRouteQuery,
} from "./route-data";

describe("routeDataKeys", () => {
  it("builds per-view prefix keys scoped to the tenant", () => {
    expect(routeDataKeys.devices("t1")).toEqual(["route", "devices", "t1"]);
    expect(routeDataKeys.jobs("t1")).toEqual(["route", "jobs", "t1"]);
    expect(routeDataKeys.agents("t1")).toEqual(["route", "agents", "t1"]);
    expect(routeDataKeys.users("t1")).toEqual(["route", "users", "t1"]);
    expect(routeDataKeys.settings("t1")).toEqual(["route", "settings", "t1"]);
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
    ]);
    expect(agentSettingsRouteQuery("t1", "a1", "cmd-1").queryKey).toEqual([
      "route",
      "agent-settings",
      "t1",
      "a1",
      "cmd-1",
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
    expect(agentsRouteQuery("t1", null).refetchInterval).toBe(60_000);
    expect(usersRouteQuery("t1").staleTime).toBe(60_000);
    expect(settingsRouteQuery("t1").staleTime).toBe(60_000);
    expect(agentSettingsRouteQuery("t1", "a1", null).staleTime).toBe(30_000);
    expect(agentSettingsRouteQuery("t1", "a1", null).refetchInterval).toBe(
      false,
    );
    expect(agentSettingsRouteQuery("t1", "a1", "cmd-1").refetchInterval).toBe(
      15_000,
    );
  });
});

describe("route query functions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("composes devices data from printers, agents, and jobs", async () => {
    apiClientMock.printers.list.mockResolvedValue({ printers: ["p1"] });
    apiClientMock.agents.list.mockResolvedValue({ agents: ["a1"] });
    apiClientMock.jobs.list.mockResolvedValue({ jobs: ["j1"] });

    const data = await devicesRouteQuery("t1").queryFn!({} as never);

    expect(apiClientMock.printers.list).toHaveBeenCalledWith("t1");
    expect(apiClientMock.agents.list).toHaveBeenCalledWith("t1");
    expect(apiClientMock.jobs.list).toHaveBeenCalledWith("t1");
    expect(data).toEqual({ printers: ["p1"], agents: ["a1"], jobs: ["j1"] });
  });

  it("composes jobs data from jobs, printers, and agents", async () => {
    apiClientMock.jobs.list.mockResolvedValue({ jobs: ["j1"] });
    apiClientMock.printers.list.mockResolvedValue({ printers: ["p1"] });
    apiClientMock.agents.list.mockResolvedValue({ agents: ["a1"] });

    const data = await jobsRouteQuery("t1").queryFn!({} as never);

    expect(data).toEqual({ jobs: ["j1"], printers: ["p1"], agents: ["a1"] });
  });

  it("omits the command fetch for the agents view without a commandId", async () => {
    apiClientMock.agents.list.mockResolvedValue({ agents: ["a1"] });
    apiClientMock.printers.list.mockResolvedValue({ printers: ["p1"] });

    const data = await agentsRouteQuery("t1", null).queryFn!({} as never);

    expect(apiClientMock.commands.get).not.toHaveBeenCalled();
    expect(data).toEqual({
      agents: ["a1"],
      printers: ["p1"],
      command: null,
      commandData: null,
    });
  });

  it("fetches and parses the selected command for the agents view", async () => {
    const command = { id: "cmd-1" } as Command;
    apiClientMock.agents.list.mockResolvedValue({ agents: [] });
    apiClientMock.printers.list.mockResolvedValue({ printers: [] });
    apiClientMock.commands.get.mockResolvedValue(command);

    const data = await agentsRouteQuery("t1", "cmd-1").queryFn!({} as never);

    expect(apiClientMock.commands.get).toHaveBeenCalledWith("t1", "cmd-1");
    expect(parseCommandResultMock).toHaveBeenCalledWith(command);
    expect(data.command).toBe(command);
    expect(data.commandData).toEqual({ parsed: true });
  });

  it("composes users data from users and join links", async () => {
    apiClientMock.users.list.mockResolvedValue({
      users: ["u1"],
      identities: ["i1"],
    });
    apiClientMock.users.joinLinks.mockResolvedValue({ join_links: ["l1"] });

    const data = await usersRouteQuery("t1").queryFn!({} as never);

    expect(apiClientMock.users.list).toHaveBeenCalledWith("t1");
    expect(apiClientMock.users.joinLinks).toHaveBeenCalledWith("t1");
    expect(data).toEqual({
      users: ["u1"],
      identities: ["i1"],
      joinLinks: ["l1"],
    });
  });

  it("composes settings data from tokens, agents, printers, and audit events", async () => {
    apiClientMock.settings.tenantTokens.mockResolvedValue({
      tenant_tokens: ["tt1"],
    });
    apiClientMock.agents.list.mockResolvedValue({ agents: ["a1"] });
    apiClientMock.printers.list.mockResolvedValue({ printers: ["p1"] });
    apiClientMock.settings.auditEvents.mockResolvedValue({
      audit_events: ["e1"],
    });

    const data = await settingsRouteQuery("t1").queryFn!({} as never);

    expect(data).toEqual({
      tenantTokens: ["tt1"],
      agents: ["a1"],
      printers: ["p1"],
      auditEvents: ["e1"],
    });
  });

  it("selects the agent and its printers for the agent settings view", async () => {
    const agent = { id: "a1" } as Agent;
    const own = { agent_id: "a1" } as Printer;
    const other = { agent_id: "a2" } as Printer;
    apiClientMock.agents.list.mockResolvedValue({
      agents: [agent, { id: "a2" }],
    });
    apiClientMock.printers.list.mockResolvedValue({ printers: [own, other] });

    const data = await agentSettingsRouteQuery("t1", "a1", null).queryFn!(
      {} as never,
    );

    expect(data.agent).toBe(agent);
    expect(data.printers).toEqual([own]);
    expect(data.command).toBeNull();
    expect(data.commandData).toBeNull();
  });

  it("returns a null agent when the agent is missing", async () => {
    apiClientMock.agents.list.mockResolvedValue({ agents: [] });
    apiClientMock.printers.list.mockResolvedValue({ printers: [] });

    const data = await agentSettingsRouteQuery("t1", "a1", null).queryFn!(
      {} as never,
    );

    expect(data.agent).toBeNull();
  });
});
