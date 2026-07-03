'use client'

import { useTranslations } from 'next-intl'

import type { Health, Severity } from './dashboard-attention'
import {
  StatCell,
  StatusIcon,
} from './dashboard-status'
import { computeVerdict } from './dashboard-status-model'
import type { LiveState } from './dashboard-runtime-helpers'
import { dashboardSidebarHref } from './dashboard-shell'

export function FleetStatusStrip({
  health,
  attentionCount,
  topSeverity,
  liveState,
  lastEventAt,
  fleetEmpty,
  tenantId,
}: {
  health: Health
  attentionCount: number
  topSeverity: Severity | null
  liveState: LiveState
  lastEventAt: string | null
  fleetEmpty: boolean
  tenantId?: string
}) {
  const t = useTranslations('overview.verdict')
  const tStat = useTranslations('overview.stat')
  const tAria = useTranslations('overview')
  const verdict = computeVerdict({ attentionCount, topSeverity, liveState, fleetEmpty }, t)
  const dashboardQuery = { tenant: tenantId }

  return (
    <section
      aria-label={tAria('ariaFleet')}
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
          className="grid flex-1 grid-cols-2 gap-3 sm:grid-cols-3 sm:gap-0 sm:pl-5"
          aria-hidden={fleetEmpty}
        >
          <StatCell
            href="#printers"
            label={tStat('printers')}
            value={fleetEmpty ? tStat('dash') : tStat('printersValue', { online: health.printersOnline, total: health.printersTotal })}
            note={health.printersTotal - health.printersOnline > 0 ? tStat('printersNote', { count: health.printersTotal - health.printersOnline }) : null}
            state={health.printersOnline < health.printersTotal ? 'warning' : 'success'}
          />
          <StatCell
            href={dashboardSidebarHref('agents', dashboardQuery)}
            label={tStat('agents')}
            value={fleetEmpty ? tStat('dash') : tStat('agentsValue', { connected: health.agentsConnected, total: health.agentsTotal })}
            note={health.agentsTotal - health.agentsConnected > 0 ? tStat('agentsNote', { count: health.agentsTotal - health.agentsConnected }) : null}
            separatorClassName={verdict.tone.separator}
            state={health.agentsConnected < health.agentsTotal ? 'warning' : 'success'}
          />
          <StatCell
            href={dashboardSidebarHref('jobs', dashboardQuery)}
            label={tStat('activeJobs')}
            value={fleetEmpty ? tStat('dash') : tStat('activeJobsValue', { count: health.jobsActive })}
            note={health.jobsFailed > 0 ? tStat('activeJobsNote', { count: health.jobsFailed }) : null}
            separatorClassName={verdict.tone.separator}
            state={health.jobsFailed > 0 ? 'critical' : 'success'}
          />
        </div>
      </div>
    </section>
  )
}
