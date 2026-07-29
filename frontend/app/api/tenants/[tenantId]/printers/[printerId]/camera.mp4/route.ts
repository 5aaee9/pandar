import { hubProxy } from "@/app/hub-proxy";

export const dynamic = "force-dynamic";

export const GET = hubProxy<{ tenantId: string; printerId: string }>({
  method: "GET",
  path: "/printers/{printerId}/camera.mp4",
  contentTypeFallback: "video/mp4",
});
