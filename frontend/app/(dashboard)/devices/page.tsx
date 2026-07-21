import {
  getAuthForRequest,
  getTenantsForRequest,
  getIdentityForRequest,
  resolveEffectiveTenants,
  resolveSelectedTenant,
} from "../../dashboard-data";
import { DevicesPageClient } from "./devices-page-client";

const configuredTenantId = process.env.APP_TENANT_ID;

export default async function DevicesPage({
  searchParams,
}: {
  searchParams: Promise<{ tenant?: string | string[] }>;
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

  return (
    <DevicesPageClient
      auth={auth}
      selectedTenant={selectedTenant}
    />
  );
}
