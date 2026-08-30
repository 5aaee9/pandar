import { describe, expect, it } from "vitest";

import type { PrinterCompatibility } from "./printer-compatibility";
import { dispatchPrintOptionCapabilities } from "./dispatch-print-options-model";

const compatibility: PrinterCompatibility = {
  normalized_model: "X2D",
  external_storage: "unknown",
  ftps_tls_1_2_cap: true,
  features: {
    chamber_temperature: "unknown",
    drying: "unknown",
    dual_nozzle: "supported",
    flow_calibration: "supported",
    vibration_calibration: "unknown",
    nozzle_offset_calibration: "supported",
    live_controls: "supported",
  },
  print_options: {
    timelapse: true,
    bed_leveling: { modes: [2, 1, 0], default_mode: 2 },
    flow_calibration: { modes: [1, 0], default_mode: 1 },
    nozzle_offset_calibration: { modes: [2, 1, 0], default_mode: 0 },
  },
  chamber_fan: "supported",
  nozzle_layout: "main_auxiliary",
};

describe("dispatchPrintOptionCapabilities", () => {
  it("maps the canonical server projection to view properties", () => {
    expect(dispatchPrintOptionCapabilities(compatibility)).toEqual({
      timelapse: true,
      bedLeveling: { modes: [2, 1, 0], defaultMode: 2 },
      flowCalibration: { modes: [1, 0], defaultMode: 1 },
      nozzleOffsetCalibration: { modes: [2, 1, 0], defaultMode: 0 },
    });
  });

  it.each([null, undefined])(
    "uses conservative safe-off capabilities without a projection",
    (projection) => {
      expect(dispatchPrintOptionCapabilities(projection)).toEqual({
        timelapse: false,
        bedLeveling: null,
        flowCalibration: null,
        nozzleOffsetCalibration: null,
      });
    },
  );
});
