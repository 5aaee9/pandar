import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";

import type { Command } from "./dashboard-types";
import { showCommandResult } from "./dashboard-runtime-event-notifications";
import { parsePrinterOperationPayload } from "./printer-operation-payload";

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

const recoveryPayload = {
  printer_id: "printer-1",
  serial_number: "20P123",
  operation: {
    type: "handle_print_error",
    error_action: "resume",
    print_error: 83_918_929,
    printer_job_id: "native-job",
    sequence_id: 0,
  },
};

describe("parsePrinterOperationPayload", () => {
  it("parses the typed Rust HandlePrintError payload with numeric sequence zero", () => {
    expect(parsePrinterOperationPayload(JSON.stringify(recoveryPayload))).toEqual(
      recoveryPayload,
    );
  });

  it.each(["resume", "ignore", "stop"] as const)(
    "accepts the closed native error action %s",
    (errorAction) => {
      const payload = {
        ...recoveryPayload,
        operation: { ...recoveryPayload.operation, error_action: errorAction },
      };
      expect(parsePrinterOperationPayload(JSON.stringify(payload))).toEqual(payload);
    },
  );

  it("accepts the exact Rust u32 maximum print error", () => {
    const payload = {
      ...recoveryPayload,
      operation: {
        ...recoveryPayload.operation,
        print_error: 4_294_967_295,
      },
    };

    expect(parsePrinterOperationPayload(JSON.stringify(payload))).toEqual(payload);
  });

  it.each([
    ["malformed JSON", "{"],
    ["array", "[]"],
    [
      "unknown operation",
      JSON.stringify({
        ...recoveryPayload,
        operation: { ...recoveryPayload.operation, type: "future" },
      }),
    ],
    [
      "print error above u32 maximum",
      JSON.stringify({
        ...recoveryPayload,
        operation: {
          ...recoveryPayload.operation,
          print_error: 4_294_967_296,
        },
      }),
    ],
    [
      "string sequence",
      JSON.stringify({
        ...recoveryPayload,
        operation: { ...recoveryPayload.operation, sequence_id: "0" },
      }),
    ],
    [
      "unknown error action",
      JSON.stringify({
        ...recoveryPayload,
        operation: { ...recoveryPayload.operation, error_action: "retry" },
      }),
    ],
    [
      "missing printer id",
      JSON.stringify({ serial_number: "20P123", operation: recoveryPayload.operation }),
    ],
  ])("rejects %s", (_label, payloadJson) => {
    expect(parsePrinterOperationPayload(payloadJson)).toBeNull();
  });
});

function command(
  payloadJson: string,
  overrides: Partial<Command> = {},
): Command {
  return {
    id: "command-1",
    tenant_id: "tenant-1",
    agent_id: "agent-1",
    printer_id: "printer-1",
    kind: "printer_operation",
    status: "succeeded",
    payload_json: payloadJson,
    error: null,
    result_json: JSON.stringify({ sequence_id: "0" }),
    created_at: "2026-07-10T00:00:00Z",
    updated_at: "2026-07-10T00:00:01Z",
    ...overrides,
  };
}

const translate = (key: string) =>
  ({
    printerControlCompleted: "Printer control completed",
    printerControlFailed: "Printer control failed",
    recoveryCommandSent:
      "Recovery command sent; waiting for printer status confirmation",
  })[key] ?? key;

describe("recovery command-result classification", () => {
  beforeEach(() => vi.clearAllMocks());

  it.each([
    ["malformed", "{"],
    [
      "unknown",
      JSON.stringify({
        ...recoveryPayload,
        operation: {
          ...recoveryPayload.operation,
          type: "future_operation",
        },
      }),
    ],
    [
      "u32-overflow print error",
      JSON.stringify({
        ...recoveryPayload,
        operation: {
          ...recoveryPayload.operation,
          print_error: 4_294_967_296,
        },
      }),
    ],
    [
      "nonzero",
      JSON.stringify({
        ...recoveryPayload,
        operation: { ...recoveryPayload.operation, sequence_id: 20_042 },
      }),
    ],
  ])("keeps the generic success toast for %s payloads", (_label, payloadJson) => {
    showCommandResult(command(payloadJson), translate);

    expect(toast.success).toHaveBeenCalledWith("Printer control completed", {
      description: "#0",
    });
  });

  it("keeps the existing full failure toast for a sequence-zero recovery", () => {
    showCommandResult(
      command(JSON.stringify(recoveryPayload), {
        status: "failed",
        error: "publish failed: mqtt connection closed: broker reset",
      }),
      translate,
    );

    expect(toast.error).toHaveBeenCalledWith(
      "publish failed: mqtt connection closed: broker reset",
      { description: "#0" },
    );
    expect(toast.success).not.toHaveBeenCalled();
  });
});
