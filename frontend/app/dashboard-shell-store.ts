import { create } from "zustand";

import type { Job, Printer, Tenant } from "./dashboard-types";
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

type DashboardShellState = {
  registration: { token: string; data: RouteRegistration } | null;
  livePrinters: Printer[];
  liveJobs: Job[];
  liveView: DashboardView | null;
  liveTenantId: string | null;
  notifications: RuntimeNotification[];
  liveState: LiveState;
  lastEventAt: string | null;
  actionToast: { message: string; severity: "success" | "error" } | null;
  errorBanner: string | null;
  registerRouteData: (data: RouteRegistration) => string;
  unregisterRouteData: (token: string) => void;
};

export const useDashboardShellStore = create<DashboardShellState>((set, get) => ({
  registration: null,
  livePrinters: [],
  liveJobs: [],
  liveView: null,
  liveTenantId: null,
  notifications: [],
  liveState: "idle",
  lastEventAt: null,
  actionToast: null,
  errorBanner: null,
  registerRouteData: (data) => {
    const token = crypto.randomUUID();
    set({
      registration: { token, data },
      liveView: data.view,
      liveTenantId: data.tenant?.id ?? null,
      livePrinters: data.initialPrinters,
      liveJobs: data.initialJobs,
    });
    return token;
  },
  unregisterRouteData: (token) => {
    const current = get().registration;
    if (current?.token === token) {
      set({
        registration: null,
        liveView: null,
        liveTenantId: null,
        livePrinters: [],
        liveJobs: [],
      });
    }
  },
}));
