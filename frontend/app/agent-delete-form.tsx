'use client'

import { useId, type ReactNode } from 'react'
import { toast } from 'sonner'

import { HoverCard, HoverCardContent, HoverCardTrigger } from '@/components/ui/hover-card'

import { ConfirmForm } from './confirm-dialog'

const DELETE_BUTTON_CLASS =
  'h-9 rounded-md border border-red-300 px-3 text-sm font-medium text-red-700 disabled:pointer-events-none disabled:border-slate-200 disabled:text-slate-400'

export function AgentDeleteForm({
  action,
  title,
  message,
  confirmLabel,
  buttonLabel,
  disabled,
  disabledMessage,
  children,
}: {
  action: (formData: FormData) => void
  title: string
  message: string
  confirmLabel: string
  buttonLabel: string
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
          render={<span className="inline-flex cursor-not-allowed" onClick={() => toast.warning(disabledMessage)} />}
        >
          <button aria-describedby={disabledMessageId} className={DELETE_BUTTON_CLASS} disabled type="button">
            {buttonLabel}
          </button>
          <span className="sr-only" id={disabledMessageId}>
            {disabledMessage}
          </span>
        </HoverCardTrigger>
        <HoverCardContent align="end" className="w-auto max-w-xs text-sm text-slate-700" side="top">
          {disabledMessage}
        </HoverCardContent>
      </HoverCard>
    )
  }

  return (
    <ConfirmForm
      action={action}
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
