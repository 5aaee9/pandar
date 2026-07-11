"use client";

import { useTranslations } from "next-intl";

import { formatLayers, formatProgress, formatRemaining } from "./job-format";
import type { PrinterPrintState } from "./printer-live-types";

type PrintPresentation =
  | "printing"
  | "paused"
  | "preparing"
  | "slicing"
  | "finished";

const COARSE_VETO = new Set(["IDLE", "OFFLINE", "FAILED"]);

export function PrinterPrintStatus({
  coarseStatus,
  print,
}: {
  coarseStatus: string;
  print: PrinterPrintState | null;
}) {
  const t = useTranslations("printMonitor");
  const tJob = useTranslations("jobFormat");
  const presentation =
    COARSE_VETO.has(coarseStatus.toUpperCase()) || !print
      ? null
      : printPresentation(print.gcode_state);

  if (!presentation || !print) {
    return (
      <div data-testid="printer-print-status">
        <div className="text-xs font-medium text-muted-foreground">
          {t("statusLabel")}
        </div>
        <div className="mt-0.5 text-sm font-medium text-foreground">
          {coarseStatus}
        </div>
      </div>
    );
  }

  const showNumeric = presentation !== "preparing" && presentation !== "slicing";
  const progress = showNumeric ? clampedProgress(print.progress_percent) : null;
  const formattedPrint = {
    print: {
      progress_percent: progress,
      current_layer: showNumeric ? print.current_layer : null,
      total_layers: showNumeric ? print.total_layers : null,
    },
  };

  return (
    <div className="min-w-0" data-testid="printer-print-status">
      <div className="text-xs font-medium text-muted-foreground">
        {t(presentation)}
      </div>
      <div className="mt-0.5 truncate text-sm font-medium text-foreground">
        {printTaskName(print, t("unknownTask"))}
      </div>
      {showNumeric && progress !== null ? (
        <div
          aria-label={t("progress")}
          aria-valuemax={100}
          aria-valuemin={0}
          aria-valuenow={progress}
          className="mt-2 h-1.5 overflow-hidden rounded-full bg-background"
          role="progressbar"
        >
          <div
            className="h-full rounded-full bg-primary"
            style={{ width: `${progress}%` }}
          />
        </div>
      ) : null}
      <div className="mt-1.5 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
        <span>{formatProgress(formattedPrint)}</span>
        <span>{formatLayers(formattedPrint, tJob)}</span>
        <span>
          {formatRemaining(showNumeric ? print.remaining_time_minutes : null, tJob)}
        </span>
      </div>
    </div>
  );
}

function printPresentation(gcodeState: string | null): PrintPresentation | null {
  switch (gcodeState) {
    case "RUNNING":
    case "PRINTING":
      return "printing";
    case "PAUSE":
    case "PAUSED":
      return "paused";
    case "PREPARE":
      return "preparing";
    case "SLICING":
      return "slicing";
    case "FINISH":
      return "finished";
    default:
      return null;
  }
}

function printTaskName(print: PrinterPrintState, fallback: string): string {
  const subtask = print.subtask_name?.trim();
  if (subtask) {
    return subtask;
  }
  const file = print.gcode_file?.trim();
  if (!file) {
    return fallback;
  }
  return file.split(/[\\/]/).at(-1) || fallback;
}

function clampedProgress(progress: number | null): number | null {
  return progress === null ? null : Math.min(100, Math.max(0, progress));
}
