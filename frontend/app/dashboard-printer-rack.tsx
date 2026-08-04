'use client'

import type { ReactNode } from 'react'
import { useTranslations } from 'next-intl'
import { CheckCircle2Icon, RotateCwIcon } from 'lucide-react'

import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'

import { controlPrinter } from './actions'
import { ConfirmForm } from './confirm-dialog'
import type { Printer, PrinterNozzleSystem } from './dashboard-types'

const RACK_SLOT_IDS = [16, 17, 18, 19, 20, 21] as const
const RACK_NOZZLE_ID_ALL = 255

type RackNozzle = PrinterNozzleSystem['nozzle']['info'][number]
type Translate = ReturnType<typeof useTranslations>

export function PrinterRackPanel({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const system = printer.nozzle_system
  if (!system || system.nozzle.info.length === 0) {
    return null
  }

  const mounted = system.nozzle.info.filter((nozzle) => nozzle.id < 16)
  const rackNozzles = new Map(
    system.nozzle.info
      .filter((nozzle) => nozzle.id >= 16)
      .map((nozzle) => [nozzle.id, nozzle]),
  )
  const disabled = rackOperationsDisabled(printer)

  return (
    <div className="mt-4 space-y-2">
      <div className="flex items-center justify-between gap-2">
        <div className="text-xs font-medium text-muted-foreground">{t('rackLabel')}</div>
        <RackHolderStatus holder={system.holder} />
      </div>
      {mounted.length > 0 ? (
        <div className="rounded-md bg-muted/40 p-2">
          <div className="mb-1.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
            {t('rackFixed')}
          </div>
          <div className="flex flex-wrap gap-1.5">
            {mounted.map((nozzle) => (
              <span
                className="inline-flex items-center rounded-md bg-background px-2 py-1 text-xs font-medium text-foreground"
                key={nozzle.id}
              >
                {nozzleLabel(nozzle, t, t('rackHotendUnknown'))}
              </span>
            ))}
          </div>
        </div>
      ) : null}
      <div>
        <div className="mb-1.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
          {t('rackSwappable')}
        </div>
        <div className="grid grid-cols-3 gap-1.5 sm:grid-cols-6">
          {RACK_SLOT_IDS.map((slotId, index) => (
            <RackSlotButton
              disabled={disabled}
              key={slotId}
              nozzle={rackNozzles.get(slotId) ?? null}
              printer={printer}
              slotId={slotId}
              slotNumber={index + 1}
            />
          ))}
        </div>
      </div>
      <div className="grid grid-cols-3 gap-1.5" role="group">
        <RackMoveButton action={0} disabled={disabled} label={t('rackMoveCentre')} printer={printer} />
        <RackMoveButton action={1} disabled={disabled} label={t('rackMoveATop')} printer={printer} />
        <RackMoveButton action={2} disabled={disabled} label={t('rackMoveBTop')} printer={printer} />
      </div>
      <div className="grid grid-cols-2 gap-1.5" role="group">
        <RackOperationForm
          action="holder_nozzle_refresh"
          confirm
          disabled={disabled}
          icon={<RotateCwIcon />}
          label={t('rackRereadAll')}
          nozzleId={RACK_NOZZLE_ID_ALL}
          printer={printer}
        />
        <RackOperationForm
          action="nozzle_info_confirm"
          disabled={disabled}
          icon={<CheckCircle2Icon />}
          label={t('rackConfirmAll')}
          nozzleId={RACK_NOZZLE_ID_ALL}
          printer={printer}
        />
      </div>
    </div>
  )
}

function RackHolderStatus({ holder }: { holder: PrinterNozzleSystem['holder'] }) {
  const t = useTranslations('inventory')
  if (!holder) {
    return null
  }
  const parts: string[] = []
  if (holder.pos !== null && holder.pos !== undefined) {
    parts.push(
      holder.pos === 1
        ? t('rackPositionATop')
        : holder.pos === 2
          ? t('rackPositionBTop')
          : holder.pos === 3
            ? t('rackPositionCentre')
            : t('rackPositionUnknown'),
    )
  }
  if (holder.stat !== null && holder.stat !== undefined && holder.stat > 0) {
    parts.push(t('rackStatusBusy'))
  }
  if (holder.info === 0) {
    parts.push(t('rackNotCalibrated'))
  } else if (holder.info === 1) {
    parts.push(t('rackCalibrated'))
  }
  if (parts.length === 0) {
    return null
  }
  return <div className="text-xs text-muted-foreground">{parts.join(' · ')}</div>
}

function RackSlotButton({
  printer,
  slotId,
  slotNumber,
  nozzle,
  disabled,
}: {
  printer: Printer
  slotId: number
  slotNumber: number
  nozzle: RackNozzle | null
  disabled: boolean
}) {
  const t = useTranslations('inventory')
  const label = nozzle ? nozzleLabel(nozzle, t, t('rackHotendUnknown')) : t('rackSlotEmpty')
  const flow = nozzle ? nozzleFlowLabel(nozzle.type, t) : null
  return (
    <Popover>
      <PopoverTrigger
        aria-label={t('rackSlotAria', { number: slotNumber, hotend: label })}
        className="flex min-h-12 w-full flex-col items-center justify-center rounded-md bg-muted/50 px-1 py-1.5 text-center transition hover:bg-muted focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
        type="button"
      >
        <span className="text-[10px] font-medium text-muted-foreground">{slotNumber}</span>
        <span className="w-full truncate text-xs font-semibold text-foreground" title={label}>
          {nozzle ? nozzleDiameterLabel(nozzle) : label}
        </span>
        {nozzle ? (
          <span className="w-full truncate text-[10px] text-muted-foreground" title={label}>
            {nozzleMaterialLabel(nozzle.type, t)}
          </span>
        ) : null}
      </PopoverTrigger>
      <PopoverContent align="start" className="w-52 gap-0 p-2">
        <div className="rounded-sm bg-muted px-2 py-2 text-xs font-medium text-foreground">
          {t('rackSlotAria', { number: slotNumber, hotend: label })}
        </div>
        <div className="grid gap-1 px-2 py-2 text-xs">
          {flow ? (
            <div className="flex justify-between gap-3">
              <span className="text-muted-foreground">{t('rackFlowType')}</span>
              <span className="font-medium">{flow}</span>
            </div>
          ) : null}
          {nozzle?.type ? (
            <div className="flex justify-between gap-3">
              <span className="text-muted-foreground">{t('rackHotendCode')}</span>
              <span className="font-mono font-medium">{nozzle.type}</span>
            </div>
          ) : null}
          {nozzle?.wear !== null && nozzle?.wear !== undefined ? (
            <div className="flex justify-between gap-3">
              <span className="text-muted-foreground">{t('rackWear')}</span>
              <span className="font-medium">{formatWear(nozzle.wear)}</span>
            </div>
          ) : null}
        </div>
        {nozzle ? (
          <div className="grid gap-1">
            <RackOperationForm
              action="holder_nozzle_refresh"
              confirm
              disabled={disabled}
              icon={<RotateCwIcon />}
              label={t('rackRereadSlot')}
              nozzleId={slotId}
              printer={printer}
            />
            <RackOperationForm
              action="nozzle_info_confirm"
              disabled={disabled}
              icon={<CheckCircle2Icon />}
              label={t('rackConfirmSlot')}
              nozzleId={slotId}
              printer={printer}
            />
          </div>
        ) : null}
      </PopoverContent>
    </Popover>
  )
}

function RackMoveButton({
  printer,
  action,
  label,
  disabled,
}: {
  printer: Printer
  action: number
  label: string
  disabled: boolean
}) {
  const t = useTranslations('inventory')
  return (
    <ConfirmForm
      action={controlPrinter}
      buttonAriaLabel={label}
      buttonClassName="inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-md px-2 text-sm font-semibold transition-colors duration-150 ease-out disabled:bg-muted/60 disabled:text-muted-foreground enabled:bg-primary/10 enabled:text-primary enabled:hover:bg-primary/15 dark:enabled:bg-primary/20"
      buttonLabel={label}
      confirmLabel={label}
      disabled={disabled}
      message={t('rackMoveWarningMessage')}
      title={t('rackMoveWarningTitle')}
      tone="default"
    >
      <input name="tenant_id" type="hidden" value={printer.tenant_id} />
      <input name="printer_id" type="hidden" value={printer.id} />
      <input name="action" type="hidden" value="nozzle_holder_ctrl" />
      <input name="holder_action" type="hidden" value={action} />
    </ConfirmForm>
  )
}

function RackOperationForm({
  printer,
  action,
  nozzleId,
  label,
  icon,
  disabled,
  confirm,
}: {
  printer: Printer
  action: 'holder_nozzle_refresh' | 'nozzle_info_confirm'
  nozzleId: number
  label: string
  icon: ReactNode
  disabled: boolean
  confirm?: boolean
}) {
  const t = useTranslations('inventory')
  const fields = (
    <>
      <input name="tenant_id" type="hidden" value={printer.tenant_id} />
      <input name="printer_id" type="hidden" value={printer.id} />
      <input name="action" type="hidden" value={action} />
      <input name="nozzle_id" type="hidden" value={nozzleId} />
    </>
  )
  if (confirm) {
    return (
      <ConfirmForm
        action={controlPrinter}
        buttonAriaLabel={label}
        buttonClassName="inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-md px-2 text-sm font-semibold transition-colors duration-150 ease-out disabled:bg-muted/60 disabled:text-muted-foreground enabled:bg-primary/10 enabled:text-primary enabled:hover:bg-primary/15 dark:enabled:bg-primary/20 [&_svg]:size-4"
        buttonLabel={<>{icon}{label}</>}
        confirmLabel={label}
        disabled={disabled}
        message={t('rackMoveWarningMessage')}
        title={t('rackMoveWarningTitle')}
        tone="default"
      >
        {fields}
      </ConfirmForm>
    )
  }
  return (
    <form action={controlPrinter}>
      {fields}
      <button
        className="flex h-8 w-full items-center justify-center gap-1.5 rounded-md px-2 text-sm font-semibold transition-colors duration-150 ease-out disabled:bg-muted/60 disabled:text-muted-foreground enabled:bg-primary/10 enabled:text-primary enabled:hover:bg-primary/15 dark:enabled:bg-primary/20 [&_svg]:size-4"
        disabled={disabled}
        type="submit"
      >
        {icon}
        {label}
      </button>
    </form>
  )
}

function nozzleLabel(nozzle: RackNozzle, t: Translate, fallback: string) {
  return [nozzleDiameterLabel(nozzle), nozzleMaterialLabel(nozzle.type, t)]
    .filter(Boolean)
    .join(' ') || fallback
}

function nozzleDiameterLabel(nozzle: RackNozzle) {
  return Number.isFinite(nozzle.diameter)
    ? `${Number(nozzle.diameter.toFixed(2))} mm`
    : null
}

// Maps hotend type codes to material display names. Codes are either full
// text ("hardened_steel") or 4-character codes ("HS01", "XH05") whose last
// two digits carry the material: 00 stainless, 01 hardened, 05 tungsten.
function nozzleMaterialLabel(type: string | null | undefined, t: Translate): string | null {
  const raw = type?.trim()
  if (!raw) {
    return null
  }
  const lower = raw.toLowerCase()
  if (lower.includes('hardened')) return t('nozzleHardenedSteel')
  if (lower.includes('stainless')) return t('nozzleStainlessSteel')
  if (lower.includes('tungsten')) return t('nozzleTungstenCarbide')
  if (lower.includes('brass')) return t('nozzleBrass')
  if (raw.length >= 4) {
    const material = raw.slice(2, 4)
    if (material === '00') return t('nozzleStainlessSteel')
    if (material === '01') return t('nozzleHardenedSteel')
    if (material === '05') return t('nozzleTungstenCarbide')
  }
  if (raw === '00') return t('nozzleStainlessSteel')
  if (raw === '01') return t('nozzleHardenedSteel')
  if (raw === '05') return t('nozzleTungstenCarbide')
  return raw
}

// Flow type from the hotend code: "HH" high flow, "HS" standard; otherwise
// the second character follows Bambu Studio's map (A/X standard, E high,
// U TPU high, B E3D high). Returns null when the code carries no flow data.
function nozzleFlowLabel(type: string | null | undefined, t: Translate): string | null {
  const raw = type?.trim()
  if (!raw || raw.length < 2) {
    return null
  }
  if (raw.startsWith('HH')) return t('nozzleHighFlow')
  if (raw.startsWith('HS')) return t('nozzleStandardFlow')
  switch (raw.charAt(1)) {
    case 'A':
    case 'X':
      return t('nozzleStandardFlow')
    case 'E':
      return t('nozzleHighFlow')
    case 'U':
      return t('nozzleTpuHighFlow')
    case 'B':
      return t('nozzleE3dHighFlow')
    default:
      return null
  }
}

function formatWear(wear: number) {
  if (!Number.isFinite(wear)) {
    return '-'
  }
  return wear <= 1 ? `${Math.round(wear * 100)}%` : wear.toFixed(2)
}

function rackOperationsDisabled(printer: Printer) {
  const coarseStatus = printer.status.toLowerCase()
  if (['offline', 'failed'].includes(coarseStatus)) {
    return true
  }
  const printState = printer.print?.gcode_state?.toLowerCase() ?? coarseStatus
  return ['running', 'printing', 'paused', 'pause'].includes(printState)
}
