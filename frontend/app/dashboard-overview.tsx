'use client'

import { useEffect, useState } from 'react'

import type { AttentionItem, Health, Severity } from './dashboard-attention'
import {
  AttentionRow,
  computeVerdict,
  StatCell,
  StatusIcon,
} from './dashboard-status'
import type { LiveState } from './dashboard-runtime-helpers'
import type { Tenant } from './dashboard-types'

export type NavSection = { id: string; label: string }

export const NAV_SECTIONS: NavSection[] = [
  { id: 'printers', label: 'Printers' },
  { id: 'jobs', label: 'Print jobs' },
  { id: 'dispatch', label: 'Dispatch' },
  { id: 'recovery', label: 'Recovery' },
  { id: 'diagnostics', label: 'Diagnostics' },
  { id: 'activity', label: 'Live activity' },
  { id: 'admin', label: 'Admin' },
]

export function FleetStatusStrip({
  health,
  attentionCount,
  topSeverity,
  liveState,
  lastEventAt,
  fleetEmpty,
}: {
  health: Health
  attentionCount: number
  topSeverity: Severity | null
  liveState: LiveState
  lastEventAt: string | null
  fleetEmpty: boolean
}) {
  const verdict = computeVerdict({ attentionCount, topSeverity, liveState, fleetEmpty })

  return (
    <section
      aria-label="Fleet status"
      className={`overflow-hidden rounded-lg border ${verdict.tone.border} ${verdict.tone.surface}`}
    >
      <div className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:gap-5">
        <div className="flex min-w-0 items-center gap-3">
          <StatusIcon severity={verdict.severity} className="h-6 w-6 shrink-0" />
          <div className="min-w-0">
            <div className={`text-sm font-semibold ${verdict.tone.ink}`}>{verdict.title}</div>
            <div className={`mt-0.5 text-xs ${verdict.tone.sub}`}>{verdict.detail}</div>
          </div>
        </div>
        <div
          className="grid flex-1 grid-cols-2 gap-3 sm:grid-cols-3 sm:gap-0 sm:divide-x sm:divide-slate-200 sm:border-l sm:border-slate-200 sm:pl-5"
          aria-hidden={fleetEmpty}
        >
          <StatCell
            href="#printers"
            label="Printers"
            value={fleetEmpty ? '—' : `${health.printersOnline}/${health.printersTotal} online`}
            note={health.printersTotal - health.printersOnline > 0 ? `${health.printersTotal - health.printersOnline} offline` : null}
            state={health.printersOnline < health.printersTotal ? 'warning' : 'success'}
          />
          <StatCell
            href="#printers"
            label="Agents"
            value={fleetEmpty ? '—' : `${health.agentsConnected}/${health.agentsTotal} connected`}
            note={health.agentsTotal - health.agentsConnected > 0 ? `${health.agentsTotal - health.agentsConnected} down` : null}
            state={health.agentsConnected < health.agentsTotal ? 'warning' : 'success'}
          />
          <StatCell
            href="#jobs"
            label="Active jobs"
            value={fleetEmpty ? '—' : `${health.jobsActive} active`}
            note={health.jobsFailed > 0 ? `${health.jobsFailed} failed` : null}
            state={health.jobsFailed > 0 ? 'critical' : 'success'}
          />
        </div>
      </div>
    </section>
  )
}

export function NeedsAttention({
  items,
  selectedTenant,
}: {
  items: AttentionItem[]
  selectedTenant: Tenant | null
}) {
  if (items.length === 0) {
    return null
  }

  let lastAgent = ''
  let groupIndex = -1

  return (
    <section
      aria-label="Needs attention"
      className="overflow-hidden rounded-lg border border-slate-200 bg-white"
    >
      <div className="flex items-center justify-between border-b border-slate-200 px-4 py-3">
        <div>
          <h2 className="text-base font-semibold text-slate-900">Needs attention</h2>
          <p className="mt-0.5 text-sm text-slate-600">
            {items.length} {items.length === 1 ? 'exception' : 'exceptions'} across the fleet
          </p>
        </div>
        <span className="text-xs text-slate-600">Grouped by agent</span>
      </div>
      <ul className="divide-y divide-slate-200">
        {items.map((item) => {
          const showGroup = item.agentName !== lastAgent
          if (showGroup) {
            lastAgent = item.agentName
            groupIndex += 1
          }
          return (
            <AttentionRow
              key={item.id}
              item={item}
              showGroup={showGroup}
              zebra={groupIndex % 2 === 1}
              tenant={selectedTenant}
            />
          )
        })}
      </ul>
    </section>
  )
}

export function SectionNav({
  attentionBySection,
}: {
  attentionBySection: Record<string, number>
}) {
  const active = useActiveSection(NAV_SECTIONS.map((section) => section.id))
  return (
    <nav aria-label="Sections" className="sticky top-0 z-20 border-y border-slate-200 bg-slate-100/95 backdrop-blur">
      <ul className="flex gap-1 overflow-x-auto px-1 py-2">
        {NAV_SECTIONS.map((section) => {
          const count = attentionBySection[section.id] ?? 0
          const isActive = active === section.id
          return (
            <li key={section.id}>
              <a
                href={`#${section.id}`}
                aria-current={isActive ? 'true' : undefined}
                className={`flex items-center gap-1.5 whitespace-nowrap rounded-md px-2.5 py-1 text-sm transition-colors ${
                  isActive
                    ? 'bg-slate-900 text-white'
                    : 'text-slate-600 hover:bg-slate-200/60 hover:text-slate-900'
                }`}
              >
                {section.label}
                {count > 0 ? (
                  <span
                    className={`inline-flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[10px] font-semibold ${
                      isActive ? 'bg-white/20 text-white' : 'bg-red-100 text-red-700'
                    }`}
                  >
                    {count}
                  </span>
                ) : null}
              </a>
            </li>
          )
        })}
      </ul>
    </nav>
  )
}

function useActiveSection(ids: string[]) {
  const [active, setActive] = useState(ids[0] ?? '')
  const key = ids.join(',')
  useEffect(() => {
    const visible = new Map<string, number>()
    const elements = ids
      .map((id) => document.getElementById(id))
      .filter((element): element is HTMLElement => element !== null)
    if (elements.length === 0) {
      return
    }
    const pick = () => {
      let best: { id: string; ratio: number } | null = null
      for (const [id, ratio] of visible) {
        if (ratio > 0 && (!best || ratio > best.ratio)) {
          best = { id, ratio }
        }
      }
      if (best) {
        setActive(best.id)
      }
    }
    const observers: IntersectionObserver[] = []
    for (const element of elements) {
      const id = element.id
      const observer = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            visible.set(id, entry.isIntersecting ? entry.intersectionRatio : 0)
          }
          pick()
        },
        { rootMargin: '-20% 0px -70% 0px', threshold: [0, 0.25, 0.5, 1] },
      )
      observer.observe(element)
      observers.push(observer)
    }
    return () => observers.forEach((observer) => observer.disconnect())
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key])
  return active
}
