'use client'

import { useId, type ReactNode } from 'react'
import {
  BotIcon,
  ChevronDownIcon,
  ClockIcon,
  LayersIcon,
  RotateCcwIcon,
  PrinterIcon,
  Trash2Icon,
} from 'lucide-react'
import { useFormatter, useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import { Button } from '@/components/ui/button'
import { reprintJob } from './job-actions'
import { formatBytes } from './dashboard-format'
import {
  formatArtifactMetadata,
  formatJobMaterial,
  formatJobRecoveryState,
} from './dashboard-runtime-helpers'
import type { Job } from './dashboard-types'
import { StatusBadge } from './dashboard-ui'
import { formatLayers, formatRemaining } from './job-format'

const REPRINTABLE_PRINT_STATUSES = new Set([
  'stalled',
  'completed',
  'failed',
  'cancelled',
])

export function JobRow({
  job,
  tenantId,
  printerName,
  agentName,
  canDelete,
  deleteUnavailableReason,
  onDelete,
}: {
  job: Job
  tenantId: string
  printerName?: string
  agentName?: string
  canDelete: boolean
  deleteUnavailableReason: string
  onDelete: () => void
}) {
  const t = useTranslations('inventory')
  const tMat = useTranslations('material')
  const tRec = useTranslations('recovery.state')
  const tJf = useTranslations('jobFormat')
  const tMonitor = useTranslations('printMonitor')
  const format = useFormatter()
  const titleId = useId()
  const num = (n: number) => format.number(n)
  const updated = job.print.updated_at ?? job.updated_at
  const deleteHelpId = useId()
  const currentLayer = job.print.current_layer ?? job.print.last_layer ?? null
  const progressValue =
    job.print.progress_percent ?? job.print.last_progress_percent ?? null
  const progress =
    progressValue === null ? null : Math.min(100, Math.max(0, progressValue))
  const hasLayers = currentLayer !== null
  const hasRemaining = job.print.remaining_time_minutes !== null
  const canReprint = REPRINTABLE_PRINT_STATUSES.has(
    job.print.status.toLowerCase(),
  )

  return (
    <li className="px-4 py-4 transition-colors hover:bg-muted/30">
      <article aria-labelledby={titleId}>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div className="min-w-0">
            <h3
              className="truncate text-sm font-semibold text-foreground"
              id={titleId}
            >
              {job.artifact.filename}
            </h3>
            <p className="mt-0.5 truncate text-xs text-muted-foreground">
              {t('updatedPrefix')}{' '}
              <time dateTime={updated}>
                <FormattedDate value={updated} />
              </time>
            </p>
          </div>
          <div className="flex shrink-0 flex-wrap items-center gap-2">
            <StatusPill label={t('dispatch')} value={job.status} />
            <StatusPill label={t('print')} value={job.print.status} />
            {canReprint ? (
              <form action={reprintJob}>
                <input name="tenant_id" type="hidden" value={tenantId} />
                <input name="return_to" type="hidden" value="jobs" />
                <input name="job_id" type="hidden" value={job.id} />
                <Button
                  aria-label={t('reprintJobAriaLabel', {
                    filename: job.artifact.filename,
                  })}
                  size="sm"
                  type="submit"
                  variant="outline"
                >
                  <RotateCcwIcon aria-hidden="true" />
                  {t('reprintJob')}
                </Button>
              </form>
            ) : null}
            <Button
              aria-describedby={canDelete ? undefined : deleteHelpId}
              aria-haspopup="dialog"
              aria-label={t('deleteJobAriaLabel', {
                filename: job.artifact.filename,
              })}
              disabled={!canDelete}
              onClick={onDelete}
              size="sm"
              type="button"
              variant="destructive"
            >
              <Trash2Icon aria-hidden="true" />
              {t('deleteJob')}
            </Button>
            {!canDelete ? (
              <span className="sr-only" id={deleteHelpId}>
                {deleteUnavailableReason}
              </span>
            ) : null}
          </div>
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <span className="inline-flex max-w-full items-center gap-1 rounded-md bg-muted px-2 py-1 text-xs font-medium text-muted-foreground">
            <PrinterIcon aria-hidden="true" className="size-3.5 shrink-0" />
            <span className="truncate">
              {printerName ?? t('unknownPrinter')}
            </span>
          </span>
          <span className="inline-flex max-w-full items-center gap-1 rounded-md bg-muted px-2 py-1 text-xs font-medium text-muted-foreground">
            <BotIcon aria-hidden="true" className="size-3.5 shrink-0" />
            <span className="truncate">{agentName ?? t('unknownAgent')}</span>
          </span>
        </div>

        <div className="mt-3 rounded-md bg-muted/60 p-3">
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <p className="text-sm font-medium text-foreground">
              {formatJobRecoveryState(job, tRec)}
            </p>
            {progress !== null || hasLayers || hasRemaining ? (
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                {progress !== null ? (
                  <span className="font-semibold text-foreground">
                    {format.number(progress)}%
                  </span>
                ) : null}
                {hasLayers ? (
                  <span className="inline-flex items-center gap-1">
                    <LayersIcon aria-hidden="true" className="size-3.5" />
                    {formatLayers(job, tJf)}
                  </span>
                ) : null}
                {hasRemaining ? (
                  <span className="inline-flex items-center gap-1">
                    <ClockIcon aria-hidden="true" className="size-3.5" />
                    {formatRemaining(job.print.remaining_time_minutes, tJf)}
                  </span>
                ) : null}
              </div>
            ) : null}
          </div>
          {progress !== null ? (
            <div
              aria-label={tMonitor('progress')}
              aria-valuemax={100}
              aria-valuemin={0}
              aria-valuenow={progress}
              className="mt-2 h-1.5 overflow-hidden rounded-full bg-background"
              role="progressbar"
            >
              <div
                className="h-full rounded-full bg-primary"
                style={{ width: progress.toString() + '%' }}
              />
            </div>
          ) : null}
        </div>

        {job.error || job.print.error ? (
          <div className="mt-3 space-y-1 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {job.error ? <p className="break-words">{job.error}</p> : null}
            {job.print.error ? (
              <p className="break-words">{job.print.error}</p>
            ) : null}
          </div>
        ) : null}

        <details className="group mt-3 border-t border-border pt-2">
          <summary className="flex w-fit cursor-pointer list-none items-center gap-1 text-xs font-medium text-muted-foreground hover:text-foreground [&::-webkit-details-marker]:hidden">
            <ChevronDownIcon
              aria-hidden="true"
              className="size-3.5 transition-transform group-open:rotate-180 motion-reduce:transition-none"
            />
            {t('details')}
          </summary>
          <dl className="mt-3 grid gap-x-6 gap-y-3 sm:grid-cols-2">
            <DetailItem label={t('projectLabel')} wide>
              {formatArtifactMetadata(job, tMat)}
            </DetailItem>
            <DetailItem label={t('artifactLabel')}>
              {job.artifact.content_type} ·{' '}
              {formatBytes(job.artifact.size_bytes, num)}
            </DetailItem>
            <DetailItem label={t('materialLabel')}>
              {formatJobMaterial(job, tMat)}
            </DetailItem>
            <DetailItem label={t('jobLabel')}>
              <span className="break-all font-mono text-xs">{job.id}</span>
            </DetailItem>
            {job.print.active_file ? (
              <DetailItem label={t('fileLabel')}>
                {job.print.active_file}
              </DetailItem>
            ) : null}
            {job.print.printer_state ? (
              <DetailItem label={t('stateLabel')}>
                {job.print.printer_state}
              </DetailItem>
            ) : null}
            <DetailItem label={t('createdLabel')}>
              <time dateTime={job.created_at}>
                <FormattedDate value={job.created_at} />
              </time>
            </DetailItem>
            <DetailItem label={t('startedLabel')}>
              {job.print.started_at ? (
                <time dateTime={job.print.started_at}>
                  <FormattedDate value={job.print.started_at} />
                </time>
              ) : (
                '—'
              )}
            </DetailItem>
            <DetailItem label={t('finishedLabel')}>
              {job.print.finished_at ? (
                <time dateTime={job.print.finished_at}>
                  <FormattedDate value={job.print.finished_at} />
                </time>
              ) : (
                '—'
              )}
            </DetailItem>
          </dl>
        </details>
      </article>
    </li>
  )
}

function StatusPill({ label, value }: { label: string; value: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      <span className="text-xs text-muted-foreground">{label}</span>
      <StatusBadge value={value} />
    </span>
  )
}

function DetailItem({
  label,
  wide = false,
  children,
}: {
  label: string
  wide?: boolean
  children: ReactNode
}) {
  return (
    <div className={wide ? 'sm:col-span-2' : undefined}>
      <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
      <dd className="mt-0.5 break-words text-sm text-foreground">{children}</dd>
    </div>
  )
}
