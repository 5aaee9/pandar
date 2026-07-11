export type HandlePrintErrorOperation = {
  type: "handle_print_error";
  error_action: "resume" | "ignore" | "stop";
  print_error: number;
  printer_job_id: string;
  sequence_id: number;
};

export type PrinterOperationPayload = {
  printer_id: string;
  serial_number: string;
  operation: HandlePrintErrorOperation;
};

const U32_MAX = 4_294_967_295;

export function parsePrinterOperationPayload(
  payloadJson: string,
): PrinterOperationPayload | null {
  try {
    const payload = JSON.parse(payloadJson) as unknown;
    if (!isRecord(payload) || !isRecord(payload.operation)) {
      return null;
    }
    const operation = payload.operation;
    if (
      typeof payload.printer_id !== "string" ||
      typeof payload.serial_number !== "string" ||
      operation.type !== "handle_print_error" ||
      !isErrorAction(operation.error_action) ||
      !isU32(operation.print_error) ||
      typeof operation.printer_job_id !== "string" ||
      !isNonnegativeInteger(operation.sequence_id)
    ) {
      return null;
    }
    return {
      printer_id: payload.printer_id,
      serial_number: payload.serial_number,
      operation: {
        type: "handle_print_error",
        error_action: operation.error_action,
        print_error: operation.print_error,
        printer_job_id: operation.printer_job_id,
        sequence_id: operation.sequence_id,
      },
    };
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isErrorAction(
  value: unknown,
): value is HandlePrintErrorOperation["error_action"] {
  return value === "resume" || value === "ignore" || value === "stop";
}

function isNonnegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function isU32(value: unknown): value is number {
  return isNonnegativeInteger(value) && value <= U32_MAX;
}
