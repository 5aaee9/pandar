"use client";

import { useQuery } from "@tanstack/react-query";

import { apiClient } from "../../api-client";
import { parseCommandResult } from "../../command-result-parser";
import { DashboardViewContent } from "../../dashboard-view-content";
import { QueryErrorBoundary } from "../../query-error-boundary";
import type { AuthMetadata, Tenant } from "../../dashboard-types";

export function AgentsPageClient({
  auth,
  selectedTenant,
  adminUnavailable,
  commandId,
}: {
  auth: AuthMetadata;
  selectedTenant: Tenant;
  adminUnavailable: boolean;
  commandId: string | null;
}) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["route", "agents", selectedTenant.id, commandId],
    queryFn: async () => {
      const [agents, printers, command] = await Promise.all([
        apiClient.agents.list(selectedTenant.id),
        apiClient.printers.list(selectedTenant.id),
        commandId ? apiClient.commands.get(selectedTenant.id, commandId) : Promise.resolve(null),
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

  if (isLoading) {
    return (
      <div className="flex items-center justify-center p-8">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-muted border-t-primary" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="rounded-md border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive">
        Failed to load agents: {error instanceof Error ? error.message : "Unknown error"}
      </div>
    );
  }

  const { agents, printers, command, commandData } = data ?? {
    agents: [],
    printers: [],
    command: null,
    commandData: null,
  };

  return (
    <QueryErrorBoundary>
      <DashboardViewContent
      view="agents"
      auth={auth}
      selectedTenant={selectedTenant}
      printers={printers}
      agents={agents}
      jobs={[]}
      health={{
        printersTotal: printers.length,
        printersOnline: printers.filter((p) => p.status === "online").length,
        agentsTotal: agents.length,
        agentsConnected: agents.filter((a) => a.status === "online").length,
        jobsActive: 0,
        jobsFailed: 0,
      }}
      attentionItems={[]}
      topSeverity={null}
      liveState="idle"
      lastEventAt={null}
      fleetEmpty={printers.length === 0}
      nowMs={0}
      selectedCommand={command}
      commandData={commandData}
      notifications={[]}
      tenantTokens={[]}
      auditEvents={[]}
      adminUnavailable={adminUnavailable}
      adminLoadError={false}
      canManageJobs={true}
    />
    </QueryErrorBoundary>
  );
}
