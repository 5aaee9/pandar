import type { Agent, AuthMetadata, Job, Printer, Tenant } from './dashboard-types'
import { notificationSeverity } from './dashboard-attention'
import { StatusIcon } from './dashboard-status'
import {
  DetailGroup,
  DetailLine,
  EmptyState,
  formatBytes,
  formatDate,
  SectionHeader,
  StatusBadge,
} from './dashboard-ui'
import type { LiveState, RuntimeNotification } from './dashboard-runtime-helpers'
import {
  formatArtifactMetadata,
  formatJobMaterial,
  formatJobRecoveryState,
  formatPrinterMaterials,
} from './dashboard-runtime-helpers'
import { formatLayers, formatProgress, formatRemaining } from './job-format'

export function RuntimeStatusPanel({
  auth,
  authLabel,
  liveState,
  lastEventAt,
  notifications,
  selectedTenant,
}: {
  auth: AuthMetadata
  authLabel: string
  liveState: LiveState
  lastEventAt: string | null
  notifications: RuntimeNotification[]
  selectedTenant: Tenant | null
}) {
  return (
    <section className="grid gap-3 rounded-md border border-slate-300 bg-white px-4 py-3 lg:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)]">
      <div className="grid gap-2 text-sm sm:grid-cols-2">
        <RuntimeField label="Tenant" value={selectedTenant ? selectedTenant.display_name : 'No tenant'} />
        <RuntimeField label="WebSocket" value={liveState} />
        <RuntimeField label="Last event" value={lastEventAt ? formatDate(lastEventAt) : '-'} />
        <RuntimeField label="Auth" value={`${authLabel} · cookie ${auth.cookieName}`} />
      </div>
      <div role="status" aria-live="polite" aria-label="Live notifications">
        <div className="text-xs font-medium uppercase text-slate-500">Notifications</div>
        {notifications.length === 0 ? (
          <div className="mt-2 text-sm text-slate-600">No live notifications</div>
        ) : (
          <ul className="mt-2 max-h-64 divide-y divide-slate-200 overflow-y-auto">
            {notifications.map((notification) => {
              const severity = notificationSeverity(notification.title, notification.detail)
              return (
                <li
                  key={`${notification.key}-${notification.timestamp}`}
                  className="flex min-w-0 items-start gap-2 py-1.5"
                >
                  <StatusIcon severity={severity} className="mt-0.5 h-4 w-4 shrink-0" />
                  <div className="min-w-0">
                    <div className="truncate text-sm font-medium text-slate-950">{notification.title}</div>
                    <div className="truncate text-xs text-slate-600">{notification.detail}</div>
                    <div className="text-xs text-slate-500">{formatDate(notification.timestamp)}</div>
                  </div>
                </li>
              )
            })}
          </ul>
        )}
      </div>
    </section>
  )
}

function RuntimeField({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-xs font-medium uppercase text-slate-500">{label}</div>
      <div className="mt-0.5 break-words text-sm font-medium text-slate-900">{value}</div>
    </div>
  )
}

export function PrinterInventory({
  selectedTenant,
  printers,
}: {
  selectedTenant: Tenant | null
  printers: Printer[]
}) {
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <SectionHeader
        title="Printer inventory"
        subtitle={selectedTenant ? `${selectedTenant.display_name} (${selectedTenant.slug})` : 'No tenant selected'}
        meta={`${printers.length} reported`}
      />
      {!selectedTenant ? (
        <EmptyState title="No tenants" message="Create a tenant through the hub API before printers can be reported." />
      ) : printers.length === 0 ? (
        <EmptyState title="No printers reported" message="Connect an agent and run a printer refresh to populate this inventory." />
      ) : (
        <div className="divide-y divide-slate-200">
          {printers.map((printer) => {
            const material = formatPrinterMaterials(printer)
            return (
              <div key={printer.id} className="grid gap-3 px-4 py-3 text-sm lg:grid-cols-[1.2fr_1fr_1.3fr_1.5fr]">
                <div className="min-w-0">
                  <div className="truncate font-medium text-slate-950">{printer.name}</div>
                  <div className="truncate font-mono text-xs text-slate-600">{printer.serial_number}</div>
                  <div className="text-xs text-slate-600">{printer.model ?? 'Unknown model'}</div>
                </div>
                <div>
                  <StatusBadge value={printer.status} />
                  <div className="mt-1 text-xs text-slate-600">{formatDate(printer.last_seen_at)}</div>
                </div>
                <div>
                  <div className="text-slate-800">{material.summary}</div>
                  <div className="text-xs text-slate-600">{material.detail}</div>
                </div>
                <div className="min-w-0 font-mono text-xs text-slate-700">
                  <div className="truncate">POST /api/v1/tenants/{selectedTenant.id}/printers/{printer.id}/jobs</div>
                  <div className="truncate">Agent {printer.agent_id}</div>
                </div>
              </div>
            )
          })}
        </div>
      )}
    </section>
  )
}

export function JobHistory({ selectedTenant, jobs }: { selectedTenant: Tenant | null; jobs: Job[] }) {
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <SectionHeader
        title="Print jobs"
        subtitle="Queued, dispatched, and physical print history"
        meta={`${jobs.length} jobs`}
      />
      {!selectedTenant ? (
        <EmptyState title="No tenant selected" message="Select a tenant to inspect jobs." />
      ) : jobs.length === 0 ? (
        <EmptyState title="No jobs" message="Create a print job through the printer dispatch API to populate history." />
      ) : (
        <div className="divide-y divide-slate-200">
          {jobs.map((job) => (
            <JobRow key={job.id} job={job} />
          ))}
        </div>
      )}
    </section>
  )
}

function JobRow({ job }: { job: Job }) {
  return (
    <div className="grid gap-3 px-4 py-3 text-sm xl:grid-cols-[1.1fr_1.1fr_1fr_1fr_1.2fr]">
      <div className="min-w-0">
        <div className="truncate font-medium text-slate-950">{job.artifact.filename}</div>
        <div className="truncate text-xs text-slate-600">
          {job.artifact.content_type} · {formatBytes(job.artifact.size_bytes)}
        </div>
        <div className="truncate text-xs text-slate-700">{formatArtifactMetadata(job)}</div>
        <div className="truncate font-mono text-xs text-slate-600">Job {job.id}</div>
        <div className="truncate font-mono text-xs text-slate-600">Artifact {job.artifact.id}</div>
      </div>
      <div className="min-w-0">
        <div className="flex flex-wrap gap-2">
          <StatusPill label="Dispatch" value={job.status} />
          <StatusPill label="Print" value={job.print.status} />
        </div>
        <div className="mt-1 truncate font-mono text-xs text-slate-600">Command {job.command.id}</div>
        <div className="truncate text-xs text-slate-600">{job.command.kind}</div>
        <div className="mt-1 text-xs text-slate-700">{formatJobRecoveryState(job)}</div>
        {job.error ? <div className="mt-1 text-xs text-red-700">{job.error}</div> : null}
        {job.print.error ? <div className="mt-1 text-xs text-red-700">{job.print.error}</div> : null}
      </div>
      <div className="min-w-0">
        <div className="font-mono text-xs text-slate-700">Printer {job.printer_id}</div>
        <div className="font-mono text-xs text-slate-700">Agent {job.agent_id}</div>
        <div className="mt-1 text-xs text-slate-600">Created {formatDate(job.created_at)}</div>
        <div className="text-xs text-slate-600">Updated {formatDate(job.updated_at)}</div>
      </div>
      <div>
        <div className="font-medium text-slate-900">{formatProgress(job)}</div>
        <div className="text-xs text-slate-600">{formatLayers(job)}</div>
        <div className="text-xs text-slate-600">{formatRemaining(job.print.remaining_time_minutes)}</div>
        {job.print.active_file ? <div className="mt-1 truncate text-xs text-slate-700">File {job.print.active_file}</div> : null}
        {job.print.printer_state ? <div className="truncate text-xs text-slate-600">State {job.print.printer_state}</div> : null}
        <div className="mt-1 text-xs text-slate-600">
          Started {job.print.started_at ? formatDate(job.print.started_at) : '-'}
        </div>
        <div className="text-xs text-slate-600">
          Finished {job.print.finished_at ? formatDate(job.print.finished_at) : '-'}
        </div>
      </div>
      <div className="min-w-0 text-slate-700">
        <div className="text-xs font-medium uppercase text-slate-500">Material</div>
        <div className="mt-1 text-sm">{formatJobMaterial(job)}</div>
      </div>
    </div>
  )
}

function StatusPill({ label, value }: { label: string; value: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      <span className="text-xs text-slate-500">{label}</span>
      <StatusBadge value={value} />
    </span>
  )
}

export function TenantSettings({
  auth,
  authLabel,
  selectedTenant,
  agents,
  printers,
}: {
  auth: AuthMetadata
  authLabel: string
  selectedTenant: Tenant | null
  agents: Agent[]
  printers: Printer[]
}) {
  const tenantId = selectedTenant?.id ?? '{tenant_id}'
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <SectionHeader
        title="Tenant settings"
        subtitle={selectedTenant ? `${selectedTenant.display_name} operational references` : 'No tenant selected'}
        meta="No token values shown"
      />
      <div className="grid gap-4 px-4 py-3 text-sm lg:grid-cols-3">
        <DetailGroup title="Tenant">
          <DetailLine label="ID" value={selectedTenant?.id ?? '-'} mono />
          <DetailLine label="Slug" value={selectedTenant?.slug ?? '-'} />
          <DetailLine label="Created" value={selectedTenant ? formatDate(selectedTenant.created_at) : '-'} />
        </DetailGroup>
        <DetailGroup title="Authentication">
          <DetailLine label="Source" value={authLabel} />
          <DetailLine label="Provider" value={auth.provider} />
          <DetailLine label="Cookie name" value={auth.cookieName} mono />
          <DetailLine label="Secret values" value="Hidden" />
        </DetailGroup>
        <DetailGroup title="Operations">
          <DetailLine label="Agent pairing" value={`/api/v1/tenants/${tenantId}/agent-pairings`} mono />
          <DetailLine label="API tokens" value={`/api/v1/tenants/${tenantId}/users/{user_id}/api-tokens`} mono />
          <DetailLine label="Diagnostics" value="Discovery and diagnostics panel" />
        </DetailGroup>
      </div>
      <div className="border-t border-slate-200 px-4 py-3">
        <div className="text-xs font-medium uppercase text-slate-500">Linked agents</div>
        {agents.length === 0 ? (
          <div className="mt-2 text-sm text-slate-600">No linked agents</div>
        ) : (
          <div className="mt-2 flex flex-wrap gap-2">
            {agents.map((agent) => (
              <span key={agent.id} className="rounded border border-slate-300 px-2 py-1 text-xs">
                {agent.name} · {agent.status}
              </span>
            ))}
          </div>
        )}
      </div>
      <div className="border-t border-slate-200 px-4 py-3">
        <div className="text-xs font-medium uppercase text-slate-500">Printer compatibility</div>
        {printers.length === 0 ? (
          <div className="mt-2 text-sm text-slate-600">No reported printers</div>
        ) : (
          <div className="mt-2 grid gap-2 md:grid-cols-2">
            {printers.map((printer) => (
              <div key={printer.id} className="min-w-0 rounded border border-slate-300 px-2 py-2 text-xs">
                <div className="truncate font-medium text-slate-900">{printer.name}</div>
                <div className="truncate text-slate-600">{printer.model ?? 'Unknown model'}</div>
                <div className="mt-1 truncate font-mono text-slate-700">
                  POST /api/v1/tenants/{tenantId}/agents/{printer.agent_id}/diagnose-printer
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  )
}
