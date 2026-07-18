"use client";

import { useCallback, useEffect, useReducer, useRef } from "react";
import { useTranslations } from "next-intl";

import type {
  AuthMetadata,
  Job,
  Printer,
  Tenant,
} from "./dashboard-types";
import type {
  LiveState,
  RuntimeNotification,
} from "./dashboard-runtime-helpers";
import { replacePrinterInventory } from "./printer-reconciliation";
import {
  startPrinterReconciliationCoordinator,
  type PrinterReconciliationCoordinator,
} from "./printer-reconciliation-coordinator";

type DashboardRuntimeEventsArgs = {
  apiUrl: string;
  auth: Pick<AuthMetadata, "source">;
  enabled?: boolean;
  selectedTenant: Tenant | null;
  initialPrinters: Printer[];
  initialJobs: Job[];
};

export type DashboardRuntimeEvents = {
  liveState: LiveState;
  lastEventAt: string | null;
  notifications: RuntimeNotification[];
  printers: Printer[];
  jobs: Job[];
  retry: () => void;
};

type RuntimeState = Omit<DashboardRuntimeEvents, "retry">;

type RuntimeAction =
  | { type: "reset"; printers: Printer[]; jobs: Job[] }
  | { type: "live-state"; value: LiveState }
  | { type: "last-event"; value: string }
  | { type: "notification"; value: RuntimeNotification }
  | { type: "printers"; value: Printer[] }
  | { type: "jobs"; value: Job[] };

function runtimeReducer(state: RuntimeState, action: RuntimeAction): RuntimeState {
  switch (action.type) {
    case "reset":
      return { ...state, printers: action.printers, jobs: action.jobs };
    case "live-state":
      return { ...state, liveState: action.value };
    case "last-event":
      return { ...state, lastEventAt: action.value };
    case "notification":
      return {
        ...state,
        notifications: [action.value, ...state.notifications].slice(0, 12),
      };
    case "printers":
      return { ...state, printers: action.value };
    case "jobs":
      return { ...state, jobs: action.value };
  }
}

export function useDashboardRuntimeEvents({
  apiUrl,
  auth,
  enabled = true,
  selectedTenant,
  initialPrinters,
  initialJobs,
}: DashboardRuntimeEventsArgs): DashboardRuntimeEvents {
  const [state, dispatch] = useReducer(runtimeReducer, {
    liveState: "idle",
    lastEventAt: null,
    notifications: [],
    printers: replacePrinterInventory(initialPrinters),
    jobs: initialJobs,
  });
  const tenantId = selectedTenant?.id ?? null;
  const notificationKeys = useRef(new Set<string>());
  const coordinatorRef = useRef<PrinterReconciliationCoordinator | null>(null);
  const inventoryTenantRef = useRef(tenantId);
  const jobsBaselineRef = useRef(initialJobs);
  const liveJobUpdatesRef = useRef(new Map<string, Job>());
  const printersRef = useRef(state.printers);
  const jobsRef = useRef(state.jobs);
  const translateCommandResult = useTranslations("runtime.commandResult");
  const retry = useCallback(() => coordinatorRef.current?.retry(), []);

  useEffect(() => {
    const tenantChanged = inventoryTenantRef.current !== tenantId;
    const jobsChanged = jobsBaselineRef.current !== initialJobs;
    if (!tenantChanged && !jobsChanged) {
      return;
    }
    jobsBaselineRef.current = initialJobs;
    if (tenantChanged) {
      inventoryTenantRef.current = tenantId;
      liveJobUpdatesRef.current.clear();
      const resetPrinters = replacePrinterInventory(initialPrinters);
      printersRef.current = resetPrinters;
      jobsRef.current = initialJobs;
      dispatch({ type: "reset", printers: resetPrinters, jobs: initialJobs });
      if (tenantId === null) {
        dispatch({ type: "live-state", value: "idle" });
      }
      return;
    }

    const seen = new Set<string>();
    const merged = initialJobs.map((job) => {
      seen.add(job.id);
      return liveJobUpdatesRef.current.get(job.id) ?? job;
    });
    const eventOnly = [...liveJobUpdatesRef.current.values()].filter(
      ({ id }) => !seen.has(id),
    );
    const jobs = [...eventOnly, ...merged];
    jobsRef.current = jobs;
    dispatch({ type: "jobs", value: jobs });
  }, [initialJobs, initialPrinters, tenantId]);

  useEffect(() => {
    if (!enabled || tenantId === null) {
      coordinatorRef.current = null;
      return;
    }

    const coordinator = startPrinterReconciliationCoordinator({
      apiUrl,
      authSource: auth.source,
      tenantId,
      translateCommandResult,
      getPrinters: () => printersRef.current,
      getJobs: () => jobsRef.current,
      setPrinters: (printers) => {
        printersRef.current = printers;
        dispatch({ type: "printers", value: printers });
      },
      applyJobProgress: (job) => {
        liveJobUpdatesRef.current.set(job.id, job);
        const jobs = jobsRef.current.some(({ id }) => id === job.id)
          ? jobsRef.current.map((current) =>
              current.id === job.id ? job : current,
            )
          : [job, ...jobsRef.current];
        jobsRef.current = jobs;
        dispatch({ type: "jobs", value: jobs });
      },
      setLiveState: (value) => dispatch({ type: "live-state", value }),
      setLastEventAt: (value) => dispatch({ type: "last-event", value }),
      addNotification: (value) => {
        if (notificationKeys.current.has(value.key)) {
          return;
        }
        notificationKeys.current.add(value.key);
        dispatch({ type: "notification", value });
      },
    });
    coordinatorRef.current = coordinator;
    return () => {
      if (coordinatorRef.current === coordinator) {
        coordinatorRef.current = null;
      }
      coordinator.stop();
    };
  }, [
    apiUrl,
    auth.source,
    enabled,
    tenantId,
    translateCommandResult,
  ]);

  return { ...state, retry };
}
