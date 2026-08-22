"use client";

import { useQuery } from "@tanstack/react-query";

import {
  computeAttention,
  computeHealth,
  topSeverityOf,
} from "../../dashboard-attention";
import { DashboardViewContent } from "../../dashboard-view-content";
import { QueryErrorBoundary } from "../../query-error-boundary";
import { agentsRouteQuery } from "../../route-data";
import type { AuthMetadata, Tenant } from "../../dashboard-types";

export function AgentsPageClient({
  auth,
  selectedTenant,
  adminUnavailable,
  commandId,
  discoveryId,
}: {
  auth: AuthMetadata;
  selectedTenant: Tenant;
  adminUnavailable: boolean;
  commandId: string | null;
  discoveryId: string | null;
}) {
  const { data, isLoading, error } = useQuery(
    agentsRouteQuery(selectedTenant.id, commandId, discoveryId),
  );

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
        Failed to load agents:{" "}
        {error instanceof Error ? error.message : "Unknown error"}
      </div>
    );
  }

  const {
    agents,
    printers,
    command,
    commandData,
    discoveryCommand,
    discoveryData,
  } = data ?? {
    agents: [],
    printers: [],
    command: null,
    commandData: null,
    discoveryCommand: null,
    discoveryData: null,
  };

  const health = computeHealth(agents, printers, []);
  const attentionItems = computeAttention({
    agents,
    printers,
    jobs: [],
    nowMs: 0,
  });

  return (
    <QueryErrorBoundary>
      <DashboardViewContent
        view="agents"
        auth={auth}
        selectedTenant={selectedTenant}
        printers={printers}
        agents={agents}
        jobs={[]}
        health={health}
        attentionItems={attentionItems}
        topSeverity={topSeverityOf(attentionItems)}
        liveState="idle"
        lastEventAt={null}
        fleetEmpty={printers.length === 0}
        nowMs={0}
        selectedCommand={command}
        commandData={commandData}
        discoveryCommand={discoveryCommand}
        discoveryData={discoveryData}
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
