import { useActionState, useMemo, useState } from 'react'
import { useTranslations } from 'next-intl'
import { LinkIcon, TrashIcon } from 'lucide-react'

import { createJoinLink, revokeJoinLink, updateTenantUserRole } from './admin-actions'
import {
  actionButtonSm,
  actionButtonSmDanger,
  inputSmClasses,
  monoIdClasses,
  rowHoverClasses,
  tableScrollClasses,
} from '../lib/utils'
import type { JoinLink, Tenant, User, UserIdentity } from './dashboard-types'
import { EmptyState, Tag } from './dashboard-ui'
import { ConfirmForm } from './confirm-dialog'
import { roles, useAdminDate } from './admin-model'
import { Input, PrimaryButton, SecretActionResult, Select, Subhead } from './admin-panel-shared'

export function CreateJoinLinkForm({ tenantId }: { tenantId: string }) {
  const [nonce, setNonce] = useState(0)
  return (
    <CreateJoinLinkFormInner
      key={nonce}
      onCreateAnother={() => setNonce((value) => value + 1)}
      tenantId={tenantId}
    />
  )
}

function CreateJoinLinkFormInner({
  tenantId,
  onCreateAnother,
}: {
  tenantId: string
  onCreateAnother: () => void
}) {
  const t = useTranslations('admin')
  const [state, formAction, pending] = useActionState(createJoinLink, null)
  const locked = pending || state?.ok === true

  return (
    <form action={formAction} className="grid gap-3">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <div className="text-sm font-semibold text-foreground">{t('createJoinLink')}</div>
      <Select name="role" label={t('role')} values={roles} defaultValue="viewer" disabled={locked} />
      <Input name="email_constraint" label={t('verifiedEmail')} type="email" disabled={locked} />
      <div className="grid gap-3 sm:grid-cols-2">
        <Input
          name="expires_in_seconds"
          label={t('ttlSeconds')}
          defaultValue="604800"
          min="1"
          required
          type="number"
          disabled={locked}
        />
        <Input
          name="max_uses"
          label={t('maxUses')}
          defaultValue="1"
          min="1"
          required
          type="number"
          disabled={locked}
        />
      </div>
      {locked ? null : <PrimaryButton label={pending ? t('creating') : t('createLink')} />}
      <SecretActionResult state={state} />
      {state?.ok ? (
        <button
          className="inline-flex items-center gap-1 self-start text-xs font-medium text-primary underline-offset-4 transition-colors duration-150 ease-out hover:text-primary/80 hover:underline"
          onClick={onCreateAnother}
          type="button"
        >
          <LinkIcon aria-hidden="true" className="size-3" />
          {t("createAnother")}
        </button>
      ) : null}
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
  const identitiesByUser = useMemo(() => {
    const map = new Map<string, UserIdentity[]>()
    for (const identity of identities) {
      const current = map.get(identity.user_id) ?? []
      current.push(identity)
      map.set(identity.user_id, current)
    }
    return map
  }, [identities])
  return (
    <div>
      <Subhead title={t('users')} meta={t('usersMeta', { count: users.length })} />
      {users.length === 0 ? (
        <EmptyState title={t('noUsersTitle')} message={t('noUsersMessage')} />
      ) : (
        <div className={tableScrollClasses}>
          <table className="min-w-full text-left text-sm">
            <thead className="bg-muted/60 text-xs font-semibold text-muted-foreground">
              <tr>
                <th className="px-4 py-2.5">{t('colUser')}</th>
                <th className="px-4 py-2.5">{t('colRole')}</th>
                <th className="px-4 py-2.5">{t('colIdentities')}</th>
                <th className="px-4 py-2.5">{t('colUpdate')}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {users.map((user) => {
                const linked = identitiesByUser.get(user.id) ?? []
                return (
                  <UserRow
                    key={user.id}
                    tenantId={tenantId}
                    user={user}
                    linked={linked}
                    roles={roles}
                    t={t}
                  />
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

function UserRow({
  tenantId,
  user,
  linked,
  roles,
  t,
}: {
  tenantId: string
  user: User
  linked: UserIdentity[]
  roles: string[]
  t: ReturnType<typeof useTranslations>
}) {
  return (
    <tr key={user.id} className={rowHoverClasses}>
      <td className="px-4 py-3">
        <div className="font-medium text-foreground">{user.display_name}</div>
        <div className="text-muted-foreground">{user.email}</div>
        <div className={monoIdClasses}>{user.id}</div>
      </td>
      <td className="px-4 py-3"><Tag value={user.role} /></td>
      <td className="px-4 py-3 text-xs text-muted-foreground">
        {linked.length === 0 ? '-' : linked.map((identity) => identity.provider).join(', ')}
      </td>
      <td className="px-4 py-3">
        <form action={updateTenantUserRole} className="flex items-center gap-2">
          <input name="tenant_id" type="hidden" value={tenantId} />
          <input name="user_id" type="hidden" value={user.id} />
          <select
            aria-label={t('roleFor', { user: user.display_name })}
            name="role"
            defaultValue={user.role}
            className={inputSmClasses}
          >
            {roles.map((role) => <option key={role} value={role}>{role}</option>)}
          </select>
          <button
            aria-label={t('saveRoleFor', { user: user.display_name })}
            className={actionButtonSm}
            type="submit"
          >
            {t('save')}
          </button>
        </form>
      </td>
    </tr>
  )
}

function JoinLinksTable({ tenantId, joinLinks }: { tenantId: string; joinLinks: JoinLink[] }) {
  const t = useTranslations('admin')
  const formatDate = useAdminDate()
  return (
    <div className="border-t border-border">
      <Subhead title={t('joinLinks')} meta={t('joinLinksMeta', { count: joinLinks.length })} />
      {joinLinks.length === 0 ? (
        <EmptyState title={t('noJoinLinksTitle')} message={t('noJoinLinksMessage')} />
      ) : (
        <div className="divide-y divide-border">
          {joinLinks.map((link) => (
            <div
              key={link.id}
              className={`grid gap-3 px-4 py-3 text-sm ${rowHoverClasses} lg:grid-cols-[minmax(0,1fr)_auto]`}
            >
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <Tag value={link.role} />
                  <span className="text-xs text-muted-foreground">
                    {t('usedRatio', { used: link.used_count, max: link.max_uses })}
                  </span>
                  {link.revoked_at ? (
                    <span className="inline-flex items-center gap-1 rounded-md border border-destructive/40 bg-destructive/10 px-1.5 py-0.5 text-xs font-medium text-destructive">
                      <TrashIcon aria-hidden="true" className="size-3" />
                      {t('revoked')}
                    </span>
                  ) : null}
                </div>
                <div className={`mt-1.5 ${monoIdClasses}`}>{link.id}</div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {link.email_constraint ? t('emailConstraint', { email: link.email_constraint }) : t('anyVerifiedEmail')} · {t('expires', { date: formatDate(link.expires_at) })}
                </div>
              </div>
              <ConfirmForm
                action={revokeJoinLink}
                buttonClassName={actionButtonSmDanger}
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
