import { redirect } from "next/navigation";

import {
  getDashboardRequestContext,
  dashboardSidebarDefaultOpen,
} from "../dashboard-data";
import { dashboardAuthRedirectTarget } from "../auth-redirect";
import { authProviderConfig } from "../auth-provider";
import { DashboardShellProvider } from "../dashboard-shell-provider";
import { DashboardShellLayout } from "../dashboard-shell-layout";
import { OnboardingPanel } from "../onboarding-panel";

export default async function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const { auth, identity, effectiveTenants, selectedTenant } =
    await getDashboardRequestContext();

  const authProvider = authProviderConfig();
  const redirectTarget = dashboardAuthRedirectTarget({
    source: auth.source,
    provider: authProvider,
    meStatus: identity.status ?? undefined,
  });
  if (redirectTarget) {
    redirect(redirectTarget);
  }

  if (
    effectiveTenants.length === 0 &&
    auth.provider !== "none" &&
    identity.me
  ) {
    return <OnboardingPanel me={identity.me} />;
  }

  const sidebarDefaultOpen = await dashboardSidebarDefaultOpen();

  return (
    <DashboardShellProvider selectedTenant={selectedTenant}>
      <DashboardShellLayout
        sidebarDefaultOpen={sidebarDefaultOpen}
        tenants={effectiveTenants}
        auth={auth}
      >
        {children}
      </DashboardShellLayout>
    </DashboardShellProvider>
  );
}
