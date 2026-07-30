import { queryOptions } from "@tanstack/react-query";

import { apiClient } from "./api-client";
import { parseCommandResult } from "./command-result-parser";
import type {
  Agent,
  AuditEvent,
  Command,
  CommandResultData,
  Job,
  JoinLink,
  Printer,
  TenantToken,
  User,
  UserIdentity,
} from "./dashboard-types";

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
        apiClient.printers.list(tenantId),
        apiClient.agents.list(tenantId),
        apiClient.jobs.list(tenantId),
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
        apiClient.jobs.list(tenantId),
        apiClient.printers.list(tenantId),
        apiClient.agents.list(tenantId),
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
        apiClient.agents.list(tenantId),
        apiClient.printers.list(tenantId),
        commandId
          ? apiClient.commands.get(tenantId, commandId)
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
        apiClient.users.list(tenantId),
        apiClient.users.joinLinks(tenantId),
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
        apiClient.settings.tenantTokens(tenantId),
        apiClient.agents.list(tenantId),
        apiClient.printers.list(tenantId),
        apiClient.settings.auditEvents(tenantId),
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
        apiClient.agents.list(tenantId),
        apiClient.printers.list(tenantId),
        commandId
          ? apiClient.commands.get(tenantId, commandId)
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
