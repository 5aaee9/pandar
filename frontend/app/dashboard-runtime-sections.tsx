import { notificationSeverity } from './dashboard-attention'
import { StatusIcon } from './dashboard-status'
import type { Agent, AuthMetadata, Printer, Tenant } from './dashboard-types'
import { DetailGroup, DetailLine, formatDate, SectionHeader } from './dashboard-ui'
import type { LiveState, RuntimeNotification } from './dashboard-runtime-helpers'

export function RuntimeStatusPanel({
  auth,
  authLabel,
  liveState,
  lastEventAt,
  notifications,
  selectedTenant,
}: {
  auth: AuthMetadata
  authLabel: string
  liveState: LiveState
  lastEventAt: string | null
  notifications: RuntimeNotification[]
  selectedTenant: Tenant | null
}) {
  return (
    <section className="grid gap-3 rounded-md border border-slate-300 bg-white px-4 py-3 lg:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)]">
      <div className="grid gap-2 text-sm sm:grid-cols-2">
        <RuntimeField label="Tenant" value={selectedTenant ? selectedTenant.display_name : 'No tenant'} />
        <RuntimeField label="WebSocket" value={liveState} />
        <RuntimeField label="Last event" value={lastEventAt ? formatDate(lastEventAt) : '-'} />
        <RuntimeField label="Auth" value={`${authLabel} · cookie ${auth.cookieName}`} />
      </div>
      <div role="status" aria-live="polite" aria-label="Live notifications">
        <div className="text-xs font-medium text-slate-500">Notifications</div>
        {notifications.length === 0 ? (
          <div className="mt-2 text-sm text-slate-600">No live notifications</div>
        ) : (
          <ul className="mt-2 max-h-64 divide-y divide-slate-200 overflow-y-auto">
            {notifications.map((notification) => {
              const severity = notificationSeverity(notification.title, notification.detail)
              return (
                <li
                  key={`${notification.key}-${notification.timestamp}`}
                  className="flex min-w-0 items-start gap-2 py-1.5"
                >
                  <StatusIcon severity={severity} className="mt-0.5 h-4 w-4 shrink-0" />
                  <div className="min-w-0">
                    <div className="truncate text-sm font-medium text-slate-950">{notification.title}</div>
                    <div className="truncate text-xs text-slate-600">{notification.detail}</div>
                    <div className="text-xs text-slate-500">{formatDate(notification.timestamp)}</div>
                  </div>
                </li>
              )
            })}
          </ul>
        )}
      </div>
    </section>
  )
}

function RuntimeField({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-xs font-medium text-slate-500">{label}</div>
      <div className="mt-0.5 break-words text-sm font-medium text-slate-900">{value}</div>
    </div>
  )
}

export function TenantSettings({
  auth,
  authLabel,
  selectedTenant,
  agents,
  printers,
}: {
  auth: AuthMetadata
  authLabel: string
  selectedTenant: Tenant | null
  agents: Agent[]
  printers: Printer[]
}) {
  const tenantId = selectedTenant?.id ?? '{tenant_id}'
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-slate-50">
      <SectionHeader
        title="Tenant settings"
        subtitle={selectedTenant ? `${selectedTenant.display_name} operational references` : 'No tenant selected'}
        meta="No token values shown"
      />
      <div className="grid gap-4 px-4 py-3 text-sm lg:grid-cols-3">
        <DetailGroup title="Tenant">
          <DetailLine label="ID" value={selectedTenant?.id ?? '-'} mono />
          <DetailLine label="Slug" value={selectedTenant?.slug ?? '-'} />
          <DetailLine label="Created" value={selectedTenant ? formatDate(selectedTenant.created_at) : '-'} />
        </DetailGroup>
        <DetailGroup title="Authentication">
          <DetailLine label="Source" value={authLabel} />
          <DetailLine label="Provider" value={auth.provider} />
          <DetailLine label="Cookie name" value={auth.cookieName} mono />
          <DetailLine label="Secret values" value="Hidden" />
        </DetailGroup>
        <DetailGroup title="Operations">
          <DetailLine label="Diagnostics" value="See the Diagnostics section" />
        </DetailGroup>
      </div>
      <details className="border-t border-slate-200 px-4 py-2">
        <summary className="cursor-pointer select-none text-xs font-medium text-slate-500">Developer reference</summary>
        <div className="mt-2 grid gap-1 text-sm">
          <DetailLine label="Agent pairing" value={`/api/v1/tenants/${tenantId}/agent-pairings`} mono />
          <DetailLine label="API tokens" value={`/api/v1/tenants/${tenantId}/users/{user_id}/api-tokens`} mono />
        </div>
      </details>
      <div className="border-t border-slate-200 px-4 py-3">
        <div className="text-xs font-medium text-slate-500">Linked agents</div>
        {agents.length === 0 ? (
          <div className="mt-2 text-sm text-slate-600">No linked agents</div>
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
        <div className="text-xs font-medium text-slate-500">Printer compatibility</div>
        {printers.length === 0 ? (
          <div className="mt-2 text-sm text-slate-600">No reported printers</div>
        ) : (
          <div className="mt-2 grid gap-2 md:grid-cols-2">
            {printers.map((printer) => (
              <div key={printer.id} className="min-w-0 rounded border border-slate-300 px-2 py-2 text-xs">
                <div className="truncate font-medium text-slate-900">{printer.name}</div>
                <div className="truncate text-slate-600">{printer.model ?? 'Unknown model'}</div>
                <div className="mt-1 text-slate-500">Run diagnostics from the Diagnostics section</div>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  )
}
