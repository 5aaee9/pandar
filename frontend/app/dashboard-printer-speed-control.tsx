'use client'

import { useTranslations } from 'next-intl'
import { GaugeIcon, Loader2Icon } from 'lucide-react'

import { Button } from '@/components/ui/button'

import type { Printer } from './dashboard-types'
import {
  PrinterControlFields,
  usePrinterControl,
} from './printer-controls'
import type { PrinterSpeedMode } from './printer-controls'

const speedModes: Array<{
  mode: PrinterSpeedMode
  label: 'printSpeedSilent' | 'printSpeedStandard' | 'printSpeedSport' | 'printSpeedLudicrous'
}> = [
  { mode: 1, label: 'printSpeedSilent' },
  { mode: 2, label: 'printSpeedStandard' },
  { mode: 3, label: 'printSpeedSport' },
  { mode: 4, label: 'printSpeedLudicrous' },
]

export function PrinterSpeedControl({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const enabled = printSpeedControlEnabled(printer)
  const currentMode = speedModes.find(({ mode }) => mode === printer.print?.speed_level)

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2 text-xs font-medium text-muted-foreground">
        <span className="flex items-center gap-1">
          <GaugeIcon className="size-3.5" />
          {t('printSpeed')}
        </span>
        {currentMode ? (
          <span className="font-semibold text-foreground">{t(currentMode.label)}</span>
        ) : null}
      </div>
      <div aria-label={t('printSpeed')} className="grid grid-cols-4 gap-1.5" role="group">
        {speedModes.map(({ mode, label }) => (
          <SpeedModeButton
            enabled={enabled}
            current={mode === currentMode?.mode}
            key={mode}
            label={t(label)}
            mode={mode}
            printer={printer}
          />
        ))}
      </div>
    </div>
  )
}

function SpeedModeButton({
  printer,
  mode,
  label,
  enabled,
  current,
}: {
  printer: Printer
  mode: PrinterSpeedMode
  label: string
  enabled: boolean
  current: boolean
}) {
  const { formAction, pending } = usePrinterControl()

  return (
    <form action={formAction}>
      <PrinterControlFields
        intent={{ action: 'set_print_speed', speedMode: mode }}
        printer={printer}
      />
      <Button
        aria-pressed={current}
        className="w-full px-1"
        disabled={!enabled || pending}
        size="sm"
        type="submit"
        variant={current ? 'secondary' : 'outline'}
      >
        {pending ? <Loader2Icon className="animate-spin" /> : null}
        {label}
      </Button>
    </form>
  )
}

function printSpeedControlEnabled(printer: Printer) {
  const coarseStatus = printer.status.toLowerCase()
  const printState = printer.print?.gcode_state?.toLowerCase() ?? coarseStatus
  return (
    !['idle', 'offline', 'failed'].includes(coarseStatus)
    && ['running', 'printing', 'paused', 'pause'].includes(printState)
  )
}
