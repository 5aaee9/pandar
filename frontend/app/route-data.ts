import { queryOptions } from "@tanstack/react-query";

import { apiIdSegment } from "./api-path";
import { parseCommandResult } from "./command-result-parser";
import {
  DISCOVERY_COMMAND_KIND,
  isTerminalCommandStatus,
} from "./command-status";
import type {
  Agent,
  AgentList,
  AuditEvent,
  AuditEventList,
  Command,
  CommandResultData,
  DiscoveryResultData,
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
  settingsAdmin: (tenantId: string) =>
    ["route", "settings-admin", tenantId] as const,
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
  discoveryCommand: Command | null;
  discoveryData: DiscoveryResultData | null;
};

export type UsersRouteData = {
  users: User[];
  identities: UserIdentity[];
  joinLinks: JoinLink[];
};

export type SettingsRouteData = {
  agents: Agent[];
  printers: Printer[];
};

export type SettingsAdminRouteData = {
  tenantTokens: TenantToken[];
  auditEvents: AuditEvent[];
};

export type AgentSettingsRouteData = {
  agent: Agent | null;
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

export function agentsRouteQuery(
  tenantId: string,
  commandId: string | null,
  discoveryId: string | null = null,
) {
  return queryOptions({
    queryKey: [
      ...routeDataKeys.agents(tenantId),
      commandId,
      discoveryId,
    ] as const,
    queryFn: async (): Promise<AgentsRouteData> => {
      const [agents, printers, command, listedDiscoveryCommand] =
        await Promise.all([
          fetchRouteJson<AgentList>(tenantId, "/agents"),
          fetchRouteJson<PrinterList>(tenantId, "/printers"),
          commandId
            ? fetchRouteJson<Command>(
                tenantId,
                `/commands/${apiIdSegment(commandId, "command_id")}`,
              )
            : Promise.resolve(null),
          discoveryId && discoveryId !== commandId
            ? fetchRouteJson<Command>(
                tenantId,
                `/commands/${apiIdSegment(discoveryId, "command_id")}`,
              )
            : Promise.resolve(null),
        ]);
      const commandData = command ? parseCommandResult(command) : null;
      let discoveryCommand: Command | null = null;
      let discoveryData: DiscoveryResultData | null = null;
      if (command?.kind === DISCOVERY_COMMAND_KIND) {
        discoveryCommand = command;
        discoveryData =
          commandData?.type === "printer_discovery" ? commandData : null;
      } else if (listedDiscoveryCommand?.kind === DISCOVERY_COMMAND_KIND) {
        discoveryCommand = listedDiscoveryCommand;
        const parsed = parseCommandResult(listedDiscoveryCommand);
        discoveryData = parsed?.type === "printer_discovery" ? parsed : null;
      }
      return {
        agents: agents.agents,
        printers: printers.printers,
        command,
        commandData,
        discoveryCommand,
        discoveryData,
      };
    },
    staleTime: 30 * 1000,
    refetchInterval: (query) => {
      const data = query.state.data;
      const hasPendingCommand = [data?.command, data?.discoveryCommand].some(
        (command) => command && !isTerminalCommandStatus(command.status),
      );
      return hasPendingCommand ? 2 * 1000 : 60 * 1000;
    },
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
      const [agents, printers] = await Promise.all([
        fetchRouteJson<AgentList>(tenantId, "/agents"),
        fetchRouteJson<PrinterList>(tenantId, "/printers"),
      ]);
      return {
        agents: agents.agents,
        printers: printers.printers,
      };
    },
    staleTime: 60 * 1000,
  });
}

export function settingsAdminRouteQuery(tenantId: string) {
  return queryOptions({
    queryKey: routeDataKeys.settingsAdmin(tenantId),
    queryFn: async (): Promise<SettingsAdminRouteData> => {
      const [tenantTokens, auditEvents] = await Promise.all([
        fetchRouteJson<TenantTokenList>(tenantId, "/tenant-tokens"),
        fetchRouteJson<AuditEventList>(tenantId, "/audit-events"),
      ]);
      return {
        tenantTokens: tenantTokens.tenant_tokens,
        auditEvents: auditEvents.audit_events,
      };
    },
    staleTime: 60 * 1000,
  });
}

export function agentSettingsRouteQuery(tenantId: string, agentId: string) {
  return queryOptions({
    queryKey: routeDataKeys.agentSettings(tenantId, agentId),
    queryFn: async (): Promise<AgentSettingsRouteData> => {
      const agents = await fetchRouteJson<AgentList>(tenantId, "/agents");
      return {
        agent:
          agents.agents.find((candidate) => candidate.id === agentId) ?? null,
      };
    },
    staleTime: 60 * 1000,
  });
}
