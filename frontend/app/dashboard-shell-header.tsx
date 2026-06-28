'use client'

import { useTranslations } from 'next-intl'

import { LanguageSwitcher } from '../components/language-switcher'
import { SidebarTrigger } from '../components/ui/sidebar'
import type { Tenant } from './dashboard-types'
import {
  dashboardTenantHref,
  dashboardViewTitleKey,
  type DashboardQuery,
  type DashboardView,
} from './dashboard-shell'

export function DashboardShellHeader({
  query,
  selectedTenant,
  tenants,
  view,
}: {
  query: DashboardQuery
  selectedTenant: Tenant | null
  tenants: Tenant[]
  view: DashboardView
}) {
  const tShell = useTranslations('dashboardShell')
  return (
    <header className="sticky top-0 z-20 flex min-h-16 shrink-0 flex-col gap-3 border-b border-slate-200 bg-white/95 px-4 py-3 backdrop-blur sm:flex-row sm:items-center sm:justify-between">
      <div className="flex min-w-0 items-center gap-3">
        <SidebarTrigger className="-ml-1" />
        <div className="min-w-0">
          <div className="text-xs font-medium uppercase text-slate-500">{tShell('brand')}</div>
          <h1 className="truncate text-lg font-semibold text-slate-950">
            {tShell(dashboardViewTitleKey(view))}
          </h1>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <TenantSwitcher
          query={query}
          selectedTenant={selectedTenant}
          tenants={tenants}
          view={view}
        />
        <LanguageSwitcher />
      </div>
    </header>
  )
}

function TenantSwitcher({
  query,
  selectedTenant,
  tenants,
  view,
}: {
  query: DashboardQuery
  selectedTenant: Tenant | null
  tenants: Tenant[]
  view: DashboardView
}) {
  const tShell = useTranslations('dashboardShell')
  return (
    <label className="flex items-center gap-2 text-sm">
      <span className="sr-only">{tShell('tenant')}</span>
      <select
        className="h-9 max-w-56 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950"
        disabled={tenants.length === 0}
        onChange={(event) => {
          const tenantId = event.currentTarget.value
          if (tenantId) {
            window.location.assign(dashboardTenantHref(view, tenantId, query))
          }
        }}
        value={selectedTenant?.id ?? ''}
      >
        {selectedTenant ? null : <option value="">{tShell('noTenant')}</option>}
        {tenants.map((tenant) => (
          <option key={tenant.id} value={tenant.id}>
            {tenant.display_name}
          </option>
        ))}
      </select>
    </label>
  )
}
