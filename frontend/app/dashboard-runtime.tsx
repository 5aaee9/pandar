'use client'

import { useEffect, useMemo, useRef, useState } from 'react'

import { DiagnosticsSection, LinkedAgentsSection } from './diagnostics-panel'
import { DispatchForm } from './dispatch-form'
import { RecoveryActions } from './recovery-actions'
import { TenantAdminPanel } from './admin-panel'
import type {
  Agent,
  AuthMetadata,
  AuditEvent,
  Command,
  CommandResultData,
  Job,
  Printer,
  PrinterEvent,
  PrinterEventTicket,
  Summary,
  Tenant,
  TenantToken,
  User,
  UserIdentity,
} from './dashboard-types'
import {
  formatAuthSource,
  formatJobRecoveryState,
  mergeJob,
  mergePrinter,
  printerEventWebSocketUrl,
  type LiveState,
  type RuntimeNotification,
} from './dashboard-runtime-helpers'
import {
  JobHistory,
  PrinterInventory,
  RuntimeStatusPanel,
  TenantSettings,
} from './dashboard-runtime-sections'
import { Metric } from './dashboard-ui'

type DashboardRuntimeProps = {
  apiUrl: string
  configuredTenantId?: string
  summary: Summary | null
  tenants: Tenant[]
  selectedTenant: Tenant | null
  initialPrinters: Printer[]
  agents: Agent[]
  initialJobs: Job[]
  users: User[]
  userIdentities: UserIdentity[]
  tenantTokens: TenantToken[]
  auditEvents: AuditEvent[]
  adminUnavailable: boolean
  actionStatus?: string
  selectedCommand: Command | null
  commandData: CommandResultData | null
  errors: string[]
  auth: AuthMetadata
}

const retryDelays = [1000, 2000, 5000, 10000]

export function DashboardRuntime({
  apiUrl,
  configuredTenantId,
  summary,
  tenants,
  selectedTenant,
  initialPrinters,
  agents,
  initialJobs,
  users,
  userIdentities,
  tenantTokens,
  auditEvents,
  adminUnavailable,
  actionStatus,
  selectedCommand,
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

  useEffect(() => setPrinters(initialPrinters), [initialPrinters])
  useEffect(() => setJobs(initialJobs), [initialJobs])

  const authLabel = useMemo(() => formatAuthSource(auth.source), [auth.source])

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
          title: 'Live connection',
          detail: 'Live updates unavailable because no server-side auth token is configured.',
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
          title: 'Live connection',
          detail: failures >= 3 ? 'Live updates unavailable; retrying.' : 'Live updates disconnected; retrying.',
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
        title: 'Printer state',
        detail: `${printer.name} (${printer.serial_number})`,
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
          title: formatJobRecoveryState(job),
          detail: job.error ?? `${job.artifact.filename} dispatch ${job.status}`,
          timestamp,
        })
      }
      if (job.print.status !== previous.print.status && job.print.status.toLowerCase() === 'failed') {
        addNotification({
          key: `job:${job.id}:print:failed:${job.print.error ?? ''}`,
          title: 'Physical print',
          detail: job.print.error ?? job.artifact.filename,
          timestamp,
        })
      }
      if (job.print.status !== previous.print.status && job.print.status.toLowerCase() === 'completed') {
        addNotification({
          key: `job:${job.id}:print:completed`,
          title: 'Print complete',
          detail: job.artifact.filename,
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

  return (
    <main className="min-h-screen bg-slate-100 px-4 py-5 text-slate-950 sm:px-6 lg:px-8">
      <section className="mx-auto flex max-w-7xl flex-col gap-5">
        <Header apiUrl={apiUrl} tenants={tenants} selectedTenant={selectedTenant} />

        {errors.length > 0 ? (
          <div className="rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-950">
            Hub data is incomplete. {errors.join('; ')}.
          </div>
        ) : null}

        {actionStatus ? (
          <div className="rounded-md border border-cyan-200 bg-cyan-50 px-3 py-2 text-sm text-cyan-950">
            {formatActionStatus(actionStatus)}
          </div>
        ) : null}

        <RuntimeStatusPanel
          auth={auth}
          authLabel={authLabel}
          liveState={liveState}
          lastEventAt={lastEventAt}
          notifications={notifications}
          selectedTenant={selectedTenant}
        />

        <section className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <Metric label="Tenants" value={summary?.tenants ?? (configuredTenantId ? 1 : undefined)} />
          <Metric label="Agents" value={summary?.agents ?? (configuredTenantId ? agents.length : undefined)} />
          <Metric label="Printers" value={summary?.printers ?? (configuredTenantId ? printers.length : undefined)} />
          <Metric label="Commands" value={summary?.commands} />
        </section>

        <LinkedAgentsSection selectedTenant={selectedTenant} agents={agents} />
        <PrinterInventory selectedTenant={selectedTenant} printers={printers} />
        <DispatchForm selectedTenant={selectedTenant} printers={printers} />
        <RecoveryActions selectedTenant={selectedTenant} agents={agents} printers={printers} jobs={jobs} />
        <DiagnosticsSection
          selectedTenant={selectedTenant}
          printers={printers}
          selectedCommand={selectedCommand}
          commandData={commandData}
        />
        <JobHistory selectedTenant={selectedTenant} jobs={jobs} />
        <TenantSettings
          auth={auth}
          authLabel={authLabel}
          selectedTenant={selectedTenant}
          agents={agents}
          printers={printers}
        />
        <TenantAdminPanel
          selectedTenant={selectedTenant}
          users={users}
          userIdentities={userIdentities}
          tenantTokens={tenantTokens}
          agents={agents}
          auditEvents={auditEvents}
          unavailable={adminUnavailable}
        />
      </section>
    </main>
  )
}

function formatActionStatus(status: string) {
  return status
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

function Header({
  apiUrl,
  tenants,
  selectedTenant,
}: {
  apiUrl: string
  tenants: Tenant[]
  selectedTenant: Tenant | null
}) {
  return (
    <header className="flex flex-col gap-3 border-b border-slate-300 pb-4 md:flex-row md:items-end md:justify-between">
      <div>
        <h1 className="text-2xl font-semibold">Pandar Operations</h1>
        <p className="mt-1 text-sm text-slate-600">Tenant printer inventory from {apiUrl}</p>
      </div>
      {tenants.length > 1 ? (
        <form className="flex min-w-72 items-end gap-2" action="/">
          <label className="flex flex-1 flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">Tenant</span>
            <select
              name="tenant"
              defaultValue={selectedTenant?.id}
              className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950"
            >
              {tenants.map((tenant) => (
                <option key={tenant.id} value={tenant.id}>
                  {tenant.display_name}
                </option>
              ))}
            </select>
          </label>
          <button className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white" type="submit">
            View
          </button>
        </form>
      ) : null}
    </header>
  )
}
