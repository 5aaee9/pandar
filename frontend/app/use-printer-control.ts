"use client";

import { controlPrinter } from "./actions";
import { useActionStatusFeedback } from "./mutation-feedback";

export function usePrinterControl() {
  return useActionStatusFeedback(controlPrinter, "printer_control_queued");
}
