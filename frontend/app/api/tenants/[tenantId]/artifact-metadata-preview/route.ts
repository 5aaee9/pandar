import { NextResponse } from "next/server";

import { apiHeaders } from "../../../../api-auth";
import { apiIdSegment, invalidApiIdResponse, isApiId } from "@/app/api-path";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

export const dynamic = "force-dynamic";

type RouteContext = {
  params: Promise<{
    tenantId: string;
  }>;
};

export async function POST(request: Request, context: RouteContext) {
  const { tenantId } = await context.params;
  if (!isApiId(tenantId)) {
    return invalidApiIdResponse();
  }
  const headers = new Headers(await apiHeaders());
  const contentType = request.headers.get("content-type");
  if (contentType) {
    headers.set("content-type", contentType);
  }
  const upstreamUrl = `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/artifact-metadata-preview`;

  const response = await fetch(upstreamUrl, {
    method: "POST",
    cache: "no-store",
    headers,
    body: request.body,
    duplex: "half",
  } as RequestInit & { duplex: "half" });

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
