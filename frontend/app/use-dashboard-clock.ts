'use client'

import { useEffect, useState } from 'react'

import type { Printer } from './dashboard-types'
import { PRINTER_ONLINE_AGE_MS } from './printer-presence'

const CLOCK_INTERVAL_MS = 60_000
const MAX_TIMEOUT_MS = 2_147_483_647

export function useDashboardClock(printers: readonly Pick<Printer, 'last_seen_at'>[]) {
  const [nowMs, setNowMs] = useState(0)

  useEffect(() => {
    let deadlineTimer: ReturnType<typeof setTimeout> | undefined
    let disposed = false
    const deadlines = printers
      .map((printer) => Date.parse(printer.last_seen_at))
      .filter((value) => Number.isFinite(value))
      .map((value) => value + PRINTER_ONLINE_AGE_MS)
      .sort((left, right) => left - right)

    const update = () => setNowMs(Date.now())
    const scheduleNextDeadline = () => {
      if (disposed) {
        return
      }
      const current = Date.now()
      const deadline = deadlines.find((candidate) => candidate > current)
      if (deadline === undefined) {
        return
      }
      deadlineTimer = setTimeout(() => {
        deadlineTimer = undefined
        update()
        scheduleNextDeadline()
      }, Math.min(deadline - current, MAX_TIMEOUT_MS))
    }

    update()
    const interval = setInterval(update, CLOCK_INTERVAL_MS)
    scheduleNextDeadline()
    return () => {
      disposed = true
      clearInterval(interval)
      if (deadlineTimer !== undefined) {
        clearTimeout(deadlineTimer)
      }
    }
  }, [printers])

  return nowMs
}
