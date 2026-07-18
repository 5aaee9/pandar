"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"
import {
  BotIcon,
  Building2Icon,
  ClipboardListIcon,
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
  { view: "jobs", icon: ClipboardListIcon },
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
    <Sidebar
      variant="inset"
      collapsible="icon"
      mobileTitle={t("navigation")}
      mobileDescription={t("sidebarDescription")}
      {...props}
    >
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              size="lg"
              render={
                <Link href={dashboardSidebarHref("devices", query)} prefetch={false}>
                  <div className="flex aspect-square size-8 items-center justify-center rounded-md bg-sidebar-primary text-sidebar-primary-foreground">
                    <Building2Icon className="size-4" />
                  </div>
                  <div className="grid flex-1 text-left text-sm leading-tight">
                    <span className="truncate font-semibold">{t("brand")}</span>
                    <span className="truncate text-xs text-sidebar-foreground/70">
                      {selectedTenant ? selectedTenant.display_name : t("noTenant")}
                    </span>
                  </div>
                </Link>
              }
            />
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <nav aria-label={t("navigation")}>
          <SidebarGroup>
            <SidebarGroupLabel>{t("navigation")}</SidebarGroupLabel>
            <SidebarMenu>
              {navItems.map((item) => {
                const Icon = item.icon
                const isActive = activeView === item.view
                return (
                  <SidebarMenuItem key={item.view}>
                    <SidebarMenuButton
                      isActive={isActive}
                      tooltip={t(item.view)}
                      render={
                        <Link
                          aria-current={isActive ? "page" : undefined}
                          href={dashboardSidebarHref(item.view, query)}
                          prefetch={false}
                        >
                          <Icon />
                          <span>{t(item.view)}</span>
                        </Link>
                      }
                    />
                  </SidebarMenuItem>
                )
              })}
            </SidebarMenu>
          </SidebarGroup>
        </nav>

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
              tenants.map((tenant) => {
                const isSelected = tenant.id === selectedTenant?.id
                return (
                  <SidebarMenuItem key={tenant.id}>
                    <SidebarMenuButton
                      isActive={isSelected}
                      render={
                        <Link
                          aria-current={isSelected ? "true" : undefined}
                          href={dashboardTenantHref(activeView, tenant.id, query)}
                          prefetch={false}
                        >
                          <span>{tenant.display_name}</span>
                        </Link>
                      }
                    />
                  </SidebarMenuItem>
                )
              })
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
