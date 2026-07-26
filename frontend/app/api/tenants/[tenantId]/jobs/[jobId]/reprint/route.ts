import { NextResponse } from "next/server";

import { apiHeaders } from "../../../../../../api-auth";
import { apiIdSegment, invalidApiIdResponse, isApiId } from "@/app/api-path";
import { rejectCrossOriginMutation } from "@/app/request-security";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

export const dynamic = "force-dynamic";

type RouteContext = {
  params: Promise<{
    tenantId: string;
    jobId: string;
  }>;
};

export async function POST(request: Request, context: RouteContext) {
  const rejected = rejectCrossOriginMutation(request);
  if (rejected) return rejected;
  const { tenantId, jobId } = await context.params;
  if (!isApiId(tenantId) || !isApiId(jobId)) {
    return invalidApiIdResponse();
  }
  const headers = new Headers(await apiHeaders());
  headers.set("content-type", "application/json");
  const upstreamUrl = `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, "tenant_id")}/jobs/${apiIdSegment(jobId, "job_id")}/reprint`;
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
  const headers = new Headers({ "cache-control": "no-store" });
  const contentType = source.get("content-type");
  if (contentType) headers.set("content-type", contentType);
  return headers;
}
