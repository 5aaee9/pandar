import { queryOptions } from "@tanstack/react-query";

import { apiIdSegment } from "./api-path";
import { parseCommandResult } from "./command-result-parser";
import { decodeHubResponse } from "./hub-contract";
import type {
  HubSchemaMap,
  HubSchemaName,
} from "./generated/hub-api-schema-map";
import {
  DISCOVERY_COMMAND_KIND,
  isTerminalCommandStatus,
} from "./command-status";
import type {
  Agent,
  AuditEvent,
  Command,
  CommandResultData,
  DiscoveryResultData,
  Job,
  JoinLink,
  Printer,
  TenantToken,
  User,
  UserIdentity,
} from "./dashboard-types";

// Browser reads cross the Hub proxy same-origin; never fetch the Hub directly.
async function fetchRouteJson<Name extends HubSchemaName>(
  tenantId: string,
  path: string,
  schemaName: Name,
): Promise<HubSchemaMap[Name]> {
  const response = await fetch(
    `/api/tenants/${apiIdSegment(tenantId, "tenant_id")}${path}`,
  );
  if (!response.ok) {
    throw new Error(`Route data error: ${response.status}`);
  }
  return decodeHubResponse(schemaName, await response.json());
}

/** Canonical tenant-scoped owners for mutable dashboard resources. */
export const resourceDataKeys = {
  printers: (tenantId: string) => ["tenant", tenantId, "printers"] as const,
  agents: (tenantId: string) => ["tenant", tenantId, "agents"] as const,
  jobs: (tenantId: string) => ["tenant", tenantId, "jobs"] as const,
  users: (tenantId: string) => ["tenant", tenantId, "users"] as const,
  joinLinks: (tenantId: string) => ["tenant", tenantId, "join-links"] as const,
  tenantTokens: (tenantId: string) =>
    ["tenant", tenantId, "tenant-tokens"] as const,
  auditEvents: (tenantId: string) =>
    ["tenant", tenantId, "audit-events"] as const,
};

export type AgentsCommandRouteData = {
  command: Command | null;
  commandData: CommandResultData | null;
  discoveryCommand: Command | null;
  discoveryData: DiscoveryResultData | null;
};

export type UsersResourceData = {
  users: User[];
  identities: UserIdentity[];
};

export function printersResourceQuery(tenantId: string) {
  return queryOptions({
    queryKey: resourceDataKeys.printers(tenantId),
    queryFn: async (): Promise<Printer[]> =>
      (await fetchRouteJson(tenantId, "/printers", "PrinterList")).printers,
    staleTime: 10 * 1000,
    refetchInterval: 30 * 1000,
  });
}

export function agentsResourceQuery(tenantId: string) {
  return queryOptions({
    queryKey: resourceDataKeys.agents(tenantId),
    queryFn: async (): Promise<Agent[]> =>
      (await fetchRouteJson(tenantId, "/agents", "AgentList")).agents,
    staleTime: 30 * 1000,
    refetchInterval: 60 * 1000,
  });
}

export function jobsResourceQuery(tenantId: string) {
  return queryOptions({
    queryKey: resourceDataKeys.jobs(tenantId),
    queryFn: async (): Promise<Job[]> =>
      (await fetchRouteJson(tenantId, "/jobs", "JobList")).jobs,
    staleTime: 10 * 1000,
    refetchInterval: 30 * 1000,
  });
}

export function usersResourceQuery(tenantId: string) {
  return queryOptions({
    queryKey: resourceDataKeys.users(tenantId),
    queryFn: async (): Promise<UsersResourceData> => {
      const users = await fetchRouteJson(tenantId, "/users", "UserList");
      return { users: users.users, identities: users.identities };
    },
    staleTime: 60 * 1000,
  });
}

export function joinLinksResourceQuery(tenantId: string) {
  return queryOptions({
    queryKey: resourceDataKeys.joinLinks(tenantId),
    queryFn: async (): Promise<JoinLink[]> =>
      (await fetchRouteJson(tenantId, "/join-links", "JoinLinkList"))
        .join_links,
    staleTime: 60 * 1000,
  });
}

export function tenantTokensResourceQuery(tenantId: string) {
  return queryOptions({
    queryKey: resourceDataKeys.tenantTokens(tenantId),
    queryFn: async (): Promise<TenantToken[]> =>
      (await fetchRouteJson(tenantId, "/tenant-tokens", "TenantTokenList"))
        .tenant_tokens,
    staleTime: 60 * 1000,
  });
}

export function auditEventsResourceQuery(tenantId: string) {
  return queryOptions({
    queryKey: resourceDataKeys.auditEvents(tenantId),
    queryFn: async (): Promise<AuditEvent[]> =>
      (await fetchRouteJson(tenantId, "/audit-events", "AuditEventList"))
        .audit_events,
    staleTime: 60 * 1000,
  });
}

export function devicesRouteQueries(tenantId: string) {
  return [
    printersResourceQuery(tenantId),
    agentsResourceQuery(tenantId),
    jobsResourceQuery(tenantId),
  ] as const;
}

export function jobsRouteQueries(tenantId: string) {
  return [
    jobsResourceQuery(tenantId),
    printersResourceQuery(tenantId),
    agentsResourceQuery(tenantId),
  ] as const;
}

export function settingsRouteQueries(tenantId: string) {
  return [
    agentsResourceQuery(tenantId),
    printersResourceQuery(tenantId),
  ] as const;
}

export function settingsAdminRouteQueries(tenantId: string) {
  return [
    tenantTokensResourceQuery(tenantId),
    auditEventsResourceQuery(tenantId),
  ] as const;
}

export function usersRouteQueries(tenantId: string) {
  return [
    usersResourceQuery(tenantId),
    joinLinksResourceQuery(tenantId),
  ] as const;
}

export function agentSettingsRouteQuery(tenantId: string, agentId: string) {
  return {
    ...agentsResourceQuery(tenantId),
    select: (agents: Agent[]) =>
      agents.find((candidate) => candidate.id === agentId) ?? null,
  };
}

export function agentsCommandRouteQuery(
  tenantId: string,
  commandId: string | null,
  discoveryId: string | null = null,
) {
  return queryOptions({
    queryKey: [
      "route",
      "agents-commands",
      tenantId,
      commandId,
      discoveryId,
    ] as const,
    queryFn: async (): Promise<AgentsCommandRouteData> => {
      const [command, listedDiscoveryCommand] = await Promise.all([
        commandId
          ? fetchRouteJson(
              tenantId,
              `/commands/${apiIdSegment(commandId, "command_id")}`,
              "Command",
            )
          : Promise.resolve(null),
        discoveryId && discoveryId !== commandId
          ? fetchRouteJson(
              tenantId,
              `/commands/${apiIdSegment(discoveryId, "command_id")}`,
              "Command",
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
