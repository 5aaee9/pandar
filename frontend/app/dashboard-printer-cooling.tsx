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

import type { Printer } from './dashboard-types'

type CoolingFan = NonNullable<Printer['cooling_system']>['fans'][number]
type CoolingMode = NonNullable<Printer['cooling_system']>['mode']

const openFrameModels = new Set(['A1', 'A1 MINI', 'A2L', 'P1P'])
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
    .filter((fan) => fan.kind !== 'chamber' || hasChamberFan(printer.model))
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
          {fans.map((fan) => <CoolingFanCard fan={fan} key={fan.kind} />)}
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

function CoolingFanCard({ fan }: { fan: CoolingFan }) {
  const t = useTranslations('inventory')
  const speed = Math.max(0, Math.min(100, fan.speed_percent))

  return (
    <div className="rounded-md bg-muted/50 px-3 py-2">
      <div className="flex items-center justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5 text-xs font-medium text-muted-foreground">
          {fanIcon(fan.kind, `size-3.5 shrink-0 ${speed > 0 ? 'text-sky-500' : ''}`)}
          <span className="truncate">{coolingFanLabel(fan.kind, t)}</span>
        </span>
        <span className="text-sm font-semibold text-foreground">{speed}%</span>
      </div>
      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-background">
        <div className="h-full rounded-full bg-sky-500 transition-[width]" style={{ width: `${speed}%` }} />
      </div>
    </div>
  )
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

function hasChamberFan(model: string | null) {
  if (!model) {
    return true
  }
  const normalized = model.toUpperCase().replace(/^BAMBU LAB\s+/, '')
  return !openFrameModels.has(normalized)
}
