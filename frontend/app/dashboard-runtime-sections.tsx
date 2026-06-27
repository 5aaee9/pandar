'use client'

import type { ReactNode } from 'react'
import { useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import { notificationSeverity } from './dashboard-attention'
import { StatusIcon } from './dashboard-status'
import type { Agent, AuthMetadata, Printer, Tenant } from './dashboard-types'
import { DetailGroup, DetailLine, SectionHeader } from './dashboard-ui'
import {
  formatAuthSource,
  formatLiveState,
  type LiveState,
  type RuntimeNotification,
} from './dashboard-runtime-helpers'

export function RuntimeStatusPanel({
  auth,
  liveState,
  lastEventAt,
  notifications,
  selectedTenant,
}: {
  auth: AuthMetadata
  liveState: LiveState
  lastEventAt: string | null
  notifications: RuntimeNotification[]
  selectedTenant: Tenant | null
}) {
  const t = useTranslations('tenantSettings')
  const tLive = useTranslations('runtime.liveState')
  const tAuth = useTranslations('runtime.authSource')
  return (
    <section className="grid gap-3 rounded-md border border-slate-300 bg-white px-4 py-3 lg:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)]">
      <div className="grid gap-2 text-sm sm:grid-cols-2">
        <RuntimeField label={t('tenant')} value={selectedTenant ? selectedTenant.display_name : t('noTenant')} />
        <RuntimeField label={t('webSocket')} value={formatLiveState(liveState, tLive)} />
        <RuntimeField
          label={t('lastEvent')}
          value={lastEventAt ? <FormattedDate value={lastEventAt} /> : '-'}
        />
        <RuntimeField
          label={t('auth')}
          value={t('authValue', { label: formatAuthSource(auth.source, tAuth), cookie: auth.cookieName })}
        />
      </div>
      <div role="status" aria-live="polite" aria-label={t('liveNotificationsAria')}>
        <div className="text-xs font-medium text-slate-500">{t('notifications')}</div>
        {notifications.length === 0 ? (
          <div className="mt-2 text-sm text-slate-600">{t('noNotifications')}</div>
        ) : (
          <ul className="mt-2 max-h-64 divide-y divide-slate-200 overflow-y-auto">
            {notifications.map((notification) => (
              <NotificationRow key={`${notification.key}-${notification.timestamp}`} notification={notification} />
            ))}
          </ul>
        )}
      </div>
    </section>
  )
}

function NotificationRow({
  notification,
}: {
  notification: RuntimeNotification
}) {
  const tTitle = useTranslations(notification.titleKey.namespace)
  const tDetail = useTranslations(notification.detailKey.namespace)
  const severity = notificationSeverity(notification.titleKey.key, notification.detailKey.key)
  return (
    <li className="flex min-w-0 items-start gap-2 py-1.5">
      <StatusIcon severity={severity} className="mt-0.5 h-4 w-4 shrink-0" />
      <div className="min-w-0">
        <div className="truncate text-sm font-medium text-slate-950">{tTitle(notification.titleKey.key, notification.titleKey.values)}</div>
        <div className="truncate text-xs text-slate-600">{tDetail(notification.detailKey.key, notification.detailKey.values)}</div>
        <div className="text-xs text-slate-500"><FormattedDate value={notification.timestamp} /></div>
      </div>
    </li>
  )
}

function RuntimeField({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div>
      <div className="text-xs font-medium text-slate-500">{label}</div>
      <div className="mt-0.5 break-words text-sm font-medium text-slate-900">{value}</div>
    </div>
  )
}

export function TenantSettings({
  auth,
  selectedTenant,
  agents,
  printers,
}: {
  auth: AuthMetadata
  selectedTenant: Tenant | null
  agents: Agent[]
  printers: Printer[]
}) {
  const t = useTranslations('tenantSettings')
  const tAuth = useTranslations('runtime.authSource')
  const tenantId = selectedTenant?.id ?? '{tenant_id}'
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-slate-50">
      <SectionHeader
        title={t('title')}
        subtitle={selectedTenant ? t('subtitleTenant', { name: selectedTenant.display_name }) : t('subtitleNone')}
        meta={t('meta')}
      />
      <div className="grid gap-4 px-4 py-3 text-sm lg:grid-cols-3">
        <DetailGroup title={t('groupTenant')}>
          <DetailLine label={t('id')} value={selectedTenant?.id ?? '-'} mono />
          <DetailLine label={t('slug')} value={selectedTenant?.slug ?? '-'} />
          <DetailLine label={t('created')} value={selectedTenant ? <FormattedDate value={selectedTenant.created_at} /> : '-'} />
        </DetailGroup>
        <DetailGroup title={t('groupAuth')}>
          <DetailLine label={t('source')} value={formatAuthSource(auth.source, tAuth)} />
          <DetailLine label={t('provider')} value={auth.provider} />
          <DetailLine label={t('cookieName')} value={auth.cookieName} mono />
          <DetailLine label={t('secretValues')} value={t('hidden')} />
        </DetailGroup>
        <DetailGroup title={t('groupOps')}>
          <DetailLine label={t('diagnosticsLabel')} value={t('diagnosticsValue')} />
        </DetailGroup>
      </div>
      <details className="border-t border-slate-200 px-4 py-2">
        <summary className="cursor-pointer select-none text-xs font-medium text-slate-500">{t('developerRef')}</summary>
        <div className="mt-2 grid gap-1 text-sm">
          <DetailLine label={t('agentPairing')} value={`/api/v1/tenants/${tenantId}/agent-pairings`} mono />
          <DetailLine label={t('apiTokens')} value={`/api/v1/tenants/${tenantId}/users/{user_id}/api-tokens`} mono />
        </div>
      </details>
      <div className="border-t border-slate-200 px-4 py-3">
        <div className="text-xs font-medium text-slate-500">{t('linkedAgents')}</div>
        {agents.length === 0 ? (
          <div className="mt-2 text-sm text-slate-600">{t('noLinkedAgents')}</div>
        ) : (
          <div className="mt-2 flex flex-wrap gap-2">
            {agents.map((agent) => (
              <span key={agent.id} className="rounded border border-slate-300 px-2 py-1 text-xs">
                {agent.name} · {agent.status}
              </span>
            ))}
          </div>
        )}
      </div>
      <div className="border-t border-slate-200 px-4 py-3">
        <div className="text-xs font-medium text-slate-500">{t('printerCompat')}</div>
        {printers.length === 0 ? (
          <div className="mt-2 text-sm text-slate-600">{t('noPrinters')}</div>
        ) : (
          <div className="mt-2 grid gap-2 md:grid-cols-2">
            {printers.map((printer) => (
              <div key={printer.id} className="min-w-0 rounded border border-slate-300 px-2 py-2 text-xs">
                <div className="truncate font-medium text-slate-900">{printer.name}</div>
                <div className="truncate text-slate-600">{printer.model ?? t('unknownModel')}</div>
                <div className="mt-1 text-slate-500">{t('runDiagnostics')}</div>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  )
}
