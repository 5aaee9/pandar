'use client'

import { useState } from 'react'

import { OFFLINE_PRINTER_STATUSES } from './dashboard-attention'
import type { Agent, Job, Printer, Tenant } from './dashboard-types'
import { EmptyState, formatBytes, formatDate, SectionHeader, StatusBadge } from './dashboard-ui'
import {
  formatArtifactMetadata,
  formatJobMaterial,
  formatJobRecoveryState,
  formatPrinterMaterials,
} from './dashboard-runtime-helpers'
import { formatLayers, formatProgress, formatRemaining } from './job-format'

export function PrinterInventory({
  selectedTenant,
  printers,
  agents,
}: {
  selectedTenant: Tenant | null
  printers: Printer[]
  agents: Agent[]
}) {
  const [query, setQuery] = useState('')
  const [status, setStatus] = useState('all')
  const normalizedQuery = query.trim().toLowerCase()
  const filtered = printers.filter((printer) => {
    const needsAttention = OFFLINE_PRINTER_STATUSES.has(printer.status.toLowerCase())
    if (status === 'online' && needsAttention) {
      return false
    }
    if (status === 'attention' && !needsAttention) {
      return false
    }
    if (normalizedQuery) {
      const haystack = `${printer.name} ${printer.serial_number}`.toLowerCase()
      if (!haystack.includes(normalizedQuery)) {
        return false
      }
    }
    return true
  })

  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <SectionHeader
        title="Printer inventory"
        subtitle={selectedTenant ? `${selectedTenant.display_name} (${selectedTenant.slug})` : 'No tenant selected'}
        meta={`${printers.length} reported`}
      />
      {!selectedTenant ? (
        <EmptyState title="No tenants" message="Ask your administrator to create a tenant and assign you to it, then select it in the header. Printers appear here once an agent reports them." />
      ) : printers.length === 0 ? (
        <EmptyState title="No printers reported" message="Connect an agent and run a printer refresh to populate this inventory." />
      ) : (
        <>
          <FilterBar
            query={query}
            onQueryChange={setQuery}
            queryPlaceholder="Search name or serial"
            status={status}
            onStatusChange={setStatus}
            statusOptions={[
              { value: 'all', label: 'All statuses' },
              { value: 'online', label: 'Online' },
              { value: 'attention', label: 'Needs attention' },
            ]}
          />
            {filtered.length === 0 ? (
            <EmptyState title="No matches" message="No printers match your search or filter." />
            ) : (
            <div className="divide-y divide-slate-200">
              {filtered.map((printer) => {
                const material = formatPrinterMaterials(printer)
                const agent = agents.find((candidate) => candidate.id === printer.agent_id)
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
                    <div className="min-w-0 text-xs text-slate-600">
                      Managed by <span className="font-medium text-slate-800">{agent?.name ?? 'Unknown agent'}</span>
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </>
      )}
    </section>
  )
}

const TERMINAL_JOB_STATUSES = new Set(['completed', 'failed', 'cancelled'])

function jobMatchesStatus(job: Job, status: string): boolean {
  const dispatch = job.status.toLowerCase()
  const physical = job.print.status.toLowerCase()
  if (status === 'active') {
    return !TERMINAL_JOB_STATUSES.has(dispatch) && !TERMINAL_JOB_STATUSES.has(physical)
  }
  if (status === 'failed') {
    return dispatch === 'failed' || physical === 'failed'
  }
  if (status === 'completed') {
    return dispatch === 'completed' || physical === 'completed'
  }
  return true
}

export function JobHistory({
  selectedTenant,
  jobs,
  printers,
  agents,
}: {
  selectedTenant: Tenant | null
  jobs: Job[]
  printers: Printer[]
  agents: Agent[]
}) {
  const [query, setQuery] = useState('')
  const [status, setStatus] = useState('all')
  const normalizedQuery = query.trim().toLowerCase()
  const filtered = jobs.filter((job) => {
    if (!jobMatchesStatus(job, status)) {
      return false
    }
    if (normalizedQuery) {
      const haystack = `${job.artifact.filename} ${job.id}`.toLowerCase()
      if (!haystack.includes(normalizedQuery)) {
        return false
      }
    }
    return true
  })

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
        <EmptyState title="No jobs" message="Dispatch a project file from the Dispatch section to a printer to create your first print job." />
      ) : (
        <>
          <FilterBar
            query={query}
            onQueryChange={setQuery}
            queryPlaceholder="Search filename or job id"
            status={status}
            onStatusChange={setStatus}
            statusOptions={[
              { value: 'all', label: 'All jobs' },
              { value: 'active', label: 'Active' },
              { value: 'failed', label: 'Failed' },
              { value: 'completed', label: 'Completed' },
            ]}
          />
          {filtered.length === 0 ? (
            <EmptyState title="No matches" message="No jobs match your search or filter." />
          ) : (
            <div className="divide-y divide-slate-200" role="list" aria-label="Print jobs">
              {filtered.map((job) => {
                const printer = printers.find((candidate) => candidate.id === job.printer_id)
                const agent = agents.find((candidate) => candidate.id === job.agent_id)
                return (
                  <JobRow
                    key={job.id}
                    job={job}
                    printerName={printer?.name}
                    agentName={agent?.name}
                  />
                )
              })}
            </div>
          )}
        </>
      )}
    </section>
  )
}

function FilterBar({
  query,
  onQueryChange,
  queryPlaceholder,
  status,
  onStatusChange,
  statusOptions,
}: {
  query: string
  onQueryChange: (value: string) => void
  queryPlaceholder: string
  status: string
  onStatusChange: (value: string) => void
  statusOptions: Array<{ value: string; label: string }>
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-slate-200 px-4 py-2">
      <input
        aria-label={queryPlaceholder}
        className="min-w-40 flex-1 rounded-md border border-slate-300 bg-white px-2 py-1 text-sm"
        onChange={(event) => onQueryChange(event.target.value)}
        placeholder={queryPlaceholder}
        type="search"
        value={query}
      />
      <select
        aria-label="Filter by status"
        className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm"
        onChange={(event) => onStatusChange(event.target.value)}
        value={status}
      >
        {statusOptions.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  )
}

function JobRow({
  job,
  printerName,
  agentName,
}: {
  job: Job
  printerName?: string
  agentName?: string
}) {
  const updated = job.print.updated_at ?? job.updated_at
  return (
    <div
      aria-label={`${job.artifact.filename}, dispatch ${job.status}, print ${job.print.status}, ${formatProgress(job)}`}
      className="px-4 py-3"
      role="listitem"
    >
      <div className="grid gap-3 text-sm xl:grid-cols-[1.4fr_1fr_1fr_1fr]">
        <div className="min-w-0">
          <div className="truncate font-medium text-slate-950">{job.artifact.filename}</div>
          <div className="truncate text-xs text-slate-500">Updated {formatDate(updated)}</div>
        </div>
        <div className="min-w-0">
          <div className="flex flex-wrap gap-2">
            <StatusPill label="Dispatch" value={job.status} />
            <StatusPill label="Print" value={job.print.status} />
          </div>
          {job.error ? <div className="mt-1 truncate text-xs text-red-700">{job.error}</div> : null}
          {job.print.error ? <div className="mt-1 truncate text-xs text-red-700">{job.print.error}</div> : null}
        </div>
        <div className="min-w-0 text-xs text-slate-600">
          <div className="truncate font-medium text-slate-900">{printerName ?? 'Unknown printer'}</div>
          <div className="truncate">{agentName ?? 'Unknown agent'}</div>
        </div>
        <div>
          <div className="font-medium text-slate-900">{formatProgress(job)}</div>
          <div className="text-xs text-slate-600">{formatLayers(job)}</div>
          <div className="text-xs text-slate-600">{formatRemaining(job.print.remaining_time_minutes)}</div>
        </div>
      </div>
      <details className="mt-2">
        <summary className="cursor-pointer select-none text-xs font-medium text-slate-500">Details</summary>
        <div className="mt-2 grid gap-2 text-xs text-slate-600 sm:grid-cols-2 lg:grid-cols-3">
          <div className="sm:col-span-2 lg:col-span-3">
            <span className="text-slate-500">Recovery: </span>
            {formatJobRecoveryState(job)}
          </div>
          <div className="sm:col-span-2 lg:col-span-3 truncate">
            <span className="text-slate-500">Project: </span>
            {formatArtifactMetadata(job)}
          </div>
          <div>
            <span className="text-slate-500">Artifact: </span>
            {job.artifact.content_type} · {formatBytes(job.artifact.size_bytes)}
          </div>
          <div>
            <span className="text-slate-500">Material: </span>
            {formatJobMaterial(job)}
          </div>
          <div>
            <span className="text-slate-500">Job: </span>
            <span className="font-mono">{job.id}</span>
          </div>
          {job.print.active_file ? (
            <div className="truncate">
              <span className="text-slate-500">File: </span>
              {job.print.active_file}
            </div>
          ) : null}
          {job.print.printer_state ? (
            <div>
              <span className="text-slate-500">State: </span>
              {job.print.printer_state}
            </div>
          ) : null}
          <div>
            <span className="text-slate-500">Created: </span>
            {formatDate(job.created_at)}
          </div>
          <div>
            <span className="text-slate-500">Started: </span>
            {job.print.started_at ? formatDate(job.print.started_at) : '-'}
          </div>
          <div>
            <span className="text-slate-500">Finished: </span>
            {job.print.finished_at ? formatDate(job.print.finished_at) : '-'}
          </div>
        </div>
      </details>
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
