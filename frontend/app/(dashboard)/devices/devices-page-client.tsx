"use client";

import { useQueries } from "@tanstack/react-query";

import {
  computeAttention,
  computeHealth,
  topSeverityOf,
} from "../../dashboard-attention";
import { DevicesView } from "../../dashboard-view-content";
import { QueryErrorBoundary } from "../../query-error-boundary";
import { devicesRouteQueries } from "../../route-data";
import type { Printer, Tenant } from "../../dashboard-types";
import { useDashboardClock } from "../../use-dashboard-clock";

const EMPTY_PRINTERS: Printer[] = [];

export function DevicesPageClient({
  selectedTenant,
}: {
  selectedTenant: Tenant;
}) {
  const [printersQuery, agentsQuery, jobsQuery] = useQueries({
    queries: devicesRouteQueries(selectedTenant.id),
  });
  const nowMs = useDashboardClock(printersQuery.data ?? EMPTY_PRINTERS);
  const isLoading = [printersQuery, agentsQuery, jobsQuery].some(
    (query) => query.isLoading,
  );
  const error = printersQuery.error ?? agentsQuery.error ?? jobsQuery.error;

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
        Failed to load devices:{" "}
        {error instanceof Error ? error.message : "Unknown error"}
      </div>
    );
  }

  const printers = printersQuery.data ?? [];
  const agents = agentsQuery.data ?? [];
  const jobs = jobsQuery.data ?? [];
  const health = computeHealth(agents, printers, jobs);
  const attentionItems = computeAttention({ agents, printers, jobs, nowMs });

  return (
    <QueryErrorBoundary>
      <DevicesView
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
      />
    </QueryErrorBoundary>
  );
}
