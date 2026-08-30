'use client'

import { useTranslations } from 'next-intl'
import {
  AirVentIcon,
  CpuIcon,
  FanIcon,
  FlameIcon,
  SnowflakeIcon,
  WindIcon,
} from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover'

import type { Printer } from './dashboard-types'
import { PrinterControlFields, usePrinterControl } from './printer-controls'

type CoolingFan = NonNullable<Printer['cooling_system']>['fans'][number]
type CoolingMode = NonNullable<Printer['cooling_system']>['mode']

const fanOrder: CoolingFan['kind'][] = [
  'part_cooling',
  'auxiliary',
  'chamber',
  'hotend',
  'hotend_second',
  'inner_loop',
  'controller',
  'auxiliary_second',
]

export function PrinterCoolingSystem({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const cooling = printer.cooling_system
  if (!cooling) {
    return null
  }

  const fans = cooling.fans
    .filter(
      (fan) =>
        fan.kind !== 'chamber' || printer.compatibility?.chamber_fan === 'supported',
    )
    .sort((left, right) => fanOrder.indexOf(left.kind) - fanOrder.indexOf(right.kind))
  if (fans.length === 0 && !cooling.mode) {
    return null
  }

  return (
    <section aria-label={t('coolingSystem')} className="mt-4 space-y-2">
      <div className="flex items-center justify-between gap-2">
        <div className="text-xs font-medium text-muted-foreground">{t('coolingSystem')}</div>
        {cooling.mode ? <CoolingModeBadge mode={cooling.mode} /> : null}
      </div>
      {fans.length > 0 ? (
        <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
          {fans.map((fan) => (
            <CoolingFanCard
              airduct={cooling.mode !== null && cooling.mode !== undefined}
              fan={fan}
              key={fan.kind}
              printer={printer}
            />
          ))}
        </div>
      ) : null}
    </section>
  )
}

function CoolingModeBadge({ mode }: { mode: CoolingMode }) {
  const t = useTranslations('inventory')
  const heating = mode === 'heating'
  const Icon = heating ? FlameIcon : SnowflakeIcon

  return (
    <span className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
      <Icon className={`size-3.5 ${heating ? 'text-orange-500' : 'text-sky-500'}`} />
      {coolingModeLabel(mode, t)}
    </span>
  )
}

function CoolingFanCard({
  fan,
  printer,
  airduct,
}: {
  fan: CoolingFan
  printer: Printer
  airduct: boolean
}) {
  const t = useTranslations('inventory')
  const speed = Math.max(0, Math.min(100, fan.speed_percent))
  const label = coolingFanLabel(fan.kind, t)
  const fanIndex = controllableFanIndex(fan.kind)
  const content = <CoolingFanReading fan={fan} label={label} speed={speed} />

  if (fanIndex === null) {
    return <div className="rounded-md bg-muted/50 px-3 py-2">{content}</div>
  }

  const controlLabel = t('setCoolingFanSpeed', { fan: label })

  return (
    <Popover>
      <PopoverTrigger
        disabled={printer.status.toLowerCase() === 'offline'}
        render={
          <Button
            aria-label={controlLabel}
            className="h-auto w-full flex-col items-stretch rounded-md bg-muted/50 px-3 py-2 text-left font-normal whitespace-normal hover:bg-muted"
            type="button"
            variant="ghost"
          />
        }
      >
        {content}
      </PopoverTrigger>
      <PopoverContent sideOffset={8}>
        <PopoverTitle className="text-center text-base font-semibold">{controlLabel}</PopoverTitle>
        <div aria-label={controlLabel} className="grid grid-cols-3 gap-1.5" role="group">
          {[0, 50, 100].map((speedPercent) => (
            <CoolingFanPreset
              airduct={airduct}
              fanIndex={fanIndex}
              key={speedPercent}
              printer={printer}
              speedPercent={speedPercent}
            />
          ))}
        </div>
      </PopoverContent>
    </Popover>
  )
}

function CoolingFanReading({
  fan,
  label,
  speed,
}: {
  fan: CoolingFan
  label: string
  speed: number
}) {
  return (
    <>
      <span className="flex items-start gap-1.5 text-xs font-medium text-muted-foreground">
        {fanIcon(fan.kind, `mt-0.5 size-3.5 shrink-0 ${speed > 0 ? 'text-sky-500' : ''}`)}
        <span className="min-w-0 whitespace-normal break-words">{label}</span>
      </span>
      <span className="mt-1 block text-sm font-semibold text-foreground">{speed}%</span>
      <span className="mt-2 block h-1.5 overflow-hidden rounded-full bg-background">
        <span className="block h-full rounded-full bg-sky-500 transition-[width]" style={{ width: `${speed}%` }} />
      </span>
    </>
  )
}

function CoolingFanPreset({
  printer,
  fanIndex,
  speedPercent,
  airduct,
}: {
  printer: Printer
  fanIndex: 1 | 2 | 3
  speedPercent: number
  airduct: boolean
}) {
  const t = useTranslations('inventory')
  const { formAction, pending } = usePrinterControl()

  return (
    <form action={formAction}>
      <PrinterControlFields
        intent={{ action: 'set_fan_speed', fanIndex, speedPercent, airduct }}
        printer={printer}
      />
      <Button className="w-full" disabled={pending} size="sm" type="submit" variant="outline">
        {speedPercent === 0 ? t('temperatureOff') : `${speedPercent}%`}
      </Button>
    </form>
  )
}

function controllableFanIndex(kind: CoolingFan['kind']): 1 | 2 | 3 | null {
  switch (kind) {
    case 'part_cooling': return 1
    case 'auxiliary': return 2
    case 'chamber': return 3
    case 'hotend':
    case 'hotend_second':
    case 'controller':
    case 'inner_loop':
    case 'auxiliary_second':
      return null
  }
}

function coolingModeLabel(mode: CoolingMode, t: ReturnType<typeof useTranslations>) {
  switch (mode) {
    case 'cooling': return t('coolingModeCooling')
    case 'heating': return t('coolingModeHeating')
    case 'exhaust': return t('coolingModeExhaust')
    case 'full_cooling': return t('coolingModeFullCooling')
    case null:
    case undefined:
      return ''
  }
}

function coolingFanLabel(kind: CoolingFan['kind'], t: ReturnType<typeof useTranslations>) {
  switch (kind) {
    case 'hotend': return t('coolingFanHotend')
    case 'part_cooling': return t('coolingFanPart')
    case 'auxiliary': return t('coolingFanAuxiliary')
    case 'chamber': return t('coolingFanChamber')
    case 'hotend_second': return t('coolingFanHotendSecond')
    case 'controller': return t('coolingFanController')
    case 'inner_loop': return t('coolingFanInnerLoop')
    case 'auxiliary_second': return t('coolingFanAuxiliarySecond')
  }
}

function fanIcon(kind: CoolingFan['kind'], className: string) {
  switch (kind) {
    case 'part_cooling':
    case 'hotend':
    case 'hotend_second':
      return <FanIcon className={className} />
    case 'auxiliary':
    case 'auxiliary_second':
    case 'inner_loop':
      return <WindIcon className={className} />
    case 'chamber':
      return <AirVentIcon className={className} />
    case 'controller':
      return <CpuIcon className={className} />
  }
}
