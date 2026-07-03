'use client'

import { useRef, useState } from 'react'
import { useTranslations } from 'next-intl'
import {
  BoxIcon,
  BotIcon,
  ClockIcon,
  MoreVerticalIcon,
  PrinterIcon,
  RotateCwIcon,
  TrashIcon,
} from 'lucide-react'

import { FormattedDate } from '../components/formatted-date'
import { deletePrinter, refreshPrinterMaterials } from './actions'
import type { Printer } from './dashboard-types'
import { PrinterMaterialsPanel } from './dashboard-printer-materials'
import { PrinterTemperatureControls } from './dashboard-printer-temperature-controls'
import { StatusBadge } from './dashboard-ui'
import { ConfirmDialog } from './confirm-dialog'
export function PrinterCard({
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

      <PrinterTemperatureControls printer={printer} />

      <PrinterMaterialsPanel printer={printer} />
    </article>
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
    </div>
  )
}
