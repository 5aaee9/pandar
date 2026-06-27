'use client'

import { useState } from 'react'
import { useFormatter, useTranslations } from 'next-intl'

import { controlPrinter, duplicateJob, refreshAllAgents, refreshPrinters, reprintJob, retryDispatchJob, retryDispatchJobs } from './actions'
import { ConfirmForm } from './confirm-dialog'
import type { Agent, Job, Printer, Tenant } from './dashboard-types'
import { EmptyState, SectionHeader } from './dashboard-ui'
import { formatArtifactMetadata, formatJobRecoveryState } from './dashboard-runtime-helpers'

const liveControlModelKeys = new Set(['A1', 'A1MINI', 'A1M', 'A1MIN', 'BAMBULABA1MINI', 'BAMBULABA1', 'P2S', 'N7', 'X2D', 'N6'])

function useLocaleDate() {
  const format = useFormatter()
  return (value: string) => {
    const d = new Date(value)
    if (Number.isNaN(d.getTime())) return value
    return format.dateTime(d, { dateStyle: 'medium', timeStyle: 'short', timeZone: 'UTC' })
  }
}

export function RecoveryActions({
  selectedTenant,
  agents,
  printers,
  jobs,
}: {
  selectedTenant: Tenant | null
  agents: Agent[]
  printers: Printer[]
  jobs: Job[]
}) {
  const t = useTranslations('recoveryPage')
  const tRec = useTranslations('recovery.state')
  const tMat = useTranslations('material')
  const formatDate = useLocaleDate()
  const [selected, setSelected] = useState<Set<string>>(new Set())

  const toggleJob = (jobId: string) => {
    setSelected((current) => {
      const next = new Set(current)
      if (next.has(jobId)) {
        next.delete(jobId)
      } else {
        next.add(jobId)
      }
      return next
    })
  }

  const failedJobIds = jobs.filter((job) => dispatchFailed(job)).map((job) => job.id)
  const allSelected = failedJobIds.length > 0 && failedJobIds.every((id) => selected.has(id))

  const toggleSelectAll = () => {
    setSelected(allSelected ? new Set() : new Set(failedJobIds))
  }

  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <SectionHeader
        title={t('title')}
        subtitle={t('subtitle')}
        meta={t('meta', { count: jobs.length })}
      />
      {!selectedTenant ? (
        <EmptyState title={t('noTenantTitle')} message={t('noTenantMessage')} />
      ) : (
        <div>
          <div className="flex flex-wrap items-center gap-2 border-b border-slate-200 px-4 py-3">
            {agents.length === 0 ? (
              <div className="text-sm text-slate-600">{t('noAgentsRefresh')}</div>
            ) : (
              <>
                {agents.length > 1 ? (
                  <form action={refreshAllAgents}>
                    <input name="tenant_id" type="hidden" value={selectedTenant.id} />
                    {agents.map((agent) => (
                      <input key={agent.id} name="agent_id" type="hidden" value={agent.id} />
                    ))}
                    <button className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white hover:bg-cyan-800" type="submit">
                      {t('refreshAllAgents')}
                    </button>
                  </form>
                ) : null}
                {agents.map((agent) => (
                  <form key={agent.id} action={refreshPrinters}>
                    <input name="tenant_id" type="hidden" value={selectedTenant.id} />
                    <input name="agent_id" type="hidden" value={agent.id} />
                    <button className="h-9 rounded-md border border-slate-300 px-3 text-sm font-medium text-slate-800" type="submit">
                      {t('refreshAgent', { name: agent.name })}
                    </button>
                  </form>
                ))}
              </>
            )}
          </div>
          {jobs.length === 0 ? (
            <EmptyState title={t('noJobsTitle')} message={t('noJobsMessage')} />
          ) : (
            <>
              {failedJobIds.length > 0 ? (
                <div
                  aria-live="polite"
                  className="flex flex-wrap items-center gap-3 border-b border-slate-200 bg-slate-50 px-4 py-2 text-sm"
                  role="status"
                >
                  <span className="text-slate-600">
                    {selected.size > 0
                      ? t('selectedOfFailed', { selected: selected.size, failed: failedJobIds.length })
                      : t('failedCount', { failed: failedJobIds.length })}
                  </span>
                  <button
                    className="font-medium text-cyan-700 hover:underline"
                    onClick={toggleSelectAll}
                    type="button"
                  >
                    {allSelected ? t('deselectAll') : t('selectAll')}
                  </button>
                  {selected.size > 0 ? (
                    <form action={retryDispatchJobs}>
                      <input name="tenant_id" type="hidden" value={selectedTenant.id} />
                      {Array.from(selected).map((jobId) => (
                        <input key={jobId} name="job_id" type="hidden" value={jobId} />
                      ))}
                      <button className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white hover:bg-cyan-800" type="submit">
                        {t('retrySelected', { count: selected.size })}
                      </button>
                    </form>
                  ) : null}
                </div>
              ) : null}
              <div className="divide-y divide-slate-200">
                {jobs.map((job) => {
                  const printer = printers.find((candidate) => candidate.id === job.printer_id)
                  const failed = dispatchFailed(job)
                  return (
                    <div key={job.id} className="grid gap-3 px-4 py-3 text-sm lg:grid-cols-[minmax(0,1fr)_minmax(320px,auto)]">
                      <div className="min-w-0">
                        <div className="flex items-start gap-2">
                          {failed ? (
                            <input
                              aria-label={t('selectJobAria', { filename: job.artifact.filename })}
                              checked={selected.has(job.id)}
                              onChange={() => toggleJob(job.id)}
                              type="checkbox"
                              className="mt-1 h-4 w-4 accent-cyan-700"
                            />
                          ) : null}
                          <div className="min-w-0">
                            <div className="truncate font-medium text-slate-950">{job.artifact.filename}</div>
                            <div className="truncate text-xs text-slate-700">{formatArtifactMetadata(job, tMat, formatDate)}</div>
                            <div className="mt-1 text-xs text-slate-600">{formatJobRecoveryState(job, tRec)}</div>
                          </div>
                        </div>
                        {printRunning(job) ? (
                          <LiveControlPanel tenantId={selectedTenant.id} printer={printer} />
                        ) : null}
                      </div>
                      <div className="flex flex-wrap gap-2">
                        {failed ? (
                          <ReasonForm action={retryDispatchJob} tenantId={selectedTenant.id} jobId={job.id} label={t('retryDispatch')} placeholder={t('reasonPlaceholder')} />
                        ) : null}
                        {printTerminal(job) ? (
                          <ReasonForm action={reprintJob} tenantId={selectedTenant.id} jobId={job.id} label={t('reprint')} placeholder={t('reasonPlaceholder')} />
                        ) : null}
                        <DuplicateForm tenantId={selectedTenant.id} jobId={job.id} printers={printers} />
                      </div>
                    </div>
                  )
                })}
              </div>
            </>
          )}
        </div>
      )}
    </section>
  )
}

function LiveControlPanel({ tenantId, printer }: { tenantId: string; printer: Printer | undefined }) {
  const t = useTranslations('recoveryPage')

  if (!printer) {
    return <div className="mt-1 text-xs text-slate-600">{t('printerUnavailable')}</div>
  }

  if (!liveControlsAvailable(printer)) {
    return <div className="mt-1 text-xs text-slate-600">{t('liveUnavailable')}</div>
  }

  return (
    <div className="mt-2 flex flex-wrap gap-2">
      <PrinterControlForm tenantId={tenantId} printerId={printer.id} action="pause" label={t('queuePause')} />
      <PrinterControlForm tenantId={tenantId} printerId={printer.id} action="resume" label={t('queueResume')} />
      <ConfirmForm
        action={controlPrinter}
        buttonClassName="h-8 rounded-md border border-slate-300 px-2 text-xs font-medium"
        buttonLabel={t('queueStop')}
        title={t('stopTitle')}
        message={t('stopMessage')}
        confirmLabel={t('stopConfirm')}
        tone="danger"
      >
        <input name="tenant_id" type="hidden" value={tenantId} />
        <input name="printer_id" type="hidden" value={printer.id} />
        <input name="action" type="hidden" value="stop" />
      </ConfirmForm>
      <form action={controlPrinter} className="flex gap-2">
        <input name="tenant_id" type="hidden" value={tenantId} />
        <input name="printer_id" type="hidden" value={printer.id} />
        <input name="action" type="hidden" value="set_print_speed" />
        <select name="speed_mode" className="h-8 w-24 rounded-md border border-slate-300 bg-white px-2 text-xs">
          <option value="1">{t('silent')}</option>
          <option value="2">{t('standard')}</option>
          <option value="3">{t('sport')}</option>
          <option value="4">{t('ludicrous')}</option>
        </select>
        <button className="h-8 rounded-md border border-slate-300 px-2 text-xs font-medium" type="submit">{t('queueSpeed')}</button>
      </form>
    </div>
  )
}

function PrinterControlForm({
  tenantId,
  printerId,
  action,
  label,
}: {
  tenantId: string
  printerId: string
  action: string
  label: string
}) {
  return (
    <form action={controlPrinter}>
      <input name="tenant_id" type="hidden" value={tenantId} />
      <input name="printer_id" type="hidden" value={printerId} />
      <input name="action" type="hidden" value={action} />
      <button className="h-8 rounded-md border border-slate-300 px-2 text-xs font-medium" type="submit">{label}</button>
    </form>
  )
}

function liveControlsAvailable(printer: Printer) {
  const normalized = printer.model?.trim().toUpperCase().replace(/[ _-]/g, '')
  return normalized ? liveControlModelKeys.has(normalized) : false
}

function dispatchFailed(job: Job): boolean {
  return job.status.toLowerCase() === 'failed' || job.command.status.toLowerCase() === 'failed'
}

function printRunning(job: Job): boolean {
  return job.print.status.toLowerCase() === 'running'
}

function printTerminal(job: Job): boolean {
  return ['completed', 'failed', 'cancelled'].includes(job.print.status.toLowerCase())
}

function ReasonForm({ action, tenantId, jobId, label, placeholder }: { action: (formData: FormData) => void; tenantId: string; jobId: string; label: string; placeholder: string }) {
  return (
    <form action={action} className="flex gap-2">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <input name="job_id" type="hidden" value={jobId} />
      <input className="h-8 w-28 rounded-md border border-slate-300 px-2 text-xs" name="reason" placeholder={placeholder} />
      <button className="h-8 rounded-md border border-slate-300 px-2 text-xs font-medium" type="submit">{label}</button>
    </form>
  )
}

function DuplicateForm({ tenantId, jobId, printers }: { tenantId: string; jobId: string; printers: Printer[] }) {
  const t = useTranslations('recoveryPage')
  return (
    <form action={duplicateJob} className="flex flex-wrap gap-2">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <input name="job_id" type="hidden" value={jobId} />
      <select name="printer_id" className="h-8 rounded-md border border-slate-300 bg-white px-2 text-xs">
        <option value="">{t('samePrinter')}</option>
        {printers.map((printer) => <option key={printer.id} value={printer.id}>{printer.name}</option>)}
      </select>
      <input className="h-8 w-16 rounded-md border border-slate-300 px-2 text-xs" min="1" name="plate_id" placeholder={t('platePlaceholder')} type="number" />
      <button className="h-8 rounded-md border border-slate-300 px-2 text-xs font-medium" type="submit">{t('duplicate')}</button>
    </form>
  )
}
