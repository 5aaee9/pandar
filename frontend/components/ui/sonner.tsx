'use client'

import { Toaster as Sonner, type ToasterProps } from 'sonner'

function Toaster({ ...props }: ToasterProps) {
  return (
    <Sonner
      className="toaster group"
      closeButton
      richColors
      toastOptions={{
        classNames: {
          toast: 'group toast rounded-md border border-slate-200 bg-white text-slate-950 shadow-lg',
          description: 'text-slate-600',
          actionButton: 'bg-slate-900 text-slate-50',
          cancelButton: 'bg-slate-100 text-slate-900',
        },
      }}
      {...props}
    />
  )
}

export { Toaster }
