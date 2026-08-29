'use client'

import { useActionState } from 'react'
import { useTranslations } from 'next-intl'
import { useQueryClient } from '@tanstack/react-query'
import { UserPlusIcon } from 'lucide-react'

import { createAgentPairing } from './admin-actions'
import { Button } from '@/components/ui/button'
import { mutationResources, useInvalidateOnSuccess } from './mutation-invalidation'
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
  const queryClient = useQueryClient()
  const [state, formAction, pending] = useActionState(createAgentPairing, null)
  useInvalidateOnSuccess(state, queryClient, tenantId, mutationResources.agent)
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
        <Button
          className="h-auto gap-1 self-start px-0 text-xs"
          onClick={onCreateAnother}
          type="button"
          variant="link"
        >
          <UserPlusIcon aria-hidden="true" className="size-3" />
          {t('createAnother')}
        </Button>
      ) : null}
    </form>
  )
}
