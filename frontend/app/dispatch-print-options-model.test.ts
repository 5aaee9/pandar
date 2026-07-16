import { describe, expect, it } from "vitest";

import {
  type CalibrationOption,
  dispatchPrintOptionCapabilities,
  isDualNozzleModel,
} from "./dispatch-print-options-model";

const auto: CalibrationOption = { modes: [2, 1, 0], defaultMode: 2 };
const autoOff: CalibrationOption = { modes: [2, 1, 0], defaultMode: 0 };
const onOff: CalibrationOption = { modes: [1, 0], defaultMode: 1 };

describe("dispatchPrintOptionCapabilities", () => {
  it.each([
    {
      models: ["N6", "X2D", "Bambu Lab X2D"],
      bed: auto,
      flow: auto,
      nozzle: autoOff,
    },
    {
      models: ["N7", "P2S", "O1S", "H2S", "N9", "A2L"],
      bed: auto,
      flow: auto,
      nozzle: null,
    },
    {
      models: [
        "N1",
        "A1_MINI",
        "A1 MINI",
        "A1 Mini",
        "N2S",
        "A1",
        "BL-P001",
        "X1C",
        "BL-P002",
        "X1",
        "C13",
        "X1E",
      ],
      bed: onOff,
      flow: onOff,
      nozzle: null,
    },
    {
      models: ["C11", "P1P", "C12", "P1S"],
      bed: onOff,
      flow: null,
      nozzle: null,
    },
    {
      models: ["O1C", "O1C2", "H2C", "O1D", "H2D", "O1E", "H2DPRO"],
      bed: auto,
      flow: auto,
      nozzle: auto,
    },
  ])(
    "matches the Bambu Studio profile for $models",
    ({ models, bed, flow, nozzle }) => {
      for (const model of models) {
        expect(dispatchPrintOptionCapabilities(model)).toEqual({
          timelapse: true,
          bedLeveling: bed,
          flowCalibration: flow,
          nozzleOffsetCalibration: nozzle,
        });
      }
    },
  );

  it.each([null, "", "Mystery Model"])(
    "uses conservative safe-off capabilities for %s",
    (model) => {
      expect(dispatchPrintOptionCapabilities(model)).toEqual({
        timelapse: false,
        bedLeveling: null,
        flowCalibration: null,
        nozzleOffsetCalibration: null,
      });
    },
  );
});

describe("isDualNozzleModel", () => {
  it.each([
    "N6", "Bambu Lab X2D", "O1C", "O1C2", "H2C", "O1D", "H2D", "O1E", "H2D Pro",
  ])("recognizes Studio dual-nozzle model %s", (model) => {
    expect(isDualNozzleModel(model)).toBe(true);
  });

  it.each([null, "", "P1S", "Mystery Model"])("rejects non-dual model %s", (model) => {
    expect(isDualNozzleModel(model)).toBe(false);
  });
});
