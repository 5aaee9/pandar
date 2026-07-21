"use client";

import type { ReactNode } from "react";

import type { Job, Printer, Tenant } from "./dashboard-types";
import type { DashboardView } from "./dashboard-shell";
import { useDashboardShellStore } from "./dashboard-shell-store";

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
  const livePrinters = useDashboardShellStore((state) => state.livePrinters);
  const liveJobs = useDashboardShellStore((state) => state.liveJobs);
  const liveView = useDashboardShellStore((state) => state.liveView);
  const liveTenantId = useDashboardShellStore((state) => state.liveTenantId);

  const isLiveDataValid =
    liveView === view && liveTenantId === selectedTenant?.id;

  const printers = isLiveDataValid ? livePrinters : initialPrinters;
  const jobs = isLiveDataValid ? liveJobs : initialJobs;

  return <>{children({ printers, jobs, view, tenant: selectedTenant })}</>;
}
