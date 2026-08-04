'use client'

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
import type { Job } from './dashboard-types'

export function DeleteJobDialog({
  target,
  deleting,
  error,
  onClose,
  onConfirm,
}: {
  target: Job | null
  deleting: boolean
  error: boolean
  onClose: () => void
  onConfirm: () => void
}) {
  const t = useTranslations('inventory')

  return (
    <Dialog
      open={target !== null}
      onOpenChange={(open) => {
        if (!open && !deleting) {
          onClose()
        }
      }}
    >
      <DialogContent closeLabel={t('closeDialog')} showCloseButton={!deleting}>
        <DialogHeader>
          <DialogTitle>{t('deleteJobTitle')}</DialogTitle>
          <DialogDescription>
            {target
              ? t('deleteJobDescription', {
                  filename: target.artifact.filename,
                })
              : null}
          </DialogDescription>
          {error ? (
            <p className="text-sm text-destructive" role="alert">
              {t('deleteJobFailed')}
            </p>
          ) : null}
        </DialogHeader>
        <DialogFooter>
          <Button
            disabled={deleting}
            onClick={onClose}
            type="button"
            variant="outline"
          >
            {t('cancel')}
          </Button>
          <Button
            disabled={deleting}
            onClick={onConfirm}
            type="button"
            variant="destructive"
          >
            {deleting ? t('deletingJob') : t('confirmDeleteJob')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
