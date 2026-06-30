"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import {
  BotIcon,
  Building2Icon,
  LogOutIcon,
  MonitorIcon,
  SettingsIcon,
  UsersIcon,
} from "lucide-react"

import type { AuthMetadata, Tenant } from "@/app/dashboard-types"
import {
  dashboardSidebarHref,
  dashboardTenantHref,
  logoutHref,
  type DashboardQuery,
  type DashboardView,
} from "@/app/dashboard-shell"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"

type AppSidebarProps = React.ComponentProps<typeof Sidebar> & {
  activeView: DashboardView
  auth: AuthMetadata
  query: DashboardQuery
  selectedTenant: Tenant | null
  tenants: Tenant[]
}

const navItems: Array<{
  view: DashboardView
  icon: React.ComponentType<{ className?: string }>
}> = [
  { view: "devices", icon: MonitorIcon },
  { view: "agents", icon: BotIcon },
  { view: "users", icon: UsersIcon },
  { view: "settings", icon: SettingsIcon },
]

export function AppSidebar({
  activeView,
  auth,
  query,
  selectedTenant,
  tenants,
  ...props
}: AppSidebarProps) {
  const t = useTranslations("dashboardShell")
  const signOutHref = logoutHref(auth)

  return (
    <Sidebar variant="inset" collapsible="icon" {...props}>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              size="lg"
              render={
                <a href={dashboardSidebarHref("devices", query)}>
                  <div className="flex aspect-square size-8 items-center justify-center rounded-md bg-sidebar-primary text-sidebar-primary-foreground">
                    <Building2Icon className="size-4" />
                  </div>
                  <div className="grid flex-1 text-left text-sm leading-tight">
                    <span className="truncate font-semibold">{t("brand")}</span>
                    <span className="truncate text-xs text-sidebar-foreground/70">
                      {selectedTenant ? selectedTenant.display_name : t("noTenant")}
                    </span>
                  </div>
                </a>
              }
            />
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>{t("navigation")}</SidebarGroupLabel>
          <SidebarMenu>
            {navItems.map((item) => {
              const Icon = item.icon
              return (
                <SidebarMenuItem key={item.view}>
                  <SidebarMenuButton
                    isActive={activeView === item.view}
                    tooltip={t(item.view)}
                    render={
                      <a href={dashboardSidebarHref(item.view, query)}>
                        <Icon />
                        <span>{t(item.view)}</span>
                      </a>
                    }
                  />
                </SidebarMenuItem>
              )
            })}
          </SidebarMenu>
        </SidebarGroup>

        <SidebarGroup className="group-data-[collapsible=icon]:hidden">
          <SidebarGroupLabel>{t("tenants")}</SidebarGroupLabel>
          <SidebarMenu>
            {tenants.length === 0 ? (
              <SidebarMenuItem>
                <SidebarMenuButton disabled>
                  <span>{t("noTenant")}</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ) : (
              tenants.map((tenant) => (
                <SidebarMenuItem key={tenant.id}>
                  <SidebarMenuButton
                    isActive={tenant.id === selectedTenant?.id}
                    render={
                      <a href={dashboardTenantHref(activeView, tenant.id, query)}>
                        <span>{tenant.display_name}</span>
                      </a>
                    }
                  />
                </SidebarMenuItem>
              ))
            )}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
      {signOutHref ? (
        <SidebarFooter>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton
                render={
                  <a href={signOutHref}>
                    <LogOutIcon />
                    <span>{t("logout")}</span>
                  </a>
                }
              />
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarFooter>
      ) : null}
    </Sidebar>
  )
}
