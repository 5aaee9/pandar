'use client'

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
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
  PrinterEvent,
  PrinterEventTicket,
  Summary,
  Tenant,
  TenantToken,
  User,
  UserIdentity,
} from './dashboard-types'
import { DashboardViewContent } from './dashboard-view-content'
import {
  jobRecoveryStateKey,
  mergeJob,
  mergePrinter,
  printerEventWebSocketUrl,
  type LiveState,
  type RuntimeNotification,
} from './dashboard-runtime-helpers'
import { computeAttention, computeHealth, maxSeverity } from './dashboard-attention'
import { DashboardShellHeader } from './dashboard-shell-header'
import type { DashboardQuery, DashboardView } from './dashboard-shell'
import { ActionStatusToast } from './action-status-toast'

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

const retryDelays = [1000, 2000, 5000, 10000]

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
  const [printers, setPrinters] = useState(initialPrinters)
  const [jobs, setJobs] = useState(initialJobs)
  const [liveState, setLiveState] = useState<LiveState>('idle')
  const [lastEventAt, setLastEventAt] = useState<string | null>(null)
  const [notifications, setNotifications] = useState<RuntimeNotification[]>([])
  const notificationKeys = useRef<Set<string>>(new Set())
  const [nowMs, setNowMs] = useState(0)
  const [consumedActionStatus, setConsumedActionStatus] = useState<string | null>(null)
  const pendingActionStatus = actionStatus && consumedActionStatus !== actionStatus ? actionStatus : undefined
  const consumeActionStatus = useCallback((status: string) => {
    setConsumedActionStatus(status)
  }, [])

  useEffect(() => setPrinters(initialPrinters), [initialPrinters])
  useEffect(() => setJobs(initialJobs), [initialJobs])

  useEffect(() => {
    const addNotification = (notification: RuntimeNotification) => {
      if (notificationKeys.current.has(notification.key)) {
        return
      }
      notificationKeys.current.add(notification.key)
      setNotifications((current) => [notification, ...current].slice(0, 12))
    }

    if (!selectedTenant || auth.source === 'none') {
      setLiveState(selectedTenant ? 'unavailable' : 'idle')
      if (selectedTenant) {
        addNotification({
          key: `live:${selectedTenant.id}:auth-unavailable`,
          titleKey: { namespace: 'runtime.notification', key: 'liveTitle' },
          detailKey: { namespace: 'runtime.notification', key: 'liveUnavailable' },
          timestamp: new Date().toISOString(),
        })
      }
      return
    }

    let stopped = false
    let socket: WebSocket | null = null
    let retryTimer: ReturnType<typeof setTimeout> | null = null
    let failures = 0
    let outage = 0
    let notifiedOutage = -1

    const scheduleRetry = () => {
      if (stopped) {
        return
      }
      const delay = retryDelays[Math.min(failures - 1, retryDelays.length - 1)]
      setLiveState(failures >= 3 ? 'unavailable' : 'disconnected')
      if (notifiedOutage !== outage) {
        notifiedOutage = outage
        addNotification({
          key: `live:${selectedTenant.id}:disconnected:${outage}`,
          titleKey: { namespace: 'runtime.notification', key: 'liveTitle' },
          detailKey: {
            namespace: 'runtime.notification',
            key: failures >= 3 ? 'liveRetryingUnavailable' : 'liveDisconnectedRetrying',
          },
          timestamp: new Date().toISOString(),
        })
      }
      retryTimer = setTimeout(connect, delay)
    }

    const connect = async () => {
      setLiveState('connecting')
      try {
        const response = await fetch(
          `/api/tenants/${encodeURIComponent(selectedTenant.id)}/printer-events/ticket`,
          { method: 'POST' },
        )
        if (!response.ok) {
          throw new Error(`ticket ${response.status}`)
        }
        const { ticket } = (await response.json()) as PrinterEventTicket
        socket = new WebSocket(printerEventWebSocketUrl(apiUrl, selectedTenant.id, ticket))
        socket.onopen = () => {
          failures = 0
          outage += 1
          setLiveState('live')
        }
        socket.onmessage = (message) => {
          const event = JSON.parse(message.data as string) as PrinterEvent
          const observedAt = new Date().toISOString()
          setLastEventAt(observedAt)
          if (event.type === 'printer_snapshot') {
            setPrinters((current) => {
              const previous = current.find((printer) => printer.id === event.printer.id) ?? null
              notifyPrinter(previous, event.printer, observedAt)
              return mergePrinter(current, event.printer)
            })
          } else {
            setJobs((current) => {
              const previous = current.find((job) => job.id === event.job.id) ?? null
              notifyJob(previous, event.job, observedAt)
              return mergeJob(current, event.job)
            })
          }
        }
        socket.onerror = () => {
          socket?.close()
        }
        socket.onclose = () => {
          failures += 1
          scheduleRetry()
        }
      } catch {
        failures += 1
        scheduleRetry()
      }
    }

    const notifyPrinter = (previous: Printer | null, printer: Printer, timestamp: string) => {
      if (!previous || previous.status === printer.status || printer.status.toLowerCase() !== 'offline') {
        return
      }
      addNotification({
        key: `printer:${printer.id}:offline:${printer.last_seen_at}`,
        titleKey: { namespace: 'runtime.notification', key: 'printerStateTitle' },
        detailKey: {
          namespace: 'runtime.notification',
          key: 'printerDetail',
          values: { name: printer.name, serial: printer.serial_number },
        },
        timestamp,
      })
    }

    const notifyJob = (previous: Job | null, job: Job, timestamp: string) => {
      if (!previous) {
        return
      }
      if (
        (job.status.toLowerCase() === 'failed' && previous.status !== job.status) ||
        (Boolean(job.error) && previous.error !== job.error)
      ) {
        addNotification({
          key: `job:${job.id}:dispatch:${job.status}:${job.error ?? ''}`,
          titleKey: { namespace: 'recovery.state', key: jobRecoveryStateKey(job) },
          detailKey: job.error
            ? {
                namespace: 'runtime.notification',
                key: 'jobErrorFallback',
                values: { filename: job.artifact.filename },
              }
            : {
                namespace: 'runtime.notification',
                key: 'jobDispatchDetail',
                values: { filename: job.artifact.filename, status: job.status },
              },
          timestamp,
        })
      }
      if (job.print.status !== previous.print.status && job.print.status.toLowerCase() === 'failed') {
        addNotification({
          key: `job:${job.id}:print:failed:${job.print.error ?? ''}`,
          titleKey: { namespace: 'runtime.notification', key: 'printFailedTitle' },
          detailKey: {
            namespace: 'runtime.notification',
            key: 'jobErrorFallback',
            values: { filename: job.print.error ?? job.artifact.filename },
          },
          timestamp,
        })
      }
      if (job.print.status !== previous.print.status && job.print.status.toLowerCase() === 'completed') {
        addNotification({
          key: `job:${job.id}:print:completed`,
          titleKey: { namespace: 'runtime.notification', key: 'printCompleteTitle' },
          detailKey: {
            namespace: 'runtime.notification',
            key: 'jobErrorFallback',
            values: { filename: job.artifact.filename },
          },
          timestamp,
        })
      }
    }

    connect()

    return () => {
      stopped = true
      if (retryTimer) {
        clearTimeout(retryTimer)
      }
      socket?.close()
    }
  }, [apiUrl, auth.source, selectedTenant])

  useEffect(() => {
    const update = () => setNowMs(Date.now())
    update()
    const interval = setInterval(update, 60_000)
    return () => clearInterval(interval)
  }, [])

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
    status: pendingActionStatus,
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
        <DashboardShellHeader
          query={dashboardQuery}
          selectedTenant={selectedTenant}
          tenants={tenants}
          view={view}
        />
        <main className="mx-auto flex w-full max-w-7xl flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8">
          <ActionStatusToast status={pendingActionStatus} onConsumed={consumeActionStatus} />

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
            liveState={liveState}
            lastEventAt={lastEventAt}
            fleetEmpty={fleetEmpty}
            printers={printers}
            agents={agents}
            jobs={jobs}
            selectedCommand={selectedCommand}
            commandData={commandData}
            notifications={notifications}
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
