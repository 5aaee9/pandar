import { hubProxy } from "@/app/hub-proxy";

export const dynamic = "force-dynamic";

export const GET = hubProxy<{ tenantId: string }>({
  method: "GET",
  path: "/users",
});
