import { useTranslations } from 'next-intl'
import { FlameIcon, Loader2Icon } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '../components/ui/popover'
import type { Printer } from './dashboard-types'
import {
  PrinterControlFields,
  printerControlFieldNames,
  usePrinterControl,
} from './printer-controls'

export type DryingProps = {
  amsId: number
  active: boolean
  remainingMinutes: number | null
  filamentTypes: string[]
}

// Studio DevAms::DryStatus: 1=checking, 2=drying, 3=cooling, 4=stopping.
const ACTIVE_DRY_STATUSES = new Set([1, 2, 3, 4])

export function dryingProps(
  unit: NonNullable<Printer['materials']>['ams_units'][number],
): DryingProps | null {
  if (unit.unit_kind !== 'ams_2_pro' && unit.unit_kind !== 'ams_ht') return null
  const amsId = intValue(unit.unit_id)
  if (amsId === null) return null
  const status = intValue(unit.dry_status)
  const remaining = intValue(unit.dry_time_minutes)
  const filamentTypes = [
    ...new Set(
      (unit.trays ?? [])
        .filter((tray) => tray.exists !== false)
        .map((tray) => tray.type?.trim())
        .filter((type): type is string => Boolean(type)),
    ),
  ]
  return {
    amsId,
    active: status !== null && ACTIVE_DRY_STATUSES.has(status),
    remainingMinutes: remaining !== null && remaining > 0 ? remaining : null,
    filamentTypes,
  }
}

export function DryingControl({ printer, drying }: { printer: Printer; drying: DryingProps }) {
  const t = useTranslations('inventory')
  if (drying.active) {
    return (
      <span className="inline-flex items-center gap-2">
        <span className="inline-flex items-center gap-1 text-warning">
          <FlameIcon aria-hidden="true" className="size-3" />
          {drying.remainingMinutes !== null
            ? t('dryingRemaining', { time: formatDryTime(drying.remainingMinutes) })
            : t('dryingActive')}
        </span>
        <StopDryingForm amsId={drying.amsId} printer={printer} />
      </span>
    )
  }
  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            aria-label={t('dryFilament')}
            className="h-auto gap-1 rounded-sm px-1.5 py-0.5 text-xs hover:bg-accent dark:hover:bg-accent [&_svg]:size-3"
            type="button"
            variant="ghost"
          />
        }
      >
        <FlameIcon aria-hidden="true" />
        {t('dryFilament')}
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64 p-3">
        <StartDryingForm drying={drying} printer={printer} />
      </PopoverContent>
    </Popover>
  )
}

function StopDryingForm({ printer, amsId }: { printer: Printer; amsId: number }) {
  const t = useTranslations('inventory')
  const { formAction, pending } = usePrinterControl()
  return (
    <form action={formAction}>
      <PrinterControlFields printer={printer} intent={{ action: 'ams_stop_drying', amsId }} />
      <Button
        className="h-auto gap-1 rounded-sm px-1.5 py-0.5 text-xs hover:bg-accent hover:underline disabled:text-muted-foreground"
        disabled={pending}
        type="submit"
        variant="ghost"
      >
        {pending ? <Loader2Icon aria-hidden="true" className="size-3 animate-spin" /> : null}
        {t('cancelDrying')}
      </Button>
    </form>
  )
}

function StartDryingForm({ printer, drying }: { printer: Printer; drying: DryingProps }) {
  const t = useTranslations('inventory')
  const { formAction, pending } = usePrinterControl()
  return (
    <form action={formAction} className="space-y-2">
      <PrinterControlFields printer={printer} intent={{ action: 'ams_start_drying', amsId: drying.amsId }} />
      <label className="grid gap-1 text-xs">
        <span className="text-muted-foreground">{t('dryingFilament')}</span>
        {drying.filamentTypes.length > 0 ? (
          <select
            className="h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
            name={printerControlFieldNames.filament}
          >
            {drying.filamentTypes.map((type) => (
              <option key={type} value={type}>
                {type}
              </option>
            ))}
          </select>
        ) : (
          <Input defaultValue="PLA" name={printerControlFieldNames.filament} required />
        )}
      </label>
      <div className="grid grid-cols-2 gap-2">
        <label className="grid gap-1 text-xs">
          <span className="text-muted-foreground">{t('dryingTemperature')}</span>
          <Input
            defaultValue={55}
            inputMode="numeric"
            max={85}
            min={45}
            name={printerControlFieldNames.temperatureCelsius}
            required
            type="number"
          />
        </label>
        <label className="grid gap-1 text-xs">
          <span className="text-muted-foreground">{t('dryingDuration')}</span>
          <Input
            defaultValue={8}
            inputMode="numeric"
            max={24}
            min={1}
            name={printerControlFieldNames.durationHours}
            required
            type="number"
          />
        </label>
      </div>
      <label className="flex items-center gap-2 text-xs text-foreground">
        <input name={printerControlFieldNames.rotateTray} type="checkbox" />
        {t('dryingRotateTray')}
      </label>
      <Button className="w-full" disabled={pending} size="sm" type="submit">
        {pending ? <Loader2Icon className="animate-spin" /> : null}
        {t('startDrying')}
      </Button>
    </form>
  )
}

function formatDryTime(minutes: number) {
  const hours = Math.floor(minutes / 60)
  const mins = minutes % 60
  return hours > 0 ? `${hours}h ${mins}m` : `${mins}m`
}

function intValue(value?: string | number | null) {
  if (value === undefined || value === null) return null
  if (typeof value === 'string' && value.trim() === '') return null
  const parsed = Number(value)
  return Number.isInteger(parsed) ? parsed : null
}
