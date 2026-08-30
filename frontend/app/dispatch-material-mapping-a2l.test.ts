import { describe, expect, it } from "vitest";

import type { Printer } from "./dashboard-types";
import { printerCompatibility } from "./printer-compatibility.test-utils";
import {
  materialMappingPayload,
  printerAmsSlots,
  type ProjectFilament,
} from "./dispatch-material-mapping";

describe("A2L mixed AMS Lite material mapping", () => {
  it("uses Studio global tray ids 24 through 27 instead of conventional unit offsets", () => {
    const slots = printerAmsSlots(a2lPrinter());

    expect(slots.map((slot) => slot.globalTrayId)).toEqual([24, 25, 26, 27]);
    expect(slots.map((slot) => slot.legacyTrayId)).toEqual([24, 25, 26, 27]);

    const filament: ProjectFilament = {
      mappingIndex: 0,
      filamentId: "1",
      trayInfoIdx: "GFA00",
      filamentType: "PLA",
      color: "#FF0000",
      nozzleId: null,
    };
    const payload = materialMappingPayload(
      [filament],
      slots,
      new Map([[0, "ams:0:0"]]),
    );

    expect(payload.amsMapping).toEqual([24]);
    expect(payload.amsMapping2).toEqual([{ ams_id: 0, slot_id: 0 }]);
    expect(payload.mappingValid).toBe(true);
    expect(payload.usesAms).toBe(true);
  });
});

function a2lPrinter(): Pick<Printer, "materials" | "compatibility"> {
  return {
    compatibility: printerCompatibility("a2l"),
    materials: {
      observed_at: "2026-08-01T00:00:00Z",
      active_tray: null,
      external_spools: [],
      ams_units: [
        {
          unit_id: "0",
          unit_kind: "ams_lite_mixed",
          trays: Array.from({ length: 4 }, (_, slot) => ({
            tray_id: String(slot),
            type: slot === 0 ? "PLA" : "PETG",
            color: slot === 0 ? "FF0000" : "00FF00",
            exists: true,
          })),
        },
      ],
    },
  };
}
