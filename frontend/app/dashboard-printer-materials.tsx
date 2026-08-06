

import { useTranslations } from 'next-intl'
import { CheckCircle2Icon, DownloadIcon, DropletsIcon, RotateCwIcon, ThermometerIcon, UploadIcon } from 'lucide-react'

import { DryingControl, type DryingProps, dryingProps } from './dashboard-printer-drying'
import { SlotOperationForm } from './dashboard-printer-slot-operation-form'
import type { Printer } from './dashboard-types'
import { mixedAmsLiteGlobalTrayId } from './material-tray-routing'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '../components/ui/popover'
export function PrinterMaterialsPanel({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const materials = printer.materials
  const amsUnits = materials?.ams_units ?? []
  const externalSpools = materials?.external_spools.filter((spool) => spool.exists !== false) ?? []

  if (!materials || (amsUnits.length === 0 && externalSpools.length === 0)) {
    return (
      <div className="mt-4 rounded-md bg-muted/40 p-3 text-sm">
        <div className="font-medium text-foreground">{t('filamentsLabel')}</div>
        <div className="mt-1 text-muted-foreground">{t('noMaterialReport')}</div>
      </div>
    )
  }

  return (
    <div className="mt-4 space-y-2">
      <div className="text-xs font-medium text-muted-foreground">{t('filamentsLabel')}</div>
      <div className="grid gap-2">
        {amsUnits.map((unit) => (
          <MaterialUnitCard
            key={unit.unit_id ?? 'ams'}
            printer={printer}
            title={amsUnitName(unit.unit_id)}
            toolhead={unit.toolhead}
            humidity={unit.humidity}
            temperature={unit.temperature_celsius}
            drying={dryingProps(unit)}
            slots={(unit.trays ?? []).flatMap((tray) =>
              tray.exists === false
                ? []
                : [{
                key: `ams:${unit.unit_id ?? 'unknown'}:${tray.tray_id ?? tray.global_tray_id ?? 'unknown'}`,
                amsId: parseOptionalInt(unit.unit_id),
                externalId: null,
                globalTrayId: tray.global_tray_id ?? globalTrayId(unit.unit_id, unit.unit_kind, tray.tray_id),
                slotId: parseOptionalInt(tray.tray_id),
                label: materialLabel(tray, t('unknownMaterial')),
                color: tray.color,
                multiColor: tray.multi_color,
                remaining: percentValue(tray.remaining_estimate),
                kValue: stringValue(tray.k_value),
                toolhead: tray.toolhead ?? unit.toolhead,
              }],
            )}
            active={materials.active_tray}
          />
        ))}
        {externalSpools.length > 0 ? (
          <MaterialUnitCard
            printer={printer}
            title={t('external')}
            slots={externalSpools.map((spool, index) => ({
              key: `external:${spool.external_id ?? spool.global_tray_id ?? spool.tray_id ?? 'unknown'}`,
              amsId: 255,
              externalId: spool.external_id ?? null,
              globalTrayId: spool.global_tray_id ?? parseOptionalInt(spool.external_id),
              slotId: parseOptionalInt(spool.tray_id) ?? index,
              label: materialLabel(spool, t('unknownMaterial')),
              color: spool.color,
              multiColor: spool.multi_color,
              remaining: percentValue(spool.remaining_estimate),
              kValue: stringValue(spool.k_value),
              toolhead: spool.toolhead,
            }))}
            active={materials.active_tray}
          />
        ) : null}
      </div>
    </div>
  )
}

export type MaterialSlot = {
  key: string
  amsId: number | null
  externalId: string | null
  globalTrayId: number | null
  slotId: number | null
  label: string
  color?: string | null
  multiColor?: string[] | null
  remaining: RemainingValue
  kValue: string | null
  toolhead?: string | null
}

type RemainingValue = number | 'unsupported' | null

type ActiveMaterialTray = NonNullable<Printer['materials']>['active_tray']

function MaterialUnitCard({
  printer,
  title,
  toolhead,
  humidity,
  temperature,
  drying,
  slots,
  active,
}: {
  printer: Printer
  title: string
  toolhead?: string | null
  humidity?: number | string | null
  temperature?: number | string | null
  drying?: DryingProps | null
  slots: MaterialSlot[]
  active: ActiveMaterialTray
}) {
  return (
    <div className="rounded-md bg-muted/40 p-2">
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="truncate text-sm font-semibold text-foreground">{title}</span>
          {toolhead ? <ToolheadBadge value={toolhead} /> : null}
        </div>
        <div className="flex shrink-0 items-center gap-2 text-xs text-muted-foreground">
          {humidity !== undefined && humidity !== null ? (
            <span className="inline-flex items-center gap-1">
              <DropletsIcon className="size-3" />
              {humidity}%
            </span>
          ) : null}
          {temperature !== undefined && temperature !== null ? (
            <span className="inline-flex items-center gap-1">
              <ThermometerIcon className="size-3" />
              {formatTemperature(temperature)}
            </span>
          ) : null}
          {drying ? <DryingControl drying={drying} printer={printer} /> : null}
        </div>
      </div>
      <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-4">
        {slots.map((slot, index) => (
          <MaterialSlotButton
            active={isActiveSlot(slot, active)}
            key={slot.key}
            printer={printer}
            slot={slot}
            title={title}
            index={index}
          />
        ))}
      </div>
    </div>
  )
}

function MaterialSlotButton({
  printer,
  title,
  slot,
  index,
  active,
}: {
  printer: Printer
  title: string
  slot: MaterialSlot
  index: number
  active: boolean
}) {
  const t = useTranslations('inventory')
  const labelParts = [
    t('slotAria', {
      title,
      number: slot.slotId !== null ? slot.slotId + 1 : index + 1,
      material: slot.label,
    }),
  ]
  if (active) labelParts.push(t('activeTray'))
  if (slot.remaining !== null) {
    labelParts.push(`${t('filamentRemaining')}: ${remainingLabel(slot.remaining, t('filamentUnsupported'))}`)
  }
  const accessibleLabel = labelParts.join(', ')
  const colorHex = slotColorHex(slot)
  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            aria-label={accessibleLabel}
            className={`h-auto min-h-14 w-full flex-col items-stretch justify-between whitespace-normal rounded-md bg-background p-2 text-left font-normal hover:bg-accent dark:hover:bg-accent ${
              active ? 'ring-2 ring-success' : ''
            }`}
            type="button"
            variant="ghost"
          />
        }
      >
        <div className="flex items-start justify-between gap-1">
          <span
            aria-hidden="true"
            className="inline-flex size-4 items-center justify-center rounded-full text-[10px] font-bold text-white shadow-sm"
            style={{ background: slotSwatch(slot) }}
          >
            {slot.slotId !== null ? slot.slotId + 1 : index + 1}
          </span>
          <span className="flex items-center gap-1">
            {active ? <CheckCircle2Icon aria-hidden="true" className="size-3.5 text-success" /> : null}
            {slot.toolhead ? <ToolheadBadge value={slot.toolhead} /> : null}
          </span>
        </div>
        <div>
          <div className="truncate text-xs font-semibold text-foreground">{slot.label}</div>
          <FillBar value={slot.remaining} />
        </div>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-56 gap-0 p-2">
        <div className="flex items-center gap-2 rounded-sm bg-muted px-2 py-2">
          <span
            aria-hidden="true"
            className="size-5 shrink-0 rounded-sm ring-1 ring-foreground/10"
            style={{ background: slotSwatch(slot) }}
          />
          <span className="font-mono text-xs font-medium text-foreground">
            {colorHex ?? t('filamentColorUnknown')}
          </span>
        </div>
        <div className="grid gap-1 px-2 py-2 text-xs">
          <div className="flex justify-between gap-3">
            <span className="text-muted-foreground">{t('filamentConfig')}</span>
            <span className="font-medium">{slot.label}</span>
          </div>
          {slot.kValue ? (
            <div className="flex justify-between gap-3">
              <span className="text-muted-foreground">{t('filamentKValue')}</span>
              <span className="font-medium">{slot.kValue}</span>
            </div>
          ) : null}
          {slot.remaining !== null ? (
            <div className="space-y-1">
              <div className="flex justify-between gap-3">
                <span className="text-muted-foreground">{t('filamentRemaining')}</span>
                <span className="font-medium">{remainingLabel(slot.remaining, t('filamentUnsupported'))}</span>
              </div>
              <FillBar value={slot.remaining} />
            </div>
          ) : null}
        </div>
        <SlotOperationForm action="ams_reread_rfid" icon={<RotateCwIcon />} label={t('rereadRfid')} printer={printer} slot={slot} />
        <SlotOperationForm action="ams_load_filament" icon={<DownloadIcon />} label={t('loadFilament')} printer={printer} slot={slot} />
        <SlotOperationForm action="ams_unload_filament" icon={<UploadIcon />} label={t('unloadFilament')} printer={printer} slot={slot} />
      </PopoverContent>
    </Popover>
  )
}

function ToolheadBadge({ value }: { value: string }) {
  return (
    <span className="inline-flex h-4 min-w-4 items-center justify-center rounded bg-muted px-1 text-[10px] font-semibold text-muted-foreground">
      {value}
    </span>
  )
}

function FillBar({ value }: { value: RemainingValue }) {
  if (value === null) return <div aria-hidden="true" className="mt-1 h-1 rounded-full bg-muted" />
  if (value === 'unsupported') {
    return <div aria-hidden="true" className="mt-1 h-1 rounded-full bg-muted-foreground/30" />
  }
  return (
    <div aria-hidden="true" className="mt-1 h-1 overflow-hidden rounded-full bg-muted">
      <div className="h-full rounded-full bg-success" style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />
    </div>
  )
}

function amsUnitName(unitId?: string) {
  const index = parseOptionalInt(unitId)
  if (index === null || index < 0 || index > 25) return `AMS-${unitId ?? '-'}`
  return `AMS-${String.fromCharCode(65 + index)}`
}

function materialLabel(value: { type?: string | null; name?: string | null; filament_id?: string | null }, fallback: string) {
  return value.type || value.name || value.filament_id || fallback
}

function percentValue(value: string | number | null | undefined) {
  const parsed = typeof value === 'number' ? value : typeof value === 'string' ? Number(value.trim()) : null
  if (parsed === -1) return 'unsupported'
  return parsed !== null && Number.isFinite(parsed) ? Math.round(parsed) : null
}

function remainingLabel(value: Exclude<RemainingValue, null>, unsupportedLabel: string) {
  return value === 'unsupported' ? unsupportedLabel : `${value}%`
}

function stringValue(value: string | number | null | undefined) {
  if (typeof value === 'number') return value.toFixed(3)
  if (typeof value === 'string' && value.trim()) return value.trim()
  return null
}

function parseOptionalInt(value?: string | number | null) {
  if (value === undefined || value === null) return null
  if (typeof value === 'string' && value.trim() === '') return null
  const parsed = Number(value)
  return Number.isInteger(parsed) ? parsed : null
}

function globalTrayId(unitId?: string, unitKind?: string | null, trayId?: string) {
  const unit = parseOptionalInt(unitId)
  const tray = parseOptionalInt(trayId)
  if (tray === null) return null
  return mixedAmsLiteGlobalTrayId(unitKind, tray) ??
    (unit !== null ? unit * 4 + tray : null)
}

function formatTemperature(value: string | number) {
  const parsed = typeof value === 'number' ? value : Number(value)
  return Number.isFinite(parsed) ? `${parsed.toFixed(1)}°C` : `${value}°C`
}

function slotSwatch(slot: MaterialSlot) {
  if (slot.multiColor?.length) {
    const colors = slot.multiColor.map((color) => `#${color.trim().replace(/^#/, '').slice(0, 6)}`)
    return `linear-gradient(90deg, ${colors.join(', ')})`
  }
  return solidSlotColor(slot)
}

function solidSlotColor(slot: MaterialSlot) {
  return slotColorHex(slot) ?? 'var(--muted-foreground)'
}

function slotColorHex(slot: MaterialSlot) {
  const raw = slot.color?.trim().replace(/^#/, '').slice(0, 6)
  return raw && /^[0-9a-fA-F]{6}$/.test(raw) ? `#${raw}`.toUpperCase() : null
}

function isActiveSlot(slot: MaterialSlot, active: ActiveMaterialTray) {
  if (!active) return false
  if (active.kind === 'external') {
    return slot.externalId !== null && slot.slotId?.toString() === active.tray_id
  }
  return (
    (active.global_tray_id !== null &&
      active.global_tray_id !== undefined &&
      slot.globalTrayId === active.global_tray_id) ||
    (slot.amsId?.toString() === active.ams_id &&
      slot.slotId?.toString() === active.tray_id)
  )
}
