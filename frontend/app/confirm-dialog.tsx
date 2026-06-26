'use client'

import { useEffect, useRef, useState, type ReactNode } from 'react'

import { FOCUS_RING } from './dashboard-status'

const DIALOG_CLASS =
  'pandar-dialog m-0 flex h-screen w-screen max-w-none items-center justify-center bg-transparent p-0'
const CARD_CLASS =
  'pandar-dialog-card w-[calc(100vw-2rem)] max-w-md rounded-lg border border-slate-300 bg-white p-5 shadow-xl'

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
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
  const ref = useRef<HTMLDialogElement>(null)

  useEffect(() => {
    const dialog = ref.current
    if (!dialog) {
      return
    }
    if (open && !dialog.open) {
      dialog.showModal()
    } else if (!open && dialog.open) {
      dialog.close()
    }
  }, [open])

  return (
    <dialog
      ref={ref}
      aria-label={title}
      aria-modal="true"
      className={DIALOG_CLASS}
      onClose={onCancel}
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onCancel()
        }
      }}
    >
      <div className={CARD_CLASS}>
        <h2 className="text-base font-semibold text-slate-900">{title}</h2>
        <p className="mt-1.5 text-sm text-slate-600">{message}</p>
        <div className="mt-5 flex justify-end gap-2">
          <button
            autoFocus
            className={`h-9 rounded-md border border-slate-300 bg-white px-3 text-sm font-medium text-slate-800 hover:bg-slate-50 ${FOCUS_RING}`}
            onClick={onCancel}
            type="button"
          >
            {cancelLabel}
          </button>
          <button
            className={`h-9 rounded-md border border-transparent px-3 text-sm font-medium text-white ${FOCUS_RING} ${
              tone === 'danger' ? 'bg-red-600 hover:bg-red-700' : 'bg-cyan-700 hover:bg-cyan-800'
            }`}
            onClick={onConfirm}
            type="button"
          >
            {confirmLabel}
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
  disabled?: boolean
  children?: ReactNode
}) {
  const formRef = useRef<HTMLFormElement>(null)
  const [open, setOpen] = useState(false)

  return (
    <>
      <form ref={formRef} action={action}>
        {children}
        <button
          className={buttonClassName}
          disabled={disabled}
          onClick={() => setOpen(true)}
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
          setOpen(false)
          formRef.current?.requestSubmit()
        }}
        onCancel={() => setOpen(false)}
      />
    </>
  )
}
