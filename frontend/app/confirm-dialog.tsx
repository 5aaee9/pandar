'use client'

import { useRef, useState, type ReactNode } from 'react'
import { useTranslations } from 'next-intl'

const DIALOG_CLASS =
  'pandar-dialog m-0 flex h-screen w-screen max-w-none items-center justify-center bg-transparent p-0'
const CARD_CLASS =
  'pandar-dialog-card w-[calc(100vw-2rem)] max-w-md rounded-lg border border-slate-300 bg-white p-5 shadow-xl'

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel,
  tone = 'danger',
  onConfirm,
  onCancel,
}: {
  open: boolean
  title: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
  tone?: 'default' | 'danger'
  onConfirm: () => void
  onCancel: () => void
}) {
  const tCommon = useTranslations('common')
  const ref = useRef<HTMLDialogElement>(null)

  return (
    <dialog
      ref={ref}
      aria-label={title}
      aria-modal="true"
      className={`${DIALOG_CLASS} ${open ? '' : 'hidden'}`}
      onClose={onCancel}
      open={open}
    >
      <div className={CARD_CLASS}>
        <h2 className="text-base font-semibold text-slate-900">{title}</h2>
        <p className="mt-1.5 text-sm text-slate-600">{message}</p>
        <div className="mt-5 flex justify-end gap-2">
          <button
            className="h-9 rounded-md border border-slate-300 bg-white px-3 text-sm font-medium text-slate-800 hover:bg-slate-50"
            onClick={onCancel}
            type="button"
          >
            {cancelLabel ?? tCommon('cancel')}
          </button>
          <button
            className={`h-9 rounded-md border border-transparent px-3 text-sm font-medium ${
              tone === 'danger' ? 'bg-red-600 text-white hover:bg-red-700' : 'bg-primary text-primary-foreground hover:bg-primary/80'
            }`}
            onClick={onConfirm}
            type="button"
          >
            {confirmLabel ?? tCommon('confirm')}
          </button>
        </div>
      </div>
    </dialog>
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
  children,
}: {
  action: (formData: FormData) => void
  title: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
  tone?: 'default' | 'danger'
  buttonClassName: string
  buttonLabel: string
  buttonAriaLabel?: string
  disabled?: boolean
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
          disabled={disabled}
          onClick={openDialog}
          type="button"
        >
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
