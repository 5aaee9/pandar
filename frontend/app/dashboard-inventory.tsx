'use client'

import { useMemo, type ReactNode } from 'react'
import { useDashboardFilterStore } from './dashboard-filter-store'
import { useFormatter, useTranslations } from 'next-intl'
import { PlusIcon, PrinterIcon } from 'lucide-react'

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
import { formatPrinterMaterials } from './dashboard-runtime-helpers'
import { FilterBar } from './dashboard-filter-bar'
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
  nowMs,
}: {
  selectedTenant: Tenant | null
  printers: Printer[]
  agents: Agent[]
  nowMs: number
}) {
  const t = useTranslations('inventory')
  const tMat = useTranslations('material')
  const formatDate = useLocaleDate()
  const query = useDashboardFilterStore((state) => state.query)
  const status = useDashboardFilterStore((state) => state.status)
  const setQuery = useDashboardFilterStore((state) => state.setQuery)
  const setStatus = useDashboardFilterStore((state) => state.setStatus)
  const normalizedQuery = query.trim().toLowerCase()
  const filtered = useMemo(
    () =>
      printers.filter((printer) => {
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
      }),
    [printers, status, normalizedQuery],
  )
  const agentNames = useMemo(
    () => new Map(agents.map((agent) => [agent.id, agent.name])),
    [agents],
  )

  return (
    <section id="printers" className="scroll-mt-20 space-y-4">
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
                const agentName = agentNames.get(printer.agent_id)
                return (
                  <div key={printer.id} className="content-visibility-auto">
                    <PrinterCard
                      agentName={agentName ?? t('unknownAgent')}
                      materialDetail={material.detail}
                      nowMs={nowMs}
                      printer={printer}
                    />
                  </div>
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
      <DialogTrigger className="group/button inline-flex h-8 shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg border border-transparent bg-primary bg-clip-padding px-2.5 text-sm font-medium text-primary-foreground outline-none transition-[color,background-color,border-color,box-shadow,opacity,transform,translate] duration-[var(--motion-duration-feedback)] ease-out hover:bg-primary/80 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 active:translate-y-px motion-reduce:active:translate-y-0 disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0">
        <PlusIcon className="size-4" />
        {t('submit')}
      </DialogTrigger>
      <DialogContent closeLabel={t('closeDialog')} className="sm:max-w-xl">
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
