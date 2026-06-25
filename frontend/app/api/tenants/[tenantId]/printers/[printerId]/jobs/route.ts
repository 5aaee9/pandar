import { NextResponse } from "next/server";

import { apiHeaders } from "../../../../../../api-auth";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

type RouteContext = {
  params: Promise<{
    tenantId: string;
    printerId: string;
  }>;
};

export async function POST(request: Request, context: RouteContext) {
  const { tenantId, printerId } = await context.params;
  const headers = new Headers(await apiHeaders());
  const contentType = request.headers.get("content-type");
  if (contentType) {
    headers.set("content-type", contentType);
  }

  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${encodeURIComponent(tenantId)}/printers/${encodeURIComponent(printerId)}/jobs`,
    {
      method: "POST",
      cache: "no-store",
      headers,
      body: request.body,
      duplex: "half",
    } as RequestInit & { duplex: "half" },
  );

  return new NextResponse(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: responseHeaders(response.headers),
  });
}

function responseHeaders(source: Headers) {
  const headers = new Headers();
  const contentType = source.get("content-type");
  if (contentType) {
    headers.set("content-type", contentType);
  }
  return headers;
}
