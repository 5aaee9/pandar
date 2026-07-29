import { hubProxy } from "@/app/hub-proxy";

export const dynamic = "force-dynamic";

export const DELETE = hubProxy<{ tenantId: string; jobId: string }>({
  method: "DELETE",
  path: "/jobs/{jobId}",
});
