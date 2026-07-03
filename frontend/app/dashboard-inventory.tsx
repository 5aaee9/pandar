'use client'

import { useRef, useState, type ReactNode } from 'react'
import { useFormatter, useTranslations } from 'next-intl'
import {
  BoxIcon,
  BotIcon,
  ClockIcon,
  DownloadIcon,
  DropletsIcon,
  MoreVerticalIcon,
  PlusIcon,
  PrinterIcon,
  RotateCwIcon,
  ThermometerIcon,
  TrashIcon,
  UploadIcon,
} from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { FormattedDate } from '../components/formatted-date'
import { controlPrinter, deletePrinter, refreshPrinterMaterials } from './actions'
import { OFFLINE_PRINTER_STATUSES } from './dashboard-attention'
import type { Agent, Job, Printer, Tenant } from './dashboard-types'
import { formatBytes } from './dashboard-format'
import { EmptyState, SectionHeader, StatusBadge } from './dashboard-ui'
import { ConfirmDialog } from './confirm-dialog'
import {
  formatArtifactMetadata,
  formatJobMaterial,
  formatJobRecoveryState,
  formatPrinterMaterials,
} from './dashboard-runtime-helpers'
import { formatLayers, formatProgress, formatRemaining } from './job-format'
import { LinkPrinterMachineForm } from './link-printer-form'

function useLocaleDate() {
  const format = useFormatter()
  return (value: string) => {
    const d = new Date(value)
    if (Number.isNaN(d.getTime())) return value
    return format.dateTime(d, { dateStyle: 'medium', timeStyle: 'short', timeZone: 'UTC' })
  }
}

export function PrinterInventory({
  selectedTenant,
  printers,
  agents,
}: {
  selectedTenant: Tenant | null
  printers: Printer[]
  agents: Agent[]
}) {
  const t = useTranslations('inventory')
  const tMat = useTranslations('material')
  const formatDate = useLocaleDate()
  const [query, setQuery] = useState('')
  const [status, setStatus] = useState('all')
  const normalizedQuery = query.trim().toLowerCase()
  const filtered = printers.filter((printer) => {
    const needsAttention = OFFLINE_PRINTER_STATUSES.has(printer.status.toLowerCase())
    if (status === 'online' && needsAttention) {
      return false
    }
    if (status === 'attention' && !needsAttention) {
      return false
    }
    if (normalizedQuery) {
      const haystack = `${printer.name} ${printer.serial_number}`.toLowerCase()
      if (!haystack.includes(normalizedQuery)) {
        return false
      }
    }
    return true
  })

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-base font-semibold text-foreground">{t('printersTitle')}</h2>
        {selectedTenant && printers.length > 0 ? (
          <LinkPrinterDialog
            agents={agents}
            selectedTenant={selectedTenant}
          />
        ) : null}
      </div>
      {!selectedTenant ? (
        <PrinterEmptyState title={t('noTenantTitle')} message={t('noTenantMessage')} />
      ) : printers.length === 0 ? (
        <PrinterEmptyState
          action={
            <LinkPrinterDialog
              agents={agents}
              selectedTenant={selectedTenant}
            />
          }
          message={t('noPrintersMessage')}
          title={t('noPrintersTitle')}
        />
      ) : (
        <>
          <FilterBar
            query={query}
            onQueryChange={setQuery}
            queryPlaceholder={t('searchName')}
            status={status}
            onStatusChange={setStatus}
            statusOptions={[
              { value: 'all', label: t('filterAll') },
              { value: 'online', label: t('filterOnline') },
              { value: 'attention', label: t('filterAttention') },
            ]}
          />
          {filtered.length === 0 ? (
            <PrinterEmptyState title={t('noMatchesTitle')} message={t('noMatchesMessage')} />
          ) : (
            <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
              {filtered.map((printer) => {
                const material = formatPrinterMaterials(printer, tMat, formatDate)
                const agent = agents.find((candidate) => candidate.id === printer.agent_id)
                return (
                  <PrinterCard
                    agentName={agent?.name ?? t('unknownAgent')}
                    key={printer.id}
                    materialDetail={material.detail}
                    printer={printer}
                  />
                )
              })}
            </div>
          )}
        </>
      )}
    </section>
  )
}

function LinkPrinterDialog({
  selectedTenant,
  agents,
}: {
  selectedTenant: Tenant
  agents: Agent[]
}) {
  const t = useTranslations('linkPrinter')
  return (
    <Dialog>
      <DialogTrigger className="group/button inline-flex h-8 shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg border border-transparent bg-primary bg-clip-padding px-2.5 text-sm font-medium text-primary-foreground outline-none transition-all hover:bg-primary/80 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 active:translate-y-px disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0">
        <PlusIcon className="size-4" />
        {t('submit')}
      </DialogTrigger>
      <DialogContent closeLabel="Close" className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t('title')}</DialogTitle>
          <DialogDescription>{t('subtitleTenant', { name: selectedTenant.display_name })}</DialogDescription>
        </DialogHeader>
        <LinkPrinterMachineForm agents={agents} selectedTenant={selectedTenant} />
      </DialogContent>
    </Dialog>
  )
}

function PrinterEmptyState({
  title,
  message,
  action,
}: {
  title: string
  message: string
  action?: ReactNode
}) {
  return (
    <Empty className="min-h-64 lg:min-h-80">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <PrinterIcon />
        </EmptyMedia>
        <EmptyTitle>{title}</EmptyTitle>
        <EmptyDescription>{message}</EmptyDescription>
      </EmptyHeader>
      {action ? <EmptyContent className="flex-row justify-center gap-2">{action}</EmptyContent> : null}
    </Empty>
  )
}

function PrinterCard({
  printer,
  agentName,
  materialDetail,
}: {
  printer: Printer
  agentName: string
  materialDetail: string
}) {
  const t = useTranslations('inventory')
  return (
    <article
      aria-label={printer.name}
      className="rounded-md border border-border bg-card p-4 text-card-foreground shadow-sm"
    >
      <div className="flex items-start gap-3">
        <div className="flex size-14 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
          <PrinterIcon className="size-7" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <h3 className="truncate text-base font-semibold text-foreground">{printer.name}</h3>
              <p className="truncate text-sm text-muted-foreground">
                {printer.model ?? t('unknownModel')} · {printer.serial_number}
              </p>
            </div>
            <PrinterActions printer={printer} />
          </div>
          <div className="mt-3 flex flex-wrap gap-2">
            <StatusBadge value={printer.status} />
            <span className="inline-flex max-w-full items-center gap-1 rounded-md bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
              <BotIcon className="size-3.5 shrink-0" />
              <span className="truncate">{agentName}</span>
            </span>
            <span className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
              <ClockIcon className="size-3.5" />
              <FormattedDate value={printer.last_seen_at} />
            </span>
          </div>
        </div>
      </div>

      <div className="mt-4 rounded-md bg-muted/60 p-3">
        <div className="flex items-center gap-3">
          <div className="flex size-11 shrink-0 items-center justify-center rounded-md bg-background text-muted-foreground">
            <BoxIcon className="size-5" />
          </div>
          <div className="min-w-0">
            <div className="text-xs font-medium text-muted-foreground">{t('statusLabel')}</div>
            <div className="mt-0.5 text-sm font-medium text-foreground">{printer.status}</div>
            <div className="mt-1 truncate text-xs text-muted-foreground">{materialDetail}</div>
          </div>
        </div>
      </div>

      <PrinterMaterialsPanel printer={printer} />

      <form action={refreshPrinterMaterials} className="mt-4">
        <input name="tenant_id" type="hidden" value={printer.tenant_id} />
        <input name="printer_id" type="hidden" value={printer.id} />
        <Button size="sm" type="submit" variant="outline">
          {t('refreshAms')}
        </Button>
      </form>
    </article>
  )
}

function PrinterMaterialsPanel({ printer }: { printer: Printer }) {
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

function PrinterActions({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const formRef = useRef<HTMLFormElement>(null)
  const [menuOpen, setMenuOpen] = useState(false)
  const [confirmOpen, setConfirmOpen] = useState(false)

  return (
    <div className="relative shrink-0">
      <button
        aria-expanded={menuOpen}
        aria-haspopup="menu"
        aria-label={t('details')}
        className="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
        onClick={() => setMenuOpen((open) => !open)}
        type="button"
      >
        <MoreVerticalIcon className="size-4" />
      </button>
      {menuOpen ? (
        <div
          className="absolute right-0 z-20 mt-1 min-w-36 rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md"
          role="menu"
        >
          <button
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm text-destructive hover:bg-muted"
            onClick={() => {
              setMenuOpen(false)
              setConfirmOpen(true)
            }}
            role="menuitem"
            type="button"
          >
            <TrashIcon className="size-4" />
            {t('deletePrinter')}
          </button>
        </div>
      ) : null}
      <form ref={formRef} action={deletePrinter}>
        <input name="tenant_id" type="hidden" value={printer.tenant_id} />
        <input name="printer_id" type="hidden" value={printer.id} />
      </form>
      <ConfirmDialog
        open={confirmOpen}
        title={t('deletePrinterTitle')}
        message={t('deletePrinterMessage', { name: printer.name })}
        confirmLabel={t('deletePrinterConfirm')}
        tone="danger"
        onConfirm={() => {
          setConfirmOpen(false)
          formRef.current?.requestSubmit()
        }}
        onCancel={() => setConfirmOpen(false)}
      />
    </div>
  )
}

const TERMINAL_JOB_STATUSES = new Set(['completed', 'failed', 'cancelled'])

function jobMatchesStatus(job: Job, status: string): boolean {
  const dispatch = job.status.toLowerCase()
  const physical = job.print.status.toLowerCase()
  if (status === 'active') {
    return !TERMINAL_JOB_STATUSES.has(dispatch) && !TERMINAL_JOB_STATUSES.has(physical)
  }
  if (status === 'failed') {
    return dispatch === 'failed' || physical === 'failed'
  }
  if (status === 'completed') {
    return dispatch === 'completed' || physical === 'completed'
  }
  return true
}

export function JobHistory({
  selectedTenant,
  jobs,
  printers,
  agents,
}: {
  selectedTenant: Tenant | null
  jobs: Job[]
  printers: Printer[]
  agents: Agent[]
}) {
  const t = useTranslations('inventory')
  const [query, setQuery] = useState('')
  const [status, setStatus] = useState('all')
  const normalizedQuery = query.trim().toLowerCase()
  const filtered = jobs.filter((job) => {
    if (!jobMatchesStatus(job, status)) {
      return false
    }
    if (normalizedQuery) {
      const haystack = `${job.artifact.filename} ${job.id}`.toLowerCase()
      if (!haystack.includes(normalizedQuery)) {
        return false
      }
    }
    return true
  })

  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <SectionHeader
        title={t('jobsTitle')}
        subtitle={t('jobsSubtitle')}
        meta={t('jobsMeta', { count: jobs.length })}
      />
      {!selectedTenant ? (
        <EmptyState title={t('jobsNoTenantTitle')} message={t('jobsNoTenantMessage')} />
      ) : jobs.length === 0 ? (
        <EmptyState title={t('jobsEmptyTitle')} message={t('jobsEmptyMessage')} />
      ) : (
        <>
          <FilterBar
            query={query}
            onQueryChange={setQuery}
            queryPlaceholder={t('searchJob')}
            status={status}
            onStatusChange={setStatus}
            statusOptions={[
              { value: 'all', label: t('jobFilterAll') },
              { value: 'active', label: t('jobFilterActive') },
              { value: 'failed', label: t('jobFilterFailed') },
              { value: 'completed', label: t('jobFilterCompleted') },
            ]}
          />
          {filtered.length === 0 ? (
            <EmptyState title={t('jobsNoMatchesTitle')} message={t('jobsNoMatchesMessage')} />
          ) : (
            <ul className="divide-y divide-slate-200" aria-label={t('jobsAria')}>
              {filtered.map((job) => {
                const printer = printers.find((candidate) => candidate.id === job.printer_id)
                const agent = agents.find((candidate) => candidate.id === job.agent_id)
                return (
                  <JobRow
                    key={job.id}
                    job={job}
                    printerName={printer?.name}
                    agentName={agent?.name}
                  />
                )
              })}
            </ul>
          )}
        </>
      )}
    </section>
  )
}

function FilterBar({
  query,
  onQueryChange,
  queryPlaceholder,
  status,
  onStatusChange,
  statusOptions,
}: {
  query: string
  onQueryChange: (value: string) => void
  queryPlaceholder: string
  status: string
  onStatusChange: (value: string) => void
  statusOptions: Array<{ value: string; label: string }>
}) {
  const t = useTranslations('inventory')
  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-slate-200 px-4 py-2">
      <input
        aria-label={queryPlaceholder}
        className="min-w-40 flex-1 rounded-md border border-slate-300 bg-white px-2 py-1 text-sm"
        onChange={(event) => onQueryChange(event.target.value)}
        placeholder={queryPlaceholder}
        type="search"
        value={query}
      />
      <select
        aria-label={t('filterStatusAria')}
        className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm"
        onChange={(event) => onStatusChange(event.target.value)}
        value={status}
      >
        {statusOptions.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  )
}

function JobRow({
  job,
  printerName,
  agentName,
}: {
  job: Job
  printerName?: string
  agentName?: string
}) {
  const t = useTranslations('inventory')
  const tMat = useTranslations('material')
  const tRec = useTranslations('recovery.state')
  const tJf = useTranslations('jobFormat')
  const formatDate = useLocaleDate()
  const format = useFormatter()
  const num = (n: number) => format.number(n)
  const updated = job.print.updated_at ?? job.updated_at
  return (
    <li
      aria-label={`${job.artifact.filename}, ${t('dispatch')} ${job.status}, ${t('print')} ${job.print.status}, ${formatProgress(job)}`}
      className="px-4 py-3"
    >
      <div className="grid gap-3 text-sm xl:grid-cols-[1.4fr_1fr_1fr_1fr]">
        <div className="min-w-0">
          <div className="truncate font-medium text-slate-950">{job.artifact.filename}</div>
          <div className="truncate text-xs text-slate-500">
            {t('updatedPrefix')} <FormattedDate value={updated} />
          </div>
        </div>
        <div className="min-w-0">
          <div className="flex flex-wrap gap-2">
            <StatusPill label={t('dispatch')} value={job.status} />
            <StatusPill label={t('print')} value={job.print.status} />
          </div>
          {job.error ? <div className="mt-1 truncate text-xs text-red-700">{job.error}</div> : null}
          {job.print.error ? <div className="mt-1 truncate text-xs text-red-700">{job.print.error}</div> : null}
        </div>
        <div className="min-w-0 text-xs text-slate-600">
          <div className="truncate font-medium text-slate-900">{printerName ?? t('unknownPrinter')}</div>
          <div className="truncate">{agentName ?? t('unknownAgent')}</div>
        </div>
        <div>
          <div className="font-medium text-slate-900">{formatProgress(job)}</div>
          <div className="text-xs text-slate-600">{formatLayers(job, tJf)}</div>
          <div className="text-xs text-slate-600">{formatRemaining(job.print.remaining_time_minutes, tJf)}</div>
        </div>
      </div>
      <details className="mt-2">
        <summary className="cursor-pointer select-none text-xs font-medium text-slate-500">{t('details')}</summary>
        <div className="mt-2 grid gap-2 text-xs text-slate-600 sm:grid-cols-2 lg:grid-cols-3">
          <div className="sm:col-span-2 lg:col-span-3">
            <span className="text-slate-500">{t('recoveryLabel')} </span>
            {formatJobRecoveryState(job, tRec)}
          </div>
          <div className="sm:col-span-2 lg:col-span-3 truncate">
            <span className="text-slate-500">{t('projectLabel')} </span>
            {formatArtifactMetadata(job, tMat, formatDate)}
          </div>
          <div>
            <span className="text-slate-500">{t('artifactLabel')} </span>
            {job.artifact.content_type} · {formatBytes(job.artifact.size_bytes, num)}
          </div>
          <div>
            <span className="text-slate-500">{t('materialLabel')} </span>
            {formatJobMaterial(job, tMat)}
          </div>
          <div>
            <span className="text-slate-500">{t('jobLabel')} </span>
            <span className="font-mono">{job.id}</span>
          </div>
          {job.print.active_file ? (
            <div className="truncate">
              <span className="text-slate-500">{t('fileLabel')} </span>
              {job.print.active_file}
            </div>
          ) : null}
          {job.print.printer_state ? (
            <div>
              <span className="text-slate-500">{t('stateLabel')} </span>
              {job.print.printer_state}
            </div>
          ) : null}
          <div>
            <span className="text-slate-500">{t('createdLabel')} </span>
            <FormattedDate value={job.created_at} />
          </div>
          <div>
            <span className="text-slate-500">{t('startedLabel')} </span>
            {job.print.started_at ? <FormattedDate value={job.print.started_at} /> : '-'}
          </div>
          <div>
            <span className="text-slate-500">{t('finishedLabel')} </span>
            {job.print.finished_at ? <FormattedDate value={job.print.finished_at} /> : '-'}
          </div>
        </div>
      </details>
    </li>
  )
}

function StatusPill({ label, value }: { label: string; value: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      <span className="text-xs text-slate-500">{label}</span>
      <StatusBadge value={value} />
    </span>
  )
}
