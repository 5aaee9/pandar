'use client'

import type { ReactNode } from 'react'
import { Loader2Icon } from 'lucide-react'

import { Button } from '@/components/ui/button'

import type { MaterialSlot } from './dashboard-printer-materials'
import type { Printer } from './dashboard-types'
import { PrinterControlFields, usePrinterControl } from './printer-controls'

export function SlotOperationForm({
  printer,
  slot,
  action,
  label,
  icon,
}: {
  printer: Printer
  slot: MaterialSlot
  action: 'ams_load_filament' | 'ams_unload_filament' | 'ams_reread_rfid'
  label: string
  icon: ReactNode
}) {
  const { formAction, pending } = usePrinterControl()

  return (
    <form action={formAction}>
      <PrinterControlFields
        printer={printer}
        intent={{
          action,
          amsId: slot.amsId,
          slotId: slot.slotId,
          globalTrayId: slot.globalTrayId,
          externalId: slot.externalId,
          extruderId: slotExtruderId(slot, printer),
        }}
      />
      <Button
        className="h-auto w-full justify-start gap-2 rounded-sm px-2 py-1.5 font-normal"
        disabled={pending}
        type="submit"
        variant="ghost"
      >
        {pending ? <Loader2Icon className="animate-spin" /> : icon}
        {label}
      </Button>
    </form>
  )
}

function slotExtruderId(slot: MaterialSlot, printer: Printer) {
  const toolhead = slot.toolhead?.trim().toUpperCase()
  if (toolhead === 'R') return 0
  if (toolhead === 'L') return 1
  if (slot.externalId === '255') return 0
  if (slot.externalId === '254') return 1
  const model = printer.model?.toLowerCase() ?? ''
  return model.includes('x2d') || model.includes('h2d') ? 0 : null
}
