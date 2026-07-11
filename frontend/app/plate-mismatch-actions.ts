import type { Printer } from "./dashboard-types";
import type { PrinterPrintState } from "./printer-live-types";

export const BUILD_PLATE_MISMATCH = 83_918_929;

export type PlateMismatchAction = "resume" | "ignore" | "stop";

const SUPPORTED_FAMILIES = new Set(["093", "094", "20P", "22E", "239", "31B"]);
const NATIVE_ACTIVE_STATES = new Set(["PREPARE", "SLICING", "RUNNING", "PAUSE"]);
const COARSE_VETO = new Set(["IDLE", "OFFLINE", "FAILED"]);

export function plateMismatchActions(
  serialNumber: string,
  print: PrinterPrintState,
): PlateMismatchAction[] {
  const family = serialNumber.slice(0, 3).toUpperCase();
  if (
    print.print_error !== BUILD_PLATE_MISMATCH ||
    !SUPPORTED_FAMILIES.has(family) ||
    !print.gcode_state ||
    !NATIVE_ACTIVE_STATES.has(print.gcode_state)
  ) {
    return [];
  }

  return print.job_state !== null && print.job_state <= 1
    ? ["resume", "ignore", "stop"]
    : ["stop"];
}

export function availablePlateMismatchActions(
  printer: Printer,
): PlateMismatchAction[] {
  if (
    COARSE_VETO.has(printer.status.toUpperCase()) ||
    !printer.print ||
    printer.print.error_generation <= 0
  ) {
    return [];
  }
  return plateMismatchActions(printer.serial_number, printer.print);
}
