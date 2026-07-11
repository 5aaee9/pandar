import { toast } from "sonner";

import type { Command, Job, Printer, PrinterEvent } from "./dashboard-types";
import {
  jobRecoveryStateKey,
  type RuntimeNotification,
} from "./dashboard-runtime-helpers";
import { mergePrinterEvent } from "./printer-reconciliation";
import { parsePrinterOperationPayload } from "./printer-operation-payload";

export type CommandResultTranslator = (key: string) => string;

type RuntimeEventHandlerOptions = {
  getPrinterBuffer: () => Printer[] | null;
  triggerPrinterResync: (printer: Printer) => void;
  getPrinters: () => Printer[];
  getJobs: () => Job[];
  setPrinters: (printers: Printer[]) => void;
  applyJobProgress: (job: Job) => void;
  setLastEventAt: (timestamp: string) => void;
  addNotification: (notification: RuntimeNotification) => void;
  translateCommandResult: CommandResultTranslator;
};

export function createDashboardRuntimeEventHandler(
  options: RuntimeEventHandlerOptions,
): (event: PrinterEvent) => void {
  return (event) => {
    const observedAt = new Date().toISOString();
    options.setLastEventAt(observedAt);
    if (event.type === "printer_snapshot") {
      const buffer = options.getPrinterBuffer();
      if (buffer) {
        buffer.push(event.printer);
        return;
      }
      const previous = options.getPrinters();
      const result = mergePrinterEvent(previous, event.printer);
      if (result.kind === "resync") {
        options.triggerPrinterResync(event.printer);
      } else if (result.kind === "applied") {
        options.setPrinters(result.printers);
        for (const notification of printerOfflineNotifications(
          previous,
          result.printers,
          observedAt,
        )) {
          options.addNotification(notification);
        }
      }
    } else if (event.type === "job_progress") {
      const jobs = options.getJobs();
      const previous = jobs.find(({ id }) => id === event.job.id) ?? null;
      for (const notification of jobNotifications(previous, event.job, observedAt)) {
        options.addNotification(notification);
      }
      options.applyJobProgress(event.job);
    } else {
      showCommandResult(event.command, options.translateCommandResult);
    }
  };
}

export function printerOfflineNotification(
  previous: Printer | null,
  printer: Printer,
  timestamp: string,
): RuntimeNotification | null {
  if (
    !previous ||
    previous.status === printer.status ||
    printer.status.toLowerCase() !== "offline"
  ) {
    return null;
  }
  return {
    key: `printer:${printer.id}:offline:${printer.last_seen_at}`,
    titleKey: { namespace: "runtime.notification", key: "printerStateTitle" },
    detailKey: {
      namespace: "runtime.notification",
      key: "printerDetail",
      values: { name: printer.name, serial: printer.serial_number },
    },
    timestamp,
  };
}

export function printerOfflineNotifications(
  previous: Printer[],
  current: Printer[],
  timestamp: string,
): RuntimeNotification[] {
  return current.flatMap((printer) => {
    const before = previous.find(({ id }) => id === printer.id) ?? null;
    const notification = printerOfflineNotification(before, printer, timestamp);
    return notification ? [notification] : [];
  });
}

export function jobNotifications(
  previous: Job | null,
  job: Job,
  timestamp: string,
): RuntimeNotification[] {
  if (!previous) {
    return [];
  }

  const notifications: RuntimeNotification[] = [];
  if (
    (job.status.toLowerCase() === "failed" && previous.status !== job.status) ||
    (Boolean(job.error) && previous.error !== job.error)
  ) {
    notifications.push({
      key: `job:${job.id}:dispatch:${job.status}:${job.error ?? ""}`,
      titleKey: { namespace: "recovery.state", key: jobRecoveryStateKey(job) },
      detailKey: job.error
        ? {
            namespace: "runtime.notification",
            key: "jobErrorFallback",
            values: { filename: job.artifact.filename },
          }
        : {
            namespace: "runtime.notification",
            key: "jobDispatchDetail",
            values: { filename: job.artifact.filename, status: job.status },
          },
      timestamp,
    });
  }
  if (
    job.print.status !== previous.print.status &&
    job.print.status.toLowerCase() === "failed"
  ) {
    notifications.push({
      key: `job:${job.id}:print:failed:${job.print.error ?? ""}`,
      titleKey: {
        namespace: "runtime.notification",
        key: "printFailedTitle",
      },
      detailKey: {
        namespace: "runtime.notification",
        key: "jobErrorFallback",
        values: { filename: job.print.error ?? job.artifact.filename },
      },
      timestamp,
    });
  }
  if (
    job.print.status !== previous.print.status &&
    job.print.status.toLowerCase() === "completed"
  ) {
    notifications.push({
      key: `job:${job.id}:print:completed`,
      titleKey: {
        namespace: "runtime.notification",
        key: "printCompleteTitle",
      },
      detailKey: {
        namespace: "runtime.notification",
        key: "jobErrorFallback",
        values: { filename: job.artifact.filename },
      },
      timestamp,
    });
  }
  return notifications;
}

export function showCommandResult(
  command: Command,
  translate: CommandResultTranslator,
): void {
  if (command.kind !== "printer_operation") {
    return;
  }
  const result = printerOperationResult(command.result_json);
  const options = result.sequenceId
    ? { description: `#${result.sequenceId}` }
    : undefined;
  if (command.status.toLowerCase() === "failed") {
    toast.error(
      command.error ?? result.error ?? translate("printerControlFailed"),
      options,
    );
  } else if (
    command.status.toLowerCase() === "succeeded" &&
    parsePrinterOperationPayload(command.payload_json)?.operation.sequence_id === 0
  ) {
    toast.success(translate("recoveryCommandSent"));
  } else {
    toast.success(translate("printerControlCompleted"), options);
  }
}

function printerOperationResult(resultJson: string | null) {
  if (!resultJson) {
    return { sequenceId: null, error: null };
  }
  try {
    const parsed = JSON.parse(resultJson) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return { sequenceId: null, error: null };
    }
    const result = parsed as Record<string, unknown>;
    return {
      sequenceId:
        typeof result.sequence_id === "string" ? result.sequence_id : null,
      error: typeof result.mqtt_error === "string" ? result.mqtt_error : null,
    };
  } catch {
    return { sequenceId: null, error: null };
  }
}
