import { queryOptions } from "@tanstack/react-query";

import { apiIdSegment } from "./api-path";
import { parseCommandResult } from "./command-result-parser";
import type {
  Agent,
  AgentList,
  AuditEvent,
  AuditEventList,
  Command,
  CommandResultData,
  Job,
  JobList,
  JoinLink,
  JoinLinkList,
  Printer,
  PrinterList,
  TenantToken,
  TenantTokenList,
  User,
  UserIdentity,
  UserList,
} from "./dashboard-types";

// Browser reads cross the Hub proxy same-origin; never fetch the Hub directly.
async function fetchRouteJson<T>(tenantId: string, path: string): Promise<T> {
  const response = await fetch(
    `/api/tenants/${apiIdSegment(tenantId, "tenant_id")}${path}`,
  );
  if (!response.ok) {
    throw new Error(`Route data error: ${response.status}`);
  }
  return response.json() as Promise<T>;
}

export const routeDataKeys = {
  devices: (tenantId: string) => ["route", "devices", tenantId] as const,
  jobs: (tenantId: string) => ["route", "jobs", tenantId] as const,
  agents: (tenantId: string) => ["route", "agents", tenantId] as const,
  users: (tenantId: string) => ["route", "users", tenantId] as const,
  settings: (tenantId: string) => ["route", "settings", tenantId] as const,
  agentSettings: (tenantId: string, agentId: string) =>
    ["route", "agent-settings", tenantId, agentId] as const,
};

export type DevicesRouteData = {
  printers: Printer[];
  agents: Agent[];
  jobs: Job[];
};

export type JobsRouteData = {
  jobs: Job[];
  printers: Printer[];
  agents: Agent[];
};

export type AgentsRouteData = {
  agents: Agent[];
  printers: Printer[];
  command: Command | null;
  commandData: CommandResultData | null;
};

export type UsersRouteData = {
  users: User[];
  identities: UserIdentity[];
  joinLinks: JoinLink[];
};

export type SettingsRouteData = {
  tenantTokens: TenantToken[];
  agents: Agent[];
  printers: Printer[];
  auditEvents: AuditEvent[];
};

export type AgentSettingsRouteData = {
  agent: Agent | null;
  printers: Printer[];
  command: Command | null;
  commandData: CommandResultData | null;
};

export function devicesRouteQuery(tenantId: string) {
  return queryOptions({
    queryKey: routeDataKeys.devices(tenantId),
    queryFn: async (): Promise<DevicesRouteData> => {
      const [printers, agents, jobs] = await Promise.all([
        fetchRouteJson<PrinterList>(tenantId, "/printers"),
        fetchRouteJson<AgentList>(tenantId, "/agents"),
        fetchRouteJson<JobList>(tenantId, "/jobs"),
      ]);
      return {
        printers: printers.printers,
        agents: agents.agents,
        jobs: jobs.jobs,
      };
    },
    staleTime: 10 * 1000,
    refetchInterval: 30 * 1000,
  });
}

export function jobsRouteQuery(tenantId: string) {
  return queryOptions({
    queryKey: routeDataKeys.jobs(tenantId),
    queryFn: async (): Promise<JobsRouteData> => {
      const [jobs, printers, agents] = await Promise.all([
        fetchRouteJson<JobList>(tenantId, "/jobs"),
        fetchRouteJson<PrinterList>(tenantId, "/printers"),
        fetchRouteJson<AgentList>(tenantId, "/agents"),
      ]);
      return {
        jobs: jobs.jobs,
        printers: printers.printers,
        agents: agents.agents,
      };
    },
    staleTime: 10 * 1000,
    refetchInterval: 30 * 1000,
  });
}

export function agentsRouteQuery(tenantId: string, commandId: string | null) {
  return queryOptions({
    queryKey: [...routeDataKeys.agents(tenantId), commandId] as const,
    queryFn: async (): Promise<AgentsRouteData> => {
      const [agents, printers, command] = await Promise.all([
        fetchRouteJson<AgentList>(tenantId, "/agents"),
        fetchRouteJson<PrinterList>(tenantId, "/printers"),
        commandId
          ? fetchRouteJson<Command>(
              tenantId,
              `/commands/${apiIdSegment(commandId, "command_id")}`,
            )
          : Promise.resolve(null),
      ]);
      return {
        agents: agents.agents,
        printers: printers.printers,
        command,
        commandData: command ? parseCommandResult(command) : null,
      };
    },
    staleTime: 30 * 1000,
    refetchInterval: 60 * 1000,
  });
}

export function usersRouteQuery(tenantId: string) {
  return queryOptions({
    queryKey: routeDataKeys.users(tenantId),
    queryFn: async (): Promise<UsersRouteData> => {
      const [users, joinLinks] = await Promise.all([
        fetchRouteJson<UserList>(tenantId, "/users"),
        fetchRouteJson<JoinLinkList>(tenantId, "/join-links"),
      ]);
      return {
        users: users.users,
        identities: users.identities,
        joinLinks: joinLinks.join_links,
      };
    },
    staleTime: 60 * 1000,
  });
}

export function settingsRouteQuery(tenantId: string) {
  return queryOptions({
    queryKey: routeDataKeys.settings(tenantId),
    queryFn: async (): Promise<SettingsRouteData> => {
      const [tenantTokens, agents, printers, auditEvents] = await Promise.all([
        fetchRouteJson<TenantTokenList>(tenantId, "/tenant-tokens"),
        fetchRouteJson<AgentList>(tenantId, "/agents"),
        fetchRouteJson<PrinterList>(tenantId, "/printers"),
        fetchRouteJson<AuditEventList>(tenantId, "/audit-events"),
      ]);
      return {
        tenantTokens: tenantTokens.tenant_tokens,
        agents: agents.agents,
        printers: printers.printers,
        auditEvents: auditEvents.audit_events,
      };
    },
    staleTime: 60 * 1000,
  });
}

export function agentSettingsRouteQuery(
  tenantId: string,
  agentId: string,
  commandId: string | null,
) {
  return queryOptions({
    queryKey: [
      ...routeDataKeys.agentSettings(tenantId, agentId),
      commandId,
    ] as const,
    queryFn: async (): Promise<AgentSettingsRouteData> => {
      const [agents, printers, command] = await Promise.all([
        fetchRouteJson<AgentList>(tenantId, "/agents"),
        fetchRouteJson<PrinterList>(tenantId, "/printers"),
        commandId
          ? fetchRouteJson<Command>(
              tenantId,
              `/commands/${apiIdSegment(commandId, "command_id")}`,
            )
          : Promise.resolve(null),
      ]);
      return {
        agent:
          agents.agents.find((candidate) => candidate.id === agentId) ?? null,
        printers: printers.printers.filter(
          (printer) => printer.agent_id === agentId,
        ),
        command,
        commandData: command ? parseCommandResult(command) : null,
      };
    },
    staleTime: 30 * 1000,
    refetchInterval: commandId ? 15 * 1000 : false,
  });
}
