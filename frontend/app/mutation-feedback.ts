"use client";

import { useActionState, useEffect, useRef } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import type { MutationActionState } from "./action-state";
import { formatActionStatus } from "./action-status";

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
  onSuccess?: () => void,
) {
  const tStatus = useTranslations("runtime.actionStatus");
  const [state, formAction, pending] = useActionState(action, null);

  useMutationFeedback(state, {
    successMessage: formatActionStatus(successStatus, tStatus),
    errorMessage: (code) => formatActionStatus(code, tStatus),
    onSuccess,
  });

  return { formAction, pending };
}
