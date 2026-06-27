'use client'

import { useFormatter } from 'next-intl'

const parseable = (value: string) => {
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? null : date
}

export function FormattedDate({ value }: { value: string }) {
  const date = parseable(value)
  const format = useFormatter()
  if (!date) {
    return <>{value}</>
  }
  return <>{format.dateTime(date, { dateStyle: 'medium', timeStyle: 'short', timeZone: 'UTC' })}</>
}
