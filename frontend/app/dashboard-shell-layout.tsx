"use client";

import type { ReactNode } from "react";

import { AppSidebar } from "../components/app-sidebar";
import { SidebarInset, SidebarProvider } from "../components/ui/sidebar";
import type { AuthMetadata, Tenant } from "./dashboard-types";
import { DashboardShellHeader } from "./dashboard-shell-header";
import { ActionStatusToast } from "./action-status-toast";
import { useDashboardShell } from "./dashboard-shell-provider";

export function DashboardShellLayout({
  children,
  sidebarDefaultOpen,
  tenants,
  auth,
}: {
  children: ReactNode;
  sidebarDefaultOpen: boolean;
  tenants: Tenant[];
  auth: AuthMetadata;
}) {
  const {
    shellView,
    shellTenant,
    actionToast,
    errorBanner,
  } = useDashboardShell();

  return (
    <SidebarProvider defaultOpen={sidebarDefaultOpen}>
      <AppSidebar
        activeView={shellView}
        auth={auth}
        query={{ tenant: shellTenant?.id }}
        selectedTenant={shellTenant}
        tenants={tenants}
      />
      <SidebarInset>
        <DashboardShellHeader view={shellView} />
        <main className="flex-1 overflow-y-auto p-4" id="main-content">
          {errorBanner ? (
            <div
              className="mb-4 rounded-md border border-destructive/50 bg-destructive/10 px-4 py-3 text-sm text-destructive"
              role="alert"
            >
              {errorBanner}
            </div>
          ) : null}
          {children}
        </main>
        {actionToast ? (
          <ActionStatusToast status={actionToast.message} />
        ) : null}
      </SidebarInset>
    </SidebarProvider>
  );
}
