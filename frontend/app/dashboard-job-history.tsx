'use client'

import { useState } from 'react'
import { useTranslations } from 'next-intl'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type { Agent, Job, Printer, Tenant } from './dashboard-types'
import { apiIdSegment } from './api-path'
import { EmptyState, SectionHeader } from './dashboard-ui'
import { JobRow } from './dashboard-job-row'

const TERMINAL_JOB_STATUSES = new Set(['stalled', 'completed', 'failed', 'cancelled'])
const CLEARABLE_JOB_STATUSES = new Set(['succeeded', 'failed'])
const CLEARABLE_PRINT_STATUSES = new Set(['stalled', 'completed', 'failed', 'cancelled'])

function isClearableJob(job: Job): boolean {
  const status = job.status.toLowerCase()
  const commandStatus = job.command.status.toLowerCase()
  const printStatus = job.print.status.toLowerCase()
  if (
    !CLEARABLE_JOB_STATUSES.has(status) ||
    !CLEARABLE_JOB_STATUSES.has(commandStatus) ||
    job.command.kind !== 'print_project_file'
  ) {
    return false
  }
  if (CLEARABLE_PRINT_STATUSES.has(printStatus)) {
    return true
  }
  return (
    printStatus === 'pending' &&
    job.print.started_at === null &&
    (job.print.progress_percent ?? 0) === 0 &&
    (job.print.current_layer ?? 0) === 0 &&
    status === 'failed'
  )
}

function jobMatchesStatus(job: Job, status: string): boolean {
  const dispatch = job.status.toLowerCase()
  const physical = job.print.status.toLowerCase()
  if (status === 'active') {
    return (
      !TERMINAL_JOB_STATUSES.has(dispatch) &&
      !TERMINAL_JOB_STATUSES.has(physical)
    )
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
  canManageJobs = true,
  onOpenDispatch,
  onClearRedirect = (url) => window.location.assign(url),
  onDeleteRedirect = (url) => window.location.assign(url),
}: {
  selectedTenant: Tenant | null
  jobs: Job[]
  nowMs: number
  printers: Printer[]
  agents: Agent[]
  canManageJobs?: boolean
  onOpenDispatch: () => void
  onClearRedirect?: (url: string) => void
  onDeleteRedirect?: (url: string) => void
}) {
  const t = useTranslations('inventory')
  const [query, setQuery] = useState('')
  const [status, setStatus] = useState('all')
  const [clearOpen, setClearOpen] = useState(false)
  const [clearing, setClearing] = useState(false)
  const [clearError, setClearError] = useState(false)
  const [deleteTarget, setDeleteTarget] = useState<Job | null>(null)
  const [deleting, setDeleting] = useState(false)
  const [deleteError, setDeleteError] = useState(false)
  const clearableCount = jobs.filter(isClearableJob).length
  const clearDisabled =
    !canManageJobs || !selectedTenant || clearableCount === 0 || clearing
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
        `/api/tenants/${apiIdSegment(selectedTenant.id, 'tenant_id')}/jobs`,
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

  const deleteJob = async () => {
    if (!selectedTenant || !deleteTarget) {
      return
    }
    setDeleting(true)
    setDeleteError(false)
    try {
      const response = await fetch(
        `/api/tenants/${apiIdSegment(selectedTenant.id, 'tenant_id')}/jobs/${apiIdSegment(deleteTarget.id, 'job_id')}`,
        { method: 'DELETE' },
      )
      if (response.ok) {
        setDeleteTarget(null)
        onDeleteRedirect(
          `/jobs?tenant=${encodeURIComponent(selectedTenant.id)}&status=job_deleted`,
        )
      } else {
        setDeleteError(true)
      }
    } catch {
      setDeleteError(true)
    } finally {
      setDeleting(false)
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
        <EmptyState
          title={t('jobsNoTenantTitle')}
          message={t('jobsNoTenantMessage')}
        />
      ) : jobs.length === 0 ? (
        <EmptyState
          title={t('jobsEmptyTitle')}
          message={t('jobsEmptyMessage')}
        />
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
            <EmptyState
              title={t('jobsNoMatchesTitle')}
              message={t('jobsNoMatchesMessage')}
            />
          ) : (
            <ul
              className="divide-y divide-slate-200"
              aria-label={t('jobsAria')}
            >
              {filtered.map((job) => {
                const printer = printers.find(
                  (candidate) => candidate.id === job.printer_id,
                )
                const agent = agents.find(
                  (candidate) => candidate.id === job.agent_id,
                )
                return (
                  <JobRow
                    tenantId={selectedTenant.id}
                    key={job.id}
                    job={job}
                    printerName={printer?.name}
                    agentName={agent?.name}
                    canDelete={canManageJobs && isClearableJob(job)}
                    deleteUnavailableReason={
                      canManageJobs
                        ? t('deleteJobUnavailable')
                        : t('deleteJobAdminOnly')
                    }
                    onDelete={() => {
                      setDeleteError(false)
                      setDeleteTarget(job)
                    }}
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
            {clearError ? (
              <p className="text-sm text-destructive" role="alert">
                {t('clearJobsFailed')}
              </p>
            ) : null}
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
      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open && !deleting) {
            setDeleteTarget(null)
            setDeleteError(false)
          }
        }}
      >
        <DialogContent
          closeLabel={t('closeDialog')}
          showCloseButton={!deleting}
        >
          <DialogHeader>
            <DialogTitle>{t('deleteJobTitle')}</DialogTitle>
            <DialogDescription>
              {deleteTarget
                ? t('deleteJobDescription', {
                    filename: deleteTarget.artifact.filename,
                  })
                : null}
            </DialogDescription>
            {deleteError ? (
              <p className="text-sm text-destructive" role="alert">
                {t('deleteJobFailed')}
              </p>
            ) : null}
          </DialogHeader>
          <DialogFooter>
            <Button
              disabled={deleting}
              onClick={() => {
                setDeleteTarget(null)
                setDeleteError(false)
              }}
              type="button"
              variant="outline"
            >
              {t('cancel')}
            </Button>
            <Button
              disabled={deleting}
              onClick={() => void deleteJob()}
              type="button"
              variant="destructive"
            >
              {deleting ? t('deletingJob') : t('confirmDeleteJob')}
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
