"use client";

import { useQueries } from "@tanstack/react-query";

import { JobsView } from "../../dashboard-view-content";
import { QueryErrorBoundary } from "../../query-error-boundary";
import { jobsRouteQueries } from "../../route-data";
import type { Tenant } from "../../dashboard-types";

export function JobsPageClient({
  selectedTenant,
  canManageJobs,
}: {
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

  return (
    <QueryErrorBoundary>
      <JobsView
        selectedTenant={selectedTenant}
        printers={printers}
        agents={agents}
        jobs={jobs}
        nowMs={0}
        canManageJobs={canManageJobs}
      />
    </QueryErrorBoundary>
  );
}
