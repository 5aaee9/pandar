'use client'

import { Axis3dIcon, Loader2Icon } from 'lucide-react'
import { useTranslations } from 'next-intl'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'

import { ConfirmForm } from './confirm-dialog'
import type { Printer } from './dashboard-types'
import { usePrinterControl } from './use-printer-control'

const AXES = [
  { id: 'x', label: 'X', feedrate: 3000 },
  { id: 'y', label: 'Y', feedrate: 3000 },
  { id: 'z', label: 'Z', feedrate: 900 },
] as const
const DISTANCES_MM = [-10, -1, 1, 10] as const

export function PrinterAxisControls({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const homeControl = usePrinterControl()

  return (
    <div className="mt-2">
      <Dialog>
        <DialogTrigger
          render={
            <Button
              className="w-full rounded-md font-semibold"
              type="button"
              variant="soft"
            />
          }
        >
          <Axis3dIcon />
          {t('moveAxes')}
        </DialogTrigger>
        <DialogContent closeLabel={t('closeMoveAxes')}>
          <DialogHeader>
            <DialogTitle>{t('moveAxesTitle')}</DialogTitle>
            <DialogDescription>{t('moveAxesDescription')}</DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            {AXES.map((axis) => (
              <div className="grid grid-cols-[3rem_1fr] items-center gap-2" key={axis.id}>
                <span className="text-sm font-semibold">{t('axisLabel', { axis: axis.label })}</span>
                <div className="grid grid-cols-4 gap-1.5">
                  {DISTANCES_MM.map((distance) => (
                    <AxisMoveButton
                      axis={axis}
                      distance={distance}
                      key={distance}
                      printer={printer}
                    />
                  ))}
                </div>
              </div>
            ))}
            <ConfirmForm
              action={homeControl.formAction}
              buttonAriaLabel={t('homeAxes')}
              buttonClassName="w-full rounded-md bg-muted font-semibold text-foreground hover:bg-muted/80 dark:hover:bg-muted/80 disabled:text-muted-foreground"
              buttonLabel={t('homeAxes')}
              buttonVariant="ghost"
              confirmLabel={t('homeAxesConfirm')}
              message={t('homeAxesMessage')}
              pending={homeControl.pending}
              title={t('homeAxesTitle')}
              tone="default"
            >
              <input name="tenant_id" type="hidden" value={printer.tenant_id} />
              <input name="printer_id" type="hidden" value={printer.id} />
              <input name="action" type="hidden" value="home" />
            </ConfirmForm>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function AxisMoveButton({
  printer,
  axis,
  distance,
}: {
  printer: Printer
  axis: (typeof AXES)[number]
  distance: number
}) {
  const t = useTranslations('inventory')
  const { formAction, pending } = usePrinterControl()
  const signed = distance > 0 ? `+${distance}` : String(distance)

  return (
    <form action={formAction}>
      <input name="tenant_id" type="hidden" value={printer.tenant_id} />
      <input name="printer_id" type="hidden" value={printer.id} />
      <input name="action" type="hidden" value="move_axes" />
      <input name="axis" type="hidden" value={axis.id} />
      <input name="delta_mm" type="hidden" value={distance} />
      <input name="feedrate_mm_per_min" type="hidden" value={axis.feedrate} />
      <Button
        aria-label={t('moveAxisBy', { axis: axis.label, distance: signed })}
        className="w-full"
        disabled={pending}
        size="sm"
        type="submit"
        variant="outline"
      >
        {pending ? <Loader2Icon className="animate-spin" /> : null}
        {signed}
      </Button>
    </form>
  )
}
