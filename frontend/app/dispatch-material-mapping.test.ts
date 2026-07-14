import { describe, expect, it } from "vitest";

import type { ArtifactMetadata, Printer } from "./dashboard-types";
import {
  autoMapProjectFilaments,
  autoMapSlotSelections,
  materialMappingPayload,
  printerAmsSlots,
  projectFilamentsForPlate,
} from "./dispatch-material-mapping";

describe("dispatch material mapping", () => {
  it("extracts every material required by the selected plate", () => {
    const metadata = fixtureMetadata();

    expect(projectFilamentsForPlate(metadata, 1)).toEqual([
      { color: "#ff0000", filamentId: "1", filamentType: "PLA", mappingIndex: 0, trayInfoIdx: null },
      { color: "#0000ff", filamentId: "2", filamentType: "PLA", mappingIndex: 1, trayInfoIdx: null },
      { color: "#111111", filamentId: "3", filamentType: "ABS", mappingIndex: 2, trayInfoIdx: null },
    ]);
    expect(projectFilamentsForPlate(metadata, 2)).toEqual([
      { color: "#00ff00", filamentId: "1", filamentType: "PETG", mappingIndex: 0, trayInfoIdx: null },
    ]);
  });

  it("offers loaded AMS slots but excludes external spools", () => {
    expect(printerAmsSlots(fixturePrinter())).toEqual([
      {
        amsId: 0,
        color: "FF0000",
        filamentId: "GFL00",
        filamentType: "PLA",
        globalTrayId: 0,
        key: "0:0",
        slotId: 0,
        trayId: "0",
        unitId: "0",
      },
      {
        amsId: 0,
        color: "0000FF",
        filamentId: "GFL01",
        filamentType: "PLA",
        globalTrayId: 1,
        key: "0:1",
        slotId: 1,
        trayId: "1",
        unitId: "0",
      },
    ]);
  });

  it("uses the AMS-HT unit id as its flat mapping value", () => {
    const printer = fixturePrinter();
    printer.materials?.ams_units.push({
      unit_id: "128",
      trays: [{ tray_id: "0", type: "PETG", color: "00FF00", exists: true }],
    });

    expect(printerAmsSlots(printer).at(-1)).toMatchObject({
      amsId: 128,
      slotId: 0,
      globalTrayId: 128,
    });
  });

  it("matches the same material type by color and leaves incompatible materials unmapped", () => {
    const filaments = projectFilamentsForPlate(fixtureMetadata(), 1);
    const slots = printerAmsSlots(fixturePrinter());

    expect(autoMapProjectFilaments(filaments, slots)).toEqual([0, 1, -1]);
  });

  it("prefers distinct slots and reuses the nearest compatible slot only when needed", () => {
    const filaments = [
      { color: "#ff0000", filamentId: "1", filamentType: "PLA", mappingIndex: 0, trayInfoIdx: null },
      { color: "#ee0000", filamentId: "2", filamentType: "PLA", mappingIndex: 1, trayInfoIdx: null },
      { color: "#dd0000", filamentId: "3", filamentType: "PLA", mappingIndex: 2, trayInfoIdx: null },
    ];
    const slots = [
      {
        amsId: 0,
        color: "FF0000",
        filamentId: "GFL00",
        filamentType: "PLA",
        globalTrayId: 0,
        key: "0:0",
        slotId: 0,
        trayId: "0",
        unitId: "0",
      },
      {
        amsId: 0,
        color: "DD0000",
        filamentId: "GFL01",
        filamentType: "PLA",
        globalTrayId: 1,
        key: "0:1",
        slotId: 1,
        trayId: "1",
        unitId: "0",
      },
    ];

    expect(autoMapProjectFilaments(filaments, slots)).toEqual([0, 0, 1]);
  });

  it("preserves one-based project filament gaps in flat and mapping2 payloads", () => {
    const filaments = [
      { color: "#ff0000", filamentId: "1", filamentType: "PLA", mappingIndex: 0, trayInfoIdx: null },
      { color: "#0000ff", filamentId: "3", filamentType: "PLA", mappingIndex: 2, trayInfoIdx: null },
    ];
    const slots = printerAmsSlots(fixturePrinter());

    expect(
      materialMappingPayload(filaments, slots, autoMapSlotSelections(filaments, slots)),
    ).toEqual({
      amsMapping: [0, -1, 1],
      amsMapping2: [
        { ams_id: 0, slot_id: 0 },
        { ams_id: 255, slot_id: 255 },
        { ams_id: 0, slot_id: 1 },
      ],
    });
  });

  it("prefers the Bambu preset id before a closer same-type color", () => {
    const filament = {
      color: "#ff0000",
      filamentId: "1",
      filamentType: "PLA",
      mappingIndex: 0,
      trayInfoIdx: "GFA00",
    };
    const slots = printerAmsSlots(fixturePrinter());
    slots[0] = { ...slots[0], color: "0000FF", filamentId: "GFA00" };
    slots[1] = { ...slots[1], color: "FF0000", filamentId: "GFA01" };

    expect(autoMapProjectFilaments([filament], slots)).toEqual([0]);
  });
});

function fixtureMetadata(): ArtifactMetadata {
  return {
    display_name: "project",
    default_plate_id: 1,
    warnings: [],
    plates: [
      {
        plate_id: 1,
        name: "Plate 1",
        estimated_time_seconds: null,
        filament_weight_grams: null,
        object_count: 1,
        objects: ["part"],
        filaments: [
          filament("1", "PLA", "#ff0000"),
          filament("2", "PLA", "#0000ff"),
          filament("3", "ABS", "#111111"),
          filament("4", "PETG", "#00ff00", 0, 0),
        ],
        has_thumbnail: false,
      },
      {
        plate_id: 2,
        name: "Plate 2",
        estimated_time_seconds: null,
        filament_weight_grams: null,
        object_count: 1,
        objects: ["other"],
        filaments: [filament("1", "PETG", "#00ff00")],
        has_thumbnail: false,
      },
    ],
  };
}

function filament(
  filamentId: string,
  filamentType: string,
  color: string,
  usedGrams: number | null = null,
  usedMeters: number | null = null,
) {
  return {
    filament_id: filamentId,
    filament_type: filamentType,
    color,
    used_grams: usedGrams,
    used_meters: usedMeters,
  };
}

function fixturePrinter(): Printer {
  return {
    id: "printer-1",
    tenant_id: "tenant-1",
    agent_id: "agent-1",
    serial_number: "SN1",
    name: "Printer One",
    model: "X1C",
    status: "idle",
    last_seen_at: "2026-07-15T00:00:00Z",
    created_at: "2026-07-15T00:00:00Z",
    materials: {
      observed_at: "2026-07-15T00:00:00Z",
      active_tray: null,
      ams_units: [
        {
          unit_id: "0",
          trays: [
            {
              tray_id: "0",
              global_tray_id: 0,
              type: "PLA",
              color: "FF0000",
              filament_id: "GFL00",
              exists: true,
            },
            {
              tray_id: "1",
              type: "PLA",
              color: "0000FF",
              filament_id: "GFL01",
              exists: true,
            },
            {
              tray_id: "2",
              type: "ABS",
              color: "111111",
              filament_id: "GFL02",
              exists: false,
            },
          ],
        },
      ],
      external_spools: [
        {
          external_id: "254",
          tray_id: "0",
          type: "PLA",
          color: "00FF00",
          exists: true,
        },
      ],
    },
  };
}
