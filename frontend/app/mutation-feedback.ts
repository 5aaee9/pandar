"use client";

import { useActionState, useCallback, useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import type { MutationActionState } from "./action-state";
import { formatActionStatus } from "./action-status";
import {
  invalidateTenantResources,
  type MutableResource,
} from "./mutation-invalidation";

export function useMutationFeedback(
  state: MutationActionState,
  options: {
    successMessage?: string;
    errorMessage?: (code: string) => string;
    silentError?: boolean;
    onSuccess?: () => void;
  },
) {
  const handledRef = useRef<MutationActionState>(null);
  const { successMessage, errorMessage, silentError, onSuccess } = options;

  useEffect(() => {
    if (!state || handledRef.current === state) {
      return;
    }
    handledRef.current = state;
    if (state.ok) {
      if (successMessage) {
        toast.success(successMessage);
      }
      onSuccess?.();
    } else if (!silentError) {
      toast.error(errorMessage ? errorMessage(state.error) : state.error);
    }
  }, [state, successMessage, errorMessage, silentError, onSuccess]);
}

export function useActionStatusFeedback(
  action: (
    state: MutationActionState,
    formData: FormData,
  ) => Promise<MutationActionState>,
  successStatus: string,
  options?: {
    invalidate?: readonly MutableResource[];
    onSuccess?: () => void;
  },
) {
  const tStatus = useTranslations("runtime.actionStatus");
  const queryClient = useQueryClient();
  const invalidate = options?.invalidate;
  const onSuccess = options?.onSuccess;
  const invalidatingAction = useCallback(
    async (state: MutationActionState, formData: FormData) => {
      const result = await action(state, formData);
      const tenantId = formData.get("tenant_id");
      if (result?.ok && typeof tenantId === "string" && invalidate) {
        await invalidateTenantResources(queryClient, tenantId, invalidate);
      }
      return result;
    },
    [action, invalidate, queryClient],
  );
  const [state, formAction, pending] = useActionState(invalidatingAction, null);

  useMutationFeedback(state, {
    successMessage: formatActionStatus(successStatus, tStatus),
    errorMessage: (code) => formatActionStatus(code, tStatus),
    onSuccess,
  });

  return { formAction, pending };
}
