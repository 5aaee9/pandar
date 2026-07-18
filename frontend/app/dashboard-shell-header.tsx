'use client'

import { useTranslations } from 'next-intl'

import { SidebarTrigger } from '../components/ui/sidebar'
import {
  dashboardViewTitleKey,
  type DashboardView,
} from './dashboard-shell'

export function DashboardShellHeader({
  view,
}: {
  view: DashboardView
}) {
  const tShell = useTranslations('dashboardShell')
  return (
    <header className="sticky top-0 z-20 flex min-h-16 shrink-0 items-center border-b border-border bg-background/95 px-4 py-3 backdrop-blur">
      <div className="flex min-w-0 items-center gap-3">
        <SidebarTrigger className="-ml-1" label={tShell('toggleSidebar')} />
        <div className="min-w-0">
          <div className="text-xs font-medium text-muted-foreground">{tShell('brand')}</div>
          <h1 className="truncate text-lg font-semibold text-foreground">
            {tShell(dashboardViewTitleKey(view))}
          </h1>
        </div>
      </div>
    </header>
  )
}
