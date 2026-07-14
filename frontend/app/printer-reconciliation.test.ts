import { describe, expect, it } from "vitest";

import type { Printer } from "./dashboard-types";
import type { PrinterPrintState } from "./printer-live-types";
import {
  clearEnrichedPrinterState,
  isEnrichedPrinter,
  mergePrinterEvent,
  replacePrinterInventory,
} from "./printer-reconciliation";

const printState: PrinterPrintState = {
  task_generation: 1,
  error_generation: 1,
  hms: [],
  job_state: 0,
  gcode_state: "RUNNING",
  task_id: "task-1",
  subtask_id: "subtask-1",
  subtask_name: "Cube",
  gcode_file: "/data/Metadata/plate_1.gcode",
  progress_percent: 25,
  remaining_time_minutes: 12,
  current_layer: 2,
  total_layers: 8,
  print_error: 83_918_929,
  printer_job_id: "job-1",
};

function printer(
  id: string,
  overrides: Partial<Printer> = {},
): Printer {
  return {
    id,
    tenant_id: "tenant-1",
    agent_id: "agent-1",
    serial_number: `serial-${id}`,
    name: `Printer ${id}`,
    model: "P1S",
    status: "RUNNING",
    last_seen_at: "2026-07-10T00:00:00Z",
    created_at: "2026-07-01T00:00:00Z",
    nozzle_temperatures: [],
    active_nozzle: null,
    bed_temperature_celsius: null,
    bed_target_temperature_celsius: null,
    chamber_temperature_celsius: null,
    chamber_light_on: null,
    materials: null,
    state_revision: 1,
    print: printState,
    ...overrides,
  };
}

function materials(observedAt: string) {
  return {
    ams_units: [],
    external_spools: [],
    active_tray: null,
    observed_at: observedAt,
  };
}

describe("printer reconciliation", () => {
  it("whole-replaces the inventory and removes printers absent from REST", () => {
    const previous = [printer("deleted"), printer("kept")];
    const baseline = [printer("kept", { state_revision: 4 })];

    const replaced = replacePrinterInventory(baseline);

    expect(previous).toHaveLength(2);
    expect(replaced.map(({ id }) => id)).toEqual(["kept"]);
    expect(replaced[0]?.state_revision).toBe(4);
  });

  it("applies shell and print data only from a higher version", () => {
    const current = printer("p1", {
      state_revision: 4,
      status: "RUNNING",
      print: { ...printState, progress_percent: 40 },
    });
    const higher = printer("p1", {
      state_revision: 5,
      status: "PAUSE",
      print: { ...printState, progress_percent: 41 },
    });

    const result = mergePrinterEvent([current], higher);

    expect(result.kind).toBe("applied");
    expect(result.printers[0]).toMatchObject({
      state_revision: 5,
      status: "PAUSE",
      print: { progress_percent: 41 },
    });
  });

  it.each([3, 4])(
    "does not let revision %s regress an explicit clear",
    (stateRevision) => {
      const cleared = printer("p1", {
        state_revision: 4,
        print: {
          ...printState,
          error_generation: 2,
          gcode_state: "IDLE",
          print_error: 0,
        },
      });
      const stalePositive = printer("p1", {
        state_revision: stateRevision,
        print: { ...printState, error_generation: 1 },
      });

      const result = mergePrinterEvent([cleared], stalePositive);

      expect(result.kind).toBe("ignored");
      expect(result.printers[0]?.print).toMatchObject({
        error_generation: 2,
        gcode_state: "IDLE",
        print_error: 0,
      });
    },
  );

  it("merges materials by observed_at independently of state revision", () => {
    const current = printer("p1", {
      state_revision: 9,
      status: "RUNNING",
      materials: materials("2026-07-10T00:00:00Z"),
    });
    const lowerRevision = printer("p1", {
      state_revision: 8,
      status: "OFFLINE",
      materials: materials("2026-07-10T00:00:01Z"),
    });

    const result = mergePrinterEvent([current], lowerRevision);

    expect(result.kind).toBe("applied");
    expect(result.printers[0]?.status).toBe("RUNNING");
    expect(result.printers[0]?.state_revision).toBe(9);
    expect(result.printers[0]?.materials?.observed_at).toBe(
      "2026-07-10T00:00:01Z",
    );
  });

  it("clears materials when a higher revision explicitly reports none", () => {
    const current = printer("p1", {
      state_revision: 4,
      materials: materials("2026-07-10T00:00:00Z"),
    });
    const cleared = printer("p1", {
      state_revision: 5,
      materials: null,
    });

    const result = mergePrinterEvent([current], cleared);

    expect(result.kind).toBe("applied");
    expect(result.printers[0]?.materials).toBeNull();
  });

  it.each([3, 4])(
    "does not let revision %s clear newer materials",
    (stateRevision) => {
      const current = printer("p1", {
        state_revision: 4,
        materials: materials("2026-07-10T00:00:00Z"),
      });
      const staleClear = printer("p1", {
        state_revision: stateRevision,
        materials: null,
      });

      const result = mergePrinterEvent([current], staleClear);

      expect(result.kind).toBe("ignored");
      expect(result.printers[0]?.materials?.observed_at).toBe(
        "2026-07-10T00:00:00Z",
      );
    },
  );

  it("accepts newer materials from a legacy event without overwriting enriched state", () => {
    const current = printer("p1", {
      state_revision: 9,
      status: "RUNNING",
      materials: materials("2026-07-10T00:00:00Z"),
    });
    const legacy = printer("p1", {
      state_revision: undefined,
      print: undefined,
      status: "OFFLINE",
      materials: materials("2026-07-10T00:00:02Z"),
    });

    const result = mergePrinterEvent([current], legacy);

    expect(result.kind).toBe("applied");
    expect(result.printers[0]).toMatchObject({
      status: "RUNNING",
      state_revision: 9,
      print: { progress_percent: 25 },
      materials: { observed_at: "2026-07-10T00:00:02Z" },
    });
  });

  it("never inserts an unknown event before an authoritative repair", () => {
    const incoming = printer("new-printer", { state_revision: 3 });

    const result = mergePrinterEvent([], incoming);

    expect(result).toEqual({ kind: "resync", printers: [] });
  });

  it("discards an unknown event when the confirmation baseline still omits it", () => {
    const incoming = printer("deleted", { state_revision: 8 });
    const confirmed = replacePrinterInventory([printer("other")]);

    const result = mergePrinterEvent(confirmed, incoming);

    expect(result.kind).toBe("resync");
    expect(result.printers.map(({ id }) => id)).toEqual(["other"]);
  });

  it("uses a newly present REST printer as baseline before replaying a higher event", () => {
    const confirmed = replacePrinterInventory([
      printer("new-printer", {
        state_revision: 2,
        print: { ...printState, progress_percent: 10 },
      }),
    ]);
    const buffered = printer("new-printer", {
      state_revision: 3,
      print: { ...printState, progress_percent: 20 },
    });

    const result = mergePrinterEvent(confirmed, buffered);

    expect(result.kind).toBe("applied");
    expect(result.printers[0]).toMatchObject({
      state_revision: 3,
      print: { progress_percent: 20 },
    });
  });

  it("keeps legacy rows displayable without exposing enriched recovery state", () => {
    const legacy = printer("legacy", {
      state_revision: undefined,
      print: undefined,
      status: "IDLE",
    });

    const [row] = replacePrinterInventory([legacy]);

    expect(row).toMatchObject({ id: "legacy", status: "IDLE" });
    expect(row?.state_revision).toBeUndefined();
    expect(row?.print).toBeUndefined();
    expect(isEnrichedPrinter(row!)).toBe(false);
  });

  it("clears recovery eligibility while retaining coarse inventory", () => {
    const [row] = clearEnrichedPrinterState([
      printer("p1", { status: "PAUSE", state_revision: 7 }),
    ]);

    expect(row).toMatchObject({ id: "p1", status: "PAUSE" });
    expect(row?.state_revision).toBeUndefined();
    expect(row?.print).toBeUndefined();
  });
});
