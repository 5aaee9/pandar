import { hubProxy } from "@/app/hub-proxy";

export const dynamic = "force-dynamic";

export const GET = hubProxy<{ tenantId: string; commandId: string }>({
  method: "GET",
  path: "/commands/{commandId}",
});
