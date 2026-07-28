import Link from 'next/link'
import { useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import { AgentDeleteForm } from './agent-delete-form'
import {
  deleteAgent,
  refreshPrinters,
} from './actions'
import { agentSettingsHref } from './dashboard-shell'
import { EmptyState, StatusBadge } from './dashboard-ui'
import type {
  Agent,
  Tenant,
} from './dashboard-types'

export function LinkedAgentsSection({
  selectedTenant,
  agents,
}: {
  selectedTenant: Tenant | null
  agents: Agent[]
}) {
  const t = useTranslations('diagnostics')
  return (
    <section className="overflow-hidden rounded-md border border-border bg-card">
      <div className="flex flex-col gap-2 border-b border-border px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold">{t('agentsTitle')}</h2>
          <p className="mt-0.5 text-sm text-muted-foreground">
            {selectedTenant
              ? t('agentsSubtitleTenant', { name: selectedTenant.display_name, slug: selectedTenant.slug })
              : t('agentsSubtitleNone')}
          </p>
        </div>
        <div className="text-sm text-muted-foreground">{t('agentsMeta', { count: agents.length })}</div>
      </div>

      {!selectedTenant ? (
        <EmptyState title={t('noTenantTitle')} message={t('noTenantMessage')} />
      ) : agents.length === 0 ? (
        <EmptyState title={t('noAgentsTitle')} message={t('noAgentsMessage')} />
      ) : (
        <div className="overflow-x-auto">
          <table className="min-w-full border-collapse text-left text-sm">
            <thead className="bg-muted/60 text-xs font-semibold text-muted-foreground">
              <tr>
                <th className="px-4 py-2">{t('colAgent')}</th>
                <th className="px-4 py-2">{t('colStatus')}</th>
                <th className="px-4 py-2">{t('colCreated')}</th>
                <th className="px-4 py-2">{t('colActions')}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {agents.map((agent) => (
                <tr key={agent.id}>
                  <td className="px-4 py-3">
                    <div className="font-medium text-foreground">{agent.name}</div>
                    <div className="font-mono text-xs text-muted-foreground">{agent.id}</div>
                  </td>
                  <td className="px-4 py-3">
                    <StatusBadge value={agent.status} />
                  </td>
                  <td className="px-4 py-3 text-muted-foreground">
                    <FormattedDate value={agent.created_at} />
                  </td>
                  <td className="px-4 py-3">
                    <div className="flex flex-wrap items-center gap-2">
                      <Link
                        aria-label={t('settingsAgentAriaLabel', { name: agent.name })}
                        className="inline-flex h-9 items-center rounded-md border border-border px-3 text-sm font-medium text-foreground transition-colors duration-150 ease-out hover:bg-muted"
                        href={agentSettingsHref(selectedTenant.id, agent.id)}
                      >
                        {t('settingsAgent')}
                      </Link>
                      <form action={refreshPrinters}>
                        <input name="tenant_id" type="hidden" value={selectedTenant.id} />
                        <input name="agent_id" type="hidden" value={agent.id} />
                        <input name="return_to" type="hidden" value="agents" />
                        <button
                          aria-label={t('refreshAgentAriaLabel', { name: agent.name })}
                          className="h-9 rounded-md border border-border px-3 text-sm font-medium text-foreground transition-colors duration-150 ease-out hover:bg-muted"
                          type="submit"
                        >
                          {t('refreshAgent')}
                        </button>
                      </form>
                      <AgentDeleteForm
                        action={deleteAgent}
                        buttonAriaLabel={t('deleteAgentAriaLabel', { name: agent.name })}
                        buttonLabel={t('deleteAgent')}
                        disabled={agent.status.toLowerCase() === 'online'}
                        disabledMessage={agent.status.toLowerCase() === 'online' ? t('deleteOnline', { name: agent.name }) : undefined}
                        title={t('deleteTitle')}
                        message={t('deleteMessage', { name: agent.name })}
                        confirmLabel={t('deleteConfirm')}
                      >
                        <input name="tenant_id" type="hidden" value={selectedTenant.id} />
                        <input name="agent_id" type="hidden" value={agent.id} />
                      </AgentDeleteForm>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}


