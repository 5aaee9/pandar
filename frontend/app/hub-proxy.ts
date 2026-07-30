import { apiHeaders } from "./api-auth";
import { apiIdSegment, invalidApiIdResponse, isApiId } from "./api-path";
import { rejectCrossOriginMutation } from "./request-security";

const apiUrl = process.env.APP_API_URL ?? "http://localhost:8080";

export type HubProxyConfig = {
  method: "GET" | "POST" | "DELETE";
  // Path suffix after /api/v1/tenants/{tenantId}, e.g. "/jobs/{jobId}/reprint".
  path: string;
  // Declared query string appended to the upstream URL, e.g. "limit=20".
  query?: string;
  body?: "stream";
  // With body: "stream": forward the request's content-type or force JSON.
  contentType?: "forward" | "json";
  // Response content-type when the upstream omits it.
  contentTypeFallback?: string;
};

export function hubProxy<P extends Record<string, string>>(
  config: HubProxyConfig,
): (request: Request, context: { params: Promise<P> }) => Promise<Response> {
  const templateParams = [...config.path.matchAll(/\{(\w+)\}/g)].map(
    (match) => match[1],
  );

  return async (request, context) => {
    if (config.method !== "GET") {
      const rejected = rejectCrossOriginMutation(request);
      if (rejected) return rejected;
    }

    const params = await context.params;
    if (!isApiId(params.tenantId)) {
      return invalidApiIdResponse();
    }
    const encoded: Record<string, string> = {};
    for (const name of templateParams) {
      const value = params[name];
      if (!value || !isApiId(value)) {
        return invalidApiIdResponse();
      }
      encoded[name] = apiIdSegment(value, name);
    }
    const path = config.path.replace(
      /\{(\w+)\}/g,
      (_, name: string) => encoded[name],
    );
    const upstreamUrl = `${apiUrl}/api/v1/tenants/${apiIdSegment(params.tenantId, "tenant_id")}${path}${config.query ? `?${config.query}` : ""}`;

    const headers = new Headers(await apiHeaders());
    const init: RequestInit & { duplex?: "half" } = {
      method: config.method,
      cache: "no-store",
      headers,
    };
    if (config.body === "stream") {
      if (config.contentType === "json") {
        headers.set("content-type", "application/json");
      } else {
        const contentType = request.headers.get("content-type");
        if (contentType) {
          headers.set("content-type", contentType);
        }
      }
      init.body = request.body;
      init.duplex = "half";
    } else {
      init.signal = request.signal;
    }

    const upstream = await fetch(upstreamUrl, init as RequestInit);
    const responseHeaders = new Headers({ "cache-control": "no-store" });
    const contentType =
      upstream.headers.get("content-type") ?? config.contentTypeFallback;
    if (contentType) {
      responseHeaders.set("content-type", contentType);
    }
    return new Response(upstream.body, {
      status: upstream.status,
      statusText: upstream.statusText,
      headers: responseHeaders,
    });
  };
}
