import {
  getAuthForRequest,
  getTenantsForRequest,
  getIdentityForRequest,
  getSelectedTenantId,
  resolveEffectiveTenants,
  resolveSelectedTenant,
} from "../../dashboard-data";
import { DevicesPageClient } from "./devices-page-client";

const configuredTenantId = process.env.APP_TENANT_ID;

export default async function DevicesPage() {
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

  return (
    <DevicesPageClient
      auth={auth}
      selectedTenant={selectedTenant}
    />
  );
}
