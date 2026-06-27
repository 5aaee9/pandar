import type { Agent, Job, Printer } from './dashboard-types'

export const STALE_MS = 15 * 60 * 1000

export type Severity = 'critical' | 'warning' | 'success' | 'info'

export const OFFLINE_PRINTER_STATUSES = new Set(['offline', 'problem'])
const ONLINE_AGENT_STATUSES = new Set(['online'])
const HEALTHY_AGENT_STATUSES = new Set(['online', 'connecting'])
const TERMINAL_JOB_STATUSES = new Set(['completed', 'failed', 'cancelled'])

const SEVERITY_RANK: Record<Severity, number> = {
  critical: 0,
  warning: 1,
  info: 2,
  success: 3,
}

export type Health = {
  printersTotal: number
  printersOnline: number
  agentsTotal: number
  agentsConnected: number
  jobsActive: number
  jobsFailed: number
}

export function computeHealth(agents: Agent[], printers: Printer[], jobs: Job[]): Health {
  return {
    printersTotal: printers.length,
    printersOnline: printers.filter((printer) => !OFFLINE_PRINTER_STATUSES.has(printer.status.toLowerCase())).length,
    agentsTotal: agents.length,
    agentsConnected: agents.filter((agent) => ONLINE_AGENT_STATUSES.has(agent.status.toLowerCase())).length,
    jobsActive: jobs.filter(isJobActive).length,
    jobsFailed: jobs.filter(isJobFailed).length,
  }
}

function isJobActive(job: Job): boolean {
  return (
    !TERMINAL_JOB_STATUSES.has(job.status.toLowerCase()) &&
    !TERMINAL_JOB_STATUSES.has(job.print.status.toLowerCase())
  )
}

function isJobFailed(job: Job): boolean {
  return job.status.toLowerCase() === 'failed' || job.print.status.toLowerCase() === 'failed'
}

export type AttentionItem = {
  id: string
  agentId: string
  agentName: string
  severity: Severity
  kind: 'agent' | 'printer' | 'job'
  title: string
  label: string
  mono: string
  sectionId: string
  ageMs: number | null
}

export function computeAttention(args: {
  agents: Agent[]
  printers: Printer[]
  jobs: Job[]
  nowMs: number
}): AttentionItem[] {
  const { agents, printers, jobs, nowMs } = args
  const agentName = (id: string) => agents.find((agent) => agent.id === id)?.name ?? 'Unknown agent'
  const items: AttentionItem[] = []

  for (const agent of agents) {
    if (!HEALTHY_AGENT_STATUSES.has(agent.status.toLowerCase())) {
      items.push({
        id: `agent:${agent.id}`,
        agentId: agent.id,
        agentName: agent.name,
        severity: statusSeverity(agent.status),
        kind: 'agent',
        title: `Agent ${prettifyToken(agent.status)}`,
        label: `${agent.name} is ${agent.status || 'offline'}`,
        mono: agent.id,
        sectionId: 'printers',
        ageMs: null,
      })
    }
  }

  for (const printer of printers) {
    if (OFFLINE_PRINTER_STATUSES.has(printer.status.toLowerCase())) {
      items.push({
        id: `printer:${printer.id}`,
        agentId: printer.agent_id,
        agentName: agentName(printer.agent_id),
        severity: statusSeverity(printer.status),
        kind: 'printer',
        title: `Printer ${prettifyToken(printer.status)}`,
        label: `${printer.name} is ${printer.status}`,
        mono: printer.serial_number,
        sectionId: 'printers',
        ageMs: null,
      })
    }
  }

  for (const job of jobs) {
    if (isJobFailed(job)) {
      const physical = job.print.status.toLowerCase() === 'failed'
      items.push({
        id: `job:${job.id}:failed`,
        agentId: job.agent_id,
        agentName: agentName(job.agent_id),
        severity: statusSeverity(physical ? job.print.status : job.status),
        kind: 'job',
        title: physical ? 'Print failed' : 'Dispatch failed',
        label: job.artifact.filename,
        mono: job.id,
        sectionId: 'recovery',
        ageMs: null,
      })
    } else if (nowMs > 0 && isJobActive(job) && isStale(job, nowMs)) {
      items.push({
        id: `job:${job.id}:stale`,
        agentId: job.agent_id,
        agentName: agentName(job.agent_id),
        severity: 'warning',
        kind: 'job',
        title: 'Job stalled',
        label: `${job.artifact.filename} · no progress for ${formatDuration(staleAgeMs(job, nowMs) ?? 0)}`,
        mono: job.id,
        sectionId: 'jobs',
        ageMs: staleAgeMs(job, nowMs),
      })
    }
  }

  return items.sort((a, b) => {
    if (a.agentName !== b.agentName) return a.agentName.localeCompare(b.agentName)
    return SEVERITY_RANK[a.severity] - SEVERITY_RANK[b.severity]
  })
}

function latestJobUpdateMs(job: Job): number {
  const candidates = [
    Date.parse(job.updated_at),
    job.print.updated_at ? Date.parse(job.print.updated_at) : NaN,
  ]
  const valid = candidates.filter((value) => !Number.isNaN(value))
  return valid.length ? Math.max(...valid) : NaN
}

function isStale(job: Job, nowMs: number): boolean {
  const updated = latestJobUpdateMs(job)
  if (Number.isNaN(updated)) return false
  return nowMs - updated > STALE_MS
}

function staleAgeMs(job: Job, nowMs: number): number | null {
  const updated = latestJobUpdateMs(job)
  if (Number.isNaN(updated)) return null
  return Math.max(0, nowMs - updated)
}

export function maxSeverity(items: AttentionItem[]): Severity | null {
  if (items.length === 0) return null
  return items.reduce<Severity>(
    (current, item) => (SEVERITY_RANK[item.severity] < SEVERITY_RANK[current] ? item.severity : current),
    items[0].severity,
  )
}

export function notificationSeverity(title: string, detail: string): Severity {
  const text = `${title} ${detail}`.toLowerCase()
  if (text.includes('failed') || text.includes('offline') || text.includes('unavailable')) return 'critical'
  if (text.includes('disconnected') || text.includes('stalled') || text.includes('stale')) return 'warning'
  if (text.includes('complete') || text.includes('completed') || text.includes('running')) return 'success'
  return 'info'
}

export function formatDuration(ms: number): string {
  const minutes = Math.round(ms / 60000)
  if (minutes < 1) return 'less than a minute'
  if (minutes === 1) return '1 minute'
  if (minutes < 60) return `${minutes} minutes`
  const hours = Math.round(minutes / 60)
  return hours === 1 ? '1 hour' : `${hours} hours`
}

const STATUS_SEVERITY: Array<{ severity: Severity; tokens: string[] }> = [
  { severity: 'success', tokens: ['online', 'ok', 'succeeded', 'completed', 'running', 'printing', 'ready'] },
  { severity: 'warning', tokens: ['warning', 'queued', 'sent', 'acknowledged', 'connecting', 'problem', 'degraded', 'pending'] },
  { severity: 'critical', tokens: ['failed', 'offline', 'unavailable', 'error', 'down'] },
]

export function statusSeverity(value: string): Severity {
  const normalized = value.toLowerCase()
  for (const group of STATUS_SEVERITY) {
    if (group.tokens.includes(normalized)) {
      return group.severity
    }
  }
  return 'info'
}

export function prettifyToken(value: string): string {
  const cleaned = value.replace(/[_-]+/g, ' ').trim()
  return cleaned.length ? cleaned.charAt(0).toUpperCase() + cleaned.slice(1) : value
}

export function statusMeta(value: string): { severity: Severity; label: string } {
  return { severity: statusSeverity(value), label: prettifyToken(value) }
}
