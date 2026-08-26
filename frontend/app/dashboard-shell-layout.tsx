"use client";

import type { ReactNode } from "react";
import { useSearchParams } from "next/navigation";

import { AppSidebar } from "../components/app-sidebar";
import { SidebarInset, SidebarProvider } from "../components/ui/sidebar";
import type { AuthMetadata, Tenant } from "./dashboard-types";
import { DashboardShellHeader } from "./dashboard-shell-header";
import { ActionStatusToast } from "./action-status-toast";
import { useDashboardShell } from "./dashboard-shell-provider";
import { DashboardCameraProvider } from "./dashboard-printer-camera-control";

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
  const { shellView, shellTenant } = useDashboardShell();
  const status = useSearchParams().get("status") ?? undefined;

  return (
    <DashboardCameraProvider>
      <SidebarProvider defaultOpen={sidebarDefaultOpen}>
        <AppSidebar
          activeView={shellView}
          auth={auth}
          selectedTenant={shellTenant}
          tenants={tenants}
        />
        <SidebarInset>
          <DashboardShellHeader view={shellView} />
          <main className="flex-1 p-4" id="main-content">{children}</main>
          <ActionStatusToast status={status} />
        </SidebarInset>
      </SidebarProvider>
    </DashboardCameraProvider>
  );
}
