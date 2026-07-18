import { useActionState } from 'react'
import { useTranslations } from 'next-intl'

import { createJoinLink, revokeJoinLink, updateTenantUserRole } from './admin-actions'
import type { JoinLink, Tenant, User, UserIdentity } from './dashboard-types'
import { EmptyState, Tag } from './dashboard-ui'
import { ConfirmForm } from './confirm-dialog'
import { roles, useAdminDate } from './admin-model'
import { Input, PrimaryButton, SecretActionResult, Select, Subhead } from './admin-panel-shared'

export function CreateJoinLinkForm({ tenantId }: { tenantId: string }) {
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

export function TenantUsersPanel({
  selectedTenant,
  users,
  userIdentities,
  joinLinks,
}: {
  selectedTenant: Tenant
  users: User[]
  userIdentities: UserIdentity[]
  joinLinks: JoinLink[]
}) {
  return (
    <>
      <UsersTable tenantId={selectedTenant.id} users={users} identities={userIdentities} />
      <JoinLinksTable tenantId={selectedTenant.id} joinLinks={joinLinks} />
    </>
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
