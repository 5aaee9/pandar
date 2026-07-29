"use client";

import { useQuery } from "@tanstack/react-query";

import { apiClient } from "../../api-client";
import { QueryErrorBoundary } from "../../query-error-boundary";
import type { Tenant } from "../../dashboard-types";
import { UsersDashboard } from "../../users-dashboard";
import { usersQueryKey } from "../../users-query";

export function UsersPageClient({
  selectedTenant,
  adminUnavailable,
  adminLoadError,
  meEmail,
}: {
  selectedTenant: Tenant;
  adminUnavailable: boolean;
  adminLoadError: boolean;
  meEmail: string | null;
}) {
  const { data, isLoading, error } = useQuery({
    queryKey: usersQueryKey(selectedTenant.id),
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
    staleTime: 60 * 1000,
    enabled: !adminUnavailable,
  });

  if (adminUnavailable) {
    return (
      <QueryErrorBoundary>
        <UsersDashboard
          adminLoadError={adminLoadError}
          adminUnavailable={adminUnavailable}
          identities={[]}
          joinLinks={[]}
          meEmail={meEmail}
          selectedTenant={selectedTenant}
          users={[]}
        />
      </QueryErrorBoundary>
    );
  }

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
    <QueryErrorBoundary>
      <UsersDashboard
        adminLoadError={adminLoadError}
        adminUnavailable={adminUnavailable}
        identities={identities}
        joinLinks={joinLinks}
        meEmail={meEmail}
        selectedTenant={selectedTenant}
        users={users}
      />
    </QueryErrorBoundary>
  );
}
