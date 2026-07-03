'use client'

import { useState, type ReactNode } from 'react'
import { useFormatter, useTranslations } from 'next-intl'
import { PlusIcon, PrinterIcon } from 'lucide-react'

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
import { OFFLINE_PRINTER_STATUSES } from './dashboard-attention'
import type { Agent, Printer, Tenant } from './dashboard-types'
import { EmptyState } from './dashboard-ui'
import { formatPrinterMaterials } from './dashboard-runtime-helpers'
import { FilterBar } from './dashboard-job-history'
import { PrinterCard } from './dashboard-printer-card'
import { LinkPrinterMachineForm } from './link-printer-form'

export { JobHistory } from './dashboard-job-history'

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
