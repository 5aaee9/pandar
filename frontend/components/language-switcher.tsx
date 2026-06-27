'use client'

import { useLocale } from 'next-intl'
import { useRouter } from 'next/navigation'
import { useTransition } from 'react'

import { setLocale } from '../i18n/actions'
import { locales, type Locale } from '../i18n/routing'
import { useSettings } from '../lib/settings-store'

const LABELS: Record<Locale, string> = {
  en: 'EN',
  zh: '中文',
}

export function LanguageSwitcher() {
  const active = useLocale() as Locale
  const router = useRouter()
  const [pending, startTransition] = useTransition()

  const choose = (next: Locale) => {
    if (next === active || pending) {
      return
    }
    startTransition(async () => {
      useSettings.setState({ locale: next })
      await setLocale(next)
      router.refresh()
    })
  }

  return (
    <div className="inline-flex items-center gap-1 rounded-md border border-slate-300 bg-white p-0.5">
      {locales.map((locale) => {
        const isActive = locale === active
        return (
          <button
            key={locale}
            className={`rounded px-2 py-0.5 text-xs font-medium transition-colors ${
              isActive ? 'bg-slate-900 text-white' : 'text-slate-600 hover:bg-slate-100'
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
