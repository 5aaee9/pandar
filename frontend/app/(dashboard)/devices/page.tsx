import {
  getAuthForRequest,
  getTenantsForRequest,
  getIdentityForRequest,
  resolveEffectiveTenants,
  resolveSelectedTenant,
  loadDevicesRoute,
} from "../../dashboard-data";
import { DashboardRouteRegistrar } from "../../dashboard-route-registrar";
import { DashboardRouteConsumer } from "../../dashboard-route-consumer";
import { DashboardViewContent } from "../../dashboard-view-content";

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

  const routeData = await loadDevicesRoute(selectedTenant.id);

  return (
    <>
      <DashboardRouteRegistrar
        view="devices"
        tenant={selectedTenant}
        command={null}
        status={null}
        errors={routeData.error ? [routeData.error] : []}
        actionStatus={null}
        initialPrinters={routeData.printers}
        initialJobs={routeData.jobs}
      />
      <DashboardRouteConsumer
        view="devices"
        selectedTenant={selectedTenant}
        initialPrinters={routeData.printers}
        initialJobs={routeData.jobs}
      >
        {(liveData) => (
          <DashboardViewContent
            view="devices"
            auth={auth}
            selectedTenant={selectedTenant}
            printers={liveData.printers}
            agents={routeData.agents}
            jobs={liveData.jobs}
            health={{
              printersTotal: liveData.printers.length,
              printersOnline: liveData.printers.filter((p) => p.status === "online").length,
              agentsTotal: routeData.agents.length,
              agentsConnected: routeData.agents.filter((a) => a.status === "online").length,
              jobsActive: liveData.jobs.filter((j) => j.status === "running").length,
              jobsFailed: liveData.jobs.filter((j) => j.status === "failed").length,
            }}
            attentionItems={[]}
            topSeverity={null}
            liveState="idle"
            lastEventAt={null}
            fleetEmpty={liveData.printers.length === 0}
            nowMs={0}
            selectedCommand={null}
            commandData={null}
            notifications={[]}
            users={[]}
            userIdentities={[]}
            tenantTokens={[]}
            joinLinks={[]}
            auditEvents={[]}
            adminUnavailable={false}
            adminLoadError={false}
            canManageJobs={true}
          />
        )}
      </DashboardRouteConsumer>
    </>
  );
}
