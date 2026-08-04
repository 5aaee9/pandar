import Link from 'next/link'
import { useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import { StatusBadge } from './dashboard-ui'
import type { Agent, Tenant } from './dashboard-types'

export function AgentSettingsPanel({
  selectedTenant,
  agent,
}: {
  selectedTenant: Tenant
  agent: Agent
}) {
  const t = useTranslations('agentSettings')

  return (
    <div className="grid gap-4">
      <Link
        className="w-fit text-sm font-medium text-muted-foreground hover:text-foreground"
        href={`/agents?tenant=${encodeURIComponent(selectedTenant.id)}`}
      >
        {t('backToAgents')}
      </Link>

      <section className="overflow-hidden rounded-md border border-border bg-card">
        <div className="flex flex-col gap-3 border-b border-border px-4 py-4 sm:flex-row sm:items-start sm:justify-between">
          <div>
            <h2 className="text-lg font-semibold text-foreground">
              {t('title', { name: agent.name })}
            </h2>
            <p className="mt-1 text-sm text-muted-foreground">{t('subtitle')}</p>
          </div>
          <StatusBadge value={agent.status} />
        </div>
        <dl className="grid gap-3 px-4 py-4 text-sm sm:grid-cols-2">
          <div>
            <dt className="text-xs font-medium text-muted-foreground">{t('agentId')}</dt>
            <dd className="mt-1 break-all font-mono text-xs text-foreground">{agent.id}</dd>
          </div>
          <div>
            <dt className="text-xs font-medium text-muted-foreground">{t('createdAt')}</dt>
            <dd className="mt-1 text-foreground"><FormattedDate value={agent.created_at} /></dd>
          </div>
        </dl>
      </section>
    </div>
  )
}
