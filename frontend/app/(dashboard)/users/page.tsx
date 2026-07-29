import {
  getAuthForRequest,
  getIdentityForRequest,
  getTenantsForRequest,
  getMembershipForRequest,
  resolveEffectiveTenants,
  resolveSelectedTenant,
} from "../../dashboard-data";
import { UsersPageClient } from "./users-page-client";

const configuredTenantId = process.env.APP_TENANT_ID;

export default async function UsersPage({
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

  const membership = auth.provider !== "none"
    ? await getMembershipForRequest(selectedTenant.id)
    : { role: null, error: null };
  const adminUnavailable = auth.provider !== "none" && (membership.role !== "tenant_admin" || membership.error !== null);
  const adminLoadError = auth.provider !== "none" && membership.role === "tenant_admin" && membership.error !== null;

  return (
    <UsersPageClient
      selectedTenant={selectedTenant}
      adminUnavailable={adminUnavailable}
      adminLoadError={adminLoadError}
      meEmail={identity.me?.identity.email ?? null}
    />
  );
}
