import { useTranslations } from 'next-intl'

import { linkPrinter } from './actions'
import type { Agent, Tenant } from './dashboard-types'
import { EmptyState } from './dashboard-ui'

export function LinkPrinterForm({
  selectedTenant,
  agents,
}: {
  selectedTenant: Tenant | null
  agents: Agent[]
}) {
  const t = useTranslations('linkPrinter')
  const defaultAgent = agents.find((agent) => agent.status.toLowerCase() === 'online') ?? agents[0]

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

      {!selectedTenant ? (
        <EmptyState title={t('noTenantTitle')} message={t('noTenantMessage')} />
      ) : agents.length === 0 || !defaultAgent ? (
        <EmptyState title={t('noAgentsTitle')} message={t('noAgentsMessage')} />
      ) : (
        <form action={linkPrinter} className="grid gap-4 px-4 py-4 lg:grid-cols-2">
          <input name="tenant_id" type="hidden" value={selectedTenant.id} />
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">{t('agent')}</span>
            <select
              className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950"
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
            <span className="text-xs font-medium text-slate-500">{t('host')}</span>
            <input
              className="h-9 rounded-md border border-slate-300 px-2 text-sm text-slate-950"
              name="host"
              required
              type="text"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">{t('serialNumber')}</span>
            <input
              className="h-9 rounded-md border border-slate-300 px-2 font-mono text-sm text-slate-950"
              name="serial_number"
              required
              type="text"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">{t('accessCode')}</span>
            <input
              autoComplete="off"
              className="h-9 rounded-md border border-slate-300 px-2 text-sm text-slate-950"
              name="access_code"
              required
              type="password"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">{t('name')}</span>
            <input
              className="h-9 rounded-md border border-slate-300 px-2 text-sm text-slate-950"
              name="name"
              type="text"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">{t('model')}</span>
            <input
              className="h-9 rounded-md border border-slate-300 px-2 text-sm text-slate-950"
              name="model"
              type="text"
            />
          </label>
          <div className="lg:col-span-2">
            <button
              className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white hover:bg-cyan-800"
              type="submit"
            >
              {t('submit')}
            </button>
          </div>
        </form>
      )}
    </section>
  )
}
