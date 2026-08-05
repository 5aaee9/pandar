'use client'

import { useState } from 'react'
import Link from 'next/link'
import { useTranslations } from 'next-intl'
import { PlusIcon } from 'lucide-react'

import { FormattedDate } from '../components/formatted-date'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from '@/components/ui/empty'
import { inputClasses } from '@/lib/utils'
import { AgentDeleteForm } from './agent-delete-form'
import { AgentPairingDialog } from './agent-pairing-dialog'
import { deleteAgent, discoverPrinters, refreshPrinters } from './actions'
import { agentSettingsHref } from './dashboard-shell'
import { SectionHeader, StatusBadge } from './dashboard-ui'
import type { Agent, Printer, Tenant } from './dashboard-types'

export function AgentsSection({
  selectedTenant,
  agents,
  printers,
  adminUnavailable,
}: {
  selectedTenant: Tenant | null
  agents: Agent[]
  printers: Printer[]
  adminUnavailable: boolean
}) {
  const t = useTranslations('agents')
  const [pairOpen, setPairOpen] = useState(false)
  const [discoverAgent, setDiscoverAgent] = useState<Agent | null>(null)
  const printerCounts = new Map<string, number>()
  for (const printer of printers) {
    printerCounts.set(
      printer.agent_id,
      (printerCounts.get(printer.agent_id) ?? 0) + 1,
    )
  }

  const canPair = selectedTenant !== null && !adminUnavailable
  const pairButton = canPair ? (
    <Button onClick={() => setPairOpen(true)} size="sm" type="button">
      <PlusIcon aria-hidden="true" />
      {t('pairAgent')}
    </Button>
  ) : null

  return (
    <section className="overflow-hidden rounded-md border border-border bg-card">
      <SectionHeader
        actions={pairButton}
        meta={t('meta', { count: agents.length })}
        subtitle={
          selectedTenant
            ? t('subtitleTenant', {
                name: selectedTenant.display_name,
                slug: selectedTenant.slug,
              })
            : t('subtitleNone')
        }
        title={t('title')}
      />

      {!selectedTenant ? (
        <Empty className="py-12">
          <EmptyHeader>
            <EmptyTitle>{t('noTenantTitle')}</EmptyTitle>
            <EmptyDescription>{t('noTenantMessage')}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : agents.length === 0 ? (
        <Empty className="py-12">
          <EmptyHeader>
            <EmptyTitle>{t('noAgentsTitle')}</EmptyTitle>
            <EmptyDescription>
              {adminUnavailable
                ? t('pairingRestricted')
                : t('noAgentsMessage')}
            </EmptyDescription>
          </EmptyHeader>
          {pairButton ? <EmptyContent>{pairButton}</EmptyContent> : null}
        </Empty>
      ) : (
        <div className="divide-y divide-border">
          {agents.map((agent) => (
            <AgentRow
              agent={agent}
              key={agent.id}
              onDiscover={() => setDiscoverAgent(agent)}
              printerCount={printerCounts.get(agent.id) ?? 0}
              selectedTenant={selectedTenant}
            />
          ))}
        </div>
      )}

      {selectedTenant ? (
        <>
          <AgentPairingDialog
            onOpenChange={setPairOpen}
            open={pairOpen}
            tenant={selectedTenant}
          />
          <DiscoverAgentDialog
            agent={discoverAgent}
            onOpenChange={(open) => {
              if (!open) setDiscoverAgent(null)
            }}
            open={discoverAgent !== null}
            tenant={selectedTenant}
          />
        </>
      ) : null}
    </section>
  )
}

function AgentRow({
  agent,
  printerCount,
  selectedTenant,
  onDiscover,
}: {
  agent: Agent
  printerCount: number
  selectedTenant: Tenant
  onDiscover: () => void
}) {
  const t = useTranslations('agents')
  const online = agent.status.toLowerCase() === 'online'

  return (
    <article
      className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
      data-agent-id={agent.id}
    >
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-medium text-foreground">
            {agent.name}
          </span>
          <StatusBadge value={agent.status} />
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
          <span className="font-mono">{agent.id}</span>
          <span aria-hidden="true">·</span>
          <span>{t('printersLinked', { count: printerCount })}</span>
          <span aria-hidden="true">·</span>
          <FormattedDate value={agent.created_at} />
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {online ? (
          <Button
            aria-label={t('discoverFor', { name: agent.name })}
            onClick={onDiscover}
            size="sm"
            type="button"
            variant="outline"
          >
            {t('discover')}
          </Button>
        ) : (
          <span title={t('discoverOffline', { name: agent.name })}>
            <Button
              aria-label={t('discoverFor', { name: agent.name })}
              disabled
              size="sm"
              type="button"
              variant="outline"
            >
              {t('discover')}
            </Button>
          </span>
        )}
        <form action={refreshPrinters}>
          <input name="tenant_id" type="hidden" value={selectedTenant.id} />
          <input name="agent_id" type="hidden" value={agent.id} />
          <input name="return_to" type="hidden" value="agents" />
          <Button
            aria-label={t('refreshFor', { name: agent.name })}
            size="sm"
            type="submit"
            variant="outline"
          >
            {t('refresh')}
          </Button>
        </form>
        <Link
          aria-label={t('settingsFor', { name: agent.name })}
          className="inline-flex h-7 shrink-0 items-center justify-center gap-1 rounded-[min(var(--radius-md),12px)] border border-border bg-background px-2.5 text-[0.8rem] font-medium whitespace-nowrap transition-colors duration-150 ease-out hover:bg-muted hover:text-foreground"
          href={agentSettingsHref(agent.id)}
        >
          {t('settings')}
        </Link>
        <AgentDeleteForm
          action={deleteAgent}
          buttonAriaLabel={t('deleteFor', { name: agent.name })}
          buttonLabel={t('delete')}
          disabled={online}
          disabledMessage={online ? t('deleteOnline', { name: agent.name }) : undefined}
          title={t('deleteTitle')}
          message={t('deleteMessage', { name: agent.name })}
          confirmLabel={t('deleteConfirm')}
        >
          <input name="tenant_id" type="hidden" value={selectedTenant.id} />
          <input name="agent_id" type="hidden" value={agent.id} />
        </AgentDeleteForm>
      </div>
    </article>
  )
}

function DiscoverAgentDialog({
  tenant,
  agent,
  open,
  onOpenChange,
}: {
  tenant: Tenant
  agent: Agent | null
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const t = useTranslations('agents')

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent closeLabel={t('discoverClose')}>
        {agent ? (
          <>
            <DialogHeader>
              <DialogTitle>{t('discoverTitle')}</DialogTitle>
              <DialogDescription>
                {t('discoverDescription', { name: agent.name })}
              </DialogDescription>
            </DialogHeader>
            <form action={discoverPrinters} className="grid gap-3" key={agent.id}>
              <input name="tenant_id" type="hidden" value={tenant.id} />
              <input name="agent_id" type="hidden" value={agent.id} />
              <label className="grid gap-1 text-sm">
                <span className="text-xs font-medium text-muted-foreground">
                  {t('discoverTimeout')}
                </span>
                <input
                  className={inputClasses}
                  defaultValue="5"
                  max="15"
                  min="1"
                  name="timeout_seconds"
                  required
                  type="number"
                />
              </label>
              <div>
                <Button type="submit">{t('discoverSubmit')}</Button>
              </div>
            </form>
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}
