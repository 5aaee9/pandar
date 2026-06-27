import { refreshPrinters, reprintJob, retryDispatchJob } from './actions'
import type { AttentionItem, Severity } from './dashboard-attention'
import type { LiveState, Translator } from './dashboard-runtime-helpers'
import type { Tenant } from './dashboard-types'

export const PILL_TONES: Record<Severity, string> = {
  success: 'border-emerald-200 bg-emerald-50 text-emerald-800',
  warning: 'border-amber-200 bg-amber-50 text-amber-800',
  critical: 'border-red-200 bg-red-50 text-red-800',
  info: 'border-slate-200 bg-slate-100 text-slate-700',
}

const TONES: Record<Severity, { border: string; surface: string; ink: string; sub: string }> = {
  critical: { border: 'border-red-200', surface: 'bg-red-50', ink: 'text-red-900', sub: 'text-red-800' },
  warning: { border: 'border-amber-200', surface: 'bg-amber-50', ink: 'text-amber-900', sub: 'text-amber-800' },
  success: { border: 'border-emerald-200', surface: 'bg-emerald-50', ink: 'text-emerald-900', sub: 'text-emerald-800' },
  info: { border: 'border-slate-200', surface: 'bg-white', ink: 'text-slate-900', sub: 'text-slate-600' },
}

type Verdict = {
  title: string
  detail: string
  severity: Severity
  tone: { border: string; surface: string; ink: string; sub: string }
}

const enVerdict: Translator = (key, values) => {
  const v = values ?? {}
  switch (key) {
    case 'noFleet.title':
      return 'No fleet configured'
    case 'noFleet.detail':
      return 'Connect an agent to start monitoring your printers.'
    case 'liveUnavailable.title':
      return 'Live updates unavailable'
    case 'liveUnavailable.detail':
      return 'Reconnecting — showing the last known state.'
    case 'liveDisconnected.title':
      return 'Live updates disconnected'
    case 'liveDisconnected.detail':
      return 'Reconnecting — showing the last known state.'
    case 'nominal.title':
      return 'All systems nominal'
    case 'nominal.detail':
      return 'No exceptions across the fleet.'
    case 'needAttention.title': {
      const count = (v.count as number) ?? 0
      return `${count} ${count === 1 ? 'item' : 'items'} need attention`
    }
    case 'needAttention.detailCritical':
      return 'Failures detected — review below.'
    case 'needAttention.detailOther':
      return 'Review the items below.'
    default:
      return key
  }
}

export function computeVerdict(args: {
  attentionCount: number
  topSeverity: Severity | null
  liveState: LiveState
  fleetEmpty: boolean
}, t: Translator = enVerdict): Verdict {
  const { attentionCount, topSeverity, liveState, fleetEmpty } = args

  if (fleetEmpty) {
    return { title: t('noFleet.title'), detail: t('noFleet.detail'), severity: 'info', tone: TONES.info }
  }

  if (liveState === 'unavailable' || liveState === 'error') {
    return { title: t('liveUnavailable.title'), detail: t('liveUnavailable.detail'), severity: 'warning', tone: TONES.warning }
  }
  if (liveState === 'disconnected') {
    return { title: t('liveDisconnected.title'), detail: t('liveDisconnected.detail'), severity: 'warning', tone: TONES.warning }
  }

  if (attentionCount === 0) {
    return { title: t('nominal.title'), detail: t('nominal.detail'), severity: 'success', tone: TONES.success }
  }

  const severity = topSeverity ?? 'warning'
  return {
    title: t('needAttention.title', { count: attentionCount }),
    detail: severity === 'critical' ? t('needAttention.detailCritical') : t('needAttention.detailOther'),
    severity,
    tone: severity === 'critical' ? TONES.critical : TONES.warning,
  }
}

export function StatusIcon({ severity, className }: { severity: Severity; className?: string }) {
  const common = {
    viewBox: '0 0 20 20',
    fill: 'currentColor',
    'aria-hidden': true,
    className,
  } as const
  const color =
    severity === 'critical'
      ? 'text-red-600'
      : severity === 'warning'
        ? 'text-amber-600'
        : severity === 'success'
          ? 'text-emerald-600'
          : 'text-slate-500'
  if (severity === 'success') {
    return (
      <svg {...common} className={`${className ?? ''} ${color}`}>
        <path
          fillRule="evenodd"
          d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.86-9.97a.75.75 0 00-1.22-.87l-3.24 4.53-1.61-1.61a.75.75 0 10-1.06 1.06l2.25 2.25a.75.75 0 001.1-.1l3.78-5.26z"
          clipRule="evenodd"
        />
      </svg>
    )
  }
  if (severity === 'critical') {
    return (
      <svg {...common} className={`${className ?? ''} ${color}`}>
        <path
          fillRule="evenodd"
          d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.28 7.22a.75.75 0 00-1.06 1.06L8.94 10l-1.72 1.72a.75.75 0 101.06 1.06L10 11.06l1.72 1.72a.75.75 0 101.06-1.06L11.06 10l1.72-1.72a.75.75 0 00-1.06-1.06L10 8.94 8.28 7.22z"
          clipRule="evenodd"
        />
      </svg>
    )
  }
  if (severity === 'warning') {
    return (
      <svg {...common} className={`${className ?? ''} ${color}`}>
        <path
          fillRule="evenodd"
          d="M8.49 2.84a1.75 1.75 0 011.02 0l5.75 2.1a1.75 1.75 0 011.13 1.65v4.46c0 2.83-1.46 5.46-3.84 6.94l-2.2 1.37a1.75 1.75 0 01-1.84 0l-2.2-1.37A8.18 8.18 0 012.7 11.05V6.59c0-.74.46-1.4 1.13-1.65l5.66-2.1zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-.26-5.74a.75.75 0 00-1.48 0l-.47 3.2a.75.75 0 001.49.22l.46-3.42z"
          clipRule="evenodd"
        />
      </svg>
    )
  }
  return (
    <svg {...common} className={`${className ?? ''} ${color}`}>
      <path d="M10 9a1 1 0 011 1v4a1 1 0 11-2 0v-4a1 1 0 011-1zM10 6.5a1.25 1.25 0 100 2.5 1.25 1.25 0 000-2.5z" />
    </svg>
  )
}

export function StatCell({
  href,
  label,
  value,
  note,
  state,
}: {
  href: string
  label: string
  value: string
  note: string | null
  state: Severity
}) {
  return (
    <a href={href} className="block rounded-md px-3 py-1 transition-colors hover:bg-slate-100/70">
      <div className="flex items-center gap-1.5">
        <StatusIcon severity={state} className="h-3.5 w-3.5" />
        <span className="text-xs text-slate-600">{label}</span>
      </div>
      <div className="mt-0.5 font-medium text-slate-900">{value}</div>
      {note ? <div className="mt-0.5 text-xs text-slate-600">{note}</div> : null}
    </a>
  )
}

export function AttentionRow({
  item,
  showGroup,
  zebra,
  tenant,
}: {
  item: AttentionItem
  showGroup: boolean
  zebra: boolean
  tenant: Tenant | null
}) {
  return (
    <li className={`px-4 py-3 ${zebra ? 'bg-slate-50/60' : ''}`}>
      {showGroup ? (
        <div className="mb-2 text-xs font-semibold text-slate-700">{item.agentName}</div>
      ) : null}
      <div className="flex flex-wrap items-center gap-3">
        <StatusIcon severity={item.severity} className="h-4 w-4 shrink-0" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-slate-900">{item.title}</div>
          <div className="truncate text-xs text-slate-600">{item.label}</div>
        </div>
        <code className="hidden shrink-0 font-mono text-xs text-slate-600 sm:block">{item.mono}</code>
        <AttentionAction item={item} tenant={tenant} />
      </div>
    </li>
  )
}

function AttentionAction({ item, tenant }: { item: AttentionItem; tenant: Tenant | null }) {
  if (!tenant) {
    return (
      <a href={`#${item.sectionId}`} className="text-xs font-medium text-cyan-700 hover:underline">
        View
      </a>
    )
  }

  if (item.kind === 'agent') {
    return (
      <form action={refreshPrinters}>
        <input name="tenant_id" type="hidden" value={tenant.id} />
        <input name="agent_id" type="hidden" value={item.agentId} />
        <button
          className={`h-8 rounded-md border border-slate-300 bg-white px-2 text-xs font-medium text-slate-800 hover:bg-slate-50`}
          type="submit"
        >
          Refresh
        </button>
      </form>
    )
  }

  if (item.kind === 'job' && item.reason === 'job_print_failed') {
    return (
      <form action={reprintJob}>
        <input name="tenant_id" type="hidden" value={tenant.id} />
        <input name="job_id" type="hidden" value={item.mono} />
        <button
          className={`h-8 rounded-md bg-cyan-700 px-2 text-xs font-medium text-white hover:bg-cyan-800`}
          type="submit"
        >
          Reprint
        </button>
      </form>
    )
  }

  if (item.kind === 'job' && item.reason === 'job_dispatch_failed') {
    return (
      <form action={retryDispatchJob}>
        <input name="tenant_id" type="hidden" value={tenant.id} />
        <input name="job_id" type="hidden" value={item.mono} />
        <button
          className={`h-8 rounded-md bg-cyan-700 px-2 text-xs font-medium text-white hover:bg-cyan-800`}
          type="submit"
        >
          Retry dispatch
        </button>
      </form>
    )
  }

  return (
    <a href={`#${item.sectionId}`} className="text-xs font-medium text-cyan-700 hover:underline">
      View
    </a>
  )
}
