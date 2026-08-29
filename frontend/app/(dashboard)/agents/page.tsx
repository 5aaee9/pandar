import {
  getDashboardRequestContext,
  getMembershipForRequest,
} from "../../dashboard-data";
import { adminAccessUnavailable } from "../../membership-policy";
import { AgentsPageClient } from "./agents-page-client";

export default async function AgentsPage({
  searchParams,
}: {
  searchParams: Promise<{
    command?: string | string[];
    discovery?: string | string[];
  }>;
}) {
  const [params, context] = await Promise.all([
    searchParams,
    getDashboardRequestContext(),
  ]);
  const { auth, selectedTenant } = context;

  if (!selectedTenant) {
    return <div>No tenant selected</div>;
  }

  const membership =
    auth.provider !== "none"
      ? await getMembershipForRequest(selectedTenant.id)
      : { role: null, error: null };
  const commandId = Array.isArray(params.command)
    ? params.command[0]
    : (params.command ?? null);
  const discoveryId = Array.isArray(params.discovery)
    ? params.discovery[0]
    : (params.discovery ?? null);

  return (
    <AgentsPageClient
      auth={auth}
      selectedTenant={selectedTenant}
      adminUnavailable={adminAccessUnavailable(auth.provider, membership)}
      commandId={commandId}
      discoveryId={discoveryId}
    />
  );
}
