export function StatusBadge({ value }: { value: string }) {
  const normalized = value.toLowerCase()
  const className =
    normalized === 'ok' || normalized === 'online' || normalized === 'succeeded'
      ? 'bg-emerald-700 text-white'
      : normalized === 'warning' ||
          normalized === 'queued' ||
          normalized === 'sent' ||
          normalized === 'acknowledged'
        ? 'bg-amber-600 text-white'
        : normalized === 'problem' || normalized === 'failed' || normalized === 'offline'
          ? 'bg-red-700 text-white'
          : 'bg-slate-800 text-white'

  return (
    <span className={`inline-flex rounded px-2 py-1 text-xs font-medium ${className}`}>
      {value}
    </span>
  )
}

export function Metric({ label, value }: { label: string; value: number | undefined }) {
  return (
    <div className="rounded-md border border-slate-300 bg-white px-4 py-3">
      <div className="text-xs font-medium uppercase text-slate-500">{label}</div>
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

export function formatBytes(value: number) {
  if (value < 1024) {
    return `${value} B`
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KiB`
  }

  return `${(value / (1024 * 1024)).toFixed(1)} MiB`
}
