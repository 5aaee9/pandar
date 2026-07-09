import { NextRequest } from "next/server";

import { apiHeaders } from "@/app/api-auth";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

export async function GET(
  _request: NextRequest,
  { params }: { params: Promise<{ tenantId: string; printerId: string }> },
) {
  const { tenantId, printerId } = await params;
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/printers/${printerId}/camera.mp4`,
    {
      headers: await apiHeaders(),
      cache: "no-store",
    },
  );

  return new Response(response.body, {
    status: response.status,
    headers: {
      "cache-control": "no-store",
      "content-type": response.headers.get("content-type") ?? "video/mp4",
    },
  });
}
