'use client'

import { useMemo, useState } from 'react'
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
import { FilterBar } from './dashboard-filter-bar'
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
  onOpenReprint,
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
  onOpenReprint: (job: Job) => void
  onClearRedirect?: (url: string) => void
  onDeleteRedirect?: (url: string) => void
}) {
  const t = useTranslations('inventory')
  const [query, setQuery] = useState('')
  const [status, setStatus] = useState('all')
  const [clearOpen, setClearOpen] = useState(false)
  const [clearing, setClearing] = useState(false)
  const [clearError, setClearError] = useState<'generic' | 'permission' | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<Job | null>(null)
  const [deleting, setDeleting] = useState(false)
  const [deleteError, setDeleteError] = useState(false)
  const clearableCount = jobs.filter(isClearableJob).length
  const clearDisabled =
    !canManageJobs || !selectedTenant || clearableCount === 0 || clearing
  const newDisabled = !selectedTenant || printers.length === 0
  const normalizedQuery = query.trim().toLowerCase()
  const filtered = useMemo(
    () =>
      jobs.filter((job) => {
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
      }),
    [jobs, status, normalizedQuery],
  )
  const printerNames = useMemo(
    () => new Map(printers.map((printer) => [printer.id, printer.name])),
    [printers],
  )
  const agentNames = useMemo(
    () => new Map(agents.map((agent) => [agent.id, agent.name])),
    [agents],
  )

  const clearJobs = async () => {
    if (!selectedTenant) {
      return
    }
    setClearing(true)
    setClearError(null)
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
        setClearError(response.status === 401 || response.status === 403 ? 'permission' : 'generic')
      }
    } catch {
      setClearError('generic')
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
    <section className="overflow-hidden rounded-md border border-border bg-card">
      <SectionHeader
        title={t('jobsTitle')}
        subtitle={t('jobsSubtitle')}
        actions={
          <>
            <Button
              aria-haspopup="dialog"
              disabled={newDisabled}
              onClick={onOpenDispatch}
              title={newDisabled ? (!selectedTenant ? t('jobsNoTenantTitle') : t('noPrintersTitle')) : undefined}
              type="button"
            >
              {t('newJob')}
            </Button>
            <Button
              disabled={clearDisabled}
              onClick={() => {
                setClearError(null)
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
            <div>
              <EmptyState
                title={t('jobsNoMatchesTitle')}
                message={t('jobsNoMatchesMessage')}
              />
              <div className="flex justify-center pb-4">
                <Button
                  onClick={() => {
                    setQuery('')
                    setStatus('all')
                  }}
                  type="button"
                  variant="outline"
                >
                  {t('clearJobFilters')}
                </Button>
              </div>
            </div>
          ) : (
            <ul
              className="divide-y divide-border"
              aria-label={t('jobsAria')}
            >
              {filtered.map((job) => (
                <JobRow
                  key={job.id}
                  job={job}
                  printerName={printerNames.get(job.printer_id)}
                  agentName={agentNames.get(job.agent_id)}
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
                  onReprint={() => onOpenReprint(job)}
                />
              ))}
            </ul>
          )}
        </>
      )}
      <Dialog
        open={clearOpen}
        onOpenChange={(open) => {
          if (!open && clearing) return
          setClearOpen(open)
        }}
      >
        <DialogContent closeLabel={t('cancel')} showCloseButton={!clearing}>
          <DialogHeader>
            <DialogTitle>{t('clearJobsTitle')}</DialogTitle>
            <DialogDescription>
              {t('clearJobsDescription', { count: clearableCount })}
            </DialogDescription>
            {clearError ? (
              <p className="text-sm text-destructive" role="alert">
                {clearError === 'permission' ? t('clearJobsFailedPermission') : t('clearJobsFailed')}
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
