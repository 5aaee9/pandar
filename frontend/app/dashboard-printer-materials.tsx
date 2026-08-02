

import type { ReactNode } from 'react'
import { useTranslations } from 'next-intl'
import { CheckCircle2Icon, DownloadIcon, DropletsIcon, RotateCwIcon, ThermometerIcon, UploadIcon } from 'lucide-react'

import { controlPrinter } from './actions'
import type { Printer } from './dashboard-types'
import { mixedAmsLiteGlobalTrayId } from './material-tray-routing'
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
            slots={(unit.trays ?? [])
              .filter((tray) => tray.exists !== false)
              .map((tray) => ({
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
              }))}
            active={materials.active_tray}
          />
        ))}
        {externalSpools.length > 0 ? (
          <MaterialUnitCard
            printer={printer}
            title={t('external')}
            slots={externalSpools.map((spool, index) => ({
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

type MaterialSlot = {
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
  slots,
  active,
}: {
  printer: Printer
  title: string
  toolhead?: string | null
  humidity?: number | string | null
  temperature?: number | string | null
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
        </div>
      </div>
      <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-4">
        {slots.map((slot, index) => (
          <MaterialSlotButton
            active={isActiveSlot(slot, active)}
            key={`${slot.externalId ?? slot.amsId ?? 'slot'}-${slot.slotId ?? index}`}
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
        aria-label={accessibleLabel}
        className={`flex min-h-14 w-full flex-col justify-between rounded-md bg-background p-2 text-left transition-colors duration-150 ease-out hover:bg-accent ${
          active ? 'ring-2 ring-success' : ''
        }`}
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

function SlotOperationForm({
  printer,
  slot,
  action,
  label,
  icon,
}: {
  printer: Printer
  slot: MaterialSlot
  action: string
  label: string
  icon: ReactNode
}) {
  const includeTarget = action !== 'ams_reread_rfid'
  const extruderId = action === 'ams_load_filament' ? slotExtruderId(slot, printer) : null

  return (
    <form action={controlPrinter}>
      <input name="tenant_id" type="hidden" value={printer.tenant_id} />
      <input name="printer_id" type="hidden" value={printer.id} />
      <input name="action" type="hidden" value={action} />
      {slot.amsId !== null ? <input name="ams_id" type="hidden" value={slot.amsId} /> : null}
      {slot.slotId !== null ? <input name="slot_id" type="hidden" value={slot.slotId} /> : null}
      {includeTarget && slot.globalTrayId !== null ? <input name="global_tray_id" type="hidden" value={slot.globalTrayId} /> : null}
      {includeTarget && slot.externalId ? <input name="external_id" type="hidden" value={slot.externalId} /> : null}
      {extruderId !== null ? <input name="extruder_id" type="hidden" value={extruderId} /> : null}
      <button
        className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm transition-colors duration-150 ease-out hover:bg-muted focus-visible:outline-2 focus-visible:outline-ring [&_svg]:size-4"
        type="submit"
      >
        {icon}
        {label}
      </button>
    </form>
  )
}

function slotExtruderId(slot: MaterialSlot, printer: Printer) {
  const toolhead = slot.toolhead?.trim().toUpperCase()
  if (toolhead === 'R') return 0
  if (toolhead === 'L') return 1
  if (slot.externalId === '255') return 0
  if (slot.externalId === '254') return 1
  const model = printer.model?.toLowerCase() ?? ''
  return model.includes('x2d') || model.includes('h2d') ? 0 : null
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

function parseOptionalInt(value?: string | null) {
  if (!value) return null
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
