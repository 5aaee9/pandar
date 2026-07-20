"use client";

import type { ReactNode } from "react";

import type { Job, Printer, Tenant } from "./dashboard-types";
import type { DashboardView } from "./dashboard-shell";
import { useDashboardShell } from "./dashboard-shell-provider";

export function DashboardRouteConsumer({
  view,
  selectedTenant,
  initialPrinters,
  initialJobs,
  children,
}: {
  view: DashboardView;
  selectedTenant: Tenant | null;
  initialPrinters: Printer[];
  initialJobs: Job[];
  children: (data: {
    printers: Printer[];
    jobs: Job[];
    view: DashboardView;
    tenant: Tenant | null;
  }) => ReactNode;
}) {
  const { livePrinters, liveJobs, liveView, liveTenantId } = useDashboardShell();

  const isLiveDataValid =
    liveView === view && liveTenantId === selectedTenant?.id;

  const printers = isLiveDataValid ? livePrinters : initialPrinters;
  const jobs = isLiveDataValid ? liveJobs : initialJobs;

  return <>{children({ printers, jobs, view, tenant: selectedTenant })}</>;
}
