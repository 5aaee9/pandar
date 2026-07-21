import {
  getAuthForRequest,
  getIdentityForRequest,
  getTenantsForRequest,
  getMembershipForRequest,
  resolveEffectiveTenants,
  resolveSelectedTenant,
} from "../../dashboard-data";
import { AgentsPageClient } from "./agents-page-client";

const configuredTenantId = process.env.APP_TENANT_ID;

export default async function AgentsPage({
  searchParams,
}: {
  searchParams: Promise<{ tenant?: string | string[]; command?: string | string[] }>;
}) {
  const params = await searchParams;
  const [auth, identity, tenantsResult] = await Promise.all([
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
  const selectedTenant = resolveSelectedTenant(params, effectiveTenants);

  if (!selectedTenant) {
    return <div>No tenant selected</div>;
  }

  const membership = auth.provider !== "none"
    ? await getMembershipForRequest(selectedTenant.id)
    : { role: null, error: null };
  const commandId = Array.isArray(params.command) ? params.command[0] : params.command ?? null;
  const adminUnavailable = auth.provider !== "none" && (membership.role !== "tenant_admin" || membership.error !== null);

  return (
    <AgentsPageClient
      auth={auth}
      selectedTenant={selectedTenant}
      adminUnavailable={adminUnavailable}
      commandId={commandId}
    />
  );
}
