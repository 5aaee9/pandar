import type {
  CalibrationOptionCapability,
  PrinterCompatibility,
} from "./printer-compatibility";

export type CalibrationMode = 0 | 1 | 2;

export type CalibrationOption = {
  modes: readonly CalibrationMode[];
  defaultMode: CalibrationMode;
};

export type DispatchPrintOptionCapabilities = {
  timelapse: boolean;
  bedLeveling: CalibrationOption | null;
  flowCalibration: CalibrationOption | null;
  nozzleOffsetCalibration: CalibrationOption | null;
};

const unknown: DispatchPrintOptionCapabilities = {
  timelapse: false,
  bedLeveling: null,
  flowCalibration: null,
  nozzleOffsetCalibration: null,
};

export function dispatchPrintOptionCapabilities(
  compatibility: PrinterCompatibility | null | undefined,
): DispatchPrintOptionCapabilities {
  if (!compatibility) return unknown;
  const options = compatibility.print_options;
  return {
    timelapse: options.timelapse,
    bedLeveling: calibrationOption(options.bed_leveling),
    flowCalibration: calibrationOption(options.flow_calibration),
    nozzleOffsetCalibration: calibrationOption(
      options.nozzle_offset_calibration,
    ),
  };
}

function calibrationOption(
  option: CalibrationOptionCapability | null,
): CalibrationOption | null {
  return option
    ? { modes: option.modes, defaultMode: option.default_mode }
    : null;
}
