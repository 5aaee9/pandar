'use client'

import { useRef, useState, type ReactNode } from 'react'
import { useTranslations } from 'next-intl'
import { Loader2Icon } from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel,
  tone = 'danger',
  pending = false,
  onConfirm,
  onCancel,
}: {
  open: boolean
  title: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
  tone?: 'default' | 'danger'
  pending?: boolean
  onConfirm: () => void
  onCancel: () => void
}) {
  const tCommon = useTranslations('common')

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onCancel()
        }
      }}
    >
      <DialogContent showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{message}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button onClick={onCancel} type="button" variant="outline">
            {cancelLabel ?? tCommon('cancel')}
          </Button>
          <Button
            disabled={pending}
            onClick={onConfirm}
            type="button"
            variant={tone === 'danger' ? 'destructive' : 'default'}
          >
            {pending ? <Loader2Icon className="animate-spin" /> : null}
            {confirmLabel ?? tCommon('confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export function ConfirmForm({
  action,
  title,
  message,
  confirmLabel,
  cancelLabel,
  tone = 'danger',
  buttonClassName,
  buttonLabel,
  buttonAriaLabel,
  disabled,
  pending = false,
  children,
}: {
  action: (formData: FormData) => void
  title: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
  tone?: 'default' | 'danger'
  buttonClassName: string
  buttonLabel: ReactNode
  buttonAriaLabel?: string
  disabled?: boolean
  pending?: boolean
  children?: ReactNode
}) {
  const formRef = useRef<HTMLFormElement>(null)
  const [open, setOpen] = useState(false)
  const openDialog = () => setOpen(true)
  const closeDialog = () => setOpen(false)

  return (
    <>
      <form ref={formRef} action={action}>
        {children}
        <button
          aria-label={buttonAriaLabel}
          className={buttonClassName}
          disabled={disabled || pending}
          onClick={openDialog}
          type="button"
        >
          {pending ? <Loader2Icon className="size-4 animate-spin" /> : null}
          {buttonLabel}
        </button>
      </form>
      <ConfirmDialog
        open={open}
        title={title}
        message={message}
        confirmLabel={confirmLabel}
        cancelLabel={cancelLabel}
        tone={tone}
        onConfirm={() => {
          closeDialog()
          formRef.current?.requestSubmit()
        }}
        onCancel={closeDialog}
      />
    </>
  )
}
