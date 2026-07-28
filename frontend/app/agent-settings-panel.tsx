import Link from 'next/link'
import { useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import { discoverPrinters } from './actions'
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

      <section className="overflow-hidden rounded-md border border-border bg-card">
        <div className="border-b border-border px-4 py-3">
          <h2 className="text-base font-semibold text-foreground">{t('discoveryTitle')}</h2>
          <p className="mt-0.5 text-sm text-muted-foreground">{t('discoveryDescription')}</p>
        </div>
        <form action={discoverPrinters} className="flex flex-col gap-4 px-4 py-4 sm:flex-row sm:items-end">
          <input name="tenant_id" type="hidden" value={selectedTenant.id} />
          <input name="agent_id" type="hidden" value={agent.id} />
          <input name="return_to" type="hidden" value="agent_settings" />
          <label className="flex flex-col gap-1 text-sm font-medium text-foreground">
            {t('timeout')}
            <input
              className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm font-normal text-foreground sm:w-32"
              defaultValue="5"
              max="15"
              min="1"
              name="timeout_seconds"
              required
              type="number"
            />
          </label>
          <button
            aria-label={t('discoverFor', { name: agent.name })}
            className="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground transition-colors duration-150 ease-out hover:bg-primary/80"
            type="submit"
          >
            {t('discover')}
          </button>
        </form>
      </section>
    </div>
  )
}
