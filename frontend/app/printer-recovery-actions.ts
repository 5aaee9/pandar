"use server";

import { apiHeaders, requireAuth } from "./api-auth";
import type { PlateMismatchAction } from "./plate-mismatch-actions";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

export type PrinterRecoveryActionState =
  | { status: "idle" }
  | { status: "sent" }
  | { status: "error"; error: string };

export async function handlePrintError(
  _previousState: PrinterRecoveryActionState,
  formData: FormData,
): Promise<PrinterRecoveryActionState> {
  await requireAuth();
  const tenantId = stringField(formData, "tenant_id").trim();
  const printerId = stringField(formData, "printer_id").trim();
  const errorAction = stringField(formData, "error_action");
  const errorGeneration = Number(stringField(formData, "error_generation"));
  if (
    !tenantId ||
    !printerId ||
    !isPlateMismatchAction(errorAction) ||
    !Number.isSafeInteger(errorGeneration) ||
    errorGeneration <= 0
  ) {
    return { status: "error", error: "invalid_printer_control" };
  }

  let response: Response;
  try {
    response = await fetch(
      `${apiUrl}/api/v1/tenants/${encodeURIComponent(tenantId)}/printers/${encodeURIComponent(printerId)}/controls`,
      {
        method: "POST",
        headers: await apiHeaders("application/json"),
        body: JSON.stringify({
          action: "handle_print_error",
          error_action: errorAction,
          error_generation: errorGeneration,
        }),
      },
    );
  } catch (cause) {
    return {
      status: "error",
      error: `Printer recovery request failed: ${formatErrorCause(cause, true)}`,
    };
  }
  return response.ok
    ? { status: "sent" }
    : { status: "error", error: await responseError(response) };
}

function stringField(formData: FormData, name: string): string {
  const value = formData.get(name);
  return typeof value === "string" ? value : "";
}

function isPlateMismatchAction(value: string): value is PlateMismatchAction {
  return value === "resume" || value === "ignore" || value === "stop";
}

async function responseError(response: Response): Promise<string> {
  let responseText: string;
  try {
    responseText = await response.text();
  } catch (cause) {
    return responseBoundaryError(response.status, "read", cause, true);
  }

  let body: unknown;
  try {
    body = JSON.parse(responseText) as unknown;
  } catch (cause) {
    return responseBoundaryError(response.status, "decode", cause, false);
  }
  if (
    typeof body === "object" &&
    body !== null &&
    !Array.isArray(body) &&
    typeof (body as Record<string, unknown>).error === "string"
  ) {
    return (body as Record<string, string>).error;
  }
  return `http_${response.status}`;
}

function responseBoundaryError(
  status: number,
  phase: "read" | "decode",
  cause: unknown,
  includeMessages: boolean,
): string {
  return `http_${status}: Printer recovery error response ${phase} failed: ${formatErrorCause(cause, includeMessages)}`;
}

function formatErrorCause(cause: unknown, includeMessages: boolean): string {
  if (!(cause instanceof Error)) {
    return includeMessages
      ? `Thrown value: ${redactBoundaryText(String(cause))}`
      : "Unknown error";
  }
  const message = includeMessages ? redactBoundaryText(cause.message) : "";
  const current = message ? `${cause.name}: ${message}` : cause.name;
  return cause.cause === undefined
    ? current
    : `${current}; caused by ${formatErrorCause(cause.cause, includeMessages)}`;
}

function redactBoundaryText(value: string): string {
  return value
    .replace(/https?:\/\/[^\s]+/gi, "[redacted URL]")
    .replace(/\b(Bearer|Basic)\s+[^\s,;]+/gi, "$1 [redacted]");
}
