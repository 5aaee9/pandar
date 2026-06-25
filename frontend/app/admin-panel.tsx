import { useActionState } from 'react'

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
import { DetailLine, EmptyState, formatDate, SectionHeader, StatusBadge } from './dashboard-ui'

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
  if (!selectedTenant) {
    return (
      <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
        <SectionHeader title="Tenant administration" subtitle="No tenant selected" meta="Admin" />
        <EmptyState title="No tenant selected" message="Select a tenant to manage users, tokens, and agent pairings." />
      </section>
    )
  }

  if (unavailable) {
    return (
      <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
        <SectionHeader
          title="Tenant administration"
          subtitle={`${selectedTenant.display_name} admin data is unavailable`}
          meta="Restricted"
        />
        <EmptyState title="Admin data unavailable" message="The current auth context cannot read tenant admin resources." />
      </section>
    )
  }

  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <SectionHeader
        title="Tenant administration"
        subtitle={`${selectedTenant.display_name} users, tokens, and audit trail`}
        meta="Secrets are not stored"
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
  const [state, formAction, pending] = useActionState(createJoinLink, null)

  return (
    <form action={formAction} className="grid gap-2">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <div className="text-sm font-semibold text-slate-950">Create join link</div>
      <Select name="role" label="Role" values={roles} />
      <Input name="email_constraint" label="Verified email" type="email" />
      <Input name="expires_in_seconds" label="TTL seconds" defaultValue="604800" />
      <Input name="max_uses" label="Max uses" defaultValue="1" />
      <PrimaryButton label={pending ? 'Creating...' : 'Create link'} />
      <SecretActionResult state={state} />
    </form>
  )
}

function CreateTenantTokenForm({ tenantId }: { tenantId: string }) {
  const [state, formAction, pending] = useActionState(createTenantToken, null)

  return (
    <form action={formAction} className="grid gap-2">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <div className="text-sm font-semibold text-slate-950">Create tenant token</div>
      <Input name="name" label="Name" />
      <Input name="scopes" label="Scopes" defaultValue="*" />
      <Input name="expires_at" label="Expires at" placeholder="2026-12-31T00:00:00Z" />
      <PrimaryButton label={pending ? 'Creating...' : 'Create token'} />
      <SecretActionResult state={state} />
    </form>
  )
}

function CreateAgentPairingForm({ tenantId }: { tenantId: string }) {
  const [state, formAction, pending] = useActionState(createAgentPairing, null)

  return (
    <form action={formAction} className="grid gap-2">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <div className="text-sm font-semibold text-slate-950">Pair agent</div>
      <Input name="name" label="Agent name" />
      <PrimaryButton label={pending ? 'Creating...' : 'Create pairing'} />
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
  return (
    <div>
      <Subhead title="Users" meta={`${users.length} users`} />
      {users.length === 0 ? (
        <EmptyState title="No users" message="Create a tenant user to assign operator or viewer access." />
      ) : (
        <div className="overflow-x-auto">
          <table className="min-w-full text-left text-sm">
            <thead className="bg-slate-50 text-xs font-semibold uppercase text-slate-600">
              <tr>
                <th className="px-4 py-2">User</th>
                <th className="px-4 py-2">Role</th>
                <th className="px-4 py-2">Identities</th>
                <th className="px-4 py-2">Update</th>
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
                    <td className="px-4 py-3"><StatusBadge value={user.role} /></td>
                    <td className="px-4 py-3 text-xs text-slate-700">
                      {linked.length === 0 ? '-' : linked.map((identity) => identity.provider).join(', ')}
                    </td>
                    <td className="px-4 py-3">
                      <form action={updateTenantUserRole} className="flex flex-wrap gap-2">
                        <input name="tenant_id" type="hidden" value={tenantId} />
                        <input name="user_id" type="hidden" value={user.id} />
                        <select name="role" defaultValue={user.role} className="h-8 rounded border border-slate-300 bg-white px-2 text-xs">
                          {roles.map((role) => <option key={role} value={role}>{role}</option>)}
                        </select>
                        <button className="h-8 rounded border border-slate-300 px-2 text-xs font-medium" type="submit">Save</button>
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
  return (
    <div className="border-t border-slate-200">
      <Subhead title="Join links" meta={`${joinLinks.length} links`} />
      {joinLinks.length === 0 ? (
        <EmptyState title="No join links" message="Create a join link to invite externally authenticated users." />
      ) : (
        <div className="divide-y divide-slate-200">
          {joinLinks.map((link) => (
            <div key={link.id} className="grid gap-3 px-4 py-3 text-sm lg:grid-cols-[minmax(0,1fr)_auto]">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <StatusBadge value={link.role} />
                  <span className="text-xs text-slate-600">
                    {link.used_count}/{link.max_uses} used
                  </span>
                  {link.revoked_at ? <span className="text-xs font-medium text-red-700">Revoked</span> : null}
                </div>
                <div className="mt-1 font-mono text-xs text-slate-600">{link.id}</div>
                <div className="mt-1 text-xs text-slate-600">
                  {link.email_constraint ? `Email ${link.email_constraint}` : 'Any verified email'} · Expires {formatDate(link.expires_at)}
                </div>
              </div>
              <form action={revokeJoinLink}>
                <input name="tenant_id" type="hidden" value={tenantId} />
                <input name="join_link_id" type="hidden" value={link.id} />
                <button className="h-8 rounded border border-red-300 px-2 text-xs font-medium text-red-700" disabled={Boolean(link.revoked_at)} type="submit">
                  {link.revoked_at ? 'Revoked' : 'Revoke'}
                </button>
              </form>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function TenantTokensTable({ tenantId, tokens }: { tenantId: string; tokens: TenantToken[] }) {
  return (
    <div className="border-t border-slate-200">
      <Subhead title="Tenant tokens" meta={`${tokens.length} tokens`} />
      {tokens.length === 0 ? (
        <EmptyState title="No tenant tokens" message="Create scoped tenant tokens for automation or plugin login." />
      ) : (
        <div className="divide-y divide-slate-200">
          {tokens.map((token) => (
            <div key={token.id} className="grid gap-3 px-4 py-3 text-sm lg:grid-cols-[minmax(0,1fr)_auto]">
              <div className="min-w-0">
                <div className="font-medium text-slate-950">{token.name}</div>
                <div className="mt-1 flex flex-wrap gap-1">
                  {token.scopes.map((scope) => <span key={scope} className="rounded bg-slate-100 px-2 py-1 text-xs">{scope}</span>)}
                </div>
                <div className="mt-1 font-mono text-xs text-slate-600">{token.id}</div>
                <div className="mt-1 text-xs text-slate-600">Expires {token.expires_at ? formatDate(token.expires_at) : 'never'}</div>
              </div>
              <div className="flex flex-wrap items-start gap-2">
                <RotateTenantTokenForm tenantId={tenantId} tokenId={token.id} />
                <form action={revokeTenantToken}>
                  <input name="tenant_id" type="hidden" value={tenantId} />
                  <input name="token_id" type="hidden" value={token.id} />
                  <button className="h-8 rounded border border-red-300 px-2 text-xs font-medium text-red-700" disabled={Boolean(token.revoked_at)} type="submit">
                    {token.revoked_at ? 'Revoked' : 'Revoke'}
                  </button>
                </form>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function RotateTenantTokenForm({ tenantId, tokenId }: { tenantId: string; tokenId: string }) {
  const [state, formAction, pending] = useActionState(rotateTenantToken, null)

  return (
    <form action={formAction} className="grid gap-2">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <input name="token_id" type="hidden" value={tokenId} />
      <button className="h-8 rounded border border-slate-300 px-2 text-xs font-medium" type="submit">
        {pending ? 'Rotating...' : 'Rotate'}
      </button>
      <SecretActionResult state={state} />
    </form>
  )
}

function SecretActionResult({ state }: { state: SecretActionState }) {
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
        <div>This token is shown once and is not persisted by the browser.</div>
      </div>
    )
  }
  if (state.kind === 'join_link') {
    return (
      <div className="grid gap-1 rounded border border-amber-200 bg-amber-50 px-2 py-2 text-xs text-amber-950">
        <div className="font-semibold">{state.message}</div>
        <code className="break-all rounded bg-white px-2 py-1 font-mono text-[11px] text-slate-950">{`/join#${state.token}`}</code>
        <div>This join token is shown once and is not persisted by the browser.</div>
      </div>
    )
  }
  return (
    <div className="grid gap-1 rounded border border-amber-200 bg-amber-50 px-2 py-2 text-xs text-amber-950">
      <div className="font-semibold">{state.message}</div>
      <pre className="overflow-x-auto rounded bg-white px-2 py-1 font-mono text-[11px] text-slate-950">{state.agentEnv}</pre>
      <div>This pairing output is shown once and is not persisted by the browser.</div>
    </div>
  )
}

function AgentsList({ agents }: { agents: Agent[] }) {
  return (
    <div>
      <Subhead title="Agents" meta={`${agents.length} linked`} />
      <div className="grid gap-2 px-4 py-3">
        {agents.length === 0 ? <div className="text-sm text-slate-600">No linked agents</div> : agents.map((agent) => (
          <div key={agent.id} className="rounded border border-slate-200 px-3 py-2 text-sm">
            <div className="flex items-center justify-between gap-2">
              <span className="font-medium text-slate-950">{agent.name}</span>
              <StatusBadge value={agent.status} />
            </div>
            <DetailLine label="ID" value={agent.id} mono />
          </div>
        ))}
      </div>
    </div>
  )
}

function AuditList({ events }: { events: AuditEvent[] }) {
  return (
    <div className="border-t border-slate-200">
      <Subhead title="Audit events" meta={`${events.length} recent`} />
      <div className="divide-y divide-slate-200">
        {events.length === 0 ? <div className="px-4 py-3 text-sm text-slate-600">No audit events</div> : events.map((event) => (
          <div key={event.id} className="px-4 py-3 text-sm">
            <div className="font-medium text-slate-950">{event.action}</div>
            <div className="mt-1 text-xs text-slate-600">{event.actor_type} · {event.target_type} · {formatDate(event.created_at)}</div>
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
  return <button className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white" type="submit">{label}</button>
}

function Subhead({ title, meta }: { title: string; meta: string }) {
  return (
    <div className="flex items-center justify-between border-b border-slate-200 px-4 py-2">
      <h3 className="text-sm font-semibold text-slate-950">{title}</h3>
      <span className="text-xs text-slate-600">{meta}</span>
    </div>
  )
}
