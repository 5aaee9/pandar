import { hubProxy } from "@/app/hub-proxy";

export const dynamic = "force-dynamic";

export const DELETE = hubProxy<{ tenantId: string }>({
  method: "DELETE",
  path: "/jobs",
});
