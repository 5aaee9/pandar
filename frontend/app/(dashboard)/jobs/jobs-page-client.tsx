"use client";

import { useQueries } from "@tanstack/react-query";

import {
  computeAttention,
  computeHealth,
  topSeverityOf,
} from "../../dashboard-attention";
import { DashboardViewContent } from "../../dashboard-view-content";
import { QueryErrorBoundary } from "../../query-error-boundary";
import { jobsRouteQueries } from "../../route-data";
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
  const [jobsQuery, printersQuery, agentsQuery] = useQueries({
    queries: jobsRouteQueries(selectedTenant.id),
  });
  const isLoading = [jobsQuery, printersQuery, agentsQuery].some(
    (query) => query.isLoading,
  );
  const error = jobsQuery.error ?? printersQuery.error ?? agentsQuery.error;

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
        Failed to load jobs:{" "}
        {error instanceof Error ? error.message : "Unknown error"}
      </div>
    );
  }

  const jobs = jobsQuery.data ?? [];
  const printers = printersQuery.data ?? [];
  const agents = agentsQuery.data ?? [];
  const health = computeHealth(agents, printers, jobs);
  const attentionItems = computeAttention({ agents, printers, jobs, nowMs: 0 });

  return (
    <QueryErrorBoundary>
      <DashboardViewContent
        view="jobs"
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
