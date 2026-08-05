'use client'

import { useMemo, useState } from 'react'
import { useDashboardFilterStore } from './dashboard-filter-store'
import { useTranslations } from 'next-intl'
import { useMutation, useQueryClient } from '@tanstack/react-query'

import { Button } from '@/components/ui/button'
import type { Agent, Job, Printer, Tenant } from './dashboard-types'
import { apiIdSegment } from './api-path'
import { JobsRouteData, routeDataKeys } from './route-data'
import { FilterBar } from './dashboard-filter-bar'
import {
  isClearableJob,
  isRetryDispatchSafe,
  jobMatchesStatus,
} from './dashboard-job-status'
import { EmptyState, SectionHeader } from './dashboard-ui'
import { JobRow } from './dashboard-job-row'
import { ClearJobsDialog } from './clear-jobs-dialog'
import { DeleteJobDialog } from './delete-job-dialog'

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
  const queryClient = useQueryClient()
  const query = useDashboardFilterStore((state) => state.query)
  const status = useDashboardFilterStore((state) => state.status)
  const setQuery = useDashboardFilterStore((state) => state.setQuery)
  const setStatus = useDashboardFilterStore((state) => state.setStatus)
  const reset = useDashboardFilterStore((state) => state.reset)
  const [clearOpen, setClearOpen] = useState(false)
  const [clearError, setClearError] = useState<'generic' | 'permission' | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<Job | null>(null)
  const [deleteError, setDeleteError] = useState(false)

  const clearMutation = useMutation({
    mutationFn: async () => {
      const response = await fetch(
        `/api/tenants/${apiIdSegment(selectedTenant!.id, 'tenant_id')}/jobs`,
        { method: 'DELETE' },
      )
      if (!response.ok) {
        throw { status: response.status }
      }
      return response
    },
    onMutate: async () => {
      await queryClient.cancelQueries({ queryKey: routeDataKeys.jobs(selectedTenant!.id) })
      const previousData = queryClient.getQueryData(routeDataKeys.jobs(selectedTenant!.id))
      queryClient.setQueryData(routeDataKeys.jobs(selectedTenant!.id), (old: JobsRouteData | undefined) => {
        if (!old) return old
        return { ...old, jobs: old.jobs.filter((job) => !isClearableJob(job)) }
      })
      return { previousData }
    },
    onSuccess: () => {
      setClearOpen(false)
      queryClient.invalidateQueries({ queryKey: routeDataKeys.jobs(selectedTenant!.id) })
      onClearRedirect(
        `/jobs?status=jobs_cleared`,
      )
    },
    onError: (error: { status?: number }, _variables, context) => {
      if (context?.previousData) {
        queryClient.setQueryData(routeDataKeys.jobs(selectedTenant!.id), context.previousData)
      }
      setClearError(
        error.status === 401 || error.status === 403 ? 'permission' : 'generic',
      )
    },
  })

  const deleteMutation = useMutation({
    mutationFn: async (jobId: string) => {
      const response = await fetch(
        `/api/tenants/${apiIdSegment(selectedTenant!.id, 'tenant_id')}/jobs/${apiIdSegment(jobId, 'job_id')}`,
        { method: 'DELETE' },
      )
      if (!response.ok) {
        throw new Error('delete failed')
      }
      return response
    },
    onMutate: async (jobId) => {
      await queryClient.cancelQueries({ queryKey: routeDataKeys.jobs(selectedTenant!.id) })
      const previousData = queryClient.getQueryData(routeDataKeys.jobs(selectedTenant!.id))
      queryClient.setQueryData(routeDataKeys.jobs(selectedTenant!.id), (old: JobsRouteData | undefined) => {
        if (!old) return old
        return { ...old, jobs: old.jobs.filter((job) => job.id !== jobId) }
      })
      return { previousData }
    },
    onSuccess: () => {
      setDeleteTarget(null)
      queryClient.invalidateQueries({ queryKey: routeDataKeys.jobs(selectedTenant!.id) })
      onDeleteRedirect(
        `/jobs?status=job_deleted`,
      )
    },
    onError: (_error, _jobId, context) => {
      if (context?.previousData) {
        queryClient.setQueryData(routeDataKeys.jobs(selectedTenant!.id), context.previousData)
      }
      setDeleteError(true)
    },
  })

  const clearing = clearMutation.isPending
  const deleting = deleteMutation.isPending
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

  const clearJobs = () => {
    if (!selectedTenant) {
      return
    }
    setClearError(null)
    clearMutation.mutate()
  }

  const deleteJob = () => {
    if (!selectedTenant || !deleteTarget) {
      return
    }
    setDeleteError(false)
    deleteMutation.mutate(deleteTarget.id)
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
                  onClick={reset}
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
                  canRetryDispatch={canManageJobs && isRetryDispatchSafe(job)}
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
      <ClearJobsDialog
        open={clearOpen}
        clearing={clearing}
        clearableCount={clearableCount}
        error={clearError}
        onOpenChange={setClearOpen}
        onConfirm={clearJobs}
      />
      <DeleteJobDialog
        target={deleteTarget}
        deleting={deleting}
        error={deleteError}
        onClose={() => {
          if (!deleting) {
            setDeleteTarget(null)
            setDeleteError(false)
          }
        }}
        onConfirm={deleteJob}
      />
    </section>
  )
}
