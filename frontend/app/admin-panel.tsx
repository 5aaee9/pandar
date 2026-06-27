import { useActionState, useRef, useState } from 'react'
import { useFormatter, useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import {
  createAgentPairing,
  createJoinLink,
  createTenantToken,
  revokeJoinLink,
  revokeTenantToken,
  rotateTenantToken,
  type SecretActionState,
  updateTenantUserRole,
} from './actions'
import type { Agent, AuditEvent, JoinLink, Tenant, TenantToken, User, UserIdentity } from './dashboard-types'
import { DetailLine, EmptyState, SectionHeader, StatusBadge, Tag } from './dashboard-ui'
import { ConfirmDialog, ConfirmForm } from './confirm-dialog'

function useAdminDate() {
  const format = useFormatter()
  return (value: string) => {
    const d = new Date(value)
    if (Number.isNaN(d.getTime())) return value
    return format.dateTime(d, { dateStyle: 'medium', timeStyle: 'short', timeZone: 'UTC' })
  }
}

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

const roles: User['role'][] = ['tenant_admin', 'operator', 'viewer']

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
          <UsersTable tenantId={selectedTenant.id} users={users} identities={userIdentities} />
          <JoinLinksTable tenantId={selectedTenant.id} joinLinks={joinLinks} />
          <TenantTokensTable tenantId={selectedTenant.id} tokens={tenantTokens} />
        </div>
        <div>
          <AgentsList agents={agents} />
          <AuditList events={auditEvents} />
        </div>
      </div>
    </section>
  )
}

function CreateJoinLinkForm({ tenantId }: { tenantId: string }) {
  const t = useTranslations('admin')
  const [state, formAction, pending] = useActionState(createJoinLink, null)

  return (
    <form action={formAction} className="grid gap-2">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <div className="text-sm font-semibold text-slate-950">{t('createJoinLink')}</div>
      <Select name="role" label={t('role')} values={roles} />
      <Input name="email_constraint" label={t('verifiedEmail')} type="email" />
      <Input name="expires_in_seconds" label={t('ttlSeconds')} defaultValue="604800" />
      <Input name="max_uses" label={t('maxUses')} defaultValue="1" />
      <PrimaryButton label={pending ? t('creating') : t('createLink')} />
      <SecretActionResult state={state} />
    </form>
  )
}

function CreateTenantTokenForm({ tenantId }: { tenantId: string }) {
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

function CreateAgentPairingForm({ tenantId }: { tenantId: string }) {
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

function UsersTable({
  tenantId,
  users,
  identities,
}: {
  tenantId: string
  users: User[]
  identities: UserIdentity[]
}) {
  const t = useTranslations('admin')
  return (
    <div>
      <Subhead title={t('users')} meta={t('usersMeta', { count: users.length })} />
      {users.length === 0 ? (
        <EmptyState title={t('noUsersTitle')} message={t('noUsersMessage')} />
      ) : (
        <div className="overflow-x-auto">
          <table className="min-w-full text-left text-sm">
            <thead className="bg-slate-50 text-xs font-semibold text-slate-600">
              <tr>
                <th className="px-4 py-2">{t('colUser')}</th>
                <th className="px-4 py-2">{t('colRole')}</th>
                <th className="px-4 py-2">{t('colIdentities')}</th>
                <th className="px-4 py-2">{t('colUpdate')}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-200">
              {users.map((user) => {
                const linked = identities.filter((identity) => identity.user_id === user.id)
                return (
                  <tr key={user.id}>
                    <td className="px-4 py-3">
                      <div className="font-medium text-slate-950">{user.display_name}</div>
                      <div className="text-slate-700">{user.email}</div>
                      <div className="font-mono text-xs text-slate-600">{user.id}</div>
                    </td>
                    <td className="px-4 py-3"><Tag value={user.role} /></td>
                    <td className="px-4 py-3 text-xs text-slate-700">
                      {linked.length === 0 ? '-' : linked.map((identity) => identity.provider).join(', ')}
                    </td>
                    <td className="px-4 py-3">
                      <form action={updateTenantUserRole} className="flex flex-wrap gap-2">
                        <input name="tenant_id" type="hidden" value={tenantId} />
                        <input name="user_id" type="hidden" value={user.id} />
                        <select name="role" defaultValue={user.role} className="h-8 rounded-md border border-slate-300 bg-white px-2 text-xs">
                          {roles.map((role) => <option key={role} value={role}>{role}</option>)}
                        </select>
                        <button className="h-8 rounded-md border border-slate-300 px-2 text-xs font-medium" type="submit">{t('save')}</button>
                      </form>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

function JoinLinksTable({ tenantId, joinLinks }: { tenantId: string; joinLinks: JoinLink[] }) {
  const t = useTranslations('admin')
  const formatDate = useAdminDate()
  return (
    <div className="border-t border-slate-200">
      <Subhead title={t('joinLinks')} meta={t('joinLinksMeta', { count: joinLinks.length })} />
      {joinLinks.length === 0 ? (
        <EmptyState title={t('noJoinLinksTitle')} message={t('noJoinLinksMessage')} />
      ) : (
        <div className="divide-y divide-slate-200">
          {joinLinks.map((link) => (
            <div key={link.id} className="grid gap-3 px-4 py-3 text-sm lg:grid-cols-[minmax(0,1fr)_auto]">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <Tag value={link.role} />
                  <span className="text-xs text-slate-600">
                    {t('usedRatio', { used: link.used_count, max: link.max_uses })}
                  </span>
                  {link.revoked_at ? <span className="text-xs font-medium text-red-700">{t('revoked')}</span> : null}
                </div>
                <div className="mt-1 font-mono text-xs text-slate-600">{link.id}</div>
                <div className="mt-1 text-xs text-slate-600">
                  {link.email_constraint ? t('emailConstraint', { email: link.email_constraint }) : t('anyVerifiedEmail')} · {t('expires', { date: formatDate(link.expires_at) })}
                </div>
              </div>
              <ConfirmForm
                action={revokeJoinLink}
                buttonClassName="h-8 rounded-md border border-red-300 px-2 text-xs font-medium text-red-700"
                buttonLabel={link.revoked_at ? t('revoked') : t('revoke')}
                disabled={Boolean(link.revoked_at)}
                title={t('revokeJoinTitle')}
                message={t('revokeJoinMessage')}
                confirmLabel={t('revokeJoinConfirm')}
                tone="danger"
              >
                <input name="tenant_id" type="hidden" value={tenantId} />
                <input name="join_link_id" type="hidden" value={link.id} />
              </ConfirmForm>
            </div>
          ))}
        </div>
      )}
    </div>
  )
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

function SecretActionResult({ state }: { state: SecretActionState }) {
  const t = useTranslations('admin')
  if (!state) {
    return null
  }
  if (!state.ok) {
    return <div className="rounded border border-red-200 bg-red-50 px-2 py-1 text-xs text-red-900">{state.error}</div>
  }
  if (state.kind === 'tenant_token') {
    return (
      <div className="grid gap-1 rounded border border-amber-200 bg-amber-50 px-2 py-2 text-xs text-amber-950">
        <div className="font-semibold">{state.message}</div>
        <code className="break-all rounded bg-white px-2 py-1 font-mono text-[11px] text-slate-950">{state.token}</code>
        <div>{t('tokenShownOnce')}</div>
      </div>
    )
  }
  if (state.kind === 'join_link') {
    return (
      <div className="grid gap-1 rounded border border-amber-200 bg-amber-50 px-2 py-2 text-xs text-amber-950">
        <div className="font-semibold">{state.message}</div>
        <code className="break-all rounded bg-white px-2 py-1 font-mono text-[11px] text-slate-950">{`/join#${state.token}`}</code>
        <div>{t('joinTokenShownOnce')}</div>
      </div>
    )
  }
  return (
    <div className="grid gap-1 rounded border border-amber-200 bg-amber-50 px-2 py-2 text-xs text-amber-950">
      <div className="font-semibold">{state.message}</div>
      <pre className="overflow-x-auto rounded bg-white px-2 py-1 font-mono text-[11px] text-slate-950">{state.agentEnv}</pre>
      <div>{t('pairingShownOnce')}</div>
    </div>
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

function Input({ name, label, defaultValue, placeholder, type = 'text' }: { name: string; label: string; defaultValue?: string; placeholder?: string; type?: string }) {
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-xs font-medium text-slate-500">{label}</span>
      <input className="h-9 rounded-md border border-slate-300 px-2 text-sm text-slate-950" defaultValue={defaultValue} name={name} placeholder={placeholder} type={type} />
    </label>
  )
}

function Select({ name, label, values }: { name: string; label: string; values: string[] }) {
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-xs font-medium text-slate-500">{label}</span>
      <select className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950" name={name}>
        {values.map((value) => <option key={value} value={value}>{value}</option>)}
      </select>
    </label>
  )
}

function PrimaryButton({ label }: { label: string }) {
  return <button className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white hover:bg-cyan-800" type="submit">{label}</button>
}

function Subhead({ title, meta }: { title: string; meta: string }) {
  return (
    <div className="flex items-center justify-between border-b border-slate-200 px-4 py-2">
      <h3 className="text-sm font-semibold text-slate-950">{title}</h3>
      <span className="text-xs text-slate-600">{meta}</span>
    </div>
  )
}
