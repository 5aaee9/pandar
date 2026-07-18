'use client'

import { useTranslations } from 'next-intl'

import type { AttentionItem } from './dashboard-attention'
import { AttentionRow } from './dashboard-status'
import type { Tenant } from './dashboard-types'

export function NeedsAttention({
  items,
  onOpenReprint,
  selectedTenant,
}: {
  items: AttentionItem[]
  onOpenReprint: (jobId: string) => void
  selectedTenant: Tenant | null
}) {
  const tAtt = useTranslations('overview')
  if (items.length === 0) {
    return null
  }

  const groupedItems = groupAttentionItems(items)

  return (
    <section
      aria-label={tAtt('ariaAttention')}
      className="overflow-hidden rounded-lg border border-slate-200 bg-white"
    >
      <div className="flex items-center justify-between border-b border-slate-200 px-4 py-3">
        <div>
          <h2 className="text-base font-semibold text-slate-900">{tAtt('attentionTitle')}</h2>
          <p className="mt-0.5 text-sm text-slate-600">
            {tAtt('attentionSubtitle', { count: items.length })}
          </p>
        </div>
        <span className="text-xs text-slate-600">{tAtt('groupedByAgent')}</span>
      </div>
      <ul className="divide-y divide-slate-200">
        {groupedItems.map(({ item, showGroup, zebra }) => (
          <AttentionRow
            key={item.id}
            item={item}
            onOpenReprint={onOpenReprint}
            showGroup={showGroup}
            zebra={zebra}
            tenant={selectedTenant}
          />
        ))}
      </ul>
    </section>
  )
}

function groupAttentionItems(items: AttentionItem[]) {
  let lastAgent = ''
  let groupIndex = -1
  return items.map((item) => {
    const showGroup = item.agentName !== lastAgent
    if (showGroup) {
      lastAgent = item.agentName
      groupIndex += 1
    }
    return { item, showGroup, zebra: groupIndex % 2 === 1 }
  })
}
