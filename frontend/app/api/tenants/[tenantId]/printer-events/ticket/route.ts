import { NextResponse } from "next/server";

import { apiHeaders } from "../../../../../api-auth";
import { apiIdSegment, invalidApiIdResponse, isApiId } from "@/app/api-path";
import type { PrinterEventTicket } from "../../../../../dashboard-types";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

export const dynamic = "force-dynamic";

type RouteContext = {
  params: Promise<{
    tenantId: string;
  }>;
};

export async function POST(_request: Request, context: RouteContext) {
  const { tenantId } = await context.params;
  if (!isApiId(tenantId)) {
    return invalidApiIdResponse();
  }
  const upstreamUrl = `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/printer-events/tickets`;
  const response = await fetch(upstreamUrl, {
    method: "POST",
    cache: "no-store",
    headers: await apiHeaders(),
  });

  if (!response.ok) {
    return NextResponse.json(
      { error: "ticket_unavailable" },
      { status: response.status },
    );
  }

  const ticket = (await response.json()) as PrinterEventTicket;
  return NextResponse.json(ticket);
}
