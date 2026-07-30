"use client";

import { useEffect, useRef } from "react";
import { toast } from "sonner";

import type { MutationActionState } from "./action-state";

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
