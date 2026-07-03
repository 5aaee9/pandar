import type { Metadata } from 'next'
import { NextIntlClientProvider } from 'next-intl'
import { getLocale, getMessages, getTranslations } from 'next-intl/server'
import type { ReactNode } from 'react'

import './globals.css'
import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import { ThemeProvider, ThemeScript } from '@/components/theme-provider'

export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations('meta')
  return { title: t('title'), description: t('description') }
}

export default async function RootLayout({
  children,
}: Readonly<{
  children: ReactNode
}>) {
  const [locale, messages] = await Promise.all([getLocale(), getMessages()])
  return (
    <html lang={locale} className="font-sans" suppressHydrationWarning>
      <head>
        <ThemeScript />
      </head>
      <body>
        <NextIntlClientProvider locale={locale} messages={messages}>
          <ThemeProvider>
            <TooltipProvider>{children}</TooltipProvider>
          </ThemeProvider>
          <Toaster />
        </NextIntlClientProvider>
      </body>
    </html>
  )
}
