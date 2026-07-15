'use client'

import dayjs from 'dayjs'
import relativeTime from 'dayjs/plugin/relativeTime'
import 'dayjs/locale/zh-cn'
import { useLocale, useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import { PRINTER_ONLINE_AGE_MS } from './printer-presence'

dayjs.extend(relativeTime)

export function PrinterLastSeen({ value, nowMs }: { value: string; nowMs: number }) {
  const t = useTranslations('inventory')
  const locale = useLocale()
  const lastSeen = dayjs(value)
  if (nowMs === 0 || !lastSeen.isValid()) {
    return <FormattedDate value={value} />
  }

  const ageMs = Math.max(0, nowMs - lastSeen.valueOf())
  if (ageMs < PRINTER_ONLINE_AGE_MS) {
    return <>{t('lastSeenOnline')}</>
  }

  const relative = lastSeen
    .locale(locale.toLowerCase().startsWith('zh') ? 'zh-cn' : 'en')
    .from(dayjs(nowMs))
  return <>{t('lastOnline', { relative })}</>
}
