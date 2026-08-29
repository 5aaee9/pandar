'use client'

import { useId, type ReactNode } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'

import { Button } from '@/components/ui/button'
import { HoverCard, HoverCardContent, HoverCardTrigger } from '@/components/ui/hover-card'

import { ConfirmForm } from './confirm-dialog'
import type { AgentDeleteResult } from './actions'
import {
  invalidateTenantResources,
  mutationResources,
} from './mutation-invalidation'

export function AgentDeleteForm({
  action,
  tenantId,
  title,
  message,
  confirmLabel,
  buttonLabel,
  buttonAriaLabel,
  disabled,
  disabledMessage,
  children,
}: {
  action: (formData: FormData) => Promise<AgentDeleteResult>
  tenantId: string
  title: string
  message: string
  confirmLabel: string
  buttonLabel: string
  buttonAriaLabel: string
  disabled?: boolean
  disabledMessage?: string
  children?: ReactNode
}) {
  const disabledMessageId = useId()
  const queryClient = useQueryClient()

  async function submit(formData: FormData) {
    const result = await action(formData)
    if (result.ok) {
      await invalidateTenantResources(
        queryClient,
        tenantId,
        mutationResources.agent,
      )
    }
    window.location.assign(result.redirectUrl)
  }

  if (disabled && disabledMessage) {
    return (
      <HoverCard>
        <HoverCardTrigger
          delay={0}
          render={
            <Button
              aria-describedby={disabledMessageId}
              aria-disabled="true"
              aria-label={buttonAriaLabel}
              className="cursor-not-allowed opacity-50"
              onClick={(event) => {
                event.preventDefault()
                toast.warning(disabledMessage)
              }}
              size="sm"
              type="button"
              variant="destructive"
            />
          }
        >
          {buttonLabel}
          <span className="sr-only" id={disabledMessageId}>
            {disabledMessage}
          </span>
        </HoverCardTrigger>
        <HoverCardContent align="end" className="w-auto max-w-xs text-sm text-foreground" side="top">
          {disabledMessage}
        </HoverCardContent>
      </HoverCard>
    )
  }

  return (
    <ConfirmForm
      action={submit}
      buttonAriaLabel={buttonAriaLabel}
      buttonLabel={buttonLabel}
      buttonSize="sm"
      buttonVariant="destructive"
      disabled={disabled}
      title={title}
      message={message}
      confirmLabel={confirmLabel}
      tone="danger"
    >
      {children}
    </ConfirmForm>
  )
}
