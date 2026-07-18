'use client'

import { useTranslations } from 'next-intl'

export function FilterBar({
  query,
  onQueryChange,
  queryPlaceholder,
  status,
  onStatusChange,
  statusOptions,
}: {
  query: string
  onQueryChange: (value: string) => void
  queryPlaceholder: string
  status: string
  onStatusChange: (value: string) => void
  statusOptions: Array<{ value: string; label: string }>
}) {
  const t = useTranslations('inventory')
  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-border px-4 py-2">
      <input
        aria-label={queryPlaceholder}
        className="min-w-40 flex-1 rounded-md border border-input bg-background px-2 py-1 text-sm text-foreground"
        onChange={(event) => onQueryChange(event.target.value)}
        placeholder={queryPlaceholder}
        type="search"
        value={query}
      />
      <select
        aria-label={t('filterStatusAria')}
        className="rounded-md border border-input bg-background px-2 py-1 text-sm text-foreground"
        onChange={(event) => onStatusChange(event.target.value)}
        value={status}
      >
        {statusOptions.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  )
}
