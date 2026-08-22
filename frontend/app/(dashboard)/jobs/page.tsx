import {
  getAuthForRequest,
  getIdentityForRequest,
  getTenantsForRequest,
  getMembershipForRequest,
  getSelectedTenantId,
  resolveEffectiveTenants,
  resolveSelectedTenant,
} from "../../dashboard-data";
import { JobsPageClient } from "./jobs-page-client";

const configuredTenantId = process.env.APP_TENANT_ID;

export default async function JobsPage() {
  const [tenantId, auth, identity, tenantsResult] = await Promise.all([
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
  const canManageJobs =
    auth.provider === "none" || membership.role !== "viewer";

  return (
    <JobsPageClient
      auth={auth}
      selectedTenant={selectedTenant}
      canManageJobs={canManageJobs}
    />
  );
}
