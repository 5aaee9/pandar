'use client'

import { useLocale, useTranslations } from 'next-intl'
import { useRouter } from 'next/navigation'
import { useTransition } from 'react'

import { locales, type Locale } from '../i18n/routing'
import { useSettings } from '../lib/settings-store'

const LABELS: Record<Locale, string> = {
  en: 'EN',
  zh: '中文',
}

export function LanguageSwitcher() {
  const active = useLocale() as Locale
  const t = useTranslations('dashboardShell')
  const router = useRouter()
  const [pending, startTransition] = useTransition()

  const choose = (next: Locale) => {
    if (next === active || pending) {
      return
    }
    startTransition(async () => {
      useSettings.setState({ locale: next })
      await fetch('/api/locale', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ locale: next }),
      })
      router.refresh()
    })
  }

  return (
    <div
      aria-label={t('languageTitle')}
      className="inline-flex items-center gap-1 rounded-md border border-border bg-background p-0.5"
      role="group"
    >
      {locales.map((locale) => {
        const isActive = locale === active
        return (
          <button
            key={locale}
            aria-label={locale === 'en' ? 'English' : '中文'}
            aria-pressed={isActive}
            className={`rounded px-2 py-0.5 text-xs font-medium transition-colors ${
              isActive
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:bg-muted hover:text-foreground'
            }`}
            disabled={pending}
            onClick={() => choose(locale)}
            type="button"
          >
            {LABELS[locale]}
          </button>
        )
      })}
    </div>
  )
}
