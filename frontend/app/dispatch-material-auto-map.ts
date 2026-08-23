import {
  materialColorDistance,
  materialPayloadColor,
} from "./dispatch-material-color";
import type {
  AmsMapping2Entry,
  AmsMappingInfoEntry,
  PrinterAmsSlot,
  ProjectFilament,
  SlotIneligibility,
} from "./dispatch-material-mapping";

const UNMAPPED = -1;
const UNMAPPED_MAPPING2 = 255;

export function slotIneligibility(
  filament: ProjectFilament,
  slot: PrinterAmsSlot,
  slots: PrinterAmsSlot[],
  useAms = true,
): SlotIneligibility | null {
  if (slot.exists === false) return "empty";
  if (!useAms && slot.kind === "ams") return "ams_disabled";

  const filamentSwitchInstalled =
    slots.find((candidate) => candidate.filamentSwitchInstalled !== null)
      ?.filamentSwitchInstalled ?? null;
  if (filamentSwitchInstalled === true) {
    if (slot.kind === "external") return "filament_switch_external";
    if (slot.toolhead === null) return "unknown_route";
  } else {
    if (slot.routingRequired && filament.nozzleId === null)
      return "unknown_route";
    if (
      slot.kind === "external" &&
      slot.routingRequired &&
      filamentSwitchInstalled === null
    ) {
      return "unknown_route";
    }
    if (filament.nozzleId !== null) {
      const expected = filament.nozzleId === 1 ? "L" : "R";
      const hasKnownRoute =
        slots.some((candidate) => candidate.toolhead !== null) ||
        slots.some((candidate) => candidate.routingRequired);
      if (slot.toolhead !== null && slot.toolhead !== expected)
        return "wrong_nozzle";
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
  const remainingFilaments = new Set(
    filaments.map(({ mappingIndex }) => mappingIndex),
  );
  const remainingSlots = new Set(slots.map(({ key }) => key));

  const firstPairByFilament = new Map<number, (typeof pairs)[number]>();
  for (const pair of pairs) {
    if (!firstPairByFilament.has(pair.mappingIndex)) {
      firstPairByFilament.set(pair.mappingIndex, pair);
    }
    if (
      remainingFilaments.has(pair.mappingIndex) &&
      remainingSlots.has(pair.slotKey)
    ) {
      selections.set(pair.mappingIndex, pair.slotKey);
      remainingFilaments.delete(pair.mappingIndex);
      remainingSlots.delete(pair.slotKey);
    }
  }

  for (const mappingIndex of remainingFilaments) {
    const pair = firstPairByFilament.get(mappingIndex);
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
  return materialMappingPayload(filaments, slots, selections, useAms)
    .amsMapping;
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
    amsMapping2[filament.mappingIndex] = {
      ams_id: slot.amsId,
      slot_id: slot.slotId,
    };
  }

  const filamentsByIndex = new Map(
    filaments.map((filament) => [filament.mappingIndex, filament]),
  );
  const orderedFilaments = Array.from({ length }, (_, mappingIndex) =>
    filamentsByIndex.get(mappingIndex),
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
          sourceColor: materialPayloadColor(filament.color),
          targetColor: materialPayloadColor(slot?.color ?? null),
        };
      })
    : null;

  const externalTypeMismatch = filaments.some((filament) => {
    const slotKey = selections.get(filament.mappingIndex);
    const slot = slotKey ? selectedSlots.get(filament.mappingIndex) : null;
    return (
      slot?.kind === "external" &&
      knownTypeMismatch(filament.filamentType, slot.filamentType)
    );
  });

  const mappingValid = selectedSlots.size === filaments.length;
  const usesAms = [...selectedSlots.values()].some(
    (slot) => slot.kind === "ams",
  );
  return {
    amsMapping,
    amsMapping2,
    amsMappingInfo,
    externalTypeMismatch,
    mappingValid,
    usesAms,
  };
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
          normalizedType(filament.filamentType) !==
            normalizedType(slot.filamentType)
        )
          return [];
        const distance =
          materialColorDistance(filament.color, slot.color) ?? 10_000;
        const presetPenalty =
          filament.trayInfoIdx && slot.filamentId
            ? filament.trayInfoIdx === slot.filamentId
              ? 0
              : 1_000
            : 1_000;
        return [
          {
            mappingIndex: filament.mappingIndex,
            slotKey: slot.key,
            distance:
              distance +
              presetPenalty +
              (slot.kind === "external" ? 20_000 : 0),
          },
        ];
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
  return (
    normalizedLeft !== "" &&
    normalizedRight !== "" &&
    normalizedLeft !== normalizedRight
  );
}

function normalizedType(value: string | null) {
  return value?.trim().toUpperCase() ?? "";
}
