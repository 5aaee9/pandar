import {
  getAuthForRequest,
  getIdentityForRequest,
  getTenantsForRequest,
  getMembershipForRequest,
  resolveEffectiveTenants,
  resolveSelectedTenant,
  loadUsersRoute,
} from "../../dashboard-data";
import { DashboardRouteRegistrar } from "../../dashboard-route-registrar";
import { DashboardRouteConsumer } from "../../dashboard-route-consumer";
import { DashboardViewContent } from "../../dashboard-view-content";

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
  const routeData = await loadUsersRoute(selectedTenant.id);
  const adminUnavailable = auth.provider !== "none" && (membership.role !== "tenant_admin" || membership.error !== null || routeData.adminError !== null);
  const adminLoadError = auth.provider !== "none" && membership.role === "tenant_admin" && (membership.error !== null || routeData.adminError !== null);

  return (
    <>
      <DashboardRouteRegistrar
        view="users"
        tenant={selectedTenant}
        command={null}
        status={null}
        errors={routeData.adminError ? [routeData.adminError] : []}
        actionStatus={null}
        initialPrinters={[]}
        initialJobs={[]}
      />
      <DashboardRouteConsumer
        view="users"
        selectedTenant={selectedTenant}
        initialPrinters={[]}
        initialJobs={[]}
      >
        {() => (
          <DashboardViewContent
            view="users"
            auth={auth}
            selectedTenant={selectedTenant}
            printers={[]}
            agents={[]}
            jobs={[]}
            health={{
              printersTotal: 0,
              printersOnline: 0,
              agentsTotal: 0,
              agentsConnected: 0,
              jobsActive: 0,
              jobsFailed: 0,
            }}
            attentionItems={[]}
            topSeverity={null}
            liveState="idle"
            lastEventAt={null}
            fleetEmpty={true}
            nowMs={0}
            selectedCommand={null}
            commandData={null}
            notifications={[]}
            users={routeData.users}
            userIdentities={routeData.identities}
            tenantTokens={[]}
            joinLinks={routeData.joinLinks}
            auditEvents={[]}
            adminUnavailable={adminUnavailable}
            adminLoadError={adminLoadError}
            canManageJobs={true}
          />
        )}
      </DashboardRouteConsumer>
    </>
  );
}
