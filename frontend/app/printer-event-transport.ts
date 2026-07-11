import type {
  Printer,
  PrinterEventTicket,
  PrinterList,
} from "./dashboard-types";
import { printerEventWebSocketUrl } from "./dashboard-runtime-helpers";

export async function fetchAuthoritativePrinters(
  tenantId: string,
  controller: AbortController,
  deadline: number,
): Promise<Printer[]> {
  const response = await fetch(
    `/api/tenants/${encodeURIComponent(tenantId)}/printers`,
    { cache: "no-store", signal: controller.signal },
  );
  if (!response.ok) {
    throw new Error(`printers ${response.status}`);
  }
  const body = await response.text();
  const parsed = JSON.parse(body) as PrinterList;
  if (!parsed || !Array.isArray(parsed.printers)) {
    throw new Error("invalid printer list");
  }
  if (performance.now() >= deadline) {
    controller.abort();
    throw new Error("printer list deadline exceeded");
  }
  return parsed.printers;
}

export async function requestPrinterEventTicket(
  tenantId: string,
  signal: AbortSignal,
): Promise<string> {
  const response = await fetch(
    `/api/tenants/${encodeURIComponent(tenantId)}/printer-events/ticket`,
    { method: "POST", signal },
  );
  if (!response.ok) {
    throw new Error(`ticket ${response.status}`);
  }
  return ((await response.json()) as PrinterEventTicket).ticket;
}

export function printerEventConnectionUrl(
  apiUrl: string,
  tenantId: string,
  ticket: string | null,
): string {
  if (ticket !== null) {
    return printerEventWebSocketUrl(apiUrl, tenantId, ticket);
  }
  const base = new URL(apiUrl);
  const basePath = base.pathname.replace(/\/$/, "");
  const url = new URL(
    `${basePath}/api/v1/tenants/${encodeURIComponent(tenantId)}/printer-events`,
    base,
  );
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url.toString();
}
