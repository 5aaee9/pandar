import { Suspense } from "react";
import { redirect } from "next/navigation";

import {
  getAuthForRequest,
  getIdentityForRequest,
  getTenantsForRequest,
  resolveEffectiveTenants,
  dashboardSidebarDefaultOpen,
} from "../dashboard-data";
import { dashboardAuthRedirectTarget } from "../auth-redirect";
import { authProviderConfig } from "../auth-provider";
import { DashboardShellProvider } from "../dashboard-shell-provider";
import { DashboardShellLayout } from "../dashboard-shell-layout";
import { OnboardingPanel } from "../onboarding-panel";

const configuredTenantId = process.env.APP_TENANT_ID;

export default async function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const [auth, identity, tenantsResult] = await Promise.all([
    getAuthForRequest(),
    getIdentityForRequest(),
    getTenantsForRequest(),
  ]);

  const authProvider = authProviderConfig();
  const redirectTarget = dashboardAuthRedirectTarget({
    source: auth.source,
    provider: authProvider,
    meStatus: identity.status ?? undefined,
  });
  if (redirectTarget) {
    redirect(redirectTarget);
  }

  const effectiveTenants = resolveEffectiveTenants(
    tenantsResult.tenants,
    identity.me,
    configuredTenantId,
    auth.provider,
  );

  if (effectiveTenants.length === 0 && auth.provider !== "none" && identity.me) {
    return <OnboardingPanel me={identity.me} />;
  }

  const sidebarDefaultOpen = await dashboardSidebarDefaultOpen();

  return (
    <Suspense fallback={null}>
      <DashboardShellProvider initialTenants={effectiveTenants}>
        <DashboardShellLayout
          sidebarDefaultOpen={sidebarDefaultOpen}
          tenants={effectiveTenants}
          auth={auth}
        >
          {children}
        </DashboardShellLayout>
      </DashboardShellProvider>
    </Suspense>
  );
}
