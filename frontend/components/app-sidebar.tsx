"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import {
  BotIcon,
  Building2Icon,
  CheckIcon,
  ChevronsUpDownIcon,
  ClipboardListIcon,
  LogOutIcon,
  MonitorIcon,
  SettingsIcon,
  UsersIcon,
} from "lucide-react"

import type { AuthMetadata, Tenant } from "@/app/dashboard-types"
import { Button } from "@/components/ui/button"
import {
  dashboardSidebarHref,
  logoutHref,
  type DashboardView,
} from "@/app/dashboard-shell"
import { setTenantCookie } from "@/app/tenant-cookie"
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
  useSidebar,
} from "@/components/ui/sidebar"
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover"

type AppSidebarProps = React.ComponentProps<typeof Sidebar> & {
  activeView: DashboardView
  auth: AuthMetadata
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
  selectedTenant,
  tenants,
  ...props
}: AppSidebarProps) {
  const t = useTranslations("dashboardShell")
  const signOutHref = logoutHref(auth)
  const [tenantAccessOpen, setTenantAccessOpen] = React.useState(false)
  const { isMobile, setOpenMobile } = useSidebar()
  const router = useRouter()
  const pathname = usePathname()

  function closeTenantAccess() {
    setTenantAccessOpen(false)
    if (isMobile) {
      setOpenMobile(false)
    }
  }

  function selectTenant(tenantId: string) {
    setTenantCookie(tenantId)
    closeTenantAccess()
    // Drop transient query context (command/status): it belongs to the
    // previously selected tenant.
    if (window.location.search) {
      router.push(pathname)
    } else {
      router.refresh()
    }
  }

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
            <Popover open={tenantAccessOpen} onOpenChange={setTenantAccessOpen}>
              <PopoverTrigger
                render={
                  <SidebarMenuButton
                    aria-expanded={tenantAccessOpen}
                    aria-haspopup="dialog"
                    aria-label={t("selectTenantAccess")}
                    size="lg"
                    tooltip={selectedTenant?.display_name ?? t("noTenant")}
                  />
                }
              >
                <div className="flex aspect-square size-8 items-center justify-center rounded-md bg-sidebar-primary text-sidebar-primary-foreground">
                  <Building2Icon className="size-4" />
                </div>
                <div className="grid min-w-0 flex-1 text-left text-sm leading-tight">
                  <span className="truncate font-semibold">
                    {selectedTenant ? selectedTenant.display_name : t("noTenant")}
                  </span>
                  <span className="truncate text-xs text-sidebar-foreground/70">
                    {t("tenantAccess")}
                  </span>
                </div>
                <ChevronsUpDownIcon className="ml-auto size-4 text-sidebar-foreground/70" />
              </PopoverTrigger>
              <PopoverContent align="start" className="w-64 p-0" sideOffset={6}>
                <PopoverTitle className="px-2 py-1.5 text-xs text-muted-foreground">
                  {t("tenantAccess")}
                </PopoverTitle>
                <div className="pb-1">
                  {tenants.length === 0 ? (
                    <div className="px-2 py-2 text-sm text-muted-foreground">
                      {t("noTenant")}
                    </div>
                  ) : (
                    tenants.map((tenant) => {
                      const isSelected = tenant.id === selectedTenant?.id
                      return (
                        <Button
                          aria-current={isSelected ? "true" : undefined}
                          className="h-auto w-full justify-start gap-2 rounded-md px-2 py-2 font-normal text-foreground"
                          key={tenant.id}
                          onClick={() => selectTenant(tenant.id)}
                          type="button"
                          variant="ghost"
                        >
                          <span className="flex size-5 items-center justify-center">
                            {isSelected ? <CheckIcon className="size-4" /> : null}
                          </span>
                          <span className="truncate">{tenant.display_name}</span>
                        </Button>
                      )
                    })
                  )}
                </div>
              </PopoverContent>
            </Popover>
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
                          href={dashboardSidebarHref(item.view)}
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
