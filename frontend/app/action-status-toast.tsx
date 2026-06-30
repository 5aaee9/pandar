'use client'

import { useEffect, useRef } from 'react'
import { useTranslations } from 'next-intl'
import { toast } from 'sonner'

import { actionStatusTone, clearStatusQueryFromUrl, formatActionStatus } from './action-status'

export function ActionStatusToast({
  status,
}: {
  status?: string
}) {
  const tStatus = useTranslations('runtime.actionStatus')
  const shownStatuses = useRef<Set<string> | null>(null)
  if (shownStatuses.current === null) {
    shownStatuses.current = new Set()
  }

  useEffect(() => {
    const shown = shownStatuses.current
    if (!status || !shown || shown.has(status)) {
      return
    }
    shown.add(status)

    const message = formatActionStatus(status, tStatus)
    const tone = actionStatusTone(status)
    if (tone === 'warning') {
      toast.warning(message)
    } else if (tone === 'error') {
      toast.error(message)
    } else {
      toast.success(message)
    }
    clearStatusQueryFromUrl()
  }, [status, tStatus])

  return null
}
