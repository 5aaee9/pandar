import { hubProxy } from "@/app/hub-proxy";

export const dynamic = "force-dynamic";

export const POST = hubProxy<{ tenantId: string; jobId: string }>({
  method: "POST",
  path: "/jobs/{jobId}/reprint",
  body: "stream",
  contentType: "json",
});
