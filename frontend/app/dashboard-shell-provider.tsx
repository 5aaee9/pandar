"use client";

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { usePathname, useSearchParams } from "next/navigation";

import type {
  Job,
  Printer,
  Tenant,
} from "./dashboard-types";
import type { DashboardView } from "./dashboard-shell";
import type { LiveState, RuntimeNotification } from "./dashboard-runtime-helpers";

export type RouteRegistration = {
  view: DashboardView;
  tenant: Tenant | null;
  command: string | null;
  status: string | null;
  errors: string[];
  actionStatus: string | null;
  initialPrinters: Printer[];
  initialJobs: Job[];
};

export type DashboardShellContextValue = {
  registerRouteData: (registration: RouteRegistration) => string;
  unregisterRouteData: (token: string) => void;
  livePrinters: Printer[];
  liveJobs: Job[];
  liveView: DashboardView | null;
  liveTenantId: string | null;
  shellView: DashboardView;
  shellTenant: Tenant | null;
  shellCommand: string | null;
  shellStatus: string | null;
  shellErrors: string[];
  shellActionStatus: string | null;
  notifications: RuntimeNotification[];
  liveState: LiveState;
  lastEventAt: string | null;
  actionToast: { message: string; severity: "success" | "error" } | null;
  errorBanner: string | null;
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
  const [registration, setRegistration] = useState<{
    token: string;
    data: RouteRegistration;
  } | null>(null);
  const [livePrinters, setLivePrinters] = useState<Printer[]>([]);
  const [liveJobs, setLiveJobs] = useState<Job[]>([]);
  const [liveView, setLiveView] = useState<DashboardView | null>(null);
  const [liveTenantId, setLiveTenantId] = useState<string | null>(null);
  const [notifications] = useState<RuntimeNotification[]>([]);
  const [liveState] = useState<LiveState>("idle");
  const [lastEventAt] = useState<string | null>(null);
  const [actionToast] = useState<{
    message: string;
    severity: "success" | "error";
  } | null>(null);
  const [errorBanner] = useState<string | null>(null);

  const view = (pathname.split("/")[1] || "devices") as DashboardView;
  const tenantParam = searchParams.get("tenant");
  const shellTenant =
    initialTenants.find((t) => t.id === tenantParam) ??
    initialTenants[0] ??
    null;

  const registerRouteData = useCallback((data: RouteRegistration) => {
    const token = crypto.randomUUID();
    setRegistration({ token, data });
    setLiveView(data.view);
    setLiveTenantId(data.tenant?.id ?? null);
    setLivePrinters(data.initialPrinters);
    setLiveJobs(data.initialJobs);
    return token;
  }, []);

  const unregisterRouteData = useCallback((token: string) => {
    setRegistration((current) => {
      if (current?.token === token) {
        setLiveView(null);
        setLiveTenantId(null);
        setLivePrinters([]);
        setLiveJobs([]);
        return null;
      }
      return current;
    });
  }, []);

  const value = useMemo<DashboardShellContextValue>(
    () => ({
      registerRouteData,
      unregisterRouteData,
      livePrinters,
      liveJobs,
      liveView,
      liveTenantId,
      shellView: view,
      shellTenant,
      shellCommand: searchParams.get("command"),
      shellStatus: searchParams.get("status"),
      shellErrors: registration?.data.errors ?? [],
      shellActionStatus: registration?.data.actionStatus ?? null,
      notifications,
      liveState,
      lastEventAt,
      actionToast,
      errorBanner,
    }),
    [
      registerRouteData,
      unregisterRouteData,
      livePrinters,
      liveJobs,
      liveView,
      liveTenantId,
      view,
      shellTenant,
      searchParams,
      registration,
      notifications,
      liveState,
      lastEventAt,
      actionToast,
      errorBanner,
    ],
  );

  return (
    <DashboardShellContext.Provider value={value}>
      {children}
    </DashboardShellContext.Provider>
  );
}
