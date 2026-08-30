import type { ArtifactMetadata, Printer } from "./dashboard-types";
import { mixedAmsLiteGlobalTrayId } from "./material-tray-routing";

export type ProjectFilament = {
  mappingIndex: number;
  filamentId: string | null;
  trayInfoIdx: string | null;
  filamentType: string | null;
  color: string | null;
  nozzleId: 0 | 1 | null;
};
export type MaterialToolhead = "L" | "R" | "LR" | null;
export type MaterialSlotKind = "ams" | "external";

export type PrinterAmsSlot = {
  key: string;
  kind: MaterialSlotKind;
  unitId: string;
  unitKind: string | null;
  trayId: string;
  amsId: number;
  slotId: number;
  globalTrayId: number;
  legacyTrayId: number;
  filamentId: string | null;
  settingId: string | null;
  filamentType: string | null;
  name: string | null;
  color: string | null;
  multiColor: string[];
  remainingEstimate: number | null;
  toolhead: MaterialToolhead;
  exists: boolean | null;
  routingRequired: boolean;
  filamentSwitchInstalled: boolean | null;
};

export type AmsMapping2Entry = {
  ams_id: number;
  slot_id: number;
};

export type AmsMappingInfoEntry = {
  ams: number;
  filamentType: string;
  filamentId: string;
  nozzleId: number;
  sourceColor: string;
  targetColor: string;
};

export type SlotIneligibility =
  | "empty"
  | "wrong_nozzle"
  | "unknown_route"
  | "material_type_mismatch"
  | "filament_switch_external"
  | "ams_disabled";

const UNMAPPED = -1;
const MAX_MAPPING_LENGTH = 32;

export function projectFilamentsForPlate(
  metadata: ArtifactMetadata,
  plateId: number,
): ProjectFilament[] {
  const plate = metadata.plates.find(
    (candidate) => candidate.plate_id === plateId,
  );
  if (!plate) return [];

  const used = new Set<number>();
  return plate.filaments.flatMap((filament, position) => {
    if (filament.used_grams === 0 && filament.used_meters === 0) return [];
    const parsedId = Number.parseInt(filament.filament_id ?? "", 10);
    const mappingIndex =
      Number.isInteger(parsedId) && parsedId > 0 ? parsedId - 1 : position;
    if (mappingIndex >= MAX_MAPPING_LENGTH || used.has(mappingIndex)) return [];
    used.add(mappingIndex);
    return [
      {
        mappingIndex,
        filamentId: filament.filament_id,
        trayInfoIdx: filament.tray_info_idx ?? null,
        filamentType: filament.filament_type,
        color: filament.color,
        nozzleId:
          filament.nozzle_id === 0 || filament.nozzle_id === 1
            ? filament.nozzle_id
            : null,
      },
    ];
  });
}

export function printerAmsSlots(
  printer: Pick<Printer, "materials" | "compatibility">,
): PrinterAmsSlot[] {
  const dualNozzle = printer.compatibility.features.dual_nozzle;
  const routingRequired = dualNozzle !== "unsupported";
  const materials = printer.materials;
  if (!materials) return [];
  const filamentSwitchInstalled = materials.filament_switch_installed ?? null;
  const inferConventionalRoutes =
    dualNozzle === "supported" &&
    filamentSwitchInstalled !== true &&
    materials.ams_units.filter(
      (unit) => unit.unit_id === "0" || unit.unit_id === "1",
    ).length === 2;

  const amsSlots = materials.ams_units.flatMap((unit) => {
    const unitId = unit.unit_id ?? "";
    const amsId = Number.parseInt(unitId, 10);
    if (!Number.isInteger(amsId)) return [];

    return (unit.trays ?? []).flatMap((tray) => {
      const trayId = tray.tray_id ?? "";
      const slotId = Number.parseInt(trayId, 10);
      if (!Number.isInteger(slotId)) return [];
      const globalTrayId =
        tray.global_tray_id ??
        mixedAmsLiteGlobalTrayId(unit.unit_kind, slotId) ??
        (amsId < 64
          ? amsId * 4 + slotId
          : amsId >= 128 && amsId <= 135
            ? amsId
            : UNMAPPED);
      return [
        {
          key: "ams:" + unitId + ":" + trayId,
          kind: "ams" as const,
          unitId,
          unitKind: unit.unit_kind ?? null,
          trayId,
          amsId,
          slotId,
          globalTrayId,
          legacyTrayId: globalTrayId,
          filamentId: tray.filament_id ?? null,
          settingId: tray.setting_id ?? null,
          filamentType: tray.type ?? null,
          name: tray.name ?? null,
          color: tray.color ?? null,
          multiColor: tray.multi_color ?? [],
          remainingEstimate: finiteNumber(tray.remaining_estimate),
          toolhead:
            materialToolhead(tray.toolhead ?? unit.toolhead) ??
            (inferConventionalRoutes
              ? unitId === "0"
                ? "R"
                : unitId === "1"
                  ? "L"
                  : null
              : null),
          exists: tray.exists ?? null,
          routingRequired,
          filamentSwitchInstalled,
        },
      ];
    });
  });

  const externalSlots = materials.external_spools.flatMap((spool) => {
    const externalId = Number.parseInt(spool.external_id ?? "", 10);
    if (!Number.isInteger(externalId)) return [];
    const trayId = spool.tray_id ?? "0";
    const inferredToolhead =
      externalId === 254 ? "L" : externalId === 255 ? "R" : null;
    return [
      {
        key: "external:" + externalId,
        kind: "external" as const,
        unitId: String(externalId),
        unitKind: "external",
        trayId,
        amsId: externalId,
        slotId: 0,
        globalTrayId: spool.global_tray_id ?? externalId,
        legacyTrayId: UNMAPPED,
        filamentId: spool.filament_id ?? null,
        settingId: spool.setting_id ?? null,
        filamentType: spool.type ?? null,
        name: spool.name ?? null,
        color: spool.color ?? null,
        multiColor: spool.multi_color ?? [],
        remainingEstimate: finiteNumber(spool.remaining_estimate),
        toolhead: materialToolhead(spool.toolhead) ?? inferredToolhead,
        exists: spool.exists ?? null,
        routingRequired,
        filamentSwitchInstalled,
      },
    ];
  });

  return [...amsSlots, ...externalSlots];
}

export {
  autoMapProjectFilaments,
  autoMapSlotSelections,
  materialMappingPayload,
  slotIneligibility,
} from "./dispatch-material-auto-map";

function materialToolhead(value: string | null | undefined): MaterialToolhead {
  const normalized = value?.trim().toUpperCase();
  return normalized === "L" || normalized === "R" || normalized === "LR"
    ? normalized
    : null;
}

function finiteNumber(value: string | number | null | undefined) {
  if (value === null || value === undefined || value === "") return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}
