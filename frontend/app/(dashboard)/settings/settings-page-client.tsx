"use client";

import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";

import { QueryErrorBoundary } from "../../query-error-boundary";
import {
  settingsAdminRouteQuery,
  settingsRouteQuery,
} from "../../route-data";
import { SettingsDashboard } from "../../settings-dashboard";
import type { AuthMetadata, Tenant } from "../../dashboard-types";

export function SettingsPageClient({
  auth,
  selectedTenant,
  membership,
}: {
  auth: AuthMetadata;
  selectedTenant: Tenant;
  membership: { role: string | null; error: string | null };
}) {
  const t = useTranslations("settingsPage");
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    const interval = setInterval(() => setNowMs(Date.now()), 60_000);
    return () => clearInterval(interval);
  }, []);
  const canAdmin =
    auth.provider === "none" ||
    (membership.role === "tenant_admin" && membership.error === null);
  const workspaceQuery = useQuery(settingsRouteQuery(selectedTenant.id));
  const adminQuery = useQuery({
    ...settingsAdminRouteQuery(selectedTenant.id),
    enabled: canAdmin,
  });

  if (workspaceQuery.isLoading) {
    return <SettingsLoading />;
  }

  if (workspaceQuery.error) {
    return (
      <div
        className="rounded-xl border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive"
        role="alert"
      >
        <div className="font-medium">{t("loadErrorTitle")}</div>
        <p className="mt-1 text-destructive/90">{t("loadErrorDescription")}</p>
        <details className="mt-2 text-xs">
          <summary className="cursor-pointer font-medium">{t("errorDetails")}</summary>
          <div className="mt-1 break-all">
            {workspaceQuery.error instanceof Error
              ? workspaceQuery.error.message
              : String(workspaceQuery.error)}
          </div>
        </details>
      </div>
    );
  }

  const workspace = workspaceQuery.data ?? { agents: [], printers: [] };
  const adminUnavailable =
    auth.provider !== "none" &&
    membership.error === null &&
    membership.role !== "tenant_admin";
  const adminLoadError =
    (auth.provider !== "none" && membership.error !== null) ||
    adminQuery.error !== null;

  return (
    <QueryErrorBoundary>
      <SettingsDashboard
        adminLoadError={adminLoadError}
        adminLoading={canAdmin && adminQuery.isLoading}
        adminUnavailable={adminUnavailable}
        agents={workspace.agents}
        auditEvents={adminQuery.data?.auditEvents ?? []}
        auth={auth}
        canAdmin={canAdmin}
        membershipRole={membership.role}
        nowMs={nowMs}
        printers={workspace.printers}
        selectedTenant={selectedTenant}
        tenantTokens={adminQuery.data?.tenantTokens ?? []}
      />
    </QueryErrorBoundary>
  );
}

function SettingsLoading() {
  return (
    <div className="mx-auto max-w-5xl animate-pulse">
      <div className="space-y-2 pb-6">
        <div className="h-8 w-40 rounded-lg bg-muted" />
        <div className="h-4 w-72 rounded-lg bg-muted/70" />
      </div>
      <div className="grid items-start gap-6 lg:grid-cols-[13rem_minmax(0,1fr)]">
        <div className="hidden h-44 rounded-xl bg-muted/70 lg:block" />
        <div className="space-y-6">
          <div className="h-64 rounded-xl bg-muted" />
          <div className="h-40 rounded-xl bg-muted" />
        </div>
      </div>
    </div>
  );
}
