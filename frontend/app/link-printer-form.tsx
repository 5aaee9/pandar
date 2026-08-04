import { useTranslations } from 'next-intl'

import { linkPrinter } from './actions'
import type { Agent, Tenant } from './dashboard-types'
import { EmptyState } from './dashboard-ui'
import { inputClasses } from '@/lib/utils'

export function LinkPrinterMachineForm({
  selectedTenant,
  agents,
}: {
  selectedTenant: Tenant | null
  agents: Agent[]
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
    <form action={linkPrinter} className="grid gap-4">
      <input name="tenant_id" type="hidden" value={selectedTenant.id} />
      <input name="type" type="hidden" value="BambuLab" />
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-xs font-medium text-muted-foreground">{t('agent')}</span>
        <select
          className={inputClasses}
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
        <span className="text-xs font-medium text-muted-foreground">{t('host')}</span>
        <input
          className={inputClasses}
          name="host"
          required
          type="text"
        />
      </label>
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-xs font-medium text-muted-foreground">{t('accessCode')}</span>
        <input
          autoComplete="off"
          className={inputClasses}
          name="access_code"
          required
          type="password"
        />
      </label>
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-xs font-medium text-muted-foreground">{t('name')}</span>
        <input
          className={inputClasses}
          name="name"
          type="text"
        />
      </label>
      <div>
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
