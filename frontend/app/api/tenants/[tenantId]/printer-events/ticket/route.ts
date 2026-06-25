import { NextResponse } from "next/server";

import { apiHeaders } from "../../../../../api-auth";
import type { PrinterEventTicket } from "../../../../../dashboard-types";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

type RouteContext = {
  params: Promise<{
    tenantId: string;
  }>;
};

export async function POST(_request: Request, context: RouteContext) {
  const { tenantId } = await context.params;
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${encodeURIComponent(tenantId)}/printer-events/tickets`,
    {
      method: "POST",
      cache: "no-store",
      headers: await apiHeaders(),
    },
  );

  if (!response.ok) {
    return NextResponse.json(
      { error: "ticket_unavailable" },
      { status: response.status },
    );
  }

  const ticket = (await response.json()) as PrinterEventTicket;
  return NextResponse.json(ticket);
}
