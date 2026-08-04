'use client'

import { useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import { Subhead } from './admin-panel-shared'
import type { AuditEvent } from './dashboard-types'

export function TenantAuditPanel({
  auditEvents,
}: {
  auditEvents: AuditEvent[]
}) {
  const t = useTranslations('admin')

  return (
    <div>
      <Subhead
        title={t('auditEvents')}
        meta={t('auditMeta', { count: auditEvents.length })}
      />
      <div className="divide-y divide-border">
        {auditEvents.length === 0 ? (
          <div className="px-4 py-3 text-sm text-muted-foreground">
            {t('noAuditEvents')}
          </div>
        ) : (
          auditEvents.map((event) => (
            <div key={event.id} className="px-4 py-3 text-sm">
              <div className="font-medium text-foreground">{event.action}</div>
              <div className="mt-1 text-xs text-muted-foreground">
                {event.actor_type} · {event.target_type} ·{' '}
                <FormattedDate value={event.created_at} />
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  )
}
