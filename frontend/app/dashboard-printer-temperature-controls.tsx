'use client'

import type { ReactNode } from 'react'
import { useRef } from 'react'
import { useTranslations } from 'next-intl'
import {
  LightbulbIcon,
  Loader2Icon,
  MaximizeIcon,
  PauseIcon,
  PlayIcon,
  SquareIcon,
  ThermometerIcon,
  VideoIcon,
} from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover'

import { apiIdSegment } from './api-path'
import { ConfirmForm } from './confirm-dialog'
import type { Printer } from './dashboard-types'
import { usePrinterControl } from './use-printer-control'
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
  action: string
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
    <div className="mt-4 grid grid-cols-2 gap-2 lg:grid-cols-[1fr_1fr_1fr_5rem]">
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
          buttonClassName="inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-md px-2 text-sm font-semibold transition-colors duration-150 ease-out disabled:bg-muted/60 disabled:text-muted-foreground enabled:bg-destructive/10 enabled:text-destructive enabled:hover:bg-destructive/20 dark:enabled:bg-destructive/20 dark:enabled:hover:bg-destructive/30 [&_svg]:size-4"
          buttonLabel={<><SquareIcon />{t('stopPrint')}</>}
          confirmLabel={t('stopPrint')}
          disabled={!controlsEnabled.stop}
          message={t('stopPrintMessage')}
          pending={stopControl.pending}
          title={t('stopPrintTitle')}
        >
          <input name="tenant_id" type="hidden" value={printer.tenant_id} />
          <input name="printer_id" type="hidden" value={printer.id} />
          <input name="action" type="hidden" value="stop" />
        </ConfirmForm>
        <PrinterInlineControl
          action={controlsEnabled.resume ? 'resume' : 'pause'}
          enabled={controlsEnabled.resume || controlsEnabled.pause}
          icon={controlsEnabled.resume ? <PlayIcon /> : <PauseIcon />}
          label={controlsEnabled.resume ? t('resumePrint') : t('pausePrint')}
          printer={printer}
          tone={controlsEnabled.resume ? 'neutral' : 'warning'}
        />
        <PrinterInlineControl
          action="set_chamber_light"
          enabled={controlsEnabled.light}
          icon={<LightbulbIcon />}
          label={t('lightControl')}
          lightOn={printer.chamber_light_on === true ? false : true}
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

function CameraDialogControl({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const frameRef = useRef<HTMLDivElement>(null)

  return (
    <Dialog>
      <DialogTrigger
        className="inline-flex min-h-8 items-center justify-center gap-2 rounded-md bg-muted/50 px-3 py-2 text-sm font-medium text-foreground transition hover:bg-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
        type="button"
      >
        <VideoIcon className="size-4" />
        {t('viewCamera')}
      </DialogTrigger>
      <DialogContent className="sm:max-w-3xl" closeLabel={t('closeCamera')}>
        <DialogHeader>
          <DialogTitle>{t('cameraTitle')}</DialogTitle>
        </DialogHeader>
        <div ref={frameRef} className="relative overflow-hidden rounded-md bg-black">
          <video
            aria-label={t('cameraTitle')}
            autoPlay
            className="aspect-video w-full bg-black object-contain"
            muted
            playsInline
            src={`/api/tenants/${apiIdSegment(printer.tenant_id, 'tenant_id')}/printers/${apiIdSegment(printer.id, 'printer_id')}/camera.mp4`}
          />
          <button
            aria-label={t('cameraFullscreen')}
            className="absolute right-3 top-3 inline-flex size-8 items-center justify-center rounded-md bg-black/70 text-white transition hover:bg-black/90 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
            onClick={() => void frameRef.current?.requestFullscreen()}
            title={t('cameraFullscreen')}
            type="button"
          >
            <MaximizeIcon className="size-4" />
          </button>
        </div>
      </DialogContent>
    </Dialog>
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
  action: string
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
        aria-label={ariaLabel}
        className="flex min-h-16 flex-col items-center justify-center rounded-md bg-muted/50 px-3 py-2 text-center transition hover:bg-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
        type="button"
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
  action: string
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
        <PrinterTemperatureHiddenFields action={action} printer={printer} />
        <Input
          aria-label={t('customTemperature')}
          inputMode="numeric"
          min="0"
          name="temperature_celsius"
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
  action: string
  temperature: number
}) {
  const t = useTranslations('inventory')
  const { formAction, pending } = usePrinterControl()

  return (
    <form action={formAction}>
      <PrinterTemperatureHiddenFields
        action={action}
        printer={printer}
        temperature={temperature}
      />
      <Button className="w-full" disabled={pending} size="sm" type="submit" variant="outline">
        {pending ? <Loader2Icon className="animate-spin" /> : null}
        {temperature === 0 ? t('temperatureOff') : `${temperature} C`}
      </Button>
    </form>
  )
}

function PrinterTemperatureHiddenFields({
  printer,
  action,
  temperature,
}: {
  printer: Printer
  action: string
  temperature?: number
}) {
  return (
    <>
      <input name="tenant_id" type="hidden" value={printer.tenant_id} />
      <input name="printer_id" type="hidden" value={printer.id} />
      <input name="action" type="hidden" value={action} />
      {temperature !== undefined ? (
        <input name="temperature_celsius" type="hidden" value={temperature} />
      ) : null}
    </>
  )
}

function PrinterInlineControl({
  printer,
  action,
  label,
  icon,
  enabled,
  lightOn,
  pressed,
  stateLabel,
  tone,
}: {
  printer: Printer
  action: string
  label: string
  icon: ReactNode
  enabled: boolean
  lightOn?: boolean
  pressed?: boolean
  stateLabel?: string
  tone: 'danger' | 'warning' | 'neutral'
}) {
  const { formAction, pending } = usePrinterControl()
  const toneClass = {
    danger:
      'enabled:bg-destructive/10 enabled:text-destructive enabled:hover:bg-destructive/20 dark:enabled:bg-destructive/20 dark:enabled:hover:bg-destructive/30',
    warning:
      'enabled:bg-warning/15 enabled:text-warning enabled:hover:bg-warning/25',
    neutral:
      'enabled:bg-primary/10 enabled:text-primary enabled:hover:bg-primary/15 dark:enabled:bg-primary/20',
  }[tone]

  return (
    <form action={formAction}>
      <input name="tenant_id" type="hidden" value={printer.tenant_id} />
      <input name="printer_id" type="hidden" value={printer.id} />
      <input name="action" type="hidden" value={action} />
      {lightOn !== undefined ? <input name="light_on" type="hidden" value={String(lightOn)} /> : null}
      <button
        aria-label={stateLabel ? `${label} ${stateLabel}` : undefined}
        aria-pressed={pressed}
        className={`inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-md px-2 text-sm font-semibold transition-colors duration-150 ease-out disabled:bg-muted/60 disabled:text-muted-foreground ${toneClass} [&_svg]:size-4`}
        disabled={!enabled || pending}
        type="submit"
      >
        {pending ? <Loader2Icon className="animate-spin" /> : icon}
        {label}
        {stateLabel ? <span className="text-xs font-normal opacity-75">{` ${stateLabel}`}</span> : null}
      </button>
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
