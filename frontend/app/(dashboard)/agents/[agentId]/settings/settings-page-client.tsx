"use client"

import { useQuery } from '@tanstack/react-query'
import { useTranslations } from 'next-intl'

import { AgentSettingsPanel } from '../../../../agent-settings-panel'
import { agentSettingsRouteQuery } from '../../../../route-data'
import { DiagnosticsSection } from '../../../../diagnostics-section'
import { EmptyState } from '../../../../dashboard-ui'
import type { Tenant } from '../../../../dashboard-types'
import { QueryErrorBoundary } from '../../../../query-error-boundary'

export function AgentSettingsPageClient({
  agentId,
  commandId,
  selectedTenant,
}: {
  agentId: string
  commandId: string | null
  selectedTenant: Tenant
}) {
  const t = useTranslations('agentSettings')
  const { data, isLoading, error } = useQuery(
    agentSettingsRouteQuery(selectedTenant.id, agentId, commandId),
  )

  if (isLoading) {
    return (
      <div className="flex items-center justify-center p-8">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-muted border-t-primary" />
      </div>
    )
  }

  if (error) {
    return (
      <div className="rounded-md border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive">
        {t('loadError', { message: error instanceof Error ? error.message : t('unknownError') })}
      </div>
    )
  }

  if (!data?.agent) {
    return <EmptyState title={t('notFoundTitle')} message={t('notFoundMessage')} />
  }

  return (
    <QueryErrorBoundary>
      <AgentSettingsPanel agent={data.agent} selectedTenant={selectedTenant} />
      {data.command ? (
        <div className="mt-4">
          <DiagnosticsSection
            commandData={data.commandData}
            printers={data.printers}
            selectedCommand={data.command}
            selectedTenant={selectedTenant}
          />
        </div>
      ) : null}
    </QueryErrorBoundary>
  )
}
