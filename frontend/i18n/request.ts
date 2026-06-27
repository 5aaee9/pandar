import { getRequestConfig } from 'next-intl/server'
import { headers, cookies } from 'next/headers'

import { defaultLocale, isLocale, type Locale } from './routing'

export default getRequestConfig(async () => {
  const cookieStore = await cookies()
  const headerList = await headers()
  const cookieLocale = cookieStore.get('locale')?.value
  const acceptLanguage = headerList.get('accept-language') ?? ''
  const locale: Locale = resolveLocale(cookieLocale, acceptLanguage)
  return {
    locale,
    messages: (await import(`../messages/${locale}.json`)).default,
  }
})

function resolveLocale(cookie: string | undefined, acceptLanguage: string): Locale {
  if (isLocale(cookie)) {
    return cookie
  }
  if (/\bzh\b|zh-/i.test(acceptLanguage)) {
    return 'zh'
  }
  return defaultLocale
}
