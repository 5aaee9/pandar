'use client'

import { useMemo } from 'react'
import { useTranslations } from 'next-intl'

import { AppSidebar } from '../components/app-sidebar'
import { SidebarInset, SidebarProvider } from '../components/ui/sidebar'
import type {
  Agent,
  AuthMetadata,
  AuditEvent,
  Command,
  CommandResultData,
  Job,
  JoinLink,
  Printer,
  Summary,
  Tenant,
  TenantToken,
  User,
  UserIdentity,
} from './dashboard-types'
import { DashboardViewContent } from './dashboard-view-content'
import { computeAttention, computeHealth, maxSeverity } from './dashboard-attention'
import { DashboardShellHeader } from './dashboard-shell-header'
import type { DashboardQuery, DashboardView } from './dashboard-shell'
import { ActionStatusToast } from './action-status-toast'
import { useDashboardClock } from './use-dashboard-clock'
import { useDashboardRuntimeEvents } from './use-dashboard-runtime-events'

type DashboardRuntimeProps = {
  apiUrl: string
  configuredTenantId?: string
  view: DashboardView
  summary: Summary | null
  tenants: Tenant[]
  selectedTenant: Tenant | null
  initialPrinters: Printer[]
  agents: Agent[]
  initialJobs: Job[]
  users: User[]
  userIdentities: UserIdentity[]
  tenantTokens: TenantToken[]
  joinLinks: JoinLink[]
  auditEvents: AuditEvent[]
  adminUnavailable: boolean
  actionStatus?: string
  selectedCommand: Command | null
  selectedCommandId?: string
  commandData: CommandResultData | null
  errors: string[]
  auth: AuthMetadata
}

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
  actionStatus,
  selectedCommand,
  selectedCommandId,
  commandData,
  errors,
  auth,
}: DashboardRuntimeProps) {
  const runtime = useDashboardRuntimeEvents({
    apiUrl,
    auth,
    selectedTenant,
    initialPrinters,
    initialJobs,
  })
  const printers = runtime.printers
  const jobs = runtime.jobs
  const nowMs = useDashboardClock(printers)

  const fleetEmpty = printers.length === 0 && agents.length === 0 && jobs.length === 0
  const health = useMemo(() => computeHealth(agents, printers, jobs), [agents, printers, jobs])
  const attentionItems = useMemo(
    () => computeAttention({ agents, printers, jobs, nowMs }),
    [agents, printers, jobs, nowMs],
  )
  const topSeverity = useMemo(() => maxSeverity(attentionItems), [attentionItems])

  const tErr = useTranslations('runtime.notification')
  const dashboardQuery: DashboardQuery = {
    tenant: selectedTenant?.id,
    command: view === 'agents' ? selectedCommandId : undefined,
    status: view === 'jobs' ? actionStatus : undefined,
  }

  return (
    <SidebarProvider>
      <AppSidebar
        activeView={view}
        auth={auth}
        query={dashboardQuery}
        selectedTenant={selectedTenant}
        tenants={tenants}
      />
      <SidebarInset className="min-h-svh bg-slate-100 text-slate-950">
        <DashboardShellHeader view={view} />
        <main className="mx-auto flex w-full max-w-7xl flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8">
          <ActionStatusToast status={actionStatus} />

          {errors.length > 0 ? (
            <div className="rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-950">
              {tErr('errorsIncomplete')} {errors.join('; ')}.
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
          />
        </main>
      </SidebarInset>
    </SidebarProvider>
  )
}
