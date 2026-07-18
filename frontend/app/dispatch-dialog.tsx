'use client'

import { useTranslations } from 'next-intl'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type { Job, Printer, Tenant } from './dashboard-types'
import { DispatchForm } from './dispatch-form'

export function DispatchDialog({
  open,
  onOpenChange,
  selectedTenant,
  printers,
  sourceJob,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  selectedTenant: Tenant | null
  printers: Printer[]
  sourceJob: Job | null
}) {
  const t = useTranslations('dispatch')
  const tInventory = useTranslations('inventory')

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[calc(100vh-2rem)] overflow-y-auto sm:max-w-3xl"
        closeLabel={tInventory('closeDialog')}
      >
        <DialogHeader>
          <DialogTitle>{sourceJob ? t('reprintTitle') : t('title')}</DialogTitle>
          <DialogDescription>
            {sourceJob ? t('reprintSubtitle') : t('subtitle')}
          </DialogDescription>
        </DialogHeader>
        {open ? (
          <DispatchForm
            key={`${selectedTenant?.id ?? 'no-tenant'}:${sourceJob?.id ?? 'new'}`}
            selectedTenant={selectedTenant}
            printers={printers}
            sourceJob={sourceJob}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  )
}
