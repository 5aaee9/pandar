"use client";

import { useQueries } from "@tanstack/react-query";

import { QueryErrorBoundary } from "../../query-error-boundary";
import { usersRouteQueries } from "../../route-data";
import type { Tenant } from "../../dashboard-types";
import { UsersDashboard } from "../../users-dashboard";

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
  const [usersOptions, joinLinksOptions] = usersRouteQueries(selectedTenant.id);
  const [usersQuery, joinLinksQuery] = useQueries({
    queries: [
      { ...usersOptions, enabled: !adminUnavailable },
      { ...joinLinksOptions, enabled: !adminUnavailable },
    ] as const,
  });
  const isLoading = usersQuery.isLoading || joinLinksQuery.isLoading;
  const error = usersQuery.error ?? joinLinksQuery.error;

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
        Failed to load users:{" "}
        {error instanceof Error ? error.message : "Unknown error"}
      </div>
    );
  }

  const { users, identities } = usersQuery.data ?? {
    users: [],
    identities: [],
  };
  const joinLinks = joinLinksQuery.data ?? [];

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
