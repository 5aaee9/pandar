import { NextIntlClientProvider } from 'next-intl'
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import en from '../messages/en.json'
import zh from '../messages/zh.json'
import { PrinterLastSeen } from './printer-last-seen'

const LAST_SEEN = '2026-07-02T00:00:00Z'
const LAST_SEEN_MS = Date.parse(LAST_SEEN)

function renderLastSeen(nowMs: number, value = LAST_SEEN, locale = 'en') {
  return render(
    <NextIntlClientProvider locale={locale} messages={locale === 'zh' ? zh : en}>
      <PrinterLastSeen nowMs={nowMs} value={value} />
    </NextIntlClientProvider>,
  )
}

describe('PrinterLastSeen', () => {
  it('switches from Online to relative text at exactly three minutes', () => {
    const { rerender } = renderLastSeen(LAST_SEEN_MS + 179_999)

    expect(screen.getByText('Online')).toBeVisible()
    expect(screen.queryByText(/^Last online/)).not.toBeInTheDocument()

    rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <PrinterLastSeen nowMs={LAST_SEEN_MS + 180_000} value={LAST_SEEN} />
      </NextIntlClientProvider>,
    )

    expect(screen.getByText('Last online 3 minutes ago')).toBeVisible()
    expect(screen.queryByText('Online')).not.toBeInTheDocument()
  })

  it('clamps future timestamps to Online', () => {
    renderLastSeen(LAST_SEEN_MS - 1)

    expect(screen.getByText('Online')).toBeVisible()
  })

  it('keeps the absolute fallback while nowMs is zero', () => {
    renderLastSeen(0)

    expect(screen.getByText(/Jul 2, 2026/)).toBeVisible()
    expect(screen.queryByText('Online')).not.toBeInTheDocument()
  })

  it('keeps the raw fallback for an invalid timestamp', () => {
    renderLastSeen(LAST_SEEN_MS + 180_000, 'invalid-last-seen')

    expect(screen.getByText('invalid-last-seen')).toBeVisible()
    expect(screen.queryByText(/^Last online/)).not.toBeInTheDocument()
  })

  it('localizes relative text in Chinese', () => {
    renderLastSeen(LAST_SEEN_MS + 180_000, LAST_SEEN, 'zh')

    expect(screen.getByText('上次在线：3 分钟前')).toBeVisible()
  })
})
