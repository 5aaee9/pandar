'use client'

import { useTranslations } from 'next-intl'

import { rowHoverClasses } from '../lib/utils'
import { Subhead } from './admin-panel-shared'
import { TenantTokensTable } from './admin-settings-token-list'
import type { Agent, Tenant, TenantToken } from './dashboard-types'
import { DetailLine, StatusBadge } from './dashboard-ui'

export function TenantSecretsPanel({
  selectedTenant,
  tenantTokens,
  agents,
  nowMs,
}: {
  selectedTenant: Tenant
  tenantTokens?: TenantToken[]
  agents?: Agent[]
  nowMs: number
}) {
  const t = useTranslations('admin')

  return (
    <>
      {tenantTokens ? (
        <TenantTokensTable
          tenantId={selectedTenant.id}
          tokens={tenantTokens}
          nowMs={nowMs}
        />
      ) : null}
      {agents ? (
        <div>
          <Subhead
            title={t('agents')}
            meta={t('agentsMeta', { count: agents.length })}
          />
          <div className="grid gap-2 px-4 py-3">
            {agents.length === 0 ? (
              <div className="text-sm text-muted-foreground">
                {t('noLinkedAgents')}
              </div>
            ) : (
              agents.map((agent) => (
                <div
                  key={agent.id}
                  className={`rounded-md border border-border bg-muted/20 px-3 py-2 text-sm ${rowHoverClasses}`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-medium text-foreground">
                      {agent.name}
                    </span>
                    <StatusBadge value={agent.status} />
                  </div>
                  <DetailLine label={t('idLabel')} value={agent.id} mono />
                </div>
              ))
            )}
          </div>
        </div>
      ) : null}
    </>
  )
}
