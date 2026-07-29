"use client";

import { useQuery } from "@tanstack/react-query";

import { apiClient } from "../../api-client";
import { OFFLINE_PRINTER_STATUSES } from "../../dashboard-attention";
import { DashboardViewContent } from "../../dashboard-view-content";
import { QueryErrorBoundary } from "../../query-error-boundary";
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
  const { data, isLoading, error } = useQuery({
    queryKey: ["route", "devices", selectedTenant.id],
    queryFn: async () => {
      const [printers, agents, jobs] = await Promise.all([
        apiClient.printers.list(selectedTenant.id),
        apiClient.agents.list(selectedTenant.id),
        apiClient.jobs.list(selectedTenant.id),
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

  return (
    <QueryErrorBoundary>
      <DashboardViewContent
      view="devices"
      auth={auth}
      selectedTenant={selectedTenant}
      printers={printers}
      agents={agents}
      jobs={jobs}
      health={{
        printersTotal: printers.length,
        printersOnline: printers.filter(
          (printer) => !OFFLINE_PRINTER_STATUSES.has(printer.status.toLowerCase()),
        ).length,
        agentsTotal: agents.length,
        agentsConnected: agents.filter((a) => a.status === "online").length,
        jobsActive: jobs.filter((j) => j.status === "running").length,
        jobsFailed: jobs.filter((j) => j.status === "failed").length,
      }}
      attentionItems={[]}
      topSeverity={null}
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
