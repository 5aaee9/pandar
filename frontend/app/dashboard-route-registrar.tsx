"use client";

import { useEffect, useMemo } from "react";

import type { Job, Printer, Tenant } from "./dashboard-types";
import type { DashboardView } from "./dashboard-shell";
import { useDashboardShellStore } from "./dashboard-shell-store";

export function DashboardRouteRegistrar({
  view,
  tenant,
  command,
  status,
  errors,
  actionStatus,
  initialPrinters,
  initialJobs,
}: {
  view: DashboardView;
  tenant: Tenant | null;
  command: string | null;
  status: string | null;
  errors: string[];
  actionStatus: string | null;
  initialPrinters: Printer[];
  initialJobs: Job[];
}) {
  const registerRouteData = useDashboardShellStore((state) => state.registerRouteData);
  const unregisterRouteData = useDashboardShellStore((state) => state.unregisterRouteData);

  const memoizedErrors = useMemo(() => errors, [errors]);
  const memoizedPrinters = useMemo(() => initialPrinters, [initialPrinters]);
  const memoizedJobs = useMemo(() => initialJobs, [initialJobs]);

  useEffect(() => {
    const token = registerRouteData({
      view,
      tenant,
      command,
      status,
      errors: memoizedErrors,
      actionStatus,
      initialPrinters: memoizedPrinters,
      initialJobs: memoizedJobs,
    });

    return () => {
      unregisterRouteData(token);
    };
  }, [
    view,
    tenant,
    command,
    status,
    actionStatus,
    memoizedErrors,
    memoizedPrinters,
    memoizedJobs,
    registerRouteData,
    unregisterRouteData,
  ]);

  return null;
}
