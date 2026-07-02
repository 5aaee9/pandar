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
    <header className="sticky top-0 z-20 flex min-h-16 shrink-0 items-center border-b border-slate-200 bg-white/95 px-4 py-3 backdrop-blur">
      <div className="flex min-w-0 items-center gap-3">
        <SidebarTrigger className="-ml-1" />
        <div className="min-w-0">
          <div className="text-xs font-medium uppercase text-slate-500">{tShell('brand')}</div>
          <h1 className="truncate text-lg font-semibold text-slate-950">
            {tShell(dashboardViewTitleKey(view))}
          </h1>
        </div>
      </div>
    </header>
  )
}
