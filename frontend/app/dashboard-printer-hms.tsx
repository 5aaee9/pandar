'use client'

import { useLocale, useTranslations } from 'next-intl'
import {
  CircleAlertIcon,
  ExternalLinkIcon,
  InfoIcon,
  TriangleAlertIcon,
} from 'lucide-react'

import type { Printer } from './dashboard-types'
import { hmsMessage } from './hms-catalog'

type PrinterHmsItem = NonNullable<Printer['print']>['hms'][number]
type HmsLevel = 'fatal' | 'serious' | 'warning' | 'info' | 'unknown'

export function PrinterHmsPanel({ printer }: { printer: Printer }) {
  const t = useTranslations('inventory')
  const locale = useLocale()
  const hms = (printer.print?.hms ?? []).flatMap((item) => {
    const code = formatHmsCode(item)
    const message = hmsMessage(printer.serial_number, code, locale)
    return message ? [{ item, code, message }] : []
  })
  if (hms.length === 0) {
    return null
  }

  return (
    <section
      aria-label={t('hmsMessages', { count: hms.length })}
      aria-live="polite"
      className="mt-4 rounded-md border border-amber-500/30 bg-amber-500/10 p-3"
    >
      <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
        <TriangleAlertIcon className="size-4 text-amber-600 dark:text-amber-400" />
        {t('hmsMessages', { count: hms.length })}
      </div>
      <ul className="mt-2 divide-y divide-amber-500/20">
        {hms.map(({ item, code, message }, index) => {
          const level = hmsLevel(item.code)
          const Icon = level === 'info' ? InfoIcon : CircleAlertIcon

          return (
            <li className="flex items-center gap-3 py-2 first:pt-0 last:pb-0" key={`${item.attr}-${item.code}-${index}`}>
              <Icon className={`size-4 shrink-0 ${levelColor(level)}`} />
              <div className="min-w-0 flex-1">
                <div className="text-sm font-medium text-foreground">{message}</div>
                <div className="flex flex-wrap gap-x-2 text-xs text-muted-foreground">
                  <span>{levelLabel(level, t)}</span>
                  <code>HMS {code}</code>
                </div>
              </div>
              <a
                className="inline-flex shrink-0 items-center gap-1 text-xs font-medium text-foreground underline-offset-4 hover:underline"
                href={hmsDetailsUrl(code, locale)}
                rel="noreferrer"
                target="_blank"
              >
                {t('hmsViewDetails')}
                <ExternalLinkIcon className="size-3" />
              </a>
            </li>
          )
        })}
      </ul>
    </section>
  )
}

function formatHmsCode(item: PrinterHmsItem) {
  const attr = (item.attr >>> 0).toString(16).padStart(8, '0')
  const rawLevel = (item.code >>> 16) & 0xffff
  const level = rawLevel >= 1 && rawLevel <= 4 ? rawLevel : 0
  const message = (item.code & 0xffff).toString(16).padStart(4, '0')
  return `${attr}00${level.toString(16).padStart(2, '0')}${message}`.toUpperCase()
}

function hmsLevel(code: number): HmsLevel {
  switch ((code >>> 16) & 0xffff) {
    case 1: return 'fatal'
    case 2: return 'serious'
    case 3: return 'warning'
    case 4: return 'info'
    default: return 'unknown'
  }
}

function levelLabel(level: HmsLevel, t: ReturnType<typeof useTranslations>) {
  switch (level) {
    case 'fatal': return t('hmsLevelFatal')
    case 'serious': return t('hmsLevelSerious')
    case 'warning': return t('hmsLevelWarning')
    case 'info': return t('hmsLevelInfo')
    case 'unknown': return t('hmsLevelUnknown')
  }
}

function levelColor(level: HmsLevel) {
  switch (level) {
    case 'fatal':
    case 'serious':
      return 'text-destructive'
    case 'warning':
      return 'text-amber-600 dark:text-amber-400'
    case 'info':
      return 'text-sky-600 dark:text-sky-400'
    case 'unknown':
      return 'text-muted-foreground'
  }
}

function hmsDetailsUrl(code: string, locale: string) {
  const language = locale.toLowerCase().startsWith('zh') ? 'zh-cn' : 'en'
  return `https://e.bambulab.com/index.php?e=${encodeURIComponent(code)}&s=device_hms&lang=${language}`
}
