'use client'

import { Axis3dIcon } from 'lucide-react'
import { useTranslations } from 'next-intl'
import { useRef, useState } from 'react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'

import { controlPrinter } from './actions'
import type { Printer } from './dashboard-types'

const AXES = [
  { id: 'x', label: 'X', feedrate: 3000 },
  { id: 'y', label: 'Y', feedrate: 3000 },
  { id: 'z', label: 'Z', feedrate: 900 },
] as const
const DISTANCES_MM = [-10, -1, 1, 10] as const

export function PrinterAxisControls({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const tCommon = useTranslations('common')
  const homeFormRef = useRef<HTMLFormElement>(null)
  const [homeConfirmOpen, setHomeConfirmOpen] = useState(false)

  return (
    <div className="mt-2">
      <Dialog>
        <DialogTrigger
          className="inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-md bg-primary/10 px-2 text-sm font-semibold text-primary transition hover:bg-primary/15"
          type="button"
        >
          <Axis3dIcon className="size-4" />
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
                  {DISTANCES_MM.map((distance) => {
                    const signed = distance > 0 ? `+${distance}` : String(distance)
                    return (
                      <form action={controlPrinter} key={distance}>
                        <input name="tenant_id" type="hidden" value={printer.tenant_id} />
                        <input name="printer_id" type="hidden" value={printer.id} />
                        <input name="action" type="hidden" value="move_axes" />
                        <input name="axis" type="hidden" value={axis.id} />
                        <input name="delta_mm" type="hidden" value={distance} />
                        <input name="feedrate_mm_per_min" type="hidden" value={axis.feedrate} />
                        <Button
                          aria-label={t('moveAxisBy', { axis: axis.label, distance: signed })}
                          className="w-full"
                          size="sm"
                          type="submit"
                          variant="outline"
                        >
                          {signed}
                        </Button>
                      </form>
                    )
                  })}
                </div>
              </div>
            ))}
            <form action={controlPrinter} ref={homeFormRef}>
              <input name="tenant_id" type="hidden" value={printer.tenant_id} />
              <input name="printer_id" type="hidden" value={printer.id} />
              <input name="action" type="hidden" value="home" />
              <button
                className="inline-flex h-8 w-full items-center justify-center rounded-md bg-muted px-2 text-sm font-semibold text-foreground hover:bg-muted/80"
                onClick={() => setHomeConfirmOpen(true)}
                type="button"
              >
                {t('homeAxes')}
              </button>
            </form>
          </div>
        </DialogContent>
      </Dialog>
      <Dialog open={homeConfirmOpen} onOpenChange={setHomeConfirmOpen}>
        <DialogContent showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>{t('homeAxesTitle')}</DialogTitle>
            <DialogDescription>{t('homeAxesMessage')}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button onClick={() => setHomeConfirmOpen(false)} type="button" variant="outline">
              {tCommon('cancel')}
            </Button>
            <Button
              onClick={() => {
                setHomeConfirmOpen(false)
                homeFormRef.current?.requestSubmit()
              }}
              type="button"
            >
              {t('homeAxesConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
