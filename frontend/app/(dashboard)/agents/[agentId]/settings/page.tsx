import { getTranslations } from 'next-intl/server'

import {
  getAuthForRequest,
  getIdentityForRequest,
  getTenantsForRequest,
  resolveEffectiveTenants,
  resolveSelectedTenant,
} from '../../../../dashboard-data'
import { EmptyState } from '../../../../dashboard-ui'
import { AgentSettingsPageClient } from './settings-page-client'

const configuredTenantId = process.env.APP_TENANT_ID

export default async function AgentSettingsPage({
  params,
  searchParams,
}: {
  params: Promise<{ agentId: string }>
  searchParams: Promise<{ tenant?: string | string[]; command?: string | string[] }>
}) {
  const [{ agentId }, query, auth, identity, tenantsResult] = await Promise.all([
    params,
    searchParams,
    getAuthForRequest(),
    getIdentityForRequest(),
    getTenantsForRequest(),
  ])
  const effectiveTenants = resolveEffectiveTenants(
    tenantsResult.tenants,
    identity.me,
    configuredTenantId,
    auth.provider,
  )
  const selectedTenant = resolveSelectedTenant(query, effectiveTenants)

  if (!selectedTenant) {
    const t = await getTranslations('diagnostics')
    return <EmptyState title={t('noTenantTitle')} message={t('noTenantMessage')} />
  }

  const commandId = Array.isArray(query.command) ? query.command[0] : query.command ?? null
  return (
    <AgentSettingsPageClient
      agentId={agentId}
      commandId={commandId}
      selectedTenant={selectedTenant}
    />
  )
}
