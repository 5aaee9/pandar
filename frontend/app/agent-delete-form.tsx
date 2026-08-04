'use client'

import { useId, type ReactNode } from 'react'
import { toast } from 'sonner'

import { HoverCard, HoverCardContent, HoverCardTrigger } from '@/components/ui/hover-card'

import { ConfirmForm } from './confirm-dialog'

const DELETE_BUTTON_CLASS =
  'h-9 rounded-md border border-destructive/40 px-3 text-sm font-medium text-destructive transition-colors duration-150 ease-out hover:bg-destructive/10 disabled:pointer-events-none disabled:border-border disabled:text-muted-foreground'

export function AgentDeleteForm({
  action,
  title,
  message,
  confirmLabel,
  buttonLabel,
  buttonAriaLabel,
  disabled,
  disabledMessage,
  children,
}: {
  action: (formData: FormData) => void
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

  if (disabled && disabledMessage) {
    return (
      <HoverCard>
        <HoverCardTrigger
          delay={0}
          render={
            <button
              aria-describedby={disabledMessageId}
              aria-disabled="true"
              aria-label={buttonAriaLabel}
              className={`${DELETE_BUTTON_CLASS} cursor-not-allowed`}
              onClick={(event) => {
                event.preventDefault()
                toast.warning(disabledMessage)
              }}
              type="button"
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
      action={action}
      buttonAriaLabel={buttonAriaLabel}
      buttonClassName={DELETE_BUTTON_CLASS}
      buttonLabel={buttonLabel}
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
