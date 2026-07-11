import { apiHeaders } from "../../../../api-auth";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

export const dynamic = "force-dynamic";

export async function GET(
  request: Request,
  context: { params: Promise<{ tenantId: string }> },
): Promise<Response> {
  const { tenantId } = await context.params;
  const upstream = await fetch(
    `${apiUrl}/api/v1/tenants/${encodeURIComponent(tenantId)}/printers`,
    {
      cache: "no-store",
      headers: await apiHeaders(),
      signal: request.signal,
    },
  );

  const headers = new Headers({ "cache-control": "no-store" });
  const contentType = upstream.headers.get("content-type");
  if (contentType) {
    headers.set("content-type", contentType);
  }

  return new Response(upstream.body, {
    status: upstream.status,
    statusText: upstream.statusText,
    headers,
  });
}
