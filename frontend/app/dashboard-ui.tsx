'use client'

import { useId, type ReactNode } from 'react'

import { prettifyToken, statusMeta } from './dashboard-attention'
import { PILL_TONES, StatusIcon } from './dashboard-status'

export function StatusBadge({ value }: { value: string }) {
  const { severity, label } = statusMeta(value)
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
  neutral: 'border-slate-200 bg-slate-100 text-slate-700',
  accent: 'border-cyan-200 bg-cyan-50 text-cyan-800',
  success: 'border-emerald-200 bg-emerald-50 text-emerald-800',
  warning: 'border-amber-200 bg-amber-50 text-amber-800',
}

export function Tag({ value, tone = 'neutral' }: { value: string; tone?: keyof typeof TAG_TONES }) {
  return (
    <span className={`inline-flex rounded-md border px-2 py-0.5 text-xs font-medium ${TAG_TONES[tone]}`}>
      {prettifyToken(value)}
    </span>
  )
}

export function HelpTip({ label, children }: { label: string; children: ReactNode }) {
  const tipId = useId()
  return (
    <span className="group relative inline-flex shrink-0">
      <button
        aria-describedby={tipId}
        aria-label={`More about ${label}`}
        className="inline-flex h-4 w-4 items-center justify-center rounded-full border border-slate-300 bg-white text-[10px] leading-none text-slate-500 hover:bg-slate-100"
        type="button"
      >
        ?
      </button>
      <span
        className="pointer-events-none absolute bottom-full left-1/2 z-30 mb-1 w-56 -translate-x-1/2 rounded-md bg-slate-900 px-2 py-1 text-center text-xs font-normal text-slate-100 opacity-0 transition-opacity duration-150 group-hover:opacity-100 group-focus-within:opacity-100"
        id={tipId}
        role="tooltip"
      >
        {children}
      </span>
    </span>
  )
}

export function Metric({ label, value }: { label: string; value: number | undefined }) {
  return (
    <div className="rounded-md border border-slate-300 bg-white px-4 py-3">
      <div className="text-xs font-medium text-slate-500">{label}</div>
      <div className="mt-1 text-2xl font-semibold">{value ?? '-'}</div>
    </div>
  )
}

export function EmptyState({ title, message }: { title: string; message: string }) {
  return (
    <div className="px-4 py-12 text-center">
      <div className="text-sm font-semibold text-slate-950">{title}</div>
      <p className="mx-auto mt-2 max-w-md text-sm text-slate-600">{message}</p>
    </div>
  )
}

export function formatDate(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return value
  }

  return date.toLocaleString('en-US', {
    dateStyle: 'medium',
    timeStyle: 'short',
    timeZone: 'UTC',
  })
}

export function formatBytes(value: number, formatNumber?: (n: number) => string) {
  const fmt = (n: number) => (formatNumber ? formatNumber(n) : n.toFixed(1))
  if (value < 1024) {
    return `${formatNumber ? formatNumber(value) : value} B`
  }
  if (value < 1024 * 1024) {
    return `${fmt(value / 1024)} KiB`
  }

  return `${fmt(value / (1024 * 1024))} MiB`
}

export function SectionHeader({ title, subtitle, meta }: { title: string; subtitle: string; meta: string }) {
  return (
    <div className="flex flex-col gap-2 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <h2 className="text-base font-semibold">{title}</h2>
        <p className="mt-0.5 text-sm text-slate-600">{subtitle}</p>
      </div>
      <div className="text-sm text-slate-600">{meta}</div>
    </div>
  )
}

export function DetailGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div>
      <div className="text-xs font-medium text-slate-500">{title}</div>
      <div className="mt-2 grid gap-1">{children}</div>
    </div>
  )
}

export function DetailLine({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[7rem_minmax(0,1fr)]">
      <div className="text-slate-500">{label}</div>
      <div className={`break-words ${mono ? 'font-mono text-xs text-slate-700' : 'text-slate-900'}`}>{value}</div>
    </div>
  )
}
