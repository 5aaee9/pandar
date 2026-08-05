'use client'

import { useState } from 'react'
import { useTranslations } from 'next-intl'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { isTerminalCommandStatus } from './command-status'
import { EmptyState, SectionHeader, Tag } from './dashboard-ui'
import { LinkPrinterMachineForm } from './link-printer-form'
import type {
  Agent,
  Command,
  DiscoveredPrinter,
  DiscoveryResultData,
  Printer,
  Tenant,
} from './dashboard-types'

export function DiscoverySection({
  selectedTenant,
  agents,
  printers,
  command,
  data,
}: {
  selectedTenant: Tenant
  agents: Agent[]
  printers: Printer[]
  command: Command
  data: DiscoveryResultData | null
}) {
  const t = useTranslations('discovery')
  const [adoptTarget, setAdoptTarget] = useState<DiscoveredPrinter | null>(null)
  const agent = agents.find((candidate) => candidate.id === command.agent_id)
  const agentName = agent?.name ?? command.agent_id
  const linkedBySerial = new Map(
    printers.map((printer) => [printer.serial_number, printer] as const),
  )
  const pending = !isTerminalCommandStatus(command.status)
  const failed =
    command.status === 'failed' || command.status === 'cancelled'

  return (
    <section className="overflow-hidden rounded-md border border-border bg-card">
      <SectionHeader
        meta={command.id}
        subtitle={t('subtitle', { agent: agentName, status: command.status })}
        title={t('title')}
      />

      {pending ? (
        <div className="flex items-center gap-3 px-4 py-8">
          <div className="h-5 w-5 shrink-0 animate-spin rounded-full border-2 border-muted border-t-primary" />
          <div>
            <div className="text-sm font-medium text-foreground">
              {t('pendingTitle')}
            </div>
            <p className="mt-0.5 text-sm text-muted-foreground">
              {t('pendingMessage', { agent: agentName })}
            </p>
          </div>
        </div>
      ) : failed ? (
        <EmptyState
          message={command.error ?? command.status}
          title={t('failedTitle')}
        />
      ) : !data || data.printers.length === 0 ? (
        <EmptyState message={t('emptyMessage')} title={t('emptyTitle')} />
      ) : (
        <div className="overflow-x-auto">
          <table className="min-w-full border-collapse text-left text-sm">
            <thead className="bg-muted/60 text-xs font-semibold text-muted-foreground">
              <tr>
                <th className="px-4 py-2">{t('colName')}</th>
                <th className="px-4 py-2">{t('colSerial')}</th>
                <th className="px-4 py-2">{t('colHost')}</th>
                <th className="px-4 py-2">{t('colModel')}</th>
                <th className="px-4 py-2">{t('colAction')}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {data.printers.map((printer) => {
                const linked = printer.serial_number
                  ? linkedBySerial.get(printer.serial_number)
                  : undefined
                return (
                  <tr key={`${printer.serial_number ?? 'unknown'}-${printer.host}`}>
                    <td className="px-4 py-3 font-medium text-foreground">
                      {printer.name ?? '-'}
                    </td>
                    <td className="px-4 py-3 font-mono text-xs text-muted-foreground">
                      {printer.serial_number ?? '-'}
                    </td>
                    <td className="px-4 py-3 font-mono text-xs text-muted-foreground">
                      {printer.host}
                    </td>
                    <td className="px-4 py-3 text-muted-foreground">
                      {printer.model ?? '-'}
                    </td>
                    <td className="px-4 py-3">
                      {linked ? (
                        <span className="inline-flex items-center gap-2">
                          <Tag tone="success" value={t('linked')} />
                          <span className="text-xs text-muted-foreground">
                            {linked.name}
                          </span>
                        </span>
                      ) : (
                        <Button
                          aria-label={t('adoptFor', {
                            name: printer.name ?? printer.host,
                          })}
                          onClick={() => setAdoptTarget(printer)}
                          size="sm"
                          type="button"
                          variant="outline"
                        >
                          {t('adopt')}
                        </Button>
                      )}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}

      <AdoptPrinterDialog
        agentName={agentName}
        command={command}
        onOpenChange={(open) => {
          if (!open) setAdoptTarget(null)
        }}
        open={adoptTarget !== null}
        target={adoptTarget}
        tenant={selectedTenant}
      />
    </section>
  )
}

function AdoptPrinterDialog({
  tenant,
  command,
  agentName,
  target,
  open,
  onOpenChange,
}: {
  tenant: Tenant
  command: Command
  agentName: string
  target: DiscoveredPrinter | null
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const t = useTranslations('discovery')

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent closeLabel={t('adoptClose')}>
        {target ? (
          <>
            <DialogHeader>
              <DialogTitle>{t('adoptTitle')}</DialogTitle>
              <DialogDescription>
                {t('adoptDescription', { agent: agentName, host: target.host })}
              </DialogDescription>
            </DialogHeader>
            <LinkPrinterMachineForm
              agents={[]}
              defaultHost={target.host}
              defaultName={target.name ?? ''}
              fixedAgentId={command.agent_id}
              key={`${target.serial_number ?? 'unknown'}-${target.host}`}
              onLinked={() => onOpenChange(false)}
              selectedTenant={tenant}
              submitLabel={t('adoptSubmit')}
            />
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}
