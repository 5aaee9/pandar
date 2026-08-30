"use client";

import { useCallback, useMemo, useState } from "react";

import type { ArtifactMetadata, Printer } from "./dashboard-types";
import {
  autoMapSlotSelections,
  materialMappingPayload,
  printerAmsSlots,
  projectFilamentsForPlate,
} from "./dispatch-material-mapping";

const EMPTY_SELECTIONS = new Map<number, string>();

type SelectionState = {
  key: string;
  selections: Map<number, string>;
};

export function useDispatchMaterialMapping(
  metadata: ArtifactMetadata | null,
  plateId: number | null,
  printer: Pick<Printer, "id" | "compatibility" | "materials"> | null,
  useAms: boolean,
) {
  const config = useMemo(() => {
    if (!metadata || plateId === null || !printer) {
      return null;
    }
    const filaments = projectFilamentsForPlate(metadata, plateId);
    const slots = printerAmsSlots(printer);
    const key = [
      printer.id,
      printer.compatibility.normalized_model,
      plateId,
      useAms,
      ...filaments.map((filament) =>
        [
          filament.mappingIndex,
          filament.filamentType,
          filament.color,
          filament.nozzleId,
        ].join(":"),
      ),
      ...slots.map((slot) =>
        [
          slot.key,
          slot.filamentType,
          slot.color,
          slot.toolhead,
          slot.exists,
          slot.filamentSwitchInstalled,
        ].join(":"),
      ),
    ].join("|");
    return {
      filaments,
      key,
      plateId,
      slots,
      initialSelections: autoMapSlotSelections(filaments, slots, useAms),
      nozzleLayout: printer.compatibility.nozzle_layout,
    };
  }, [metadata, plateId, printer, useAms]);
  const [selectionState, setSelectionState] = useState<SelectionState | null>(
    null,
  );

  const selections =
    config && selectionState?.key === config.key
      ? selectionState.selections
      : (config?.initialSelections ?? EMPTY_SELECTIONS);
  const payload = config
    ? materialMappingPayload(config.filaments, config.slots, selections, useAms)
    : null;

  const selectSlot = useCallback(
    (mappingIndex: number, slotKey: string) => {
      if (!config) {
        return;
      }
      setSelectionState((current) => {
        const base =
          current?.key === config.key
            ? current.selections
            : config.initialSelections;
        const next = new Map(base);
        if (slotKey) next.set(mappingIndex, slotKey);
        else next.delete(mappingIndex);
        return { key: config.key, selections: next };
      });
    },
    [config],
  );

  return {
    fields:
      config && payload
        ? {
            editorKey: config.key,
            filaments: config.filaments,
            nozzleLayout: config.nozzleLayout,
            onSelectSlot: selectSlot,
            payload,
            plateId: config.plateId,
            selections,
            slots: config.slots,
            useAms,
          }
        : null,
    valid: payload?.mappingValid ?? true,
  };
}
