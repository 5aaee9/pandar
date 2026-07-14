'use client'

import { useState } from 'react'
import { useFormatter, useTranslations } from 'next-intl'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { FormattedDate } from '../components/formatted-date'
import type { Agent, Job, Printer, Tenant } from './dashboard-types'
import { formatBytes } from './dashboard-format'
import { EmptyState, SectionHeader, StatusBadge } from './dashboard-ui'
import {
  formatArtifactMetadata,
  formatJobMaterial,
  formatJobRecoveryState,
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
const TERMINAL_JOB_STATUSES = new Set(['completed', 'failed', 'cancelled'])
const CLEARABLE_JOB_STATUSES = new Set(['succeeded', 'failed'])
const CLEARABLE_PRINT_STATUSES = new Set(['completed', 'failed', 'cancelled'])

function isClearableJob(job: Job): boolean {
  const status = job.status.toLowerCase()
  const printStatus = job.print.status.toLowerCase()
  if (
    !CLEARABLE_JOB_STATUSES.has(status) ||
    !CLEARABLE_JOB_STATUSES.has(job.command.status.toLowerCase()) ||
    job.command.kind !== 'print_project_file'
  ) {
    return false
  }
  if (CLEARABLE_PRINT_STATUSES.has(printStatus)) {
    return true
  }
  return (
    printStatus === 'pending' &&
    status === 'failed' &&
    job.print.started_at === null &&
    (job.print.progress_percent ?? 0) === 0 &&
    (job.print.current_layer ?? 0) === 0
  )
}

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
  onOpenDispatch,
  onClearRedirect = (url) => window.location.assign(url),
}: {
  selectedTenant: Tenant | null
  jobs: Job[]
  printers: Printer[]
  agents: Agent[]
  onOpenDispatch: () => void
  onClearRedirect?: (url: string) => void
}) {
  const t = useTranslations('inventory')
  const [query, setQuery] = useState('')
  const [status, setStatus] = useState('all')
  const [clearOpen, setClearOpen] = useState(false)
  const [clearing, setClearing] = useState(false)
  const [clearError, setClearError] = useState(false)
  const clearableCount = jobs.filter(isClearableJob).length
  const clearDisabled = !selectedTenant || clearableCount === 0 || clearing
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

  const clearJobs = async () => {
    if (!selectedTenant) {
      return
    }
    setClearing(true)
    setClearError(false)
    try {
      const response = await fetch(
        `/api/tenants/${encodeURIComponent(selectedTenant.id)}/jobs`,
        { method: 'DELETE' },
      )
      if (response.ok) {
        setClearOpen(false)
        onClearRedirect(
          `/jobs?tenant=${encodeURIComponent(selectedTenant.id)}&status=jobs_cleared`,
        )
      } else {
        setClearError(true)
      }
    } catch {
      setClearError(true)
    } finally {
      setClearing(false)
    }
  }

  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <SectionHeader
        title={t('jobsTitle')}
        subtitle={t('jobsSubtitle')}
        actions={
          <>
            <Button
              aria-haspopup="dialog"
              onClick={onOpenDispatch}
              type="button"
            >
              {t('newJob')}
            </Button>
            <Button
              disabled={clearDisabled}
              onClick={() => {
                setClearError(false)
                setClearOpen(true)
              }}
              type="button"
              variant="outline"
            >
              {t('clearJobs')}
            </Button>
          </>
        }
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
            <ul className="divide-y divide-slate-200" aria-label={t('jobsAria')}>
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
            </ul>
          )}
        </>
      )}
      <Dialog open={clearOpen} onOpenChange={setClearOpen}>
        <DialogContent closeLabel={t('cancel')} showCloseButton={!clearing}>
          <DialogHeader>
            <DialogTitle>{t('clearJobsTitle')}</DialogTitle>
            <DialogDescription>
              {t('clearJobsDescription', { count: clearableCount })}
            </DialogDescription>
            {clearError ? <p className="text-sm text-destructive">{t('clearJobsFailed')}</p> : null}
          </DialogHeader>
          <DialogFooter>
            <Button
              disabled={clearing}
              onClick={() => setClearOpen(false)}
              type="button"
              variant="outline"
            >
              {t('cancel')}
            </Button>
            <Button
              disabled={clearing}
              onClick={() => void clearJobs()}
              type="button"
              variant="destructive"
            >
              {clearing ? t('clearingJobs') : t('confirmClearJobs')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  )
}

export function FilterBar({
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
  const format = useFormatter()
  const num = (n: number) => format.number(n)
  const updated = job.print.updated_at ?? job.updated_at
  return (
    <li
      aria-label={`${job.artifact.filename}, ${t('dispatch')} ${job.status}, ${t('print')} ${job.print.status}, ${formatProgress(job)}`}
      className="px-4 py-3"
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
            {job.artifact.content_type} · {formatBytes(job.artifact.size_bytes, num)}
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
    </li>
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
