'use client'

import { useRef, useState } from 'react'
import { useTranslations } from 'next-intl'
import dynamic from 'next/dynamic'
import { useQueryClient } from '@tanstack/react-query'
import {
  BoxIcon,
  BotIcon,
  ClockIcon,
  Loader2Icon,
  MoreVerticalIcon,
  PencilIcon,
  PrinterIcon,
  RotateCwIcon,
  TrashIcon,
} from 'lucide-react'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import { inputClasses } from '@/lib/utils'
import { deletePrinter, refreshPrinterMaterials, updatePrinter } from './actions'
import type { Printer } from './dashboard-types'
import { useActionStatusFeedback } from './mutation-feedback'
import { routeDataKeys } from './route-data'
import { PrinterAxisControls } from './dashboard-printer-axis-controls'
import { PrinterCoolingSystem } from './dashboard-printer-cooling'
import { PrinterMaterialsPanel } from './dashboard-printer-materials'
import { PrinterRackPanel } from './dashboard-printer-rack'
import { StatusBadge } from './dashboard-ui'
import { ConfirmDialog } from './confirm-dialog'
import { PrinterMismatchWarning } from './printer-mismatch-dialog'
import { PrinterPrintStatus } from './printer-print-status'
import { PrinterLastSeen } from './printer-last-seen'

const PrinterTemperatureControls = dynamic(
  () =>
    import('./dashboard-printer-temperature-controls').then(
      (mod) => mod.PrinterTemperatureControls,
    ),
  {
    loading: () => (
      <div className="mt-4 grid grid-cols-2 gap-2 lg:grid-cols-[1fr_1fr_1fr_5rem]">
        <div className="h-16 animate-pulse rounded-md bg-muted/50" />
        <div className="h-16 animate-pulse rounded-md bg-muted/50" />
        <div className="h-16 animate-pulse rounded-md bg-muted/50" />
        <div className="h-16 animate-pulse rounded-md bg-muted/50" />
      </div>
    ),
  },
)

const PrinterControlsPanel = dynamic(
  () =>
    import('./dashboard-printer-temperature-controls').then(
      (mod) => mod.PrinterControlsPanel,
    ),
  {
    loading: () => (
      <div className="mt-4 space-y-2">
        <div className="h-3 w-16 animate-pulse rounded bg-muted" />
        <div className="grid grid-cols-2 gap-2">
          <div className="h-8 animate-pulse rounded-md bg-muted/50" />
          <div className="h-8 animate-pulse rounded-md bg-muted/50" />
          <div className="h-8 animate-pulse rounded-md bg-muted/50" />
          <div className="h-8 animate-pulse rounded-md bg-muted/50" />
        </div>
      </div>
    ),
  },
)

export function PrinterCard({
  printer,
  agentName,
  materialDetail,
  nowMs,
}: {
  printer: Printer
  agentName: string
  materialDetail: string
  nowMs: number
}) {
  const t = useTranslations('inventory')
  return (
    <article
      aria-label={printer.name}
      className="rounded-md border border-border bg-card p-4 text-card-foreground"
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
                {printer.model ?? t('unknownModel')} · <span className="font-mono">{printer.serial_number}</span>
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
              <PrinterLastSeen nowMs={nowMs} value={printer.last_seen_at} />
            </span>
          </div>
        </div>
      </div>

      <div className="mt-4 rounded-md bg-muted/60 p-3">
        <div className="flex items-center gap-3">
          <div className="flex size-11 shrink-0 items-center justify-center rounded-md bg-background text-muted-foreground">
            <BoxIcon className="size-5" />
          </div>
          <div className="min-w-0 flex-1">
            <PrinterPrintStatus
              coarseStatus={printer.status}
              print={printer.print ?? null}
            />
            <div className="mt-1 truncate text-xs text-muted-foreground">{materialDetail}</div>
          </div>
        </div>
      </div>

      <PrinterMismatchWarning printer={printer} />

      <PrinterTemperatureControls printer={printer} />

      <PrinterCoolingSystem printer={printer} />

      <PrinterControlsPanel printer={printer} />

      <PrinterAxisControls printer={printer} />

      <PrinterMaterialsPanel printer={printer} />

      <PrinterRackPanel printer={printer} />
    </article>
  )
}

function PrinterActions({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const formRef = useRef<HTMLFormElement>(null)
  const [menuOpen, setMenuOpen] = useState(false)
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [editOpen, setEditOpen] = useState(false)
  const queryClient = useQueryClient()

  const refreshAction = useActionStatusFeedback(
    refreshPrinterMaterials,
    'materials_refresh_queued',
  )
  const deleteAction = useActionStatusFeedback(deletePrinter, 'printer_deleted', () => {
    setConfirmOpen(false)
    void queryClient.invalidateQueries({
      queryKey: routeDataKeys.devices(printer.tenant_id),
    })
  })
  const editAction = useActionStatusFeedback(updatePrinter, 'printer_updated', () => {
    setEditOpen(false)
    void queryClient.invalidateQueries({
      queryKey: routeDataKeys.devices(printer.tenant_id),
    })
  })

  return (
    <div className="shrink-0">
      <Popover open={menuOpen} onOpenChange={setMenuOpen}>
        <PopoverTrigger
          render={
            <Button
              aria-haspopup="dialog"
              aria-label={t('detailsFor', { name: printer.name })}
              className="text-muted-foreground hover:text-foreground"
              size="icon-xs"
              type="button"
              variant="ghost"
            />
          }
        >
          <MoreVerticalIcon className="size-4" />
        </PopoverTrigger>
        <PopoverContent
          align="end"
          className="w-auto min-w-36 gap-0 p-1"
          side="bottom"
          sideOffset={4}
        >
          <Button
            className="h-auto w-full justify-start gap-2 rounded-sm px-2 py-1.5 font-normal"
            onClick={() => {
              setMenuOpen(false)
              setEditOpen(true)
            }}
            type="button"
            variant="ghost"
          >
            <PencilIcon className="size-4" />
            {t('editPrinter')}
          </Button>
          <form action={refreshAction.formAction}>
            <input name="tenant_id" type="hidden" value={printer.tenant_id} />
            <input name="printer_id" type="hidden" value={printer.id} />
            <Button
              className="h-auto w-full justify-start gap-2 rounded-sm px-2 py-1.5 font-normal"
              disabled={refreshAction.pending}
              onClick={() => setMenuOpen(false)}
              type="submit"
              variant="ghost"
            >
              {refreshAction.pending ? (
                <Loader2Icon className="size-4 animate-spin" />
              ) : (
                <RotateCwIcon className="size-4" />
              )}
              {t('refreshAms')}
            </Button>
          </form>
          <div className="my-1 border-t border-border" />
          <Button
            className="h-auto w-full justify-start gap-2 rounded-sm px-2 py-1.5 font-normal text-destructive hover:text-destructive"
            onClick={() => {
              setMenuOpen(false)
              setConfirmOpen(true)
            }}
            type="button"
            variant="ghost"
          >
            <TrashIcon className="size-4" />
            {t('deletePrinter')}
          </Button>
        </PopoverContent>
      </Popover>
      <form ref={formRef} action={deleteAction.formAction}>
        <input name="tenant_id" type="hidden" value={printer.tenant_id} />
        <input name="printer_id" type="hidden" value={printer.id} />
      </form>
      <ConfirmDialog
        open={confirmOpen}
        title={t('deletePrinterTitle')}
        message={t('deletePrinterMessage', { name: printer.name })}
        confirmLabel={t('deletePrinterConfirm')}
        tone="danger"
        pending={deleteAction.pending}
        onConfirm={() => {
          formRef.current?.requestSubmit()
        }}
        onCancel={() => setConfirmOpen(false)}
      />
      <Dialog open={editOpen} onOpenChange={setEditOpen}>
        <DialogContent closeLabel={t('closeDialog')}>
          <DialogHeader>
            <DialogTitle>{t('editPrinterTitle')}</DialogTitle>
            <DialogDescription>{t('editPrinterDescription')}</DialogDescription>
          </DialogHeader>
          <form action={editAction.formAction} className="grid gap-4">
            <input name="tenant_id" type="hidden" value={printer.tenant_id} />
            <input name="printer_id" type="hidden" value={printer.id} />
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs font-medium text-muted-foreground">{t('editPrinterName')}</span>
              <input
                className={inputClasses}
                defaultValue={printer.name}
                name="name"
                required
                type="text"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs font-medium text-muted-foreground">{t('editPrinterHost')}</span>
              <input
                className={inputClasses}
                name="host"
                type="text"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs font-medium text-muted-foreground">{t('editPrinterAccessCode')}</span>
              <input
                autoComplete="off"
                className={inputClasses}
                name="access_code"
                type="password"
              />
            </label>
            <DialogFooter className="-mx-4 -mb-4 mt-2">
              <Button disabled={editAction.pending} size="lg" type="submit">
                {editAction.pending ? <Loader2Icon className="animate-spin" /> : null}
                {t('editPrinterSubmit')}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  )
}
