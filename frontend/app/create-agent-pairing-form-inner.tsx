'use client'

import { useActionState } from 'react'
import { useTranslations } from 'next-intl'
import { UserPlusIcon } from 'lucide-react'

import { createAgentPairing } from './admin-actions'
import {
  Input,
  PrimaryButton,
  SecretActionResult,
} from './admin-panel-shared'

export function CreateAgentPairingFormInner({
  tenantId,
  onCreateAnother,
}: {
  tenantId: string
  onCreateAnother: () => void
}) {
  const t = useTranslations('admin')
  const [state, formAction, pending] = useActionState(createAgentPairing, null)
  const locked = pending || state?.ok === true

  return (
    <form action={formAction} className="grid gap-3">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <div className="text-sm font-semibold text-foreground">
        {t('pairAgent')}
      </div>
      <Input name="name" label={t('agentName')} required disabled={locked} />
      {locked ? null : (
        <PrimaryButton label={pending ? t('creating') : t('createPairing')} />
      )}
      <SecretActionResult state={state} />
      {state?.ok ? (
        <button
          className="inline-flex items-center gap-1 self-start text-xs font-medium text-primary underline-offset-4 transition-colors duration-150 ease-out hover:text-primary/80 hover:underline"
          onClick={onCreateAnother}
          type="button"
        >
          <UserPlusIcon aria-hidden="true" className="size-3" />
          {t('createAnother')}
        </button>
      ) : null}
    </form>
  )
}
