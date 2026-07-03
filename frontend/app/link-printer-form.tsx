import { useTranslations } from 'next-intl'

import { linkPrinter } from './actions'
import type { Agent, Tenant } from './dashboard-types'
import { EmptyState } from './dashboard-ui'
import { cn } from '@/lib/utils'

export function LinkPrinterForm({
  selectedTenant,
  agents,
}: {
  selectedTenant: Tenant | null
  agents: Agent[]
}) {
  const t = useTranslations('linkPrinter')

  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <div className="flex flex-col gap-2 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold">{t('title')}</h2>
          <p className="mt-0.5 text-sm text-slate-600">
            {selectedTenant
              ? t('subtitleTenant', { name: selectedTenant.display_name })
              : t('subtitleNone')}
          </p>
        </div>
        <div className="text-sm text-slate-600">{t('meta', { count: agents.length })}</div>
      </div>

      <LinkPrinterMachineForm
        agents={agents}
        className="grid gap-4 px-4 py-4 lg:grid-cols-2"
        selectedTenant={selectedTenant}
        submitClassName="lg:col-span-2"
      />
    </section>
  )
}

export function LinkPrinterMachineForm({
  selectedTenant,
  agents,
  className,
  submitClassName,
}: {
  selectedTenant: Tenant | null
  agents: Agent[]
  className?: string
  submitClassName?: string
}) {
  const t = useTranslations('linkPrinter')
  const defaultAgent = agents.find((agent) => agent.status.toLowerCase() === 'online') ?? agents[0]

  if (!selectedTenant) {
    return <EmptyState title={t('noTenantTitle')} message={t('noTenantMessage')} />
  }

  if (agents.length === 0 || !defaultAgent) {
    return <EmptyState title={t('noAgentsTitle')} message={t('noAgentsMessage')} />
  }

  return (
    <form action={linkPrinter} className={cn('grid gap-4', className)}>
      <input name="tenant_id" type="hidden" value={selectedTenant.id} />
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-xs font-medium text-muted-foreground">{t('agent')}</span>
        <select
          className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
          defaultValue={defaultAgent.id}
          name="agent_id"
          required
        >
          {agents.map((agent) => (
            <option key={agent.id} value={agent.id}>
              {t('agentOption', { name: agent.name, status: agent.status })}
            </option>
          ))}
        </select>
      </label>
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-xs font-medium text-muted-foreground">{t('type')}</span>
        <select
          className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
          defaultValue="BambuLab"
          name="type"
          required
        >
          <option value="BambuLab">{t('typeBambuLab')}</option>
        </select>
      </label>
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-xs font-medium text-muted-foreground">{t('host')}</span>
        <input
          className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
          name="host"
          required
          type="text"
        />
      </label>
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-xs font-medium text-muted-foreground">{t('accessCode')}</span>
        <input
          autoComplete="off"
          className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
          name="access_code"
          required
          type="password"
        />
      </label>
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-xs font-medium text-muted-foreground">{t('name')}</span>
        <input
          className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
          name="name"
          type="text"
        />
      </label>
      <div className={submitClassName}>
        <button
          className="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground hover:bg-primary/80"
          type="submit"
        >
          {t('submit')}
        </button>
      </div>
    </form>
  )
}
