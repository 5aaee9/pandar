"use client";

import { useQuery } from "@tanstack/react-query";

import { apiClient } from "../../api-client";
import { DashboardViewContent } from "../../dashboard-view-content";
import type { AuthMetadata, Tenant } from "../../dashboard-types";

export function UsersPageClient({
  auth,
  selectedTenant,
  adminUnavailable,
  adminLoadError,
}: {
  auth: AuthMetadata;
  selectedTenant: Tenant;
  adminUnavailable: boolean;
  adminLoadError: boolean;
}) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["route", "users", selectedTenant.id],
    queryFn: async () => {
      const [users, joinLinks] = await Promise.all([
        apiClient.users.list(selectedTenant.id),
        apiClient.users.joinLinks(selectedTenant.id),
      ]);
      return {
        users: users.users,
        identities: users.identities,
        joinLinks: joinLinks.join_links,
      };
    },
    staleTime: 30 * 1000,
  });

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
        Failed to load users: {error instanceof Error ? error.message : "Unknown error"}
      </div>
    );
  }

  const { users, identities, joinLinks } = data ?? { users: [], identities: [], joinLinks: [] };

  return (
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
      users={users}
      userIdentities={identities}
      tenantTokens={[]}
      joinLinks={joinLinks}
      auditEvents={[]}
      adminUnavailable={adminUnavailable}
      adminLoadError={adminLoadError}
      canManageJobs={true}
    />
  );
}
