import { describe, expect, it } from "vitest";

import type { ArtifactMetadata, Printer } from "./dashboard-types";
import { printerCompatibility } from "./printer-compatibility.test-utils";
import {
  autoMapProjectFilaments,
  autoMapSlotSelections,
  materialMappingPayload,
  printerAmsSlots,
  projectFilamentsForPlate,
  slotIneligibility,
  type PrinterAmsSlot,
  type ProjectFilament,
} from "./dispatch-material-mapping";

describe("dispatch material mapping", () => {
  it("extracts used plate materials with the sliced nozzle assignment", () => {
    expect(projectFilamentsForPlate(fixtureMetadata(), 1)).toEqual([
      {
        color: "#000000",
        filamentId: "1",
        filamentType: "PLA",
        mappingIndex: 0,
        nozzleId: 1,
        trayInfoIdx: "GFA00",
      },
      {
        color: "#FFFF00",
        filamentId: "2",
        filamentType: "PETG",
        mappingIndex: 1,
        nozzleId: 0,
        trayInfoIdx: "GFG00",
      },
    ]);
  });

  it("keeps AMS colors, routes, empty slots, and external spools for the custom picker", () => {
    const slots = printerAmsSlots(fixturePrinter());

    expect(slots).toHaveLength(7);
    expect(slots[0]).toMatchObject({
      key: "ams:0:0",
      kind: "ams",
      toolhead: "R",
      filamentType: "PLA",
      color: "FF0000",
      multiColor: ["FF0000", "FFFFFF"],
      remainingEstimate: 72,
    });
    expect(slots[2]).toMatchObject({
      key: "ams:0:2",
      exists: false,
    });
    expect(slots.at(-2)).toMatchObject({
      key: "external:254",
      kind: "external",
      amsId: 254,
      legacyTrayId: -1,
      toolhead: "L",
    });
    expect(slots.at(-1)).toMatchObject({
      trayId: "1",
      slotId: 0,
    });
  });

  it("rejects nozzle assignments when a legacy payload lacks routing capabilities", () => {
    const printer = fixturePrinter();
    delete printer.compatibility;
    const slots = printerAmsSlots(printer);
    const filament = projectFilamentsForPlate(fixtureMetadata(), 1)[0];

    expect(
      slots
        .filter((candidate) => candidate.kind === "ams")
        .every(
          (candidate) =>
            candidate.routingRequired && candidate.toolhead === null,
        ),
    ).toBe(true);
    expect(autoMapSlotSelections([filament], slots)).toEqual(new Map());
  });

  it("applies Studio side, type, empty-slot, and external rules", () => {
    const filament = projectFilamentsForPlate(fixtureMetadata(), 1)[0];
    const slots = printerAmsSlots(fixturePrinter());
    const byKey = new Map(slots.map((slot) => [slot.key, slot]));

    expect(
      slotIneligibility(filament, byKey.get("ams:1:0")!, slots),
    ).toBeNull();
    expect(slotIneligibility(filament, byKey.get("ams:0:0")!, slots)).toBe(
      "wrong_nozzle",
    );
    expect(slotIneligibility(filament, byKey.get("ams:1:1")!, slots)).toBe(
      "material_type_mismatch",
    );
    expect(slotIneligibility(filament, byKey.get("ams:0:2")!, slots)).toBe(
      "empty",
    );
    expect(
      slotIneligibility(filament, byKey.get("external:254")!, slots),
    ).toBeNull();
    expect(slotIneligibility(filament, byKey.get("external:255")!, slots)).toBe(
      "wrong_nozzle",
    );
    const unknownRoute = slot({
      key: "ams:3:0",
      toolhead: null,
      routingRequired: true,
    });
    expect(slotIneligibility(filament, unknownRoute, [unknownRoute])).toBe(
      "unknown_route",
    );
    const unknownSwitchExternal = slot({
      amsId: 254,
      filamentSwitchInstalled: null,
      key: "external:254",
      kind: "external",
      legacyTrayId: -1,
      toolhead: "L",
      unitId: "254",
    });
    expect(
      slotIneligibility(filament, unknownSwitchExternal, [
        unknownSwitchExternal,
      ]),
    ).toBe("unknown_route");
  });

  it("auto maps only compatible material sources on the sliced nozzle side", () => {
    const filaments = projectFilamentsForPlate(fixtureMetadata(), 1);
    const slots = printerAmsSlots(fixturePrinter());

    expect(autoMapProjectFilaments(filaments, slots)).toEqual([4, 1]);
    expect(autoMapSlotSelections(filaments, slots)).toEqual(
      new Map([
        [0, "ams:1:0"],
        [1, "ams:0:1"],
      ]),
    );
  });

  it("prefers a compatible AMS over a closer external spool for auto mapping", () => {
    const filament = {
      ...projectFilamentsForPlate(fixtureMetadata(), 1)[0],
      color: "#0000FF",
      nozzleId: 0 as const,
      trayInfoIdx: null,
    };

    expect(
      autoMapSlotSelections([filament], printerAmsSlots(fixturePrinter())),
    ).toEqual(new Map([[0, "ams:0:0"]]));
  });

  it("builds all three Studio mapping payloads with sliced nozzle ids and colors", () => {
    const filaments = projectFilamentsForPlate(fixtureMetadata(), 1);
    const slots = printerAmsSlots(fixturePrinter());
    const payload = materialMappingPayload(
      filaments,
      slots,
      autoMapSlotSelections(filaments, slots),
    );

    expect(payload).toEqual({
      amsMapping: [4, 1],
      amsMapping2: [
        { ams_id: 1, slot_id: 0 },
        { ams_id: 0, slot_id: 1 },
      ],
      amsMappingInfo: [
        {
          ams: 4,
          filamentType: "PLA",
          filamentId: "GFA00",
          nozzleId: 1,
          sourceColor: "#000000FF",
          targetColor: "#000000FF",
        },
        {
          ams: 1,
          filamentType: "PETG",
          filamentId: "GFG00",
          nozzleId: 0,
          sourceColor: "#FFFF00FF",
          targetColor: "#FFFF00FF",
        },
      ],
      externalTypeMismatch: false,
      mappingValid: true,
      usesAms: true,
    });
  });

  it("preserves external identity in mapping2/info and warns on a type mismatch", () => {
    const filament = projectFilamentsForPlate(fixtureMetadata(), 1)[0];
    const slots = printerAmsSlots(fixturePrinter());
    const payload = materialMappingPayload(
      [filament],
      slots,
      new Map([[filament.mappingIndex, "external:254"]]),
    );

    expect(payload.amsMapping).toEqual([-1]);
    expect(payload.amsMapping2).toEqual([{ ams_id: 254, slot_id: 0 }]);
    expect(payload.amsMappingInfo?.[0]).toMatchObject({
      ams: 254,
      nozzleId: 1,
    });
    expect(payload.externalTypeMismatch).toBe(true);
    expect(payload.usesAms).toBe(false);
  });

  it("omits mapping info when sparse or legacy metadata lacks a nozzle assignment", () => {
    const slots = printerAmsSlots(fixturePrinter());
    const sparse: ProjectFilament[] = [
      filament(0, "PLA", "#000000", 1),
      filament(2, "PETG", "#FFFF00", 0),
    ];

    const payload = materialMappingPayload(
      sparse,
      slots,
      autoMapSlotSelections(sparse, slots),
    );
    expect(payload.amsMapping).toEqual([4, -1, 1]);
    expect(payload.amsMappingInfo).toBeNull();

    const legacyPayload = materialMappingPayload(
      [{ ...sparse[0], nozzleId: null }],
      slots,
      new Map([[0, "ams:1:0"]]),
    );
    expect(legacyPayload.amsMappingInfo).toBeNull();
    expect(legacyPayload.mappingValid).toBe(false);
    expect(
      materialMappingPayload([sparse[0]], slots, new Map()).mappingValid,
    ).toBe(false);
  });

  it("maps only external spools when Use AMS is off and derives use_ams false", () => {
    const filaments = projectFilamentsForPlate(fixtureMetadata(), 1);
    const slots = printerAmsSlots(fixturePrinter());
    const selections = autoMapSlotSelections(filaments, slots, false);

    expect(selections).toEqual(
      new Map([
        [0, "external:254"],
        [1, "external:255"],
      ]),
    );
    expect(slotIneligibility(filaments[0], slots[0], slots, false)).toBe(
      "ams_disabled",
    );
    expect(
      materialMappingPayload(filaments, slots, selections, false),
    ).toMatchObject({
      amsMapping: [-1, -1],
      amsMapping2: [
        { ams_id: 254, slot_id: 0 },
        { ams_id: 255, slot_id: 0 },
      ],
      externalTypeMismatch: true,
      mappingValid: true,
      usesAms: false,
    });
  });
  it("allows every routed AMS but disables external spools when the switch is installed", () => {
    const filament = projectFilamentsForPlate(fixtureMetadata(), 1)[0];
    const rightAms = slot({
      filamentSwitchInstalled: true,
      key: "ams:0:0",
      toolhead: "R",
    });
    const external = slot({
      amsId: 254,
      filamentSwitchInstalled: true,
      globalTrayId: 254,
      key: "external:254",
      kind: "external",
      legacyTrayId: -1,
      toolhead: "L",
      unitId: "254",
    });
    const slots = [rightAms, external];

    expect(slotIneligibility(filament, rightAms, slots)).toBeNull();
    expect(slotIneligibility(filament, external, slots)).toBe(
      "filament_switch_external",
    );
  });

  it("ranks the project filament id independently from a slot setting id", () => {
    const filament = {
      ...projectFilamentsForPlate(fixtureMetadata(), 1)[0],
      color: "#FF0000",
    };
    const matchingFilament = slot({
      key: "ams:1:0",
      color: "0000FF",
      filamentId: "GFA00",
      settingId: "GFSA00_01",
    });
    const matchingColor = slot({
      key: "ams:1:1",
      trayId: "1",
      slotId: 1,
      globalTrayId: 1,
      legacyTrayId: 1,
      color: "FF0000",
      filamentId: "GFA99",
      settingId: "GFA00",
    });

    expect(
      autoMapSlotSelections([filament], [matchingFilament, matchingColor]),
    ).toEqual(new Map([[0, "ams:1:0"]]));
  });

  it("uses the AMS-HT unit id as its legacy flat mapping value", () => {
    const printer = fixturePrinter();
    printer.materials?.ams_units.push({
      unit_id: "128",
      unit_kind: "ams_ht",
      toolhead: "L",
      trays: [{ tray_id: "0", type: "PLA", color: "000000", exists: true }],
    });

    expect(
      printerAmsSlots(printer).find((candidate) => candidate.amsId === 128),
    ).toMatchObject({ globalTrayId: 128, legacyTrayId: 128, slotId: 0 });
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
          {
            filament_id: "1",
            tray_info_idx: "GFA00",
            nozzle_id: 1,
            filament_type: "PLA",
            color: "#000000",
            used_grams: 10,
            used_meters: 3,
          },
          {
            filament_id: "2",
            tray_info_idx: "GFG00",
            nozzle_id: 0,
            filament_type: "PETG",
            color: "#FFFF00",
            used_grams: 4,
            used_meters: 1,
          },
        ],
        has_thumbnail: false,
      },
    ],
  };
}

function fixturePrinter(): Printer {
  return {
    id: "printer-1",
    tenant_id: "tenant-1",
    agent_id: "agent-1",
    serial_number: "SN1",
    name: "Printer One",
    model: "Bambu Lab X2D",
    compatibility: printerCompatibility("x2d"),
    status: "idle",
    last_seen_at: "2026-07-15T00:00:00Z",
    created_at: "2026-07-15T00:00:00Z",
    materials: {
      filament_switch_installed: false,
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
              multi_color: ["FF0000", "FFFFFF"],
              remaining_estimate: 72,
              exists: true,
            },
            {
              tray_id: "1",
              global_tray_id: 1,
              type: "PETG",
              color: "FFFF00",
              filament_id: "GFG00",
              exists: true,
            },
            {
              tray_id: "2",
              global_tray_id: 2,
              type: null,
              color: null,
              exists: false,
            },
          ],
        },
        {
          unit_id: "1",
          trays: [
            {
              tray_id: "0",
              global_tray_id: 4,
              type: "PLA",
              color: "000000",
              filament_id: "GFA00",
              exists: true,
            },
            {
              tray_id: "1",
              global_tray_id: 5,
              type: "ABS",
              color: "0000FF",
              exists: true,
            },
          ],
        },
      ],
      external_spools: [
        {
          external_id: "254",
          tray_id: "0",
          type: "ABS",
          color: "111111",
          exists: true,
        },
        {
          external_id: "255",
          tray_id: "1",
          type: "PLA",
          color: "0000FF",
          exists: true,
        },
      ],
    },
  };
}

function filament(
  mappingIndex: number,
  filamentType: string,
  color: string,
  nozzleId: 0 | 1,
): ProjectFilament {
  return {
    mappingIndex,
    filamentId: String(mappingIndex + 1),
    trayInfoIdx: null,
    filamentType,
    color,
    nozzleId,
  };
}

function slot(overrides: Partial<PrinterAmsSlot> = {}): PrinterAmsSlot {
  return {
    key: "ams:0:0",
    kind: "ams",
    unitId: "0",
    unitKind: "ams",
    trayId: "0",
    amsId: 0,
    slotId: 0,
    globalTrayId: 0,
    legacyTrayId: 0,
    filamentId: "GFA00",
    settingId: null,
    filamentType: "PLA",
    name: null,
    color: "000000",
    multiColor: [],
    remainingEstimate: null,
    toolhead: "L",
    exists: true,
    routingRequired: true,
    filamentSwitchInstalled: null,
    ...overrides,
  };
}
