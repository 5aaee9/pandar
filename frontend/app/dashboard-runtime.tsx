"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";

import { AppSidebar } from "../components/app-sidebar";
import { SidebarInset, SidebarProvider } from "../components/ui/sidebar";
import type {
  Agent,
  AuthMetadata,
  AuditEvent,
  Command,
  CommandResultData,
  Job,
  JoinLink,
  Printer,
  Tenant,
  TenantToken,
  User,
  UserIdentity,
} from "./dashboard-types";
import { DashboardViewContent } from "./dashboard-view-content";
import {
  computeAttention,
  computeHealth,
  maxSeverity,
} from "./dashboard-attention";
import { DashboardShellHeader } from "./dashboard-shell-header";
import type { DashboardQuery, DashboardView } from "./dashboard-shell";
import { ActionStatusToast } from "./action-status-toast";
import { useDashboardClock } from "./use-dashboard-clock";
import { useDashboardRuntimeEvents } from "./use-dashboard-runtime-events";

type DashboardRuntimeProps = {
  apiUrl: string;
  configuredTenantId?: string;
  view: DashboardView;
  tenants: Tenant[];
  selectedTenant: Tenant | null;
  initialPrinters: Printer[];
  agents: Agent[];
  initialJobs: Job[];
  users: User[];
  userIdentities: UserIdentity[];
  tenantTokens: TenantToken[];
  joinLinks: JoinLink[];
  auditEvents: AuditEvent[];
  adminUnavailable: boolean;
  adminLoadError: boolean;
  canManageJobs: boolean;
  actionStatus?: string;
  selectedCommand: Command | null;
  selectedCommandId?: string;
  commandData: CommandResultData | null;
  errors: string[];
  auth: AuthMetadata;
  sidebarDefaultOpen?: boolean;
};

export function DashboardRuntime({
  apiUrl,
  tenants,
  selectedTenant,
  view,
  initialPrinters,
  agents,
  initialJobs,
  users,
  userIdentities,
  tenantTokens,
  joinLinks,
  auditEvents,
  adminUnavailable,
  adminLoadError,
  canManageJobs,
  actionStatus,
  selectedCommand,
  selectedCommandId,
  commandData,
  errors,
  auth,
  sidebarDefaultOpen,
}: DashboardRuntimeProps) {
  const runtime = useDashboardRuntimeEvents({
    apiUrl,
    auth,
    enabled: view !== "users",
    selectedTenant,
    initialPrinters,
    initialJobs,
  });
  const printers = runtime.printers;
  const jobs = runtime.jobs;
  const nowMs = useDashboardClock(printers);

  const fleetEmpty =
    printers.length === 0 && agents.length === 0 && jobs.length === 0;
  const health = useMemo(
    () => computeHealth(agents, printers, jobs),
    [agents, printers, jobs],
  );
  const attentionItems = useMemo(
    () => computeAttention({ agents, printers, jobs, nowMs }),
    [agents, printers, jobs, nowMs],
  );
  const topSeverity = useMemo(
    () => maxSeverity(attentionItems),
    [attentionItems],
  );

  const tErr = useTranslations("runtime.notification");
  const dashboardQuery: DashboardQuery = {
    tenant: selectedTenant?.id,
    command: view === "agents" ? selectedCommandId : undefined,
    status: view === "jobs" ? actionStatus : undefined,
  };

  return (
    <SidebarProvider defaultOpen={sidebarDefaultOpen}>
      <AppSidebar
        activeView={view}
        auth={auth}
        query={dashboardQuery}
        selectedTenant={selectedTenant}
        tenants={tenants}
      />
      <SidebarInset className="min-h-svh bg-muted text-foreground">
        <DashboardShellHeader view={view} />
        <main
          id="main-content"
          className="mx-auto flex w-full max-w-7xl flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8"
        >
          <ActionStatusToast status={actionStatus} />

          {errors.length > 0 ? (
            <div
              role="alert"
              className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive dark:bg-destructive/20"
            >
              {tErr("errorsIncomplete")} {errors.join("; ")}.
            </div>
          ) : null}

          <DashboardViewContent
            view={view}
            auth={auth}
            selectedTenant={selectedTenant}
            health={health}
            attentionItems={attentionItems}
            topSeverity={topSeverity}
            liveState={runtime.liveState}
            lastEventAt={runtime.lastEventAt}
            fleetEmpty={fleetEmpty}
            printers={printers}
            agents={agents}
            jobs={jobs}
            nowMs={nowMs}
            selectedCommand={selectedCommand}
            commandData={commandData}
            notifications={runtime.notifications}
            users={users}
            userIdentities={userIdentities}
            tenantTokens={tenantTokens}
            joinLinks={joinLinks}
            auditEvents={auditEvents}
            adminUnavailable={adminUnavailable}
            adminLoadError={adminLoadError}
            canManageJobs={canManageJobs}
          />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}
