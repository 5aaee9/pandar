"use client";

import { useQuery } from "@tanstack/react-query";
import { apiClient } from "./api-client";
import type { DashboardView } from "./dashboard-shell";

export function useDevicesRoute(tenantId: string | null) {
  return useQuery({
    queryKey: ["route", "devices", tenantId],
    queryFn: async () => {
      if (!tenantId) return { printers: [], agents: [], jobs: [] };
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
    enabled: tenantId !== null,
    staleTime: 30 * 1000,
  });
}

export function useJobsRoute(tenantId: string | null) {
  return useQuery({
    queryKey: ["route", "jobs", tenantId],
    queryFn: async () => {
      if (!tenantId) return { jobs: [], printers: [], agents: [] };
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
    enabled: tenantId !== null,
    staleTime: 30 * 1000,
  });
}

export function useAgentsRoute(tenantId: string | null, commandId: string | null) {
  return useQuery({
    queryKey: ["route", "agents", tenantId, commandId],
    queryFn: async () => {
      if (!tenantId) return { agents: [], printers: [], command: null, commandData: null };
      const [agents, printers, command] = await Promise.all([
        apiClient.agents.list(tenantId),
        apiClient.printers.list(tenantId),
        commandId ? apiClient.commands.get(tenantId, commandId) : Promise.resolve(null),
      ]);
      return {
        agents: agents.agents,
        printers: printers.printers,
        command,
        commandData: command ? parseCommandResult(command) : null,
      };
    },
    enabled: tenantId !== null,
    staleTime: 30 * 1000,
  });
}

export function useUsersRoute(tenantId: string | null) {
  return useQuery({
    queryKey: ["route", "users", tenantId],
    queryFn: async () => {
      if (!tenantId) return { users: [], identities: [], joinLinks: [] };
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
    enabled: tenantId !== null,
    staleTime: 30 * 1000,
  });
}

export function useSettingsRoute(tenantId: string | null) {
  return useQuery({
    queryKey: ["route", "settings", tenantId],
    queryFn: async () => {
      if (!tenantId) return { tenantTokens: [], agents: [], printers: [], auditEvents: [] };
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
    enabled: tenantId !== null,
    staleTime: 30 * 1000,
  });
}

export function useRouteData(view: DashboardView, tenantId: string | null, commandId?: string | null) {
  const devices = useDevicesRoute(view === "devices" ? tenantId : null);
  const jobs = useJobsRoute(view === "jobs" ? tenantId : null);
  const agents = useAgentsRoute(view === "agents" ? tenantId : null, commandId ?? null);
  const users = useUsersRoute(view === "users" ? tenantId : null);
  const settings = useSettingsRoute(view === "settings" ? tenantId : null);

  switch (view) {
    case "devices":
      return devices;
    case "jobs":
      return jobs;
    case "agents":
      return agents;
    case "users":
      return users;
    case "settings":
      return settings;
  }
}

import { parseCommandResult } from "./command-result-parser";