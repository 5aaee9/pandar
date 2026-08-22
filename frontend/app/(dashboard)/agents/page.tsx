import {
  getAuthForRequest,
  getIdentityForRequest,
  getTenantsForRequest,
  getMembershipForRequest,
  getSelectedTenantId,
  resolveEffectiveTenants,
  resolveSelectedTenant,
} from "../../dashboard-data";
import { AgentsPageClient } from "./agents-page-client";

const configuredTenantId = process.env.APP_TENANT_ID;

export default async function AgentsPage({
  searchParams,
}: {
  searchParams: Promise<{
    command?: string | string[];
    discovery?: string | string[];
  }>;
}) {
  const [params, tenantId, auth, identity, tenantsResult] = await Promise.all([
    searchParams,
    getSelectedTenantId(),
    getAuthForRequest(),
    getIdentityForRequest(),
    getTenantsForRequest(),
  ]);

  const effectiveTenants = resolveEffectiveTenants(
    tenantsResult.tenants,
    identity.me,
    configuredTenantId,
    auth.provider,
  );
  const selectedTenant = resolveSelectedTenant(tenantId, effectiveTenants);

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
  const adminUnavailable =
    auth.provider !== "none" &&
    (membership.role !== "tenant_admin" || membership.error !== null);

  return (
    <AgentsPageClient
      auth={auth}
      selectedTenant={selectedTenant}
      adminUnavailable={adminUnavailable}
      commandId={commandId}
      discoveryId={discoveryId}
    />
  );
}
