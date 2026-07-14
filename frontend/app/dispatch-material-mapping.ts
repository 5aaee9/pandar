import type { ArtifactMetadata, Printer } from "./dashboard-types";

export type ProjectFilament = {
  mappingIndex: number;
  filamentId: string | null;
  trayInfoIdx: string | null;
  filamentType: string | null;
  color: string | null;
};

export type PrinterAmsSlot = {
  key: string;
  unitId: string;
  trayId: string;
  amsId: number;
  slotId: number;
  globalTrayId: number;
  filamentId: string | null;
  filamentType: string | null;
  color: string | null;
};

export type AmsMapping2Entry = {
  ams_id: number;
  slot_id: number;
};

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
    }];
  });
}

export function printerAmsSlots(printer: Pick<Printer, "materials">): PrinterAmsSlot[] {
  return (printer.materials?.ams_units ?? []).flatMap((unit) => {
    const unitId = unit.unit_id ?? "";
    const amsId = Number.parseInt(unitId, 10);
    if (!Number.isInteger(amsId)) return [];

    return (unit.trays ?? []).flatMap((tray) => {
      if (
        tray.exists === false ||
        (!tray.type && !tray.name && !tray.filament_id && !tray.color)
      ) return [];
      const trayId = tray.tray_id ?? "";
      const slotId = Number.parseInt(trayId, 10);
      if (!Number.isInteger(slotId)) return [];
      const globalTrayId = tray.global_tray_id ?? (
        amsId < 64 ? amsId * 4 + slotId : amsId >= 128 && amsId <= 135 ? amsId : UNMAPPED
      );
      return [{
        key: `${unitId}:${trayId}`,
        unitId,
        trayId,
        amsId,
        slotId,
        globalTrayId,
        filamentId: tray.filament_id ?? null,
        filamentType: tray.type ?? tray.name ?? null,
        color: tray.color ?? null,
      }];
    });
  });
}

export function autoMapSlotSelections(
  filaments: ProjectFilament[],
  slots: PrinterAmsSlot[],
): Map<number, string> {
  const selections = new Map<number, string>();
  const pairs = compatiblePairs(filaments, slots);
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
): number[] {
  const selections = autoMapSlotSelections(filaments, slots);
  return materialMappingPayload(filaments, slots, selections).amsMapping;
}

export function materialMappingPayload(
  filaments: ProjectFilament[],
  slots: PrinterAmsSlot[],
  selections: ReadonlyMap<number, string>,
): { amsMapping: number[]; amsMapping2: AmsMapping2Entry[] } {
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

  for (const filament of filaments) {
    const slotKey = selections.get(filament.mappingIndex);
    const slot = slotKey ? slotsByKey.get(slotKey) : null;
    if (!slot) continue;
    amsMapping[filament.mappingIndex] = slot.globalTrayId;
    amsMapping2[filament.mappingIndex] = { ams_id: slot.amsId, slot_id: slot.slotId };
  }

  return { amsMapping, amsMapping2 };
}

function compatiblePairs(filaments: ProjectFilament[], slots: PrinterAmsSlot[]) {
  return filaments
    .flatMap((filament) =>
      slots.flatMap((slot) => {
        if (normalizedType(filament.filamentType) !== normalizedType(slot.filamentType)) return [];
        const distance = colorDistance(filament.color, slot.color);
        if (distance === null) return [];
        const presetPenalty = filament.trayInfoIdx && slot.filamentId
          ? filament.trayInfoIdx === slot.filamentId ? 1_000 : 2_000
          : 2_000;
        return [{
          mappingIndex: filament.mappingIndex,
          slotKey: slot.key,
          distance: distance + presetPenalty,
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

function normalizedType(value: string | null) {
  return value?.trim().toUpperCase() ?? "";
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
