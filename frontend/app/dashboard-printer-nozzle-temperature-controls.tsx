

import { useTranslations } from 'next-intl'
import { ArrowLeftRightIcon, CheckCircle2Icon, ThermometerIcon } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover'

import { controlPrinter } from './actions'
import type { Printer } from './dashboard-types'

const NOZZLE_TEMPERATURE_PRESETS = [0, 120, 220, 260] as const

export function NozzleTemperatureCard({
  printer,
  nozzles,
}: {
  printer: Printer
  nozzles: ReturnType<typeof presentNozzles>
}) {
  const t = useTranslations('inventory')
  const nozzle = nozzleTemperature(nozzles)
  if (!nozzle) {
    return null
  }

  return (
    <Popover>
      <PopoverTrigger
        aria-label={nozzles.length > 1 ? t('setNozzleTemperatures') : t('setNozzleTemperature')}
        className="flex min-h-16 flex-col items-center justify-center rounded-md bg-muted/50 px-3 py-2 text-center transition-colors duration-150 ease-out hover:bg-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
        type="button"
      >
        <ThermometerIcon className="size-4 text-muted-foreground" />
        <div className="mt-1 text-xs font-medium text-muted-foreground">{t('nozzleTemperature')}</div>
        {nozzle.label ? <div className="text-xs font-medium text-muted-foreground">{nozzle.label}</div> : null}
        <div className="text-sm font-semibold text-foreground">{nozzle.value}</div>
      </PopoverTrigger>
      <PopoverContent className={nozzles.length > 1 ? 'w-[min(24rem,calc(100vw-2rem))]' : 'w-72'} sideOffset={8}>
        <PopoverTitle className="text-center text-base font-semibold">
          {nozzles.length > 1 ? t('setNozzleTemperaturesTitle') : t('setNozzleTemperatureTitle')}
        </PopoverTitle>
        <NozzleTemperatureMenu nozzles={nozzles} printer={printer} />
      </PopoverContent>
    </Popover>
  )
}

export function NozzleSwitchControl({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const nozzles = presentNozzles(printer.nozzle_temperatures ?? [])
  if (nozzles.length < 2) {
    return null
  }

  const activeNozzle = activeNozzleLabel(printer)
  const targetNozzle = activeNozzle === 'L' ? 'R' : 'L'
  return (
    <form action={controlPrinter} className="h-full lg:col-start-4">
      <input name="tenant_id" type="hidden" value={printer.tenant_id} />
      <input name="printer_id" type="hidden" value={printer.id} />
      <input name="action" type="hidden" value="select_extruder" />
      <input name="extruder_id" type="hidden" value={extruderIdForNozzle(targetNozzle)} />
      <button
        aria-label={t('switchNozzleTo', { nozzle: targetNozzle })}
        className="flex h-full min-h-16 w-full flex-col items-center justify-center rounded-md bg-muted/50 px-3 py-2 text-center transition-colors duration-150 ease-out hover:bg-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
        type="submit"
      >
        <ArrowLeftRightIcon className="size-4 text-muted-foreground" />
        <div className="mt-1 flex items-start justify-center gap-2 text-xs font-semibold">
          {nozzles.map((nozzle) => (
            <span
              className={`flex flex-col items-center ${nozzle.label === activeNozzle ? 'text-primary' : 'text-muted-foreground'}`}
              key={nozzle.label}
            >
              <span className="flex items-center gap-1">
                <span>{nozzle.label}</span>
                {nozzle.label === activeNozzle ? (
                  <>
                    <CheckCircle2Icon aria-hidden="true" className="size-3 text-success" />
                    <span className="sr-only">{t('activeNozzle')}</span>
                  </>
                ) : null}
              </span>
              <NozzleInfo nozzle={nozzle} />
            </span>
          ))}
        </div>
        <div className="text-xs font-medium text-muted-foreground">{t('nozzleTemperature')}</div>
      </button>
    </form>
  )
}

function NozzleInfo({ nozzle }: { nozzle: ReturnType<typeof presentNozzles>[number] }) {
  const details = [nozzle.diameter_mm ? `${nozzle.diameter_mm} mm` : null, nozzle.nozzle_type].filter(Boolean)
  if (details.length === 0) {
    return null
  }
  return <span className="mt-0.5 text-xs font-medium leading-tight text-muted-foreground">{details.join(' ')}</span>
}

function NozzleTemperatureMenu({
  printer,
  nozzles,
}: {
  printer: Printer
  nozzles: ReturnType<typeof presentNozzles>
}) {
  const activeNozzle = activeNozzleLabel(printer)
  if (nozzles.length === 1) {
    return <NozzleTemperaturePanel nozzle={nozzles[0]} printer={printer} />
  }

  return (
    <div className="grid gap-2 sm:grid-cols-2">
      {nozzles.map((nozzle) => (
        <NozzleTemperaturePanel
          active={nozzle.label === activeNozzle}
          key={nozzle.label}
          nozzle={nozzle}
          printer={printer}
        />
      ))}
    </div>
  )
}

function NozzleTemperaturePanel({
  printer,
  nozzle,
  active = false,
}: {
  printer: Printer
  nozzle: ReturnType<typeof presentNozzles>[number]
  active?: boolean
}) {
  const t = useTranslations('inventory')
  const extruderId = nozzle.label === 'L' || nozzle.label === 'R' ? extruderIdForNozzle(nozzle.label) : null
  const title = nozzle.label === 'L' ? t('leftNozzleTemp') : nozzle.label === 'R' ? t('rightNozzleTemp') : null

  return (
    <div className={`space-y-2 rounded-md border p-2 ${active ? 'border-primary' : 'border-border'}`}>
      {title ? (
        <span className="flex items-center justify-between text-sm font-semibold">
          {title}
          {active ? (
            <span className="inline-flex items-center gap-1 text-xs font-medium text-success">
              <CheckCircle2Icon aria-hidden="true" className="size-3" />
              {t('activeNozzle')}
            </span>
          ) : null}
        </span>
      ) : null}
      <TemperatureReading current={nozzle.current_celsius} target={nozzle.target_celsius} />
      <div className="grid grid-cols-2 gap-1.5">
        {NOZZLE_TEMPERATURE_PRESETS.map((temperature) => (
          <TemperaturePresetButton
            extruderId={extruderId}
            key={temperature}
            printer={printer}
            temperature={temperature}
          />
        ))}
      </div>
      <CustomTemperatureForm extruderId={extruderId} printer={printer} />
    </div>
  )
}

export function TemperatureReading({
  current,
  target,
}: {
  current?: string | null
  target?: string | null
}) {
  const t = useTranslations('inventory')
  return (
    <div className="flex flex-wrap items-center justify-center gap-x-3 gap-y-1 text-xs font-medium text-muted-foreground">
      <span>{`${t('currentTemperature')} ${formatTemperatureValue(current)}`}</span>
      {hasActiveTargetTemperature(target) ? (
        <span>{`${t('targetTemperature')} ${formatTemperatureValue(target)}`}</span>
      ) : null}
    </div>
  )
}

function TemperaturePresetButton({
  printer,
  temperature,
  extruderId,
}: {
  printer: Printer
  temperature: number
  extruderId: number | null
}) {
  const t = useTranslations('inventory')
  return (
    <form action={controlPrinter}>
      <PrinterTemperatureHiddenFields
        extruderId={extruderId}
        printer={printer}
        temperature={temperature}
      />
      <Button className="w-full" size="sm" type="submit" variant="outline">
        {temperature === 0 ? t('temperatureOff') : `${temperature} C`}
      </Button>
    </form>
  )
}

function CustomTemperatureForm({
  printer,
  extruderId,
}: {
  printer: Printer
  extruderId: number | null
}) {
  const t = useTranslations('inventory')
  return (
    <form action={controlPrinter} className="flex gap-1.5">
      <PrinterTemperatureHiddenFields extruderId={extruderId} printer={printer} />
      <Input
        aria-label={t('customTemperature')}
        inputMode="numeric"
        min="0"
        name="temperature_celsius"
        placeholder={t('customTemperature')}
        type="number"
      />
      <Button size="sm" type="submit" variant="secondary">
        {t('setTemperature')}
      </Button>
    </form>
  )
}

function PrinterTemperatureHiddenFields({
  printer,
  temperature,
  extruderId,
}: {
  printer: Printer
  temperature?: number
  extruderId: number | null
}) {
  return (
    <>
      <input name="tenant_id" type="hidden" value={printer.tenant_id} />
      <input name="printer_id" type="hidden" value={printer.id} />
      <input name="action" type="hidden" value="set_hotend_temperature" />
      {temperature !== undefined ? (
        <input name="temperature_celsius" type="hidden" value={temperature} />
      ) : null}
      {extruderId !== null ? <input name="extruder_id" type="hidden" value={extruderId} /> : null}
    </>
  )
}

function nozzleTemperature(nozzles: ReturnType<typeof presentNozzles>) {
  if (nozzles.length === 0) {
    return null
  }
  if (nozzles.length === 1) {
    return {
      label: null,
      value: formatTemperatureValue(nozzles[0].current_celsius, false),
    }
  }
  return {
    label: nozzles.map((nozzle, index) => nozzle.label ?? String(index + 1)).join(' / '),
    value: nozzles.map((nozzle) => formatTemperatureValue(nozzle.current_celsius, false)).join(' / '),
  }
}

export function presentNozzles(nozzles: NonNullable<Printer['nozzle_temperatures']>) {
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

export function formatTemperatureValue(value?: string | null, suffix = true) {
  if (!value) {
    return suffix ? '-°C' : '-°'
  }
  const parsed = Number(value)
  const text = Number.isFinite(parsed) ? `${Math.round(parsed)}` : value
  return suffix ? `${text}°C` : `${text}°`
}

export function hasActiveTargetTemperature(value?: string | null) {
  if (!value) {
    return false
  }
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed > 0
}
