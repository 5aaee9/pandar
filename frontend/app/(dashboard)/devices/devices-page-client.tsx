"use client";

import { useQuery } from "@tanstack/react-query";

import {
  computeAttention,
  computeHealth,
  topSeverityOf,
} from "../../dashboard-attention";
import { DashboardViewContent } from "../../dashboard-view-content";
import { QueryErrorBoundary } from "../../query-error-boundary";
import { devicesRouteQuery } from "../../route-data";
import type { AuthMetadata, Printer, Tenant } from "../../dashboard-types";
import { useDashboardClock } from "../../use-dashboard-clock";

const EMPTY_PRINTERS: Printer[] = [];

export function DevicesPageClient({
  auth,
  selectedTenant,
}: {
  auth: AuthMetadata;
  selectedTenant: Tenant;
}) {
  const { data, isLoading, error } = useQuery(
    devicesRouteQuery(selectedTenant.id),
  );
  const nowMs = useDashboardClock(data?.printers ?? EMPTY_PRINTERS);

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
        Failed to load devices: {error instanceof Error ? error.message : "Unknown error"}
      </div>
    );
  }

  const { printers, agents, jobs } = data ?? { printers: [], agents: [], jobs: [] };
  const health = computeHealth(agents, printers, jobs);
  const attentionItems = computeAttention({ agents, printers, jobs, nowMs });

  return (
    <QueryErrorBoundary>
      <DashboardViewContent
      view="devices"
      auth={auth}
      selectedTenant={selectedTenant}
      printers={printers}
      agents={agents}
      jobs={jobs}
      health={health}
      attentionItems={attentionItems}
      topSeverity={topSeverityOf(attentionItems)}
      liveState="idle"
      lastEventAt={null}
      fleetEmpty={printers.length === 0}
      nowMs={nowMs}
      selectedCommand={null}
      commandData={null}
      notifications={[]}
      tenantTokens={[]}
      auditEvents={[]}
      adminUnavailable={false}
      adminLoadError={false}
      canManageJobs={true}
    />
    </QueryErrorBoundary>
  );
}
