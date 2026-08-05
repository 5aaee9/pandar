"use client";

import {
  createContext,
  useContext,
  useMemo,
  type ReactNode,
} from "react";
import { usePathname } from "next/navigation";

import type { Tenant } from "./dashboard-types";
import type { DashboardView } from "./dashboard-shell";

export type DashboardShellContextValue = {
  shellView: DashboardView;
  shellTenant: Tenant | null;
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
  selectedTenant,
}: {
  children: ReactNode;
  selectedTenant: Tenant | null;
}) {
  const pathname = usePathname();
  const view = (pathname.split("/")[1] || "devices") as DashboardView;

  const value = useMemo<DashboardShellContextValue>(
    () => ({
      shellView: view,
      shellTenant: selectedTenant,
    }),
    [view, selectedTenant],
  );

  return (
    <DashboardShellContext.Provider value={value}>
      {children}
    </DashboardShellContext.Provider>
  );
}
