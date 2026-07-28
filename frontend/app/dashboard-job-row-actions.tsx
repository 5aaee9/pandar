'use client'

import { useId } from 'react'
import { RefreshCwIcon, RotateCcwIcon, Trash2Icon } from 'lucide-react'
import { useTranslations } from 'next-intl'

import { Button } from '@/components/ui/button'
import { retryDispatchJob } from './job-actions'

const REPRINTABLE_PRINT_STATUSES = new Set([
  'stalled',
  'completed',
  'failed',
  'cancelled',
])

export function JobRowActions({
  job,
  canDelete,
  canRetryDispatch,
  deleteUnavailableReason,
  onDelete,
  onReprint,
}: {
  job: {
    id: string
    status: string
    artifact: { filename: string; tenant_id: string }
    print: { status: string }
  }
  canDelete: boolean
  canRetryDispatch: boolean
  deleteUnavailableReason: string
  onDelete: () => void
  onReprint: () => void
}) {
  const t = useTranslations('inventory')
  const deleteHelpId = useId()
  const canReprint = REPRINTABLE_PRINT_STATUSES.has(
    job.print.status.toLowerCase(),
  )

  return (
    <>
      {canRetryDispatch ? (
        <form action={retryDispatchJob}>
          <input name="tenant_id" type="hidden" value={job.artifact.tenant_id} />
          <input name="job_id" type="hidden" value={job.id} />
          <input name="return_to" type="hidden" value="jobs" />
          <Button
            aria-label={t('retryDispatchJobAriaLabel', {
              filename: job.artifact.filename,
            })}
            size="sm"
            type="submit"
            variant="outline"
          >
            <RefreshCwIcon aria-hidden="true" />
            {t('retryDispatchJob')}
          </Button>
        </form>
      ) : null}
      {canReprint ? (
        <Button
          aria-haspopup="dialog"
          aria-label={t('reprintJobAriaLabel', {
            filename: job.artifact.filename,
          })}
          onClick={onReprint}
          size="sm"
          type="button"
          variant="outline"
        >
          <RotateCcwIcon aria-hidden="true" />
          {t('reprintJob')}
        </Button>
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
    </>
  )
}
