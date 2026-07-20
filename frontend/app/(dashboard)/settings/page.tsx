import {
  getAuthForRequest,
  getIdentityForRequest,
  getTenantsForRequest,
  getMembershipForRequest,
  resolveEffectiveTenants,
  resolveSelectedTenant,
  loadSettingsRoute,
} from "../../dashboard-data";
import { DashboardRouteRegistrar } from "../../dashboard-route-registrar";
import { DashboardRouteConsumer } from "../../dashboard-route-consumer";
import { DashboardViewContent } from "../../dashboard-view-content";
import { SettingsStaticPanels } from "../../settings-static-panels";
import { TenantSettingsStatic } from "../../tenant-settings-static";
import { TenantSettingsLivePrinters } from "../../tenant-settings-live-printers";
import { LanguageSwitcher } from "../../../components/language-switcher";
import { ThemeSwitcher } from "../../../components/theme-switcher";

const configuredTenantId = process.env.APP_TENANT_ID;

export default async function SettingsPage({
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
  const routeData = await loadSettingsRoute(selectedTenant.id);
  const adminUnavailable = auth.provider !== "none" && (membership.role !== "tenant_admin" || membership.error !== null || routeData.adminError !== null);
  const adminLoadError = auth.provider !== "none" && membership.role === "tenant_admin" && (membership.error !== null || routeData.adminError !== null);

  return (
    <>
      <DashboardRouteRegistrar
        view="settings"
        tenant={selectedTenant}
        command={null}
        status={null}
        errors={routeData.adminError ? [routeData.adminError] : []}
        actionStatus={null}
        initialPrinters={routeData.printers}
        initialJobs={[]}
      />
      <DashboardRouteConsumer
        view="settings"
        selectedTenant={selectedTenant}
        initialPrinters={routeData.printers}
        initialJobs={[]}
      >
        {(liveData) => (
          <DashboardViewContent
            view="settings"
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
            selectedCommand={null}
            commandData={null}
            notifications={[]}
            users={[]}
            userIdentities={[]}
            tenantTokens={routeData.tenantTokens}
            joinLinks={[]}
            auditEvents={routeData.auditEvents}
            adminUnavailable={adminUnavailable}
            adminLoadError={adminLoadError}
            canManageJobs={true}
            settingsStaticPanels={
              <SettingsStaticPanels
                languageSwitcher={<LanguageSwitcher />}
                themeSwitcher={<ThemeSwitcher />}
              />
            }
            tenantSettingsStatic={
              <TenantSettingsStatic
                tenant={selectedTenant}
                agents={routeData.agents}
                auth={auth}
                livePrintersSlot={
                  <TenantSettingsLivePrinters
                    initialPrinters={liveData.printers}
                    selectedTenant={selectedTenant}
                  />
                }
              />
            }
          />
        )}
      </DashboardRouteConsumer>
    </>
  );
}
