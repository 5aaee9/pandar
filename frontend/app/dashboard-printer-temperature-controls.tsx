'use client'

import type { ReactNode } from 'react'
import { useTranslations } from 'next-intl'
import { ArrowLeftRightIcon, PauseIcon, SquareIcon, ThermometerIcon } from 'lucide-react'

import { controlPrinter } from './actions'
import type { Printer } from './dashboard-types'

export function PrinterTemperatureControls({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const temperatures = printerTemperatures(printer, t)
  const nozzleSwitch = nozzleSwitchControl(printer, t)
  if (temperatures.length === 0) {
    return null
  }

  return (
    <div className="mt-4 grid gap-2 sm:grid-cols-[1fr_1fr_1fr_5rem]">
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
      {nozzleSwitch}
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

function nozzleSwitchControl(printer: Printer, t: ReturnType<typeof useTranslations>) {
  const nozzles = presentNozzles(printer.nozzle_temperatures ?? [])
  if (nozzles.length < 2) {
    return null
  }

  const activeNozzle = activeNozzleLabel(printer)
  const targetNozzle = activeNozzle === 'L' ? 'R' : 'L'
  const targetExtruderId = extruderIdForNozzle(targetNozzle)
  return (
    <form action={controlPrinter} className="sm:col-start-4">
      <input name="tenant_id" type="hidden" value={printer.tenant_id} />
      <input name="printer_id" type="hidden" value={printer.id} />
      <input name="action" type="hidden" value="select_extruder" />
      <input name="extruder_id" type="hidden" value={targetExtruderId} />
      <button
        aria-label={`${t('switchNozzle')} ${nozzles.map((nozzle) => nozzle.label).join(' ')} ${t('nozzleTemperature')}`}
        className="flex min-h-16 w-full flex-col items-center justify-center rounded-md bg-muted/50 px-3 py-2 text-center transition hover:bg-muted"
        type="submit"
      >
        <ArrowLeftRightIcon className="size-4 text-yellow-500" />
        <div className="mt-1 flex items-center justify-center gap-2 text-xs font-semibold">
          {nozzles.map((nozzle) => (
            <span
              className={nozzle.label === activeNozzle ? 'text-primary' : 'text-muted-foreground'}
              key={nozzle.label}
            >
              {nozzle.label}
            </span>
          ))}
        </div>
        <div className="text-xs font-medium text-muted-foreground">{t('nozzleTemperature')}</div>
      </button>
    </form>
  )
}

function nozzleTemperature(nozzles: NonNullable<Printer['nozzle_temperatures']>) {
  const present = presentNozzles(nozzles)
  if (present.length === 0) {
    return null
  }
  if (present.length === 1) {
    const nozzle = present[0]
    return {
      label: null,
      value: formatTemperatureValue(nozzle.current_celsius, false),
    }
  }
  return {
    label: present.map((nozzle, index) => nozzle.label ?? String(index + 1)).join(' / '),
    value: present.map((nozzle) => formatTemperatureValue(nozzle.current_celsius, false)).join(' / '),
  }
}

function presentNozzles(nozzles: NonNullable<Printer['nozzle_temperatures']>) {
  return nozzles
    .filter((nozzle) => nozzle.current_celsius)
    .map((nozzle, index) => ({
      ...nozzle,
      label: nozzle.label ?? String(index + 1),
    }))
}

function activeNozzleLabel(printer: Printer) {
  const activeNozzle = normalizeToolhead(printer.active_nozzle)
  if (activeNozzle) {
    return activeNozzle
  }

  const activeTray = printer.materials?.active_tray
  if (!activeTray) {
    return null
  }

  if (activeTray.kind === 'external') {
    return normalizeToolhead(
      printer.materials?.external_spools.find(
        (spool) =>
          spool.external_id === activeTray.external_id &&
          (!activeTray.tray_id || spool.tray_id === activeTray.tray_id),
      )?.toolhead,
    )
  }

  const unit = printer.materials?.ams_units.find((ams) => ams.unit_id === activeTray.ams_id)
  const tray = unit?.trays?.find(
    (tray) =>
      tray.tray_id === activeTray.tray_id ||
      (activeTray.global_tray_id !== null &&
        activeTray.global_tray_id !== undefined &&
        tray.global_tray_id === activeTray.global_tray_id),
  )
  return normalizeToolhead(tray?.toolhead ?? unit?.toolhead)
}

function normalizeToolhead(value?: string | null) {
  const normalized = value?.trim().toUpperCase()
  return normalized === 'L' || normalized === 'R' ? normalized : null
}

function extruderIdForNozzle(nozzle: 'L' | 'R') {
  return nozzle === 'L' ? 1 : 0
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

function formatTemperatureValue(value?: string | null, suffix = true) {
  if (!value) {
    return suffix ? '-°C' : '-°'
  }
  const parsed = Number(value)
  const text = Number.isFinite(parsed) ? `${Math.round(parsed)}` : value
  return suffix ? `${text}°C` : `${text}°`
}
