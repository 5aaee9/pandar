'use client'

import { useEffect, useRef } from 'react'
import { useTranslations } from 'next-intl'
import { toast } from 'sonner'

const knownPositiveActionStatuses = new Set([
  'refresh_queued',
  'refresh_partial',
  'job_created',
  'tenant_created',
  'tenant_token_revoked',
  'join_link_accepted',
  'join_link_revoked',
  'user_created',
  'user_role_updated',
  'identity_linked',
  'retry_queued',
  'retry_partial',
  'reprint_queued',
  'duplicate_queued',
  'printer_control_queued',
])

type ActionStatusTone = 'success' | 'warning' | 'error'

type StatusTranslator = {
  (key: string): string
  has(key: string): boolean
}

export function ActionStatusToast({
  status,
  onConsumed,
}: {
  status?: string
  onConsumed: (status: string) => void
}) {
  const tStatus = useTranslations('runtime.actionStatus')
  const shownStatuses = useRef<Set<string>>(new Set())

  useEffect(() => {
    if (!status || shownStatuses.current.has(status)) {
      return
    }
    shownStatuses.current.add(status)

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
    onConsumed(status)
  }, [status, tStatus, onConsumed])

  return null
}

export function formatActionStatus(status: string, tStatus: StatusTranslator) {
  if (tStatus.has(status)) {
    return tStatus(status)
  }
  return status
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

export function actionStatusTone(status: string): ActionStatusTone {
  if (status.includes('partial')) {
    return 'warning'
  }
  if (status.startsWith('http_') || !knownPositiveActionStatuses.has(status)) {
    return 'error'
  }
  return 'success'
}

export function clearStatusQueryFromUrl() {
  const url = new URL(window.location.href)
  url.searchParams.delete('status')
  const nextUrl = `${url.pathname}${url.search}${url.hash}`
  window.history.replaceState(window.history.state, '', nextUrl)
}
