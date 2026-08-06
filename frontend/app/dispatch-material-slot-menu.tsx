"use client";

import { useMemo, useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";

import {
  slotIneligibility,
  type MaterialToolhead,
  type PrinterAmsSlot,
  type ProjectFilament,
  type SlotIneligibility,
} from "./dispatch-material-mapping";

export function DispatchMaterialSlotMenu({
  errorId,
  filament,
  invalid = false,
  materialName,
  model,
  onSelect,
  selectedKey,
  slots,
  useAms,
}: {
  errorId?: string;
  filament: ProjectFilament;
  invalid?: boolean;
  materialName: string;
  model: string | null;
  onSelect: (slotKey: string) => void;
  selectedKey: string;
  slots: PrinterAmsSlot[];
  useAms: boolean;
}) {
  const t = useTranslations("dispatch");
  const [open, setOpen] = useState(false);
  const selected = slots.find((slot) => slot.key === selectedKey) ?? null;
  const nozzle = nozzleName(t, model, filament.nozzleId);
  const sections = useMemo(() => materialSections(slots), [slots]);

  const select = (slotKey: string) => {
    onSelect(slotKey);
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        render={
          <Button
            aria-describedby={invalid ? errorId : undefined}
            aria-invalid={invalid || undefined}
            aria-label={t("mapMaterial", { material: materialName })}
            className="h-auto min-h-11 w-full justify-between rounded-md px-2.5 py-2"
            type="button"
            variant="outline"
          />
        }
      >
        {selected ? (
          <span className="flex min-w-0 items-center gap-2">
            <MaterialColorSwatch color={selected.color} colors={selected.multiColor} />
            <span className="min-w-0 text-left">
              <span className="block truncate text-sm font-medium">{slotCode(selected)}</span>
              <span className="block truncate text-xs font-normal text-muted-foreground">
                {slotMaterialName(selected, t)}
              </span>
            </span>
          </span>
        ) : (
          <span className="text-sm font-normal text-muted-foreground">{t("unmapped")}</span>
        )}
        <ChevronDown className="size-4 shrink-0 text-muted-foreground" />
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="max-h-[min(34rem,calc(100vh-3rem))] w-[min(36rem,calc(100vw-2rem))] gap-0 overflow-y-auto p-0"
        sideOffset={6}
      >
        <div className="border-b border-border px-4 py-3">
          <PopoverTitle className="text-base font-semibold">
            {t("selectFilamentForNozzle", { nozzle })}
          </PopoverTitle>
          <p className="mt-1 text-xs text-muted-foreground">{t("materialMatchingHint")}</p>
        </div>
        <div className="grid gap-0 sm:grid-cols-2">
          {sections.map((section) => (
            <MaterialSection
              filament={filament}
              key={section.id}
              onSelect={select}
              section={section}
              selectedKey={selectedKey}
              slots={slots}
              useAms={useAms}
            />
          ))}
        </div>
        <div className="border-t border-border p-2">
          <Button
            className="h-auto w-full justify-start rounded-md px-3 py-2 font-normal text-muted-foreground hover:text-muted-foreground"
            onClick={() => select("")}
            type="button"
            variant="ghost"
          >
            {t("unmapped")}
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}

function MaterialSection({
  filament,
  onSelect,
  section,
  selectedKey,
  slots,
  useAms,
}: {
  filament: ProjectFilament;
  onSelect: (slotKey: string) => void;
  section: MaterialSectionData;
  selectedKey: string;
  slots: PrinterAmsSlot[];
  useAms: boolean;
}) {
  const t = useTranslations("dispatch");
  const amsGroups = groupAmsSlots(section.slots.filter((slot) => slot.kind === "ams"));
  const external = section.slots.filter((slot) => slot.kind === "external");

  return (
    <section className={cn(
      "min-w-0 border-border px-3 py-3 sm:border-r sm:last:border-r-0",
      section.id === "LR" && "sm:col-span-2 sm:border-r-0",
    )}>
      {section.id !== "LR" ? (
        <h3 className="mb-2 text-sm font-semibold text-foreground">{sectionTitle(section.id, t)}</h3>
      ) : null}
      <div className={cn("grid gap-3", section.id === "LR" && "sm:grid-cols-2")}>
        {amsGroups.map(([unitId, unitSlots]) => (
          <div key={unitId}>
            <div className="mb-1 text-xs font-medium text-muted-foreground">
              {amsUnitName(unitSlots[0])}
            </div>
            <div className="grid grid-cols-4 gap-1.5">
              {unitSlots.map((slot) => (
                <MaterialSlotButton
                  filament={filament}
                  key={slot.key}
                  onSelect={onSelect}
                  selected={selectedKey === slot.key}
                  slot={slot}
                  slots={slots}
                  useAms={useAms}
                />
              ))}
            </div>
          </div>
        ))}
        {external.length > 0 ? (
          <div>
            {section.id !== "LR" ? (
              <div className="mb-1 text-xs font-medium text-muted-foreground">{t("external")}</div>
            ) : null}
            <div className="flex gap-1.5">
              {external.map((slot) => (
                <MaterialSlotButton
                  filament={filament}
                  key={slot.key}
                  onSelect={onSelect}
                  selected={selectedKey === slot.key}
                  slot={slot}
                  slots={slots}
                  useAms={useAms}
                />
              ))}
            </div>
          </div>
        ) : null}
      </div>
    </section>
  );
}

function MaterialSlotButton({
  filament,
  onSelect,
  selected,
  slot,
  slots,
  useAms,
}: {
  filament: ProjectFilament;
  onSelect: (slotKey: string) => void;
  selected: boolean;
  slot: PrinterAmsSlot;
  slots: PrinterAmsSlot[];
  useAms: boolean;
}) {
  const t = useTranslations("dispatch");
  const ineligibility = slotIneligibility(filament, slot, slots, useAms);
  const reason = ineligibility ? ineligibilityText(ineligibility, t) : null;
  const material = slotMaterialName(slot, t);
  const remainingLabel = slot.remainingEstimate !== null
    ? t("filamentRemainingPercent", {
        percent: Math.max(0, Math.min(100, slot.remainingEstimate)),
      })
    : null;

  return (
    <Button
      aria-disabled={ineligibility !== null}
      aria-label={[slotCode(slot), material, remainingLabel, reason].filter(Boolean).join(", ")}
      aria-pressed={selected}
      className={cn(
        "relative h-auto min-w-0 justify-start overflow-hidden whitespace-normal rounded-md border bg-background p-0 text-left font-normal",
        selected ? "border-primary ring-2 ring-primary/20" : "border-border",
        ineligibility
          ? "cursor-not-allowed opacity-40"
          : "hover:border-foreground/40 hover:bg-muted",
        slot.kind === "external" ? "w-14" : "w-full",
      )}
      onClick={() => {
        if (ineligibility) return
        onSelect(slot.key)
      }}
      title={reason ?? undefined}
      type="button"
    >
      <MaterialColorStrip color={slot.color} colors={slot.multiColor} />
      <span className="block px-1.5 pb-1.5 pt-1">
        <span className="flex items-start justify-between gap-1">
          <span className="truncate text-xs font-semibold text-foreground">{slotCode(slot)}</span>
          {selected ? <Check className="size-3 shrink-0 text-primary" /> : null}
        </span>
        <span className="block truncate text-xs leading-4 text-muted-foreground">{material}</span>
        {slot.remainingEstimate !== null ? (
          <span aria-hidden="true" className="mt-1 block h-0.5 overflow-hidden rounded bg-muted">
            <span
              className="block h-full bg-muted-foreground/50"
              style={{ width: Math.max(0, Math.min(100, slot.remainingEstimate)) + "%" }}
            />
          </span>
        ) : null}
      </span>
    </Button>
  );
}

export function MaterialColorSwatch({
  color,
  colors = [],
}: {
  color: string | null;
  colors?: string[];
}) {
  const displayColors = colorList(color, colors);
  return (
    <span
      aria-hidden="true"
      className="flex size-6 shrink-0 overflow-hidden rounded-full border border-border bg-[linear-gradient(45deg,#e2e8f0_25%,transparent_25%,transparent_75%,#e2e8f0_75%),linear-gradient(45deg,#e2e8f0_25%,white_25%,white_75%,#e2e8f0_75%)] bg-size-[8px_8px] bg-position-[0_0,4px_4px]"
    >
      {displayColors.map((displayColor, index) => (
        <span className="h-full flex-1" key={displayColor + index} style={{ backgroundColor: displayColor }} />
      ))}
    </span>
  );
}

function MaterialColorStrip({ color, colors }: { color: string | null; colors: string[] }) {
  const displayColors = colorList(color, colors);
  return (
    <span className="flex h-1 w-full bg-[linear-gradient(45deg,#cbd5e1_25%,transparent_25%,transparent_75%,#cbd5e1_75%),linear-gradient(45deg,#cbd5e1_25%,white_25%,white_75%,#cbd5e1_75%)] bg-size-[4px_4px] bg-position-[0_0,2px_2px]">
      {displayColors.map((displayColor, index) => (
        <span className="h-full flex-1" key={displayColor + index} style={{ backgroundColor: displayColor }} />
      ))}
    </span>
  );
}

type MaterialSectionData = {
  id: MaterialToolhead;
  slots: PrinterAmsSlot[];
};

function materialSections(slots: PrinterAmsSlot[]): MaterialSectionData[] {
  if (slots.some((slot) => slot.filamentSwitchInstalled === true)) {
    return [{ id: "LR", slots }];
  }
  const ordered: MaterialToolhead[] = ["L", "R", "LR", null];
  return ordered
    .map((id) => ({
      id,
      slots: slots.filter((slot) => slot.toolhead === id),
    }))
    .filter((section) => section.slots.length > 0);
}

function groupAmsSlots(slots: PrinterAmsSlot[]) {
  const groups = new Map<string, PrinterAmsSlot[]>();
  for (const slot of slots) {
    const current = groups.get(slot.unitId) ?? [];
    current.push(slot);
    groups.set(slot.unitId, current);
  }
  return [...groups.entries()].map(([unitId, unitSlots]) => [
    unitId,
    unitSlots.sort((left, right) => left.slotId - right.slotId),
  ] as const);
}

function amsUnitName(slot: PrinterAmsSlot) {
  const number = slot.amsId >= 128 && slot.amsId <= 135
    ? slot.amsId - 127
    : slot.amsId >= 0 && slot.amsId < 26 ? slot.amsId + 1 : slot.unitId;
  if (slot.unitKind === "ams_ht") return "AMS HT (" + number + ")";
  if (slot.unitKind === "ams_lite") return "AMS Lite (" + number + ")";
  if (slot.unitKind === "ams_2_pro") return "AMS 2 Pro (" + number + ")";
  return "AMS(" + number + ")";
}

function slotCode(slot: PrinterAmsSlot) {
  if (slot.kind === "external") {
    return slot.toolhead === "L" ? "Ext-L" : slot.toolhead === "R" ? "Ext-R" : "Ext";
  }
  if (slot.amsId >= 0 && slot.amsId < 26) {
    return String.fromCharCode(65 + slot.amsId) + String(slot.slotId + 1);
  }
  if (slot.amsId >= 128 && slot.amsId <= 135) {
    return "HT-" + String.fromCharCode(65 + slot.amsId - 128);
  }
  return String(slot.slotId + 1);
}

function slotMaterialName(
  slot: PrinterAmsSlot,
  t: ReturnType<typeof useTranslations>,
) {
  if (slot.exists === false) return t("emptySlot");
  return slot.name || slot.filamentType || slot.filamentId || t("unknownMaterial");
}

function sectionTitle(
  toolhead: MaterialToolhead,
  t: ReturnType<typeof useTranslations>,
) {
  if (toolhead === "L") return t("leftAms");
  if (toolhead === "R") return t("rightAms");
  if (toolhead === "LR") return t("ams");
  return t("ams");
}

function nozzleName(
  t: ReturnType<typeof useTranslations>,
  model: string | null,
  nozzleId: 0 | 1 | null,
) {
  const normalized = model?.trim().toUpperCase().replace(/^BAMBU LAB /, "");
  if (normalized === "N6" || normalized === "X2D") {
    if (nozzleId === 1) return t("mainNozzle");
    if (nozzleId === 0) return t("auxiliaryNozzle");
  }
  if (nozzleId === 1) return t("leftNozzle");
  if (nozzleId === 0) return t("rightNozzle");
  return t("selectedNozzle");
}

function ineligibilityText(
  reason: SlotIneligibility,
  t: ReturnType<typeof useTranslations>,
) {
  if (reason === "empty") return t("emptySlotReason");
  if (reason === "material_type_mismatch") return t("materialTypeMismatch");
  if (reason === "filament_switch_external") return t("filamentSwitchExternal");
  if (reason === "unknown_route") return t("unknownMaterialRoute");
  if (reason === "ams_disabled") return t("amsDisabled");
  return t("wrongNozzle");
}

function colorList(color: string | null, colors: string[]) {
  const candidates = colors.length > 0 ? colors : color ? [color] : [];
  return candidates.flatMap((candidate) => {
    const normalized = candidate.trim().replace(/^#/, "");
    return /^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(normalized)
      ? ["#" + normalized]
      : [];
  });
}
