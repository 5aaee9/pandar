'use client'

import { useState } from 'react'
import { useFormatter, useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import { OFFLINE_PRINTER_STATUSES } from './dashboard-attention'
import type { Agent, Job, Printer, Tenant } from './dashboard-types'
import { EmptyState, formatBytes, SectionHeader, StatusBadge } from './dashboard-ui'
import {
  formatArtifactMetadata,
  formatJobMaterial,
  formatJobRecoveryState,
  formatPrinterMaterials,
} from './dashboard-runtime-helpers'
import { formatLayers, formatProgress, formatRemaining } from './job-format'

function useLocaleDate() {
  const format = useFormatter()
  return (value: string) => {
    const d = new Date(value)
    if (Number.isNaN(d.getTime())) return value
    return format.dateTime(d, { dateStyle: 'medium', timeStyle: 'short', timeZone: 'UTC' })
  }
}

export function PrinterInventory({
  selectedTenant,
  printers,
  agents,
}: {
  selectedTenant: Tenant | null
  printers: Printer[]
  agents: Agent[]
}) {
  const t = useTranslations('inventory')
  const tMat = useTranslations('material')
  const formatDate = useLocaleDate()
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
        title={t('printersTitle')}
        subtitle={
          selectedTenant
            ? t('printersSubtitleTenant', { name: selectedTenant.display_name, slug: selectedTenant.slug })
            : t('printersSubtitleNone')
        }
        meta={t('printersMeta', { count: printers.length })}
      />
      {!selectedTenant ? (
        <EmptyState title={t('noTenantTitle')} message={t('noTenantMessage')} />
      ) : printers.length === 0 ? (
        <EmptyState title={t('noPrintersTitle')} message={t('noPrintersMessage')} />
      ) : (
        <>
          <FilterBar
            query={query}
            onQueryChange={setQuery}
            queryPlaceholder={t('searchName')}
            status={status}
            onStatusChange={setStatus}
            statusOptions={[
              { value: 'all', label: t('filterAll') },
              { value: 'online', label: t('filterOnline') },
              { value: 'attention', label: t('filterAttention') },
            ]}
          />
            {filtered.length === 0 ? (
            <EmptyState title={t('noMatchesTitle')} message={t('noMatchesMessage')} />
            ) : (
            <div className="divide-y divide-slate-200">
              {filtered.map((printer) => {
                const material = formatPrinterMaterials(printer, tMat, formatDate)
                const agent = agents.find((candidate) => candidate.id === printer.agent_id)
                return (
                  <div key={printer.id} className="grid gap-3 px-4 py-3 text-sm lg:grid-cols-[1.2fr_1fr_1.3fr_1.5fr]">
                    <div className="min-w-0">
                      <div className="truncate font-medium text-slate-950">{printer.name}</div>
                      <div className="truncate font-mono text-xs text-slate-600">{printer.serial_number}</div>
                      <div className="text-xs text-slate-600">{printer.model ?? t('unknownModel')}</div>
                    </div>
                    <div>
                      <StatusBadge value={printer.status} />
                      <div className="mt-1 text-xs text-slate-600">
                        <FormattedDate value={printer.last_seen_at} />
                      </div>
                    </div>
                    <div>
                      <div className="text-slate-800">{material.summary}</div>
                      <div className="text-xs text-slate-600">{material.detail}</div>
                    </div>
                    <div className="min-w-0 text-xs text-slate-600">
                      {t('managedBy')} <span className="font-medium text-slate-800">{agent?.name ?? t('unknownAgent')}</span>
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
  const t = useTranslations('inventory')
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
        title={t('jobsTitle')}
        subtitle={t('jobsSubtitle')}
        meta={t('jobsMeta', { count: jobs.length })}
      />
      {!selectedTenant ? (
        <EmptyState title={t('jobsNoTenantTitle')} message={t('jobsNoTenantMessage')} />
      ) : jobs.length === 0 ? (
        <EmptyState title={t('jobsEmptyTitle')} message={t('jobsEmptyMessage')} />
      ) : (
        <>
          <FilterBar
            query={query}
            onQueryChange={setQuery}
            queryPlaceholder={t('searchJob')}
            status={status}
            onStatusChange={setStatus}
            statusOptions={[
              { value: 'all', label: t('jobFilterAll') },
              { value: 'active', label: t('jobFilterActive') },
              { value: 'failed', label: t('jobFilterFailed') },
              { value: 'completed', label: t('jobFilterCompleted') },
            ]}
          />
          {filtered.length === 0 ? (
            <EmptyState title={t('jobsNoMatchesTitle')} message={t('jobsNoMatchesMessage')} />
          ) : (
            <div className="divide-y divide-slate-200" role="list" aria-label={t('jobsAria')}>
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
  const t = useTranslations('inventory')
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
        aria-label={t('filterStatusAria')}
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
  const t = useTranslations('inventory')
  const tMat = useTranslations('material')
  const tRec = useTranslations('recovery.state')
  const tJf = useTranslations('jobFormat')
  const formatDate = useLocaleDate()
  const updated = job.print.updated_at ?? job.updated_at
  return (
    <div
      aria-label={`${job.artifact.filename}, ${t('dispatch')} ${job.status}, ${t('print')} ${job.print.status}, ${formatProgress(job)}`}
      className="px-4 py-3"
      role="listitem"
    >
      <div className="grid gap-3 text-sm xl:grid-cols-[1.4fr_1fr_1fr_1fr]">
        <div className="min-w-0">
          <div className="truncate font-medium text-slate-950">{job.artifact.filename}</div>
          <div className="truncate text-xs text-slate-500">
            {t('updatedPrefix')} <FormattedDate value={updated} />
          </div>
        </div>
        <div className="min-w-0">
          <div className="flex flex-wrap gap-2">
            <StatusPill label={t('dispatch')} value={job.status} />
            <StatusPill label={t('print')} value={job.print.status} />
          </div>
          {job.error ? <div className="mt-1 truncate text-xs text-red-700">{job.error}</div> : null}
          {job.print.error ? <div className="mt-1 truncate text-xs text-red-700">{job.print.error}</div> : null}
        </div>
        <div className="min-w-0 text-xs text-slate-600">
          <div className="truncate font-medium text-slate-900">{printerName ?? t('unknownPrinter')}</div>
          <div className="truncate">{agentName ?? t('unknownAgent')}</div>
        </div>
        <div>
          <div className="font-medium text-slate-900">{formatProgress(job)}</div>
          <div className="text-xs text-slate-600">{formatLayers(job, tJf)}</div>
          <div className="text-xs text-slate-600">{formatRemaining(job.print.remaining_time_minutes, tJf)}</div>
        </div>
      </div>
      <details className="mt-2">
        <summary className="cursor-pointer select-none text-xs font-medium text-slate-500">{t('details')}</summary>
        <div className="mt-2 grid gap-2 text-xs text-slate-600 sm:grid-cols-2 lg:grid-cols-3">
          <div className="sm:col-span-2 lg:col-span-3">
            <span className="text-slate-500">{t('recoveryLabel')} </span>
            {formatJobRecoveryState(job, tRec)}
          </div>
          <div className="sm:col-span-2 lg:col-span-3 truncate">
            <span className="text-slate-500">{t('projectLabel')} </span>
            {formatArtifactMetadata(job, tMat, formatDate)}
          </div>
          <div>
            <span className="text-slate-500">{t('artifactLabel')} </span>
            {job.artifact.content_type} · {formatBytes(job.artifact.size_bytes)}
          </div>
          <div>
            <span className="text-slate-500">{t('materialLabel')} </span>
            {formatJobMaterial(job, tMat)}
          </div>
          <div>
            <span className="text-slate-500">{t('jobLabel')} </span>
            <span className="font-mono">{job.id}</span>
          </div>
          {job.print.active_file ? (
            <div className="truncate">
              <span className="text-slate-500">{t('fileLabel')} </span>
              {job.print.active_file}
            </div>
          ) : null}
          {job.print.printer_state ? (
            <div>
              <span className="text-slate-500">{t('stateLabel')} </span>
              {job.print.printer_state}
            </div>
          ) : null}
          <div>
            <span className="text-slate-500">{t('createdLabel')} </span>
            <FormattedDate value={job.created_at} />
          </div>
          <div>
            <span className="text-slate-500">{t('startedLabel')} </span>
            {job.print.started_at ? <FormattedDate value={job.print.started_at} /> : '-'}
          </div>
          <div>
            <span className="text-slate-500">{t('finishedLabel')} </span>
            {job.print.finished_at ? <FormattedDate value={job.print.finished_at} /> : '-'}
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
