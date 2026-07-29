import { hubProxy } from "@/app/hub-proxy";

export const dynamic = "force-dynamic";

export const POST = hubProxy<{ tenantId: string }>({
  method: "POST",
  path: "/artifact-metadata-preview",
  body: "stream",
  contentType: "forward",
});
