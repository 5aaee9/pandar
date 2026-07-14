'use client'

import { useTranslations } from 'next-intl'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type { Printer, Tenant } from './dashboard-types'
import { DispatchForm } from './dispatch-form'

export function DispatchDialog({
  open,
  onOpenChange,
  selectedTenant,
  printers,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  selectedTenant: Tenant | null
  printers: Printer[]
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
          <DialogTitle>{t('title')}</DialogTitle>
          <DialogDescription>{t('subtitle')}</DialogDescription>
        </DialogHeader>
        <DispatchForm selectedTenant={selectedTenant} printers={printers} />
      </DialogContent>
    </Dialog>
  )
}
