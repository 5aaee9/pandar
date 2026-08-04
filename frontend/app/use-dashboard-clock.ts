'use client'

import { useEffect, useState } from 'react'

import type { Printer } from './dashboard-types'
import { PRINTER_ONLINE_AGE_MS } from './printer-presence'

const CLOCK_INTERVAL_MS = 60_000
const MAX_TIMEOUT_MS = 2_147_483_647

function scheduleDeadlineUpdates(deadlines: number[], update: () => void) {
  let timer: ReturnType<typeof setTimeout> | undefined
  let stopped = false

  const scheduleNext = () => {
    if (stopped) {
      return
    }
    const current = Date.now()
    const deadline = deadlines.find((candidate) => candidate > current)
    if (deadline === undefined) {
      return
    }
    timer = setTimeout(() => {
      timer = undefined
      update()
      scheduleNext()
    }, Math.min(deadline - current, MAX_TIMEOUT_MS))
  }

  scheduleNext()
  return () => {
    stopped = true
    if (timer !== undefined) {
      clearTimeout(timer)
    }
  }
}

export function useDashboardClock(printers: readonly Pick<Printer, 'last_seen_at'>[]) {
  const [nowMs, setNowMs] = useState(0)

  useEffect(() => {
    const deadlines: number[] = []
    for (const printer of printers) {
      const lastSeenMs = Date.parse(printer.last_seen_at)
      if (Number.isFinite(lastSeenMs)) {
        deadlines.push(lastSeenMs + PRINTER_ONLINE_AGE_MS)
      }
    }
    deadlines.sort((left, right) => left - right)

    const update = () => setNowMs(Date.now())
    update()
    const interval = setInterval(update, CLOCK_INTERVAL_MS)
    const stopDeadlineUpdates = scheduleDeadlineUpdates(deadlines, update)
    return () => {
      clearInterval(interval)
      stopDeadlineUpdates()
    }
  }, [printers])

  return nowMs
}
