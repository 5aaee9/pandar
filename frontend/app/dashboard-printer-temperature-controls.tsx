'use client'

import type { ReactNode } from 'react'
import { useTranslations } from 'next-intl'
import { PauseIcon, SquareIcon, ThermometerIcon } from 'lucide-react'

import { controlPrinter } from './actions'
import type { Printer } from './dashboard-types'

export function PrinterTemperatureControls({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const temperatures = printerTemperatures(printer, t)
  if (temperatures.length === 0) {
    return null
  }
  const controlsEnabled = printerControlEnabled(printer.status)

  return (
    <div className="mt-4 grid gap-2 sm:grid-cols-[1fr_1fr_1fr_auto]">
      {temperatures.map((temperature) => (
        <div
          className="flex min-h-16 flex-col items-center justify-center rounded-md bg-muted/50 px-3 py-2 text-center"
          key={temperature.title}
        >
          <ThermometerIcon className={`size-4 ${temperature.tone}`} />
          <div className="mt-1 text-xs font-medium text-muted-foreground">{temperature.title}</div>
          {temperature.subtitle ? (
            <div className="text-xs font-medium text-muted-foreground">{temperature.subtitle}</div>
          ) : null}
          <div className="text-sm font-semibold text-foreground">{temperature.value}</div>
        </div>
      ))}
      <div className="grid grid-cols-2 gap-2 sm:w-24 sm:grid-cols-1">
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
  const nozzle = nozzleTemperature(printer.nozzle_temperatures ?? [])
  return [
    nozzle
      ? {
          title: t('nozzleTemperature'),
          subtitle: nozzle.label,
          value: nozzle.value,
          tone: 'text-orange-500',
        }
      : null,
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

function nozzleTemperature(nozzles: NonNullable<Printer['nozzle_temperatures']>) {
  const present = nozzles.filter((nozzle) => nozzle.current_celsius || nozzle.target_celsius)
  if (present.length === 0) {
    return null
  }
  if (present.length === 1) {
    const nozzle = present[0]
    return {
      label: nozzle.label ?? 'Nozzle',
      value: temperaturePair(nozzle.current_celsius, nozzle.target_celsius),
    }
  }
  return {
    label: present.map((nozzle, index) => nozzle.label ?? String(index + 1)).join(' / '),
    value: temperaturePair(present[0].current_celsius, present[0].target_celsius),
  }
}

function temperaturePair(current?: string | null, target?: string | null) {
  if (target) {
    return `${formatTemperatureValue(current, false)} / ${formatTemperatureValue(target, false)}`
  }
  return formatTemperatureValue(current)
}

function formatTemperatureValue(value?: string | null, suffix = true) {
  if (!value) {
    return suffix ? '-°C' : '-°'
  }
  const parsed = Number(value)
  const text = Number.isFinite(parsed) ? `${Math.round(parsed)}` : value
  return suffix ? `${text}°C` : `${text}°`
}
