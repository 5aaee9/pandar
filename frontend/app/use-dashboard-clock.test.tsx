import { NextIntlClientProvider } from 'next-intl'
import { act, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import en from '../messages/en.json'
import { PrinterLastSeen } from './printer-last-seen'
import { useDashboardClock } from './use-dashboard-clock'

const LAST_SEEN_MS = Date.parse('2026-07-15T00:00:00Z')

type ClockPrinter = { last_seen_at: string }

function ClockPresence({ printers }: { printers: ClockPrinter[] }) {
  const nowMs = useDashboardClock(printers)
  return (
    <>
      {printers.map((printer, index) => (
        <span data-testid={`presence-${index}`} key={`${printer.last_seen_at}:${index}`}>
          <PrinterLastSeen nowMs={nowMs} value={printer.last_seen_at} />
        </span>
      ))}
    </>
  )
}

function renderClock(printers: ClockPrinter[]) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      <ClockPresence printers={printers} />
    </NextIntlClientProvider>,
  )
}

describe('useDashboardClock', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('updates the production presence path at the exact three-minute deadline', async () => {
    vi.setSystemTime(LAST_SEEN_MS + 179_000)
    renderClock([{ last_seen_at: new Date(LAST_SEEN_MS).toISOString() }])

    expect(screen.getByText('Online')).toBeVisible()

    await act(() => vi.advanceTimersByTimeAsync(999))
    expect(screen.getByText('Online')).toBeVisible()

    await act(() => vi.advanceTimersByTimeAsync(1))
    expect(screen.getByText('Last online 3 minutes ago')).toBeVisible()
  })

  it('cleans up the old deadline and schedules a refreshed printer deadline', async () => {
    vi.setSystemTime(LAST_SEEN_MS + 179_000)
    const initial = [{ last_seen_at: new Date(LAST_SEEN_MS).toISOString() }]
    const { rerender, unmount } = renderClock(initial)
    expect(vi.getTimerCount()).toBe(2)
    const refreshed = [{ last_seen_at: new Date(Date.now()).toISOString() }]

    rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <ClockPresence printers={refreshed} />
      </NextIntlClientProvider>,
    )
    expect(vi.getTimerCount()).toBe(2)

    await act(() => vi.advanceTimersByTimeAsync(1_000))
    expect(screen.getByText('Online')).toBeVisible()

    await act(() => vi.advanceTimersByTimeAsync(179_000))
    expect(screen.getByText('Last online 3 minutes ago')).toBeVisible()

    unmount()
    expect(vi.getTimerCount()).toBe(0)
  })

  it('updates printers in order across multiple deadlines', async () => {
    vi.setSystemTime(LAST_SEEN_MS + 179_000)
    renderClock([
      { last_seen_at: new Date(LAST_SEEN_MS).toISOString() },
      { last_seen_at: new Date(LAST_SEEN_MS + 30_000).toISOString() },
    ])

    expect(screen.getByTestId('presence-0')).toHaveTextContent('Online')
    expect(screen.getByTestId('presence-1')).toHaveTextContent('Online')

    await act(() => vi.advanceTimersByTimeAsync(1_000))
    expect(screen.getByTestId('presence-0')).toHaveTextContent('Last online 3 minutes ago')
    expect(screen.getByTestId('presence-1')).toHaveTextContent('Online')

    await act(() => vi.advanceTimersByTimeAsync(30_000))
    expect(screen.getByTestId('presence-1')).toHaveTextContent('Last online 3 minutes ago')
  })

  it('ignores invalid timestamps without blocking a valid deadline', async () => {
    vi.setSystemTime(LAST_SEEN_MS + 179_000)
    renderClock([
      { last_seen_at: 'invalid-last-seen' },
      { last_seen_at: new Date(LAST_SEEN_MS).toISOString() },
    ])

    expect(screen.getByTestId('presence-0')).toHaveTextContent('invalid-last-seen')
    expect(screen.getByTestId('presence-1')).toHaveTextContent('Online')

    await act(() => vi.advanceTimersByTimeAsync(1_000))
    expect(screen.getByTestId('presence-1')).toHaveTextContent('Last online 3 minutes ago')
  })

  it('requeues the same deadline when a timer fires before wall time reaches it', async () => {
    vi.setSystemTime(LAST_SEEN_MS + 179_000)
    renderClock([{ last_seen_at: new Date(LAST_SEEN_MS).toISOString() }])

    vi.setSystemTime(LAST_SEEN_MS + 178_000)
    await act(() => vi.advanceTimersByTimeAsync(1_000))
    expect(screen.getByText('Online')).toBeVisible()

    await act(() => vi.advanceTimersByTimeAsync(999))
    expect(screen.getByText('Online')).toBeVisible()

    await act(() => vi.advanceTimersByTimeAsync(1))
    expect(screen.getByText('Last online 3 minutes ago')).toBeVisible()
  })
})
