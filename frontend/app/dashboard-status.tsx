'use client'

import { useTranslations } from 'next-intl'

import { refreshPrinters } from './actions'
import { reprintJob, retryDispatchJob } from './job-actions'
import { dashboardSidebarHref } from './dashboard-shell'
import type { AttentionItem, Severity, TextKey } from './dashboard-attention'
import type { Tenant } from './dashboard-types'

export function StatusIcon({ severity, className }: { severity: Severity; className?: string }) {
  const common = {
    viewBox: '0 0 20 20',
    fill: 'currentColor',
    'aria-hidden': true,
    className,
  } as const
  const color =
    severity === 'critical'
      ? 'text-destructive'
      : severity === 'warning'
        ? 'text-warning'
        : severity === 'success'
          ? 'text-success'
          : 'text-muted-foreground'
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
  separatorClassName,
  state,
}: {
  href?: string
  label: string
  value: string
  note: string | null
  separatorClassName?: string
  state: Severity
}) {
  const content = (
    <>
      <div className="flex items-center gap-1.5">
        <StatusIcon severity={state} className="h-3.5 w-3.5" />
        <span className="text-xs text-muted-foreground">{label}</span>
      </div>
      <div className="mt-0.5 font-medium text-foreground">{value}</div>
      {note ? <div className="mt-0.5 text-xs text-muted-foreground">{note}</div> : null}
    </>
  )
  return (
    <div
      className={`relative ${separatorClassName ? `lg:before:absolute lg:before:bottom-2 lg:before:left-2 lg:before:top-2 lg:before:w-px lg:before:content-[''] ${separatorClassName}` : ''}`}
    >
      {href ? (
        <a
          href={href}
          className={`block rounded-md px-3 py-1 transition-colors duration-150 ease-out hover:bg-accent ${separatorClassName ? 'lg:ml-4' : ''}`}
        >
          {content}
        </a>
      ) : (
        <div className={`px-3 py-1 ${separatorClassName ? 'lg:ml-4' : ''}`}>{content}</div>
      )}
    </div>
  )
}

function AttentionText({ textKey }: { textKey: TextKey }) {
  const t = useTranslations(textKey.namespace)
  return <>{t(textKey.key, textKey.values)}</>
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
    <li className={`px-4 py-3 ${zebra ? 'bg-muted/60' : ''}`}>
      {showGroup ? (
        <div className="mb-2 text-xs font-semibold text-muted-foreground">{item.agentName}</div>
      ) : null}
      <div className="flex flex-wrap items-center gap-3">
        <StatusIcon severity={item.severity} className="h-4 w-4 shrink-0" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-foreground">
            <AttentionText textKey={item.titleKey} />
          </div>
          <div className="truncate text-xs text-muted-foreground">
            <AttentionText textKey={item.labelKey} />
          </div>
        </div>
        <code className="hidden shrink-0 font-mono text-xs text-muted-foreground sm:block">{item.mono}</code>
        <AttentionAction item={item} tenant={tenant} />
      </div>
      {item.detailKey ? (
        <div className="mt-2 break-words text-xs text-destructive sm:ml-7">
          <AttentionText textKey={item.detailKey} />
        </div>
      ) : null}
    </li>
  )
}

function AttentionAction({ item, tenant }: { item: AttentionItem; tenant: Tenant | null }) {
  const tAct = useTranslations('overview.action')
  const sectionHref = item.sectionId === 'printers'
    ? '#printers'
    : dashboardSidebarHref('jobs', { tenant: tenant?.id })
  if (!tenant) {
    return (
      <a href={sectionHref} className="text-xs font-medium text-primary hover:underline">
        {tAct('view')}
      </a>
    )
  }

  if (item.kind === 'agent') {
    return (
      <form action={refreshPrinters}>
        <input name="tenant_id" type="hidden" value={tenant.id} />
        <input name="agent_id" type="hidden" value={item.agentId} />
        <button
          className={`h-8 rounded-md border border-border bg-background px-2 text-xs font-medium text-foreground transition-colors duration-150 ease-out hover:bg-accent`}
          type="submit"
        >
          {tAct('refresh')}
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
          className={`h-8 rounded-md bg-primary px-2 text-xs font-medium text-primary-foreground hover:bg-primary/80`}
          type="submit"
        >
          {tAct('reprint')}
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
          className={`h-8 rounded-md bg-primary px-2 text-xs font-medium text-primary-foreground hover:bg-primary/80`}
          type="submit"
        >
          {tAct('retryDispatch')}
        </button>
      </form>
    )
  }

  return (
    <a href={sectionHref} className="text-xs font-medium text-primary hover:underline">
      {tAct('view')}
    </a>
  )
}
