'use client'

import { useLocale, useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import { getRelativeTime } from './dayjs-relative-time'
import { PRINTER_ONLINE_AGE_MS } from './printer-presence'

export function PrinterLastSeen({ value, nowMs }: { value: string; nowMs: number }) {
  const t = useTranslations('inventory')
  const locale = useLocale()
  const lastSeen = getRelativeTime(value, nowMs, locale)
  if (!lastSeen) {
    return <FormattedDate value={value} />
  }

  const ageMs = Math.max(0, nowMs - lastSeen.timestampMs)
  if (ageMs < PRINTER_ONLINE_AGE_MS) {
    return <>{t('lastSeenOnline')}</>
  }

  return <>{t('lastOnline', { relative: lastSeen.relative })}</>
}
