'use client'

import { useId } from 'react'
import { RotateCcwIcon, Trash2Icon } from 'lucide-react'
import { useTranslations } from 'next-intl'

import { Button } from '@/components/ui/button'

const REPRINTABLE_PRINT_STATUSES = new Set([
  'stalled',
  'completed',
  'failed',
  'cancelled',
])

export function JobRowActions({
  job,
  canDelete,
  deleteUnavailableReason,
  onDelete,
  onReprint,
}: {
  job: {
    id: string
    status: string
    artifact: { filename: string }
    print: { status: string }
  }
  canDelete: boolean
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
