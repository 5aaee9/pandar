import type { Printer, PrinterMaterials } from "./dashboard-types";

export type MergeResult =
  | { kind: "applied"; printers: Printer[] }
  | { kind: "ignored"; printers: Printer[] }
  | { kind: "resync"; printers: Printer[] };

type EnrichedPrinter = Printer & {
  state_revision: number;
  print: Printer["print"];
};

export function isEnrichedPrinter(
  printer: Printer,
): printer is EnrichedPrinter {
  return printer.state_revision !== undefined && printer.print !== undefined;
}

export function replacePrinterInventory(authoritative: Printer[]): Printer[] {
  return authoritative.map(normalizeAuthoritativePrinter);
}

export function clearEnrichedPrinterState(printers: Printer[]): Printer[] {
  return printers.map(withoutEnrichedState);
}

export function mergePrinterEvent(
  current: Printer[],
  incoming: Printer,
): MergeResult {
  const index = current.findIndex(({ id }) => id === incoming.id);
  if (index === -1) {
    return { kind: "resync", printers: current };
  }

  const previous = current[index]!;
  const materials = latestMaterials(previous.materials, incoming.materials);
  let next: Printer;

  if (isEnrichedPrinter(previous)) {
    if (
      isEnrichedPrinter(incoming) &&
      incoming.state_revision > previous.state_revision
    ) {
      next = { ...incoming, materials };
    } else if (materials !== previous.materials) {
      next = { ...previous, materials };
    } else {
      return { kind: "ignored", printers: current };
    }
  } else if (isEnrichedPrinter(incoming)) {
    if (materials === previous.materials) {
      return { kind: "ignored", printers: current };
    }
    next = { ...previous, materials };
  } else {
    next = { ...withoutEnrichedState(incoming), materials };
  }

  const printers = [...current];
  printers[index] = next;
  return { kind: "applied", printers };
}

export function replayPrinterEvents(
  baseline: Printer[],
  buffer: Printer[],
): { printers: Printer[]; resync: boolean } {
  let printers = baseline;
  let resync = false;
  for (const incoming of buffer) {
    const result = mergePrinterEvent(printers, incoming);
    if (result.kind === "resync") {
      resync = true;
    } else {
      printers = result.printers;
    }
  }
  return { printers, resync };
}

function normalizeAuthoritativePrinter(printer: Printer): Printer {
  return isEnrichedPrinter(printer) ? printer : withoutEnrichedState(printer);
}

function withoutEnrichedState(printer: Printer): Printer {
  const { state_revision: _stateRevision, print: _print, ...coarse } = printer;
  return coarse;
}

function latestMaterials(
  current: PrinterMaterials | null,
  incoming: PrinterMaterials | null,
): PrinterMaterials | null {
  if (incoming === null) {
    return current;
  }
  if (current === null) {
    return incoming;
  }
  return Date.parse(incoming.observed_at) > Date.parse(current.observed_at)
    ? incoming
    : current;
}
