import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NextIntlClientProvider } from "next-intl";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import type { PrinterAmsSlot, ProjectFilament } from "./dispatch-material-mapping";
import { DispatchMaterialSlotMenu } from "./dispatch-material-slot-menu";

describe("DispatchMaterialSlotMenu", () => {
  it("uses Studio's full-width dynamic grouping when Filament Track Switch routing is active", async () => {
    const user = userEvent.setup();
    const slots = [
      slot({ key: "ams:0:0", unitId: "0", amsId: 0, globalTrayId: 0 }),
      slot({ key: "ams:1:0", unitId: "1", amsId: 1, globalTrayId: 4 }),
      slot({
        key: "external:254",
        kind: "external",
        unitId: "254",
        unitKind: "external",
        amsId: 254,
        globalTrayId: 254,
        legacyTrayId: -1,
        toolhead: "L",
      }),
      slot({
        key: "external:255",
        kind: "external",
        unitId: "255",
        unitKind: "external",
        amsId: 255,
        globalTrayId: 255,
        legacyTrayId: -1,
        toolhead: "R",
      }),
    ];

    const { getByRole, getByText, queryByText } = render(
      <NextIntlClientProvider locale="en" messages={en}>
        <DispatchMaterialSlotMenu
          filament={filament()}
          materialName="PLA (1)"
          model="Bambu Lab X2D"
          onSelect={vi.fn()}
          selectedKey="ams:1:0"
          slots={slots}
          useAms
        />
      </NextIntlClientProvider>,
    );

    await user.click(getByRole("button", { name: "Map PLA (1)" }));

    expect(getByText("AMS(1)")).toBeInTheDocument();
    expect(getByText("AMS(2)")).toBeInTheDocument();
    expect(queryByText("Shared AMS")).not.toBeInTheDocument();
    expect(queryByText("Left AMS")).not.toBeInTheDocument();
    expect(queryByText("Right AMS")).not.toBeInTheDocument();
    expect(queryByText("External")).not.toBeInTheDocument();

    const dynamicSection = getByText("AMS(1)").closest("section");
    expect(dynamicSection).toHaveClass("sm:col-span-2");
    expect(getByRole("button", {
      name: /Ext-L, PLA, Remaining \d+%, External spools are unavailable while Filament Track Switch routing is active\./,
    })).toHaveAttribute("aria-disabled", "true");
    expect(getByRole("button", {
      name: /Ext-R, PLA, Remaining \d+%, External spools are unavailable while Filament Track Switch routing is active\./,
    })).toHaveAttribute("aria-disabled", "true");
  });
});

function filament(): ProjectFilament {
  return {
    mappingIndex: 0,
    filamentId: "1",
    trayInfoIdx: "GFA00",
    filamentType: "PLA",
    color: "#000000",
    nozzleId: 1,
  };
}

function slot(overrides: Partial<PrinterAmsSlot>): PrinterAmsSlot {
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
    color: "000000FF",
    multiColor: ["000000FF"],
    remainingEstimate: 50,
    toolhead: "LR",
    exists: true,
    routingRequired: true,
    filamentSwitchInstalled: true,
    ...overrides,
  };
}
