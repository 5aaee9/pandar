"use client";

import { useQuery } from "@tanstack/react-query";

import { DashboardViewContent } from "../../dashboard-view-content";
import { QueryErrorBoundary } from "../../query-error-boundary";
import { jobsRouteQuery } from "../../route-data";
import type { AuthMetadata, Tenant } from "../../dashboard-types";

export function JobsPageClient({
  auth,
  selectedTenant,
  canManageJobs,
}: {
  auth: AuthMetadata;
  selectedTenant: Tenant;
  canManageJobs: boolean;
}) {
  const { data, isLoading, error } = useQuery(
    jobsRouteQuery(selectedTenant.id),
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
        Failed to load jobs: {error instanceof Error ? error.message : "Unknown error"}
      </div>
    );
  }

  const { jobs, printers, agents } = data ?? { jobs: [], printers: [], agents: [] };

  return (
    <QueryErrorBoundary>
      <DashboardViewContent
      view="jobs"
      auth={auth}
      selectedTenant={selectedTenant}
      printers={printers}
      agents={agents}
      jobs={jobs}
      health={{
        printersTotal: printers.length,
        printersOnline: printers.filter((p) => p.status === "online").length,
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
      nowMs={0}
      selectedCommand={null}
      commandData={null}
      notifications={[]}
      tenantTokens={[]}
      auditEvents={[]}
      adminUnavailable={false}
      adminLoadError={false}
      canManageJobs={canManageJobs}
    />
    </QueryErrorBoundary>
  );
}
