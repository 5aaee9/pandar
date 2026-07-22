"use client";

import { useQuery } from "@tanstack/react-query";

import { apiClient } from "../../api-client";
import { DashboardViewContent } from "../../dashboard-view-content";
import { QueryErrorBoundary } from "../../query-error-boundary";
import type { AuthMetadata, Tenant } from "../../dashboard-types";

export function SettingsPageClient({
  auth,
  selectedTenant,
  membership,
  settingsStaticPanels,
  tenantSettingsStatic,
}: {
  auth: AuthMetadata;
  selectedTenant: Tenant;
  membership: { role: string | null; error: string | null };
  settingsStaticPanels: React.ReactNode;
  tenantSettingsStatic: React.ReactNode;
}) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["route", "settings", selectedTenant.id],
    queryFn: async () => {
      const [tenantTokens, agents, printers, auditEvents] = await Promise.all([
        apiClient.settings.tenantTokens(selectedTenant.id),
        apiClient.agents.list(selectedTenant.id),
        apiClient.printers.list(selectedTenant.id),
        apiClient.settings.auditEvents(selectedTenant.id),
      ]);
      return {
        tenantTokens: tenantTokens.tenant_tokens,
        agents: agents.agents,
        printers: printers.printers,
        auditEvents: auditEvents.audit_events,
      };
    },
    staleTime: 60 * 1000,
  });

  const adminUnavailable =
    auth.provider !== "none" &&
    (membership.role !== "tenant_admin" || membership.error !== null || error !== null);
  const adminLoadError =
    auth.provider !== "none" &&
    membership.role === "tenant_admin" &&
    (membership.error !== null || error !== null);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center p-8">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-muted border-t-primary" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="rounded-md border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive">
        Failed to load settings: {error instanceof Error ? error.message : "Unknown error"}
      </div>
    );
  }

  const { tenantTokens, agents, printers, auditEvents } = data ?? {
    tenantTokens: [],
    agents: [],
    printers: [],
    auditEvents: [],
  };

  return (
    <QueryErrorBoundary>
      <DashboardViewContent
      view="settings"
      auth={auth}
      selectedTenant={selectedTenant}
      printers={printers}
      agents={agents}
      jobs={[]}
      health={{
        printersTotal: printers.length,
        printersOnline: printers.filter((p) => p.status === "online").length,
        agentsTotal: agents.length,
        agentsConnected: agents.filter((a) => a.status === "online").length,
        jobsActive: 0,
        jobsFailed: 0,
      }}
      attentionItems={[]}
      topSeverity={null}
      liveState="idle"
      lastEventAt={null}
      fleetEmpty={printers.length === 0}
      nowMs={0}
      selectedCommand={null}
      commandData={null}
      notifications={[]}
      users={[]}
      userIdentities={[]}
      tenantTokens={tenantTokens}
      joinLinks={[]}
      auditEvents={auditEvents}
      adminUnavailable={adminUnavailable}
      adminLoadError={adminLoadError}
      canManageJobs={true}
      settingsStaticPanels={settingsStaticPanels}
      tenantSettingsStatic={tenantSettingsStatic}
    />
    </QueryErrorBoundary>
  );
}
