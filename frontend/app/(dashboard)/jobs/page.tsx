import {
  getAuthForRequest,
  getIdentityForRequest,
  getTenantsForRequest,
  getMembershipForRequest,
  resolveEffectiveTenants,
  resolveSelectedTenant,
  loadJobsRoute,
} from "../../dashboard-data";
import { DashboardRouteRegistrar } from "../../dashboard-route-registrar";
import { DashboardRouteConsumer } from "../../dashboard-route-consumer";
import { DashboardViewContent } from "../../dashboard-view-content";

const configuredTenantId = process.env.APP_TENANT_ID;

export default async function JobsPage({
  searchParams,
}: {
  searchParams: Promise<{ tenant?: string | string[]; status?: string | string[] }>;
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
  const routeData = await loadJobsRoute(selectedTenant.id);
  const canManageJobs = auth.provider === "none" || membership.role !== "viewer";

  return (
    <>
      <DashboardRouteRegistrar
        view="jobs"
        tenant={selectedTenant}
        command={null}
        status={Array.isArray(params.status) ? params.status[0] : params.status ?? null}
        errors={routeData.error ? [routeData.error] : []}
        actionStatus={null}
        initialPrinters={routeData.printers}
        initialJobs={routeData.jobs}
      />
      <DashboardRouteConsumer
        view="jobs"
        selectedTenant={selectedTenant}
        initialPrinters={routeData.printers}
        initialJobs={routeData.jobs}
      >
        {(liveData) => (
          <DashboardViewContent
            view="jobs"
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
            canManageJobs={canManageJobs}
          />
        )}
      </DashboardRouteConsumer>
    </>
  );
}
