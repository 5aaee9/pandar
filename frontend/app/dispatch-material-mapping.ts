import type { ArtifactMetadata, Printer } from "./dashboard-types";
import { isDualNozzleModel } from "./dispatch-print-options-model";

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
const UNMAPPED_MAPPING2 = 255;
const MAX_MAPPING_LENGTH = 32;

export function projectFilamentsForPlate(
  metadata: ArtifactMetadata,
  plateId: number,
): ProjectFilament[] {
  const plate = metadata.plates.find((candidate) => candidate.plate_id === plateId);
  if (!plate) return [];

  const used = new Set<number>();
  return plate.filaments.flatMap((filament, position) => {
    if (filament.used_grams === 0 && filament.used_meters === 0) return [];
    const parsedId = Number.parseInt(filament.filament_id ?? "", 10);
    const mappingIndex = Number.isInteger(parsedId) && parsedId > 0 ? parsedId - 1 : position;
    if (mappingIndex >= MAX_MAPPING_LENGTH || used.has(mappingIndex)) return [];
    used.add(mappingIndex);
    return [{
      mappingIndex,
      filamentId: filament.filament_id,
      trayInfoIdx: filament.tray_info_idx ?? null,
      filamentType: filament.filament_type,
      color: filament.color,
      nozzleId: filament.nozzle_id === 0 || filament.nozzle_id === 1
        ? filament.nozzle_id
        : null,
    }];
  });
}

export function printerAmsSlots(printer: Pick<Printer, "materials" | "model">): PrinterAmsSlot[] {
  const routingRequired = isDualNozzleModel(printer.model);
  const materials = printer.materials;
  if (!materials) return [];
  const filamentSwitchInstalled = materials.filament_switch_installed ?? null;
  const inferConventionalRoutes = routingRequired &&
    filamentSwitchInstalled !== true &&
    materials.ams_units.filter((unit) => unit.unit_id === "0" || unit.unit_id === "1")
      .length === 2;

  const amsSlots = materials.ams_units.flatMap((unit) => {
    const unitId = unit.unit_id ?? "";
    const amsId = Number.parseInt(unitId, 10);
    if (!Number.isInteger(amsId)) return [];

    return (unit.trays ?? []).flatMap((tray) => {
      const trayId = tray.tray_id ?? "";
      const slotId = Number.parseInt(trayId, 10);
      if (!Number.isInteger(slotId)) return [];
      const globalTrayId = tray.global_tray_id ?? (
        amsId < 64 ? amsId * 4 + slotId : amsId >= 128 && amsId <= 135 ? amsId : UNMAPPED
      );
      return [{
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
        toolhead: materialToolhead(tray.toolhead ?? unit.toolhead) ??
          (inferConventionalRoutes
            ? unitId === "0" ? "R" : unitId === "1" ? "L" : null
            : null),
        exists: tray.exists ?? null,
        routingRequired,
        filamentSwitchInstalled,
      }];
    });
  });

  const externalSlots = materials.external_spools.flatMap((spool) => {
    const externalId = Number.parseInt(spool.external_id ?? "", 10);
    if (!Number.isInteger(externalId)) return [];
    const trayId = spool.tray_id ?? "0";
    const inferredToolhead = externalId === 254 ? "L" : externalId === 255 ? "R" : null;
    return [{
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
    }];
  });

  return [...amsSlots, ...externalSlots];
}

export function slotIneligibility(
  filament: ProjectFilament,
  slot: PrinterAmsSlot,
  slots: PrinterAmsSlot[],
  useAms = true,
): SlotIneligibility | null {
  if (slot.exists === false) return "empty";
  if (!useAms && slot.kind === "ams") return "ams_disabled";

  const filamentSwitchInstalled = slots.find(
    (candidate) => candidate.filamentSwitchInstalled !== null,
  )?.filamentSwitchInstalled ?? null;
  if (filamentSwitchInstalled === true) {
    if (slot.kind === "external") return "filament_switch_external";
    if (slot.toolhead === null) return "unknown_route";
  } else {
    if (slot.routingRequired && filament.nozzleId === null) return "unknown_route";
    if (
      slot.kind === "external" &&
      slot.routingRequired &&
      filamentSwitchInstalled === null
    ) {
      return "unknown_route";
    }
    if (filament.nozzleId !== null) {
      const expected = filament.nozzleId === 1 ? "L" : "R";
      const hasKnownRoute = slots.some((candidate) => candidate.toolhead !== null) ||
        slots.some((candidate) => candidate.routingRequired);
      if (slot.toolhead !== null && slot.toolhead !== expected) return "wrong_nozzle";
      if (slot.toolhead === null && hasKnownRoute) return "unknown_route";
    }
  }

  if (
    slot.kind === "ams" &&
    knownTypeMismatch(filament.filamentType, slot.filamentType)
  ) {
    return "material_type_mismatch";
  }

  return null;
}

export function autoMapSlotSelections(
  filaments: ProjectFilament[],
  slots: PrinterAmsSlot[],
  useAms = true,
): Map<number, string> {
  const selections = new Map<number, string>();
  const pairs = compatiblePairs(filaments, slots, useAms);
  const remainingFilaments = new Set(filaments.map(({ mappingIndex }) => mappingIndex));
  const remainingSlots = new Set(slots.map(({ key }) => key));

  while (true) {
    const pair = pairs.find(
      (candidate) =>
        remainingFilaments.has(candidate.mappingIndex) && remainingSlots.has(candidate.slotKey),
    );
    if (!pair) break;
    selections.set(pair.mappingIndex, pair.slotKey);
    remainingFilaments.delete(pair.mappingIndex);
    remainingSlots.delete(pair.slotKey);
  }

  for (const mappingIndex of remainingFilaments) {
    const pair = pairs.find((candidate) => candidate.mappingIndex === mappingIndex);
    if (pair) selections.set(mappingIndex, pair.slotKey);
  }

  return selections;
}

export function autoMapProjectFilaments(
  filaments: ProjectFilament[],
  slots: PrinterAmsSlot[],
  useAms = true,
): number[] {
  const selections = autoMapSlotSelections(filaments, slots, useAms);
  return materialMappingPayload(filaments, slots, selections, useAms).amsMapping;
}

export function materialMappingPayload(
  filaments: ProjectFilament[],
  slots: PrinterAmsSlot[],
  selections: ReadonlyMap<number, string>,
  useAms = true,
): {
  amsMapping: number[];
  amsMapping2: AmsMapping2Entry[];
  amsMappingInfo: AmsMappingInfoEntry[] | null;
  externalTypeMismatch: boolean;
  mappingValid: boolean;
  usesAms: boolean;
} {
  const length = filaments.reduce(
    (current, filament) => Math.max(current, filament.mappingIndex + 1),
    0,
  );
  const amsMapping = Array<number>(length).fill(UNMAPPED);
  const amsMapping2 = Array.from({ length }, () => ({
    ams_id: UNMAPPED_MAPPING2,
    slot_id: UNMAPPED_MAPPING2,
  }));
  const slotsByKey = new Map(slots.map((slot) => [slot.key, slot]));
  const selectedSlots = new Map<number, PrinterAmsSlot>();

  for (const filament of filaments) {
    const slotKey = selections.get(filament.mappingIndex);
    const slot = slotKey ? slotsByKey.get(slotKey) : null;
    if (!slot || slotIneligibility(filament, slot, slots, useAms)) continue;
    selectedSlots.set(filament.mappingIndex, slot);
    amsMapping[filament.mappingIndex] = slot.legacyTrayId;
    amsMapping2[filament.mappingIndex] = { ams_id: slot.amsId, slot_id: slot.slotId };
  }

  const orderedFilaments = Array.from(
    { length },
    (_, mappingIndex) => filaments.find((filament) => filament.mappingIndex === mappingIndex),
  );
  const amsMappingInfo = orderedFilaments.every(
    (filament): filament is ProjectFilament =>
      filament !== undefined && filament.nozzleId !== null,
  )
    ? orderedFilaments.map((filament) => {
        const slot = selectedSlots.get(filament.mappingIndex);
        return {
          ams: slot?.globalTrayId ?? UNMAPPED,
          filamentType: filament.filamentType ?? "",
          filamentId: filament.trayInfoIdx ?? "",
          nozzleId: filament.nozzleId!,
          sourceColor: payloadColor(filament.color),
          targetColor: payloadColor(slot?.color ?? null),
        };
      })
    : null;

  const externalTypeMismatch = filaments.some((filament) => {
    const slotKey = selections.get(filament.mappingIndex);
    const slot = slotKey ? selectedSlots.get(filament.mappingIndex) : null;
    return slot?.kind === "external" &&
      knownTypeMismatch(filament.filamentType, slot.filamentType);
  });

  const mappingValid = selectedSlots.size === filaments.length;
  const usesAms = [...selectedSlots.values()].some((slot) => slot.kind === "ams");
  return { amsMapping, amsMapping2, amsMappingInfo, externalTypeMismatch, mappingValid, usesAms };
}

function compatiblePairs(
  filaments: ProjectFilament[],
  slots: PrinterAmsSlot[],
  useAms: boolean,
) {
  return filaments
    .flatMap((filament) =>
      slots.flatMap((slot) => {
        if (slotIneligibility(filament, slot, slots, useAms)) return [];
        if (
          (useAms || slot.kind === "ams") &&
          normalizedType(filament.filamentType) !== normalizedType(slot.filamentType)
        ) return [];
        const distance = colorDistance(filament.color, slot.color) ?? 10_000;
        const presetPenalty = filament.trayInfoIdx && slot.filamentId
          ? filament.trayInfoIdx === slot.filamentId ? 0 : 1_000
          : 1_000;
        return [{
          mappingIndex: filament.mappingIndex,
          slotKey: slot.key,
          distance: distance + presetPenalty + (slot.kind === "external" ? 20_000 : 0),
        }];
      }),
    )
    .sort(
      (left, right) =>
        left.distance - right.distance ||
        left.mappingIndex - right.mappingIndex ||
        left.slotKey.localeCompare(right.slotKey),
    );
}

function knownTypeMismatch(left: string | null, right: string | null) {
  const normalizedLeft = normalizedType(left);
  const normalizedRight = normalizedType(right);
  return normalizedLeft !== "" && normalizedRight !== "" && normalizedLeft !== normalizedRight;
}

function normalizedType(value: string | null) {
  return value?.trim().toUpperCase() ?? "";
}

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

function payloadColor(value: string | null) {
  const normalized = value?.trim().replace(/^#/, "").toUpperCase() ?? "";
  if (/^[0-9A-F]{6}$/.test(normalized)) return "#" + normalized + "FF";
  return /^[0-9A-F]{8}$/.test(normalized) ? "#" + normalized : "";
}

function colorDistance(left: string | null, right: string | null): number | null {
  const a = parsedColor(left);
  const b = parsedColor(right);
  if (!a || !b || a.alpha !== b.alpha) return null;
  return Math.hypot(a.red - b.red, a.green - b.green, a.blue - b.blue);
}

function parsedColor(value: string | null) {
  const normalized = value?.trim().replace(/^#/, "");
  if (!normalized || !/^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(normalized)) return null;
  return {
    red: Number.parseInt(normalized.slice(0, 2), 16),
    green: Number.parseInt(normalized.slice(2, 4), 16),
    blue: Number.parseInt(normalized.slice(4, 6), 16),
    alpha: normalized.length === 8 ? Number.parseInt(normalized.slice(6, 8), 16) : 255,
  };
}
