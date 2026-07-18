import { describe, expect, it } from "vitest";

import type { Printer } from "./dashboard-types";
import {
  availablePlateMismatchActions,
  plateMismatchActions,
} from "./plate-mismatch-actions";
import type { PrinterPrintState } from "./printer-live-types";

const print: PrinterPrintState = {
  task_generation: 1,
  error_generation: 9,
  hms: [],
  job_state: 1,
  gcode_state: "PAUSE",
  task_id: null,
  subtask_id: null,
  subtask_name: null,
  gcode_file: null,
  progress_percent: null,
  remaining_time_minutes: null,
  current_layer: null,
  total_layers: null,
  print_error: 83_918_929,
  printer_job_id: "native-job",
};

const printer: Printer = {
  id: "printer-1",
  tenant_id: "tenant-1",
  agent_id: "agent-1",
  serial_number: "20P123",
  name: "Office A1",
  model: "A1",
  status: "running",
  last_seen_at: "2026-07-10T00:00:00Z",
  created_at: "2026-07-10T00:00:00Z",
  materials: null,
  print,
};

const additionalPlateRecoveryCatalog = [
  [83_918_945, ["093", "094", "20P", "22E", "239", "31B"], ["ignore", "resume"]],
  [83_918_988, ["093", "094", "20P", "22E", "239", "31B"], ["ignore", "resume"]],
  [83_919_003, ["093", "094", "20P", "22E", "239", "31B"], ["ignore", "resume"]],
  [83_919_008, ["093", "094", "20P", "239", "31B"], ["resume", "ignore"]],
] as const;

describe("plateMismatchActions", () => {
  it.each(["093", "094", "20P", "22E", "239", "31B"])(
    "uses native Resume, Ignore, Stop order for family %s",
    (family) => {
      expect(plateMismatchActions(`${family.toLowerCase()}-serial`, print)).toEqual([
        "resume",
        "ignore",
        "stop",
      ]);
    },
  );

  it.each(additionalPlateRecoveryCatalog)(
    "uses the Studio runtime action catalog for plate error %s",
    (printError, families, expectedActions) => {
      for (const family of families) {
        expect(
          plateMismatchActions(`${family.toLowerCase()}-serial`, {
            ...print,
            print_error: printError,
          }),
        ).toEqual(expectedActions);
      }
    },
  );

  it("does not invent visual encoder actions for the unsupported 22E family", () => {
    expect(
      plateMismatchActions("22E123", { ...print, print_error: 83_919_008 }),
    ).toEqual([]);
  });

  it.each(["093", "094", "20P", "22E", "239", "31B"])(
    "uses native Ignore, Resume order for build plate marker errors on family %s",
    (family) => {
      expect(
        plateMismatchActions(`${family.toLowerCase()}-serial`, {
          ...print,
          print_error: 83_918_946,
        }),
      ).toEqual(["ignore", "resume"]);
    },
  );

  it.each(["26A123", "XYZ123", "", "20"])(
    "has no inferred actions for unsupported serial %s",
    (serialNumber) => {
      expect(plateMismatchActions(serialNumber, print)).toEqual([]);
    },
  );

  it.each([0, 1])("allows Resume and Ignore for job_state %s", (jobState) => {
    expect(plateMismatchActions("20P123", { ...print, job_state: jobState })).toEqual([
      "resume",
      "ignore",
      "stop",
    ]);
  });

  it.each([null, 2, 15])("retains only cataloged Stop for unsafe job_state %s", (jobState) => {
    expect(plateMismatchActions("20P123", { ...print, job_state: jobState })).toEqual([
      "stop",
    ]);
  });

  it.each(["PREPARE", "SLICING", "RUNNING", "PAUSE"])(
    "permits only exact native active state %s",
    (gcodeState) => {
      expect(
        plateMismatchActions("20P123", { ...print, gcode_state: gcodeState }),
      ).toEqual(["resume", "ignore", "stop"]);
    },
  );

  it.each(["PRINTING", "PAUSED", "FINISH", "FAILED", "IDLE", "", null])(
    "does not widen recovery eligibility for state %s",
    (gcodeState) => {
      expect(
        plateMismatchActions("20P123", { ...print, gcode_state: gcodeState }),
      ).toEqual([]);
    },
  );

  it("requires the exact numeric mismatch error", () => {
    expect(
      plateMismatchActions("20P123", { ...print, print_error: 83_918_928 }),
    ).toEqual([]);
    expect(plateMismatchActions("20P123", { ...print, print_error: null })).toEqual([]);
  });

  it.each(["IDLE", "offline", "Failed"])(
    "applies coarse %s as a complete UI veto",
    (status) => {
      expect(availablePlateMismatchActions({ ...printer, status })).toEqual([]);
    },
  );

  it("leaves native actions available for non-vetoed coarse state", () => {
    expect(availablePlateMismatchActions(printer)).toEqual([
      "resume",
      "ignore",
      "stop",
    ]);
  });
});
