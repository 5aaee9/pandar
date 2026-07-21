import {
  getAuthForRequest,
  getIdentityForRequest,
  getTenantsForRequest,
  getMembershipForRequest,
  resolveEffectiveTenants,
  resolveSelectedTenant,
} from "../../dashboard-data";
import { SettingsStaticPanels } from "../../settings-static-panels";
import { TenantSettingsStatic } from "../../tenant-settings-static";
import { TenantSettingsLivePrinters } from "../../tenant-settings-live-printers";
import { LanguageSwitcher } from "../../../components/language-switcher";
import { ThemeSwitcher } from "../../../components/theme-switcher";
import { SettingsPageClient } from "./settings-page-client";

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

  return (
    <SettingsPageClient
      auth={auth}
      selectedTenant={selectedTenant}
      membership={membership}
      settingsStaticPanels={
        <SettingsStaticPanels
          languageSwitcher={<LanguageSwitcher />}
          themeSwitcher={<ThemeSwitcher />}
        />
      }
      tenantSettingsStatic={
        <TenantSettingsStatic
          tenant={selectedTenant}
          agents={[]}
          auth={auth}
          livePrintersSlot={
            <TenantSettingsLivePrinters
              initialPrinters={[]}
              selectedTenant={selectedTenant}
            />
          }
        />
      }
    />
  );
}
