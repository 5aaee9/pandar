import { useTranslations } from 'next-intl'

import type { Agent, AuditEvent, JoinLink, Tenant, TenantToken, User, UserIdentity } from './dashboard-types'
import { EmptyState, SectionHeader } from './dashboard-ui'
import { CreateJoinLinkForm, TenantUsersPanel } from './admin-users-panel'
import {
  CreateAgentPairingForm,
  CreateTenantTokenForm,
  TenantAuditPanel,
  TenantSecretsPanel,
} from './admin-settings-panel'

type AdminPanelProps = {
  selectedTenant: Tenant | null
  users: User[]
  userIdentities: UserIdentity[]
  tenantTokens: TenantToken[]
  joinLinks: JoinLink[]
  agents: Agent[]
  auditEvents: AuditEvent[]
  unavailable: boolean
}

export function TenantAdminPanel({
  selectedTenant,
  users,
  userIdentities,
  tenantTokens,
  joinLinks,
  agents,
  auditEvents,
  unavailable,
}: AdminPanelProps) {
  const t = useTranslations('admin')
  if (!selectedTenant) {
    return (
      <section className="overflow-hidden rounded-md border border-slate-300 bg-slate-50">
        <SectionHeader title={t('title')} subtitle={t('subtitleNone')} meta={t('metaAdmin')} />
        <EmptyState title={t('noTenantTitle')} message={t('noTenantMessage')} />
      </section>
    )
  }

  if (unavailable) {
    return (
      <section className="overflow-hidden rounded-md border border-slate-300 bg-slate-50">
        <SectionHeader
          title={t('title')}
          subtitle={t('subtitleUnavailable', { name: selectedTenant.display_name })}
          meta={t('metaRestricted')}
        />
        <EmptyState title={t('unavailableTitle')} message={t('unavailableMessage')} />
      </section>
    )
  }

  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-slate-50">
      <SectionHeader
        title={t('title')}
        subtitle={t('subtitleTenant', { name: selectedTenant.display_name })}
        meta={t('metaSecrets')}
      />

      <div className="grid gap-4 border-b border-slate-200 px-4 py-4 lg:grid-cols-3">
        <CreateJoinLinkForm tenantId={selectedTenant.id} />
        <CreateTenantTokenForm tenantId={selectedTenant.id} />
        <CreateAgentPairingForm tenantId={selectedTenant.id} />
      </div>

      <div className="grid gap-0 lg:grid-cols-[minmax(0,1.2fr)_minmax(320px,0.8fr)]">
        <div className="border-b border-slate-200 lg:border-b-0 lg:border-r">
          <TenantUsersPanel
            selectedTenant={selectedTenant}
            users={users}
            userIdentities={userIdentities}
            joinLinks={joinLinks}
          />
          <TenantSecretsPanel selectedTenant={selectedTenant} tenantTokens={tenantTokens} />
        </div>
        <div>
          <TenantSecretsPanel selectedTenant={selectedTenant} agents={agents} />
          <TenantAuditPanel selectedTenant={selectedTenant} auditEvents={auditEvents} />
        </div>
      </div>
    </section>
  )
}
