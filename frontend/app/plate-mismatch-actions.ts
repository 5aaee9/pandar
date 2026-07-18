import type { Printer } from "./dashboard-types";
import type { PrinterPrintState } from "./printer-live-types";

export const BUILD_PLATE_MISMATCH = 83_918_929;
export const BUILD_PLATE_NOT_DETECTED = 83_918_945;
export const BUILD_PLATE_MARKER_NOT_DETECTED = 83_918_946;
export const BUILD_PLATE_OFFSET = 83_918_988;
export const BUILD_PLATE_COLLISION_RISK = 83_919_003;
export const VISUAL_ENCODER_BOARD_NOT_DETECTED = 83_919_008;

export type PlateMismatchAction = "resume" | "ignore" | "stop";
export type PlateRecoveryIssueKind =
  | "mismatch"
  | "missing"
  | "marker-not-detected"
  | "misaligned"
  | "misaligned-with-debris"
  | "collision-risk"
  | "encoder-board-missing";

export type PlateRecoveryIssue = {
  code: string;
  kind: PlateRecoveryIssueKind;
};

const SUPPORTED_FAMILIES = new Set(["093", "094", "20P", "22E", "239", "31B"]);
const NATIVE_ACTIVE_STATES = new Set(["PREPARE", "SLICING", "RUNNING", "PAUSE"]);
const COARSE_VETO = new Set(["IDLE", "OFFLINE", "FAILED"]);

export function plateMismatchActions(
  serialNumber: string,
  print: PrinterPrintState,
): PlateMismatchAction[] {
  const family = serialNumber.slice(0, 3).toUpperCase();
  const nativeActions = studioPlateRecoveryActions(family, print.print_error);
  if (
    nativeActions.length === 0 ||
    !print.gcode_state ||
    !NATIVE_ACTIVE_STATES.has(print.gcode_state)
  ) {
    return [];
  }

  return print.job_state !== null && print.job_state <= 1
    ? nativeActions
    : nativeActions.filter((action) => action === "stop");
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

export function plateRecoveryIssue(
  serialNumber: string,
  printError: number | null | undefined,
): PlateRecoveryIssue | null {
  const family = serialNumber.slice(0, 3).toUpperCase();
  const kind = recoveryIssueKind(family, printError);
  return kind && printError !== null && printError !== undefined
    ? { code: formatPrintErrorCode(printError), kind }
    : null;
}

function studioPlateRecoveryActions(
  family: string,
  printError: number | null,
): PlateMismatchAction[] {
  if (!SUPPORTED_FAMILIES.has(family)) {
    return [];
  }
  switch (printError) {
    case BUILD_PLATE_MISMATCH:
      return ["resume", "ignore", "stop"];
    case BUILD_PLATE_NOT_DETECTED:
    case BUILD_PLATE_MARKER_NOT_DETECTED:
    case BUILD_PLATE_OFFSET:
    case BUILD_PLATE_COLLISION_RISK:
      return ["ignore", "resume"];
    case VISUAL_ENCODER_BOARD_NOT_DETECTED:
      return family === "22E" ? [] : ["resume", "ignore"];
    default:
      return [];
  }
}

function recoveryIssueKind(
  family: string,
  printError: number | null | undefined,
): PlateRecoveryIssueKind | null {
  switch (printError) {
    case BUILD_PLATE_MISMATCH:
      return "mismatch";
    case BUILD_PLATE_NOT_DETECTED:
      return "missing";
    case BUILD_PLATE_MARKER_NOT_DETECTED:
      return "marker-not-detected";
    case BUILD_PLATE_OFFSET:
      return family === "31B" ? "misaligned-with-debris" : "misaligned";
    case BUILD_PLATE_COLLISION_RISK:
      return "collision-risk";
    case VISUAL_ENCODER_BOARD_NOT_DETECTED:
      return "encoder-board-missing";
    default:
      return null;
  }
}

function formatPrintErrorCode(printError: number): string {
  const code = printError.toString(16).toUpperCase().padStart(8, "0");
  return `${code.slice(0, 4)}-${code.slice(4)}`;
}
