'use client'

import { useRef, useState } from 'react'
import { useTranslations } from 'next-intl'
import {
  BoxIcon,
  BotIcon,
  ClockIcon,
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
import { deletePrinter, refreshPrinterMaterials, updatePrinter } from './actions'
import type { Printer } from './dashboard-types'
import { PrinterAxisControls } from './dashboard-printer-axis-controls'
import { PrinterMaterialsPanel } from './dashboard-printer-materials'
import {
  PrinterControlsPanel,
  PrinterTemperatureControls,
} from './dashboard-printer-temperature-controls'
import { StatusBadge } from './dashboard-ui'
import { ConfirmDialog } from './confirm-dialog'
import { PrinterMismatchWarning } from './printer-mismatch-dialog'
import { PrinterPrintStatus } from './printer-print-status'
import { PrinterLastSeen } from './printer-last-seen'

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

      <PrinterControlsPanel printer={printer} />

      <PrinterAxisControls printer={printer} />

      <PrinterMaterialsPanel printer={printer} />
    </article>
  )
}

function PrinterActions({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const formRef = useRef<HTMLFormElement>(null)
  const [menuOpen, setMenuOpen] = useState(false)
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [editOpen, setEditOpen] = useState(false)

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
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm hover:bg-muted"
            onClick={() => {
              setMenuOpen(false)
              setEditOpen(true)
            }}
            role="menuitem"
            type="button"
          >
            <PencilIcon className="size-4" />
            {t('editPrinter')}
          </button>
          <form action={refreshPrinterMaterials}>
            <input name="tenant_id" type="hidden" value={printer.tenant_id} />
            <input name="printer_id" type="hidden" value={printer.id} />
            <button
              className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm hover:bg-muted"
              onClick={() => setMenuOpen(false)}
              role="menuitem"
              type="submit"
            >
              <RotateCwIcon className="size-4" />
              {t('refreshAms')}
            </button>
          </form>
          <div className="my-1 border-t border-border" />
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
      <Dialog open={editOpen} onOpenChange={setEditOpen}>
        <DialogContent closeLabel={t('closeDialog')}>
          <DialogHeader>
            <DialogTitle>{t('editPrinterTitle')}</DialogTitle>
            <DialogDescription>{t('editPrinterDescription')}</DialogDescription>
          </DialogHeader>
          <form action={updatePrinter} className="grid gap-4">
            <input name="tenant_id" type="hidden" value={printer.tenant_id} />
            <input name="printer_id" type="hidden" value={printer.id} />
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs font-medium text-muted-foreground">{t('editPrinterName')}</span>
              <input
                className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
                defaultValue={printer.name}
                name="name"
                required
                type="text"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs font-medium text-muted-foreground">{t('editPrinterHost')}</span>
              <input
                className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
                name="host"
                type="text"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-xs font-medium text-muted-foreground">{t('editPrinterAccessCode')}</span>
              <input
                autoComplete="off"
                className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
                name="access_code"
                type="password"
              />
            </label>
            <DialogFooter className="-mx-4 -mb-4 mt-2">
              <button
                className="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground hover:bg-primary/80"
                type="submit"
              >
                {t('editPrinterSubmit')}
              </button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  )
}
