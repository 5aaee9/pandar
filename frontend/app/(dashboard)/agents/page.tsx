import {
  getAuthForRequest,
  getIdentityForRequest,
  getTenantsForRequest,
  getMembershipForRequest,
  resolveEffectiveTenants,
  resolveSelectedTenant,
  loadAgentsRoute,
} from "../../dashboard-data";
import { DashboardRouteRegistrar } from "../../dashboard-route-registrar";
import { DashboardRouteConsumer } from "../../dashboard-route-consumer";
import { DashboardViewContent } from "../../dashboard-view-content";

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
  const routeData = await loadAgentsRoute(selectedTenant.id, commandId);
  const adminUnavailable = auth.provider !== "none" && (membership.role !== "tenant_admin" || membership.error !== null);

  return (
    <>
      <DashboardRouteRegistrar
        view="agents"
        tenant={selectedTenant}
        command={commandId}
        status={null}
        errors={routeData.error ? [routeData.error] : []}
        actionStatus={null}
        initialPrinters={routeData.printers}
        initialJobs={[]}
      />
      <DashboardRouteConsumer
        view="agents"
        selectedTenant={selectedTenant}
        initialPrinters={routeData.printers}
        initialJobs={[]}
      >
        {(liveData) => (
          <DashboardViewContent
            view="agents"
            auth={auth}
            selectedTenant={selectedTenant}
            printers={liveData.printers}
            agents={routeData.agents}
            jobs={[]}
            health={{
              printersTotal: liveData.printers.length,
              printersOnline: liveData.printers.filter((p) => p.status === "online").length,
              agentsTotal: routeData.agents.length,
              agentsConnected: routeData.agents.filter((a) => a.status === "online").length,
              jobsActive: 0,
              jobsFailed: 0,
            }}
            attentionItems={[]}
            topSeverity={null}
            liveState="idle"
            lastEventAt={null}
            fleetEmpty={liveData.printers.length === 0}
            nowMs={0}
            selectedCommand={routeData.command}
            commandData={routeData.commandData}
            notifications={[]}
            users={[]}
            userIdentities={[]}
            tenantTokens={[]}
            joinLinks={[]}
            auditEvents={[]}
            adminUnavailable={adminUnavailable}
            adminLoadError={false}
            canManageJobs={true}
          />
        )}
      </DashboardRouteConsumer>
    </>
  );
}
