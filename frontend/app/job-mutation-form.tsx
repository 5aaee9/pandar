"use client";

import { useQueryClient } from "@tanstack/react-query";
import type { ReactNode } from "react";

import type { JobMutationResult } from "./job-actions";
import {
  invalidateTenantResources,
  mutationResources,
} from "./mutation-invalidation";

export function JobMutationForm({
  action,
  tenantId,
  children,
}: {
  action: (formData: FormData) => Promise<JobMutationResult>;
  tenantId: string;
  children: ReactNode;
}) {
  const queryClient = useQueryClient();

  async function submit(formData: FormData) {
    const result = await action(formData);
    if (result.ok) {
      await invalidateTenantResources(
        queryClient,
        tenantId,
        mutationResources.job,
      );
    }
    window.location.assign(result.redirectUrl);
  }

  return <form action={submit}>{children}</form>;
}
