"use client";

import {
  createContext,
  useContext,
  useMemo,
  type ReactNode,
} from "react";
import { usePathname, useSearchParams } from "next/navigation";

import type { Tenant } from "./dashboard-types";
import type { DashboardView } from "./dashboard-shell";

export type DashboardShellContextValue = {
  shellView: DashboardView;
  shellTenant: Tenant | null;
  shellCommand: string | null;
  shellStatus: string | null;
};

const DashboardShellContext = createContext<DashboardShellContextValue | null>(
  null,
);

export function useDashboardShell() {
  const context = useContext(DashboardShellContext);
  if (!context) {
    throw new Error("useDashboardShell must be used within DashboardShellProvider");
  }
  return context;
}

export function DashboardShellProvider({
  children,
  initialTenants,
}: {
  children: ReactNode;
  initialTenants: Tenant[];
}) {
  const pathname = usePathname();
  const searchParams = useSearchParams();

  const view = (pathname.split("/")[1] || "devices") as DashboardView;
  const tenantParam = searchParams.get("tenant");
  const shellTenant =
    initialTenants.find((t) => t.id === tenantParam) ??
    initialTenants[0] ??
    null;

  const value = useMemo<DashboardShellContextValue>(
    () => ({
      shellView: view,
      shellTenant,
      shellCommand: searchParams.get("command"),
      shellStatus: searchParams.get("status"),
    }),
    [view, shellTenant, searchParams],
  );

  return (
    <DashboardShellContext.Provider value={value}>
      {children}
    </DashboardShellContext.Provider>
  );
}
