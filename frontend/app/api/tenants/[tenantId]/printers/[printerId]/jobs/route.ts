import { hubProxy } from "@/app/hub-proxy";

export const dynamic = "force-dynamic";

export const POST = hubProxy<{ tenantId: string; printerId: string }>({
  method: "POST",
  path: "/printers/{printerId}/jobs",
  body: "stream",
  contentType: "forward",
});
