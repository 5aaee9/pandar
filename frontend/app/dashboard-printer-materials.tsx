'use client'

import { useState, type ReactNode } from 'react'
import { useTranslations } from 'next-intl'
import { DownloadIcon, DropletsIcon, RotateCwIcon, ThermometerIcon, UploadIcon } from 'lucide-react'

import { controlPrinter } from './actions'
import type { Printer } from './dashboard-types'
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
                globalTrayId: tray.global_tray_id ?? globalTrayId(unit.unit_id, tray.tray_id),
                slotId: parseOptionalInt(tray.tray_id),
                label: materialLabel(tray),
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
              label: materialLabel(spool),
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
        <div className="flex shrink-0 items-center gap-2 text-xs text-emerald-600 dark:text-emerald-400">
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
  const [open, setOpen] = useState(false)
  const label = `${title} slot ${slot.slotId !== null ? slot.slotId + 1 : index + 1} ${slot.label}`
  return (
    <div
      className="relative"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <button
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={label}
        className={`flex min-h-14 w-full flex-col justify-between rounded-md bg-background p-2 text-left transition hover:bg-background/80 ${
          active ? 'ring-2 ring-emerald-500' : ''
        }`}
        onFocus={() => setOpen(true)}
        onClick={() => setOpen((value) => !value)}
        type="button"
      >
        <div className="flex items-start justify-between gap-1">
          <span
            className="inline-flex size-4 items-center justify-center rounded-full text-[10px] font-bold text-white shadow-sm"
            style={{ background: slotSwatch(slot) }}
          >
            {slot.slotId !== null ? slot.slotId + 1 : index + 1}
          </span>
          {slot.toolhead ? <ToolheadBadge value={slot.toolhead} /> : null}
        </div>
        <div>
          <div className="truncate text-xs font-semibold text-foreground">{slot.label}</div>
          <FillBar value={slot.remaining} />
        </div>
      </button>
      {open ? (
        <div
          className="absolute left-0 top-full z-30 w-56 pt-1"
          role="menu"
        >
          <div className="rounded-md border border-border bg-popover p-2 text-popover-foreground shadow-md">
            <div
              className="rounded-sm px-2 py-2 text-center text-sm font-semibold text-white"
              style={{ background: solidSlotColor(slot) }}
            >
              {colorName(slot.color)}
            </div>
            <div className="grid gap-1 px-2 py-2 text-xs">
              <div className="flex justify-between gap-3">
                <span className="text-muted-foreground">{t('filamentConfig')}</span>
                <span className="font-medium">{slot.label}</span>
              </div>
              {slot.kValue ? (
                <div className="flex justify-between gap-3">
                  <span className="text-muted-foreground">{t('filamentKValue')}</span>
                  <span className="font-medium text-emerald-600 dark:text-emerald-400">{slot.kValue}</span>
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
          </div>
        </div>
      ) : null}
    </div>
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
        className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm hover:bg-muted [&_svg]:size-4"
        role="menuitem"
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
    <span className="inline-flex h-4 min-w-4 items-center justify-center rounded bg-emerald-100 px-1 text-[10px] font-semibold text-emerald-700 dark:bg-emerald-500/20 dark:text-emerald-300">
      {value}
    </span>
  )
}

function FillBar({ value }: { value: RemainingValue }) {
  if (value === null) return <div className="mt-1 h-1 rounded-full bg-muted" />
  if (value === 'unsupported') {
    return <div aria-label="Unsupported remaining progress" className="mt-1 h-1 rounded-full bg-slate-400 dark:bg-slate-600" />
  }
  return (
    <div className="mt-1 h-1 overflow-hidden rounded-full bg-muted">
      <div className="h-full rounded-full bg-emerald-500" style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />
    </div>
  )
}

function amsUnitName(unitId?: string) {
  const index = parseOptionalInt(unitId)
  if (index === null || index < 0 || index > 25) return `AMS-${unitId ?? '-'}`
  return `AMS-${String.fromCharCode(65 + index)}`
}

function materialLabel(value: { type?: string | null; name?: string | null; filament_id?: string | null }) {
  return value.type || value.name || value.filament_id || 'Unknown'
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

function globalTrayId(unitId?: string, trayId?: string) {
  const unit = parseOptionalInt(unitId)
  const tray = parseOptionalInt(trayId)
  return unit !== null && tray !== null ? unit * 4 + tray : null
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
  const raw = slot.color?.trim().replace(/^#/, '').slice(0, 6)
  return raw && /^[0-9a-fA-F]{6}$/.test(raw) ? `#${raw}` : '#64748b'
}

function colorName(color?: string | null) {
  const raw = color?.trim().replace(/^#/, '').slice(0, 6).toUpperCase()
  if (!raw) return 'Unknown'
  const red = Number.parseInt(raw.slice(0, 2), 16)
  const green = Number.parseInt(raw.slice(2, 4), 16)
  const blue = Number.parseInt(raw.slice(4, 6), 16)
  if (red >= 200 && green >= 100 && green <= 190 && blue < 120) return 'Orange'
  if (green >= red && green >= blue) return 'Green'
  if (red >= green && red >= blue) return 'Red'
  if (blue >= red && blue >= green) return 'Blue'
  return 'Filament'
}

function isActiveSlot(slot: MaterialSlot, active: ActiveMaterialTray) {
  if (!active) return false
  if (active.kind === 'external') {
    return slot.externalId !== null && slot.slotId?.toString() === active.tray_id
  }
  return (
    slot.amsId?.toString() === active.ams_id &&
    slot.slotId?.toString() === active.tray_id
  )
}
