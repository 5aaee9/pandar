'use client'

import type { ReactNode } from 'react'
import { useTranslations } from 'next-intl'
import { PauseIcon, SquareIcon, ThermometerIcon } from 'lucide-react'

import { controlPrinter } from './actions'
import type { Printer } from './dashboard-types'
import {
  formatTemperatureValue,
  NozzleSwitchControl,
  NozzleTemperatureCard,
  presentNozzles,
} from './dashboard-printer-nozzle-temperature-controls'

export function PrinterTemperatureControls({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const nozzles = presentNozzles(printer.nozzle_temperatures ?? [])
  const temperatures = printerTemperatures(printer, t)
  if (nozzles.length === 0 && temperatures.length === 0) {
    return null
  }

  return (
    <div className="mt-4 grid gap-2 sm:grid-cols-[1fr_1fr_1fr_5rem]">
      {nozzles.length > 0 ? <NozzleTemperatureCard nozzles={nozzles} printer={printer} /> : null}
      {temperatures.map((temperature) => (
        <TemperatureCard
          key={temperature.title}
          subtitle={temperature.subtitle}
          title={temperature.title}
          tone={temperature.tone}
          value={temperature.value}
        />
      ))}
      <NozzleSwitchControl printer={printer} />
    </div>
  )
}

export function PrinterControlsPanel({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const controlsEnabled = printerControlEnabled(printer.status)

  return (
    <div className="mt-4 space-y-2">
      <div className="text-xs font-medium text-muted-foreground">{t('controlsLabel')}</div>
      <div aria-label={t('controlsLabel')} className="grid grid-cols-2 gap-2" role="group">
        <PrinterInlineControl
          action="stop"
          enabled={controlsEnabled.stop}
          icon={<SquareIcon />}
          label={t('stopPrint')}
          printer={printer}
          tone="danger"
        />
        <PrinterInlineControl
          action="pause"
          enabled={controlsEnabled.pause}
          icon={<PauseIcon />}
          label={t('pausePrint')}
          printer={printer}
          tone="warning"
        />
      </div>
    </div>
  )
}

function TemperatureCard({
  title,
  subtitle,
  value,
  tone,
}: {
  title: string
  subtitle: string | null
  value: string
  tone: string
}) {
  return (
    <div className="flex min-h-16 flex-col items-center justify-center rounded-md bg-muted/50 px-3 py-2 text-center">
      <ThermometerIcon className={`size-4 ${tone}`} />
      <div className="mt-1 text-xs font-medium text-muted-foreground">{title}</div>
      {subtitle ? <div className="text-xs font-medium text-muted-foreground">{subtitle}</div> : null}
      <div className="text-sm font-semibold text-foreground">{value}</div>
    </div>
  )
}

function PrinterInlineControl({
  printer,
  action,
  label,
  icon,
  enabled,
  tone,
}: {
  printer: Printer
  action: string
  label: string
  icon: ReactNode
  enabled: boolean
  tone: 'danger' | 'warning'
}) {
  const toneClass =
    tone === 'danger'
      ? 'enabled:bg-red-500/15 enabled:text-red-700 enabled:hover:bg-red-500/25 dark:enabled:text-red-300'
      : 'enabled:bg-yellow-500/20 enabled:text-yellow-800 enabled:hover:bg-yellow-500/30 dark:enabled:text-yellow-200'

  return (
    <form action={controlPrinter}>
      <input name="tenant_id" type="hidden" value={printer.tenant_id} />
      <input name="printer_id" type="hidden" value={printer.id} />
      <input name="action" type="hidden" value={action} />
      <button
        className={`inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-md px-2 text-sm font-semibold transition disabled:bg-muted/60 disabled:text-muted-foreground ${toneClass} [&_svg]:size-4`}
        disabled={!enabled}
        type="submit"
      >
        {icon}
        {label}
      </button>
    </form>
  )
}

function printerControlEnabled(status: string) {
  const normalized = status.toLowerCase()
  return {
    stop: ['running', 'printing', 'paused', 'pause'].includes(normalized),
    pause: ['running', 'printing'].includes(normalized),
  }
}

function printerTemperatures(printer: Printer, t: ReturnType<typeof useTranslations>) {
  return [
    printer.bed_temperature_celsius
      ? {
          title: t('bedTemperature'),
          subtitle: null,
          value: temperaturePair(printer.bed_temperature_celsius, printer.bed_target_temperature_celsius),
          tone: 'text-blue-500',
        }
      : null,
    printer.chamber_temperature_celsius
      ? {
          title: t('chamberTemperature'),
          subtitle: null,
          value: formatTemperatureValue(printer.chamber_temperature_celsius),
          tone: 'text-emerald-500',
        }
      : null,
  ].filter(
    (value): value is { title: string; subtitle: string | null; value: string; tone: string } =>
      value !== null,
  )
}

function temperaturePair(current?: string | null, target?: string | null) {
  if (hasActiveTargetTemperature(target)) {
    return `${formatTemperatureValue(current, false)} / ${formatTemperatureValue(target, false)}`
  }
  return formatTemperatureValue(current)
}

function hasActiveTargetTemperature(value?: string | null) {
  if (!value) {
    return false
  }
  const parsed = Number(value)
  return Number.isFinite(parsed) ? parsed !== 0 : value.trim() !== ''
}
