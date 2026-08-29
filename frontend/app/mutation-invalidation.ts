"use client";

import { useEffect, useRef } from "react";
import type { QueryClient } from "@tanstack/react-query";

import { resourceDataKeys } from "./route-data";

export type MutableResource = keyof typeof resourceDataKeys;

export const mutationResources = {
  printer: ["printers", "jobs", "auditEvents"],
  printerLink: ["printers", "agents", "auditEvents"],
  job: ["jobs", "auditEvents"],
  token: ["tenantTokens", "auditEvents"],
  user: ["users", "auditEvents"],
  joinLink: ["joinLinks", "auditEvents"],
  agent: ["agents", "printers", "jobs", "auditEvents"],
} as const satisfies Record<string, readonly MutableResource[]>;

export function invalidateTenantResources(
  queryClient: QueryClient,
  tenantId: string,
  resources: readonly MutableResource[],
) {
  return Promise.all(
    resources.map((resource) =>
      queryClient.invalidateQueries({
        queryKey: resourceDataKeys[resource](tenantId),
      }),
    ),
  );
}

export function useInvalidateOnSuccess(
  state: { ok: boolean } | null,
  queryClient: QueryClient,
  tenantId: string,
  resources: readonly MutableResource[],
) {
  const handled = useRef<typeof state>(null);

  useEffect(() => {
    if (!state?.ok || handled.current === state) {
      return;
    }
    handled.current = state;
    void invalidateTenantResources(queryClient, tenantId, resources);
  }, [queryClient, resources, state, tenantId]);
}
