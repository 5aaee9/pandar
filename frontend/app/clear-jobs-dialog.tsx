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

export function ClearJobsDialog({
  open,
  clearing,
  clearableCount,
  error,
  onOpenChange,
  onConfirm,
}: {
  open: boolean
  clearing: boolean
  clearableCount: number
  error: 'generic' | 'permission' | null
  onOpenChange: (open: boolean) => void
  onConfirm: () => void
}) {
  const t = useTranslations('inventory')

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && clearing) return
        onOpenChange(nextOpen)
      }}
    >
      <DialogContent closeLabel={t('cancel')} showCloseButton={!clearing}>
        <DialogHeader>
          <DialogTitle>{t('clearJobsTitle')}</DialogTitle>
          <DialogDescription>
            {t('clearJobsDescription', { count: clearableCount })}
          </DialogDescription>
          {error ? (
            <p className="text-sm text-destructive" role="alert">
              {error === 'permission'
                ? t('clearJobsFailedPermission')
                : t('clearJobsFailed')}
            </p>
          ) : null}
        </DialogHeader>
        <DialogFooter>
          <Button
            disabled={clearing}
            onClick={() => onOpenChange(false)}
            type="button"
            variant="outline"
          >
            {t('cancel')}
          </Button>
          <Button
            disabled={clearing}
            onClick={onConfirm}
            type="button"
            variant="destructive"
          >
            {clearing ? t('clearingJobs') : t('confirmClearJobs')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
