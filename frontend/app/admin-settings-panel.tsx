import { useActionState, useRef, useState } from 'react'
import { useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import {
  createAgentPairing,
  createTenantToken,
  revokeTenantToken,
  rotateTenantToken,
} from './actions'
import type { Agent, AuditEvent, Tenant, TenantToken } from './dashboard-types'
import { DetailLine, EmptyState, StatusBadge, Tag } from './dashboard-ui'
import { ConfirmDialog, ConfirmForm } from './confirm-dialog'
import { Input, PrimaryButton, SecretActionResult, Subhead, useAdminDate } from './admin-panel-shared'

export function CreateTenantTokenForm({ tenantId }: { tenantId: string }) {
  const t = useTranslations('admin')
  const [state, formAction, pending] = useActionState(createTenantToken, null)

  return (
    <form action={formAction} className="grid gap-2">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <div className="text-sm font-semibold text-slate-950">{t('createTenantToken')}</div>
      <Input name="name" label={t('name')} />
      <Input name="scopes" label={t('scopes')} defaultValue="*" />
      <Input name="expires_at" label={t('expiresAt')} placeholder="2026-12-31T00:00:00Z" />
      <PrimaryButton label={pending ? t('creating') : t('createToken')} />
      <SecretActionResult state={state} />
    </form>
  )
}

export function CreateAgentPairingForm({ tenantId }: { tenantId: string }) {
  const t = useTranslations('admin')
  const [state, formAction, pending] = useActionState(createAgentPairing, null)

  return (
    <form action={formAction} className="grid gap-2">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <div className="text-sm font-semibold text-slate-950">{t('pairAgent')}</div>
      <Input name="name" label={t('agentName')} />
      <PrimaryButton label={pending ? t('creating') : t('createPairing')} />
      <SecretActionResult state={state} />
    </form>
  )
}

export function TenantSecretsPanel({
  selectedTenant,
  tenantTokens,
  agents,
}: {
  selectedTenant: Tenant
  tenantTokens?: TenantToken[]
  agents?: Agent[]
}) {
  return (
    <>
      {tenantTokens ? <TenantTokensTable tenantId={selectedTenant.id} tokens={tenantTokens} /> : null}
      {agents ? <AgentsList agents={agents} /> : null}
    </>
  )
}

export function TenantAuditPanel({ auditEvents }: { selectedTenant: Tenant; auditEvents: AuditEvent[] }) {
  return <AuditList events={auditEvents} />
}

function TenantTokensTable({ tenantId, tokens }: { tenantId: string; tokens: TenantToken[] }) {
  const t = useTranslations('admin')
  const formatDate = useAdminDate()
  return (
    <div className="border-t border-slate-200">
      <Subhead title={t('tenantTokens')} meta={t('tenantTokensMeta', { count: tokens.length })} />
      {tokens.length === 0 ? (
        <EmptyState title={t('noTokensTitle')} message={t('noTokensMessage')} />
      ) : (
        <div className="divide-y divide-slate-200">
          {tokens.map((token) => (
            <div key={token.id} className="grid gap-3 px-4 py-3 text-sm lg:grid-cols-[minmax(0,1fr)_auto]">
              <div className="min-w-0">
                <div className="font-medium text-slate-950">{token.name}</div>
                <div className="mt-1 flex flex-wrap gap-1">
                  {token.scopes.map((scope) => <Tag key={scope} value={scope} />)}
                </div>
                <div className="mt-1 font-mono text-xs text-slate-600">{token.id}</div>
                <div className="mt-1 text-xs text-slate-600">{token.expires_at ? t('expires', { date: formatDate(token.expires_at) }) : t('expiresNever')}</div>
              </div>
              <div className="flex flex-wrap items-start gap-2">
                <RotateTenantTokenForm tenantId={tenantId} tokenId={token.id} />
                <ConfirmForm
                  action={revokeTenantToken}
                  buttonClassName="h-8 rounded-md border border-red-300 px-2 text-xs font-medium text-red-700"
                  buttonLabel={token.revoked_at ? t('revoked') : t('revoke')}
                  disabled={Boolean(token.revoked_at)}
                  title={t('revokeTokenTitle')}
                  message={t('revokeTokenMessage')}
                  confirmLabel={t('revokeTokenConfirm')}
                  tone="danger"
                >
                  <input name="tenant_id" type="hidden" value={tenantId} />
                  <input name="token_id" type="hidden" value={token.id} />
                </ConfirmForm>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function RotateTenantTokenForm({ tenantId, tokenId }: { tenantId: string; tokenId: string }) {
  const t = useTranslations('admin')
  const [state, formAction, pending] = useActionState(rotateTenantToken, null)
  const formRef = useRef<HTMLFormElement>(null)
  const [open, setOpen] = useState(false)

  return (
    <>
      <form ref={formRef} action={formAction} className="grid gap-2">
        <input name="tenant_id" type="hidden" value={tenantId} />
        <input name="token_id" type="hidden" value={tokenId} />
        <button className="h-8 rounded-md border border-slate-300 px-2 text-xs font-medium" disabled={pending} onClick={() => setOpen(true)} type="button">
          {pending ? t('rotating') : t('rotate')}
        </button>
        <SecretActionResult state={state} />
      </form>
      <ConfirmDialog
        open={open}
        title={t('rotateTokenTitle')}
        message={t('rotateTokenMessage')}
        confirmLabel={t('rotateTokenConfirm')}
        tone="danger"
        onConfirm={() => {
          setOpen(false)
          formRef.current?.requestSubmit()
        }}
        onCancel={() => setOpen(false)}
      />
    </>
  )
}

function AgentsList({ agents }: { agents: Agent[] }) {
  const t = useTranslations('admin')
  return (
    <div>
      <Subhead title={t('agents')} meta={t('agentsMeta', { count: agents.length })} />
      <div className="grid gap-2 px-4 py-3">
        {agents.length === 0 ? <div className="text-sm text-slate-600">{t('noLinkedAgents')}</div> : agents.map((agent) => (
          <div key={agent.id} className="rounded border border-slate-200 px-3 py-2 text-sm">
            <div className="flex items-center justify-between gap-2">
              <span className="font-medium text-slate-950">{agent.name}</span>
              <StatusBadge value={agent.status} />
            </div>
            <DetailLine label={t('idLabel')} value={agent.id} mono />
          </div>
        ))}
      </div>
    </div>
  )
}

function AuditList({ events }: { events: AuditEvent[] }) {
  const t = useTranslations('admin')
  return (
    <div className="border-t border-slate-200">
      <Subhead title={t('auditEvents')} meta={t('auditMeta', { count: events.length })} />
      <div className="divide-y divide-slate-200">
        {events.length === 0 ? <div className="px-4 py-3 text-sm text-slate-600">{t('noAuditEvents')}</div> : events.map((event) => (
          <div key={event.id} className="px-4 py-3 text-sm">
            <div className="font-medium text-slate-950">{event.action}</div>
            <div className="mt-1 text-xs text-slate-600">{event.actor_type} · {event.target_type} · <FormattedDate value={event.created_at} /></div>
          </div>
        ))}
      </div>
    </div>
  )
}
