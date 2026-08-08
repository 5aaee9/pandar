'use client'

import type { ReactNode } from 'react'
import { useTranslations } from 'next-intl'
import {
  LightbulbIcon,
  Loader2Icon,
  PauseIcon,
  PlayIcon,
  SquareIcon,
  ThermometerIcon,
} from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover'

import { CameraDialogControl } from './dashboard-printer-camera-control'
import { ConfirmForm } from './confirm-dialog'
import type { Printer } from './dashboard-types'
import {
  PrinterControlFields,
  printerControlFieldNames,
  usePrinterControl,
} from './printer-controls'
import type { PrinterTemperatureAction } from './printer-controls'
import {
  NozzleSwitchControl,
  NozzleTemperatureCard,
  TemperatureReading,
} from './dashboard-printer-nozzle-temperature-controls'
import {
  formatTemperatureValue,
  hasActiveTargetTemperature,
  presentNozzles,
} from './dashboard-printer-nozzle-temperature'

type TemperatureControl = {
  title: string
  subtitle: string | null
  current: string
  target: string | null
  value: string
  tone: string
  action: PrinterTemperatureAction
  ariaLabel: string
  popoverTitle: string
  presets: readonly number[]
}

export function PrinterTemperatureControls({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const nozzles = presentNozzles(printer.nozzle_temperatures ?? [])
  const temperatures = printerTemperatures(printer, t)
  if (nozzles.length === 0 && temperatures.length === 0) {
    return null
  }

  return (
    <div
      className={`mt-4 grid gap-2 ${
        nozzles.length > 1 ? 'grid-cols-2 lg:grid-cols-4' : 'grid-cols-2 lg:grid-cols-3'
      }`}
    >
      {nozzles.length > 0 ? <NozzleTemperatureCard nozzles={nozzles} printer={printer} /> : null}
      {temperatures.map((temperature) => (
        <TemperatureCard
          key={temperature.title}
          subtitle={temperature.subtitle}
          current={temperature.current}
          target={temperature.target}
          title={temperature.title}
          tone={temperature.tone}
          value={temperature.value}
          action={temperature.action}
          ariaLabel={temperature.ariaLabel}
          popoverTitle={temperature.popoverTitle}
          presets={temperature.presets}
          printer={printer}
        />
      ))}
      <NozzleSwitchControl printer={printer} />
    </div>
  )
}

export function PrinterControlsPanel({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const controlsEnabled = printerControlEnabled(printer)
  const stopControl = usePrinterControl()

  return (
    <div className="mt-4 space-y-2">
      <div className="text-xs font-medium text-muted-foreground">{t('controlsLabel')}</div>
      <div aria-label={t('controlsLabel')} className="grid grid-cols-2 gap-2" role="group">
        <ConfirmForm
          action={stopControl.formAction}
          buttonAriaLabel={t('stopPrint')}
          buttonClassName="w-full rounded-md font-semibold disabled:bg-muted/60 disabled:text-muted-foreground"
          buttonLabel={<><SquareIcon />{t('stopPrint')}</>}
          buttonVariant="destructive"
          confirmLabel={t('stopPrint')}
          disabled={!controlsEnabled.stop}
          message={t('stopPrintMessage')}
          pending={stopControl.pending}
          title={t('stopPrintTitle')}
        >
          <PrinterControlFields printer={printer} intent={{ action: 'stop' }} />
        </ConfirmForm>
        <PrinterInlineControl
          enabled={controlsEnabled.resume || controlsEnabled.pause}
          icon={controlsEnabled.resume ? <PlayIcon /> : <PauseIcon />}
          intent={{ action: controlsEnabled.resume ? 'resume' : 'pause' }}
          label={controlsEnabled.resume ? t('resumePrint') : t('pausePrint')}
          printer={printer}
          tone={controlsEnabled.resume ? 'neutral' : 'warning'}
        />
        <PrinterInlineControl
          enabled={controlsEnabled.light}
          icon={<LightbulbIcon />}
          intent={{ action: 'set_chamber_light', lightOn: printer.chamber_light_on === true ? false : true }}
          label={t('lightControl')}
          pressed={printer.chamber_light_on === true}
          printer={printer}
          stateLabel={printer.chamber_light_on === true ? t('lightStateOn') : t('lightStateOff')}
          tone="neutral"
        />
        <CameraDialogControl printer={printer} />
      </div>
    </div>
  )
}

function TemperatureCard({
  printer,
  action,
  ariaLabel,
  popoverTitle,
  presets,
  title,
  subtitle,
  current,
  target,
  value,
  tone,
}: {
  printer: Printer
  action: PrinterTemperatureAction
  ariaLabel: string
  popoverTitle: string
  presets: readonly number[]
  title: string
  subtitle: string | null
  current: string
  target: string | null
  value: string
  tone: string
}) {
  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            aria-label={ariaLabel}
            className="h-auto min-h-16 w-full flex-col rounded-md bg-muted/50 px-3 py-2 text-center font-normal hover:bg-muted dark:hover:bg-muted"
            type="button"
            variant="ghost"
          />
        }
      >
        <ThermometerIcon className={`size-4 ${tone}`} />
        <div className="mt-1 text-xs font-medium text-muted-foreground">{title}</div>
        {subtitle ? <div className="text-xs font-medium text-muted-foreground">{subtitle}</div> : null}
        <div className="text-sm font-semibold text-foreground">{value}</div>
      </PopoverTrigger>
      <PopoverContent className="w-72" sideOffset={8}>
        <PopoverTitle className="text-center text-base font-semibold">{popoverTitle}</PopoverTitle>
        <div className="mt-2 space-y-3">
          <TemperatureReading current={current} target={target} />
          <TemperatureMenu action={action} presets={presets} printer={printer} />
        </div>
      </PopoverContent>
    </Popover>
  )
}

function TemperatureMenu({
  printer,
  action,
  presets,
}: {
  printer: Printer
  action: PrinterTemperatureAction
  presets: readonly number[]
}) {
  const t = useTranslations('inventory')
  const customControl = usePrinterControl()

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-1.5">
        {presets.map((temperature) => (
          <TemperaturePresetButton
            action={action}
            key={temperature}
            printer={printer}
            temperature={temperature}
          />
        ))}
      </div>
      <form action={customControl.formAction} className="flex gap-1.5">
        <PrinterControlFields printer={printer} intent={{ action }} />
        <Input
          aria-label={t('customTemperature')}
          inputMode="numeric"
          min="0"
          name={printerControlFieldNames.temperatureCelsius}
          placeholder={t('customTemperature')}
          type="number"
        />
        <Button disabled={customControl.pending} size="sm" type="submit" variant="secondary">
          {customControl.pending ? <Loader2Icon className="animate-spin" /> : null}
          {t('setTemperature')}
        </Button>
      </form>
    </div>
  )
}

function TemperaturePresetButton({
  printer,
  action,
  temperature,
}: {
  printer: Printer
  action: PrinterTemperatureAction
  temperature: number
}) {
  const t = useTranslations('inventory')
  const { formAction, pending } = usePrinterControl()

  return (
    <form action={formAction}>
      <PrinterControlFields
        printer={printer}
        intent={{ action, temperatureCelsius: temperature }}
      />
      <Button className="w-full" disabled={pending} size="sm" type="submit" variant="outline">
        {pending ? <Loader2Icon className="animate-spin" /> : null}
        {temperature === 0 ? t('temperatureOff') : `${temperature} C`}
      </Button>
    </form>
  )
}

function PrinterInlineControl({
  printer,
  intent,
  label,
  icon,
  enabled,
  pressed,
  stateLabel,
  tone,
}: {
  printer: Printer
  intent:
    | { action: 'resume' | 'pause' }
    | { action: 'set_chamber_light'; lightOn: boolean }
  label: string
  icon: ReactNode
  enabled: boolean
  pressed?: boolean
  stateLabel?: string
  tone: 'danger' | 'warning' | 'neutral'
}) {
  const { formAction, pending } = usePrinterControl()
  const variant = tone === 'danger' ? 'destructive' : 'soft'
  const toneClass =
    tone === 'warning'
      ? 'bg-warning/15 text-warning hover:bg-warning/25 dark:bg-warning/15 dark:hover:bg-warning/25'
      : undefined

  return (
    <form action={formAction}>
      <PrinterControlFields printer={printer} intent={intent} />
      <Button
        aria-label={stateLabel ? `${label} ${stateLabel}` : undefined}
        aria-pressed={pressed}
        className={`w-full rounded-md font-semibold disabled:bg-muted/60 disabled:text-muted-foreground ${toneClass ?? ''}`}
        disabled={!enabled || pending}
        type="submit"
        variant={variant}
      >
        {pending ? <Loader2Icon className="animate-spin" /> : icon}
        {label}
        {stateLabel ? <span className="text-xs font-normal opacity-75">{` ${stateLabel}`}</span> : null}
      </Button>
    </form>
  )
}

function printerControlEnabled(printer: Printer) {
  const coarseStatus = printer.status.toLowerCase()
  const printState = printer.print?.gcode_state?.toLowerCase() ?? coarseStatus
  const blocked = ['idle', 'offline', 'failed'].includes(coarseStatus)
  return {
    stop: !blocked && ['running', 'printing', 'paused', 'pause'].includes(printState),
    pause: !blocked && ['running', 'printing'].includes(printState),
    resume: !blocked && ['paused', 'pause'].includes(printState),
    light: true,
  }
}

function printerTemperatures(printer: Printer, t: ReturnType<typeof useTranslations>) {
  const temperatures: TemperatureControl[] = []
  if (printer.bed_temperature_celsius) {
    temperatures.push({
      title: t('bedTemperature'),
      subtitle: null,
      current: printer.bed_temperature_celsius,
      target: printer.bed_target_temperature_celsius ?? null,
      value: temperaturePair(printer.bed_temperature_celsius, printer.bed_target_temperature_celsius),
      tone: 'text-muted-foreground',
      action: 'set_bed_temperature',
      ariaLabel: t('setBedTemperature'),
      popoverTitle: t('setBedTemperatureTitle'),
      presets: [0, 55, 75, 90],
    })
  }
  if (printer.chamber_temperature_celsius) {
    temperatures.push({
      title: t('chamberTemperature'),
      subtitle: null,
      current: printer.chamber_temperature_celsius,
      target: printer.chamber_target_temperature_celsius ?? null,
      value: formatTemperatureValue(printer.chamber_temperature_celsius),
      tone: 'text-muted-foreground',
      action: 'set_chamber_temperature',
      ariaLabel: t('setChamberTemperature'),
      popoverTitle: t('setChamberTemperatureTitle'),
      presets: [0, 35, 45, 60],
    })
  }
  return temperatures
}

function temperaturePair(current?: string | null, target?: string | null) {
  if (hasActiveTargetTemperature(target)) {
    return `${formatTemperatureValue(current, false)} / ${formatTemperatureValue(target, false)}`
  }
  return formatTemperatureValue(current)
}
