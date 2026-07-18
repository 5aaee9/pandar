'use client'

import { useId, type ReactNode } from 'react'
import { useTranslations } from 'next-intl'

import { prettifyToken, statusMeta } from './dashboard-attention'
import { StatusIcon } from './dashboard-status'
import { PILL_TONES } from './dashboard-status-model'

export function StatusBadge({ value }: { value: string }) {
  const tTokens = useTranslations('tokens')
  const tokenTranslator = (k: string) => (tTokens.has(k) ? tTokens(k) : undefined)
  const { severity, label } = statusMeta(value, tokenTranslator)
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs font-medium ${PILL_TONES[severity]}`}
    >
      <StatusIcon severity={severity} className="h-3.5 w-3.5" />
      {label}
    </span>
  )
}

const TAG_TONES = {
  neutral: 'border-border bg-muted text-muted-foreground',
  accent: 'border-border bg-accent text-accent-foreground',
  success: 'border-success/40 bg-success/10 text-success',
  warning: 'border-warning/50 bg-warning/10 text-warning',
}

export function Tag({ value, tone = 'neutral' }: { value: string; tone?: keyof typeof TAG_TONES }) {
  const tTokens = useTranslations('tokens')
  const tokenTranslator = (k: string) => (tTokens.has(k) ? tTokens(k) : undefined)
  return (
    <span className={`inline-flex rounded-md border px-2 py-0.5 text-xs font-medium ${TAG_TONES[tone]}`}>
      {prettifyToken(value, tokenTranslator)}
    </span>
  )
}

export function HelpTip({ label, children }: { label: string; children: ReactNode }) {
  const tCommon = useTranslations('common')
  const tipId = useId()
  return (
    <span className="group relative inline-flex shrink-0">
      <button
        aria-describedby={tipId}
        aria-label={tCommon('moreAbout', { label })}
        className="relative inline-flex h-4 w-4 items-center justify-center rounded-full border border-border bg-background text-[10px] leading-none text-muted-foreground transition-colors duration-150 ease-out after:absolute after:-inset-2 hover:bg-muted"
        type="button"
      >
        ?
      </button>
      <span
        className="pointer-events-none absolute bottom-full left-1/2 z-30 mb-1 w-56 -translate-x-1/2 rounded-md bg-foreground px-2 py-1 text-center text-xs font-normal text-background opacity-0 transition-opacity duration-150 group-hover:opacity-100 group-focus-within:opacity-100 motion-reduce:transition-none"
        id={tipId}
        role="tooltip"
      >
        {children}
      </span>
    </span>
  )
}

export function EmptyState({ title, message }: { title: string; message: string }) {
  return (
    <div className="px-4 py-12 text-center">
      <div className="text-sm font-semibold text-foreground">{title}</div>
      <p className="mx-auto mt-2 max-w-md text-sm text-muted-foreground">{message}</p>
    </div>
  )
}

export function SectionHeader({
  title,
  subtitle,
  meta,
  actions,
}: {
  title: string
  subtitle: string
  meta?: string
  actions?: ReactNode
}) {
  return (
    <div className="flex flex-col gap-2 border-b border-border px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <h2 className="text-base font-semibold">{title}</h2>
        <p className="mt-0.5 text-sm text-muted-foreground">{subtitle}</p>
      </div>
      {meta || actions ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          {meta ? <span>{meta}</span> : null}
          {actions}
        </div>
      ) : null}
    </div>
  )
}

export function DetailGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div>
      <div className="text-xs font-medium text-muted-foreground">{title}</div>
      <div className="mt-2 grid gap-1">{children}</div>
    </div>
  )
}

export function DetailLine({ label, value, mono }: { label: string; value: ReactNode; mono?: boolean }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[7rem_minmax(0,1fr)]">
      <div className="text-muted-foreground">{label}</div>
      <div className={`break-words ${mono ? 'font-mono text-xs text-muted-foreground' : 'text-foreground'}`}>{value}</div>
    </div>
  )
}
