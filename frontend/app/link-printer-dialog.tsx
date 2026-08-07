'use client'

import { useState, type ReactElement } from 'react'
import { useTranslations } from 'next-intl'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import type { Agent, DiscoveredPrinter, Tenant } from './dashboard-types'
import { LinkPrinterMachineForm } from './link-printer-form'

type LinkPrinterDialogProps = {
  agents: Agent[]
  selectedTenant: Tenant
  trigger: ReactElement
} & (
  | { mode: 'add' }
  | {
      mode: 'adopt'
      agentId: string
      target: DiscoveredPrinter
    }
)

export function LinkPrinterDialog(props: LinkPrinterDialogProps) {
  const t = useTranslations('linkPrinter')
  const tDiscovery = useTranslations('discovery')
  const [open, setOpen] = useState(false)
  const adopt = props.mode === 'adopt'
  const agentName = adopt
    ? (props.agents.find((agent) => agent.id === props.agentId)?.name ?? props.agentId)
    : ''

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={props.trigger} />
      <DialogContent
        className="sm:max-w-xl"
        closeLabel={adopt ? tDiscovery('adoptClose') : t('closeDialog')}
      >
        <DialogHeader>
          <DialogTitle>{adopt ? tDiscovery('adoptTitle') : t('title')}</DialogTitle>
          <DialogDescription>
            {adopt
              ? tDiscovery('adoptDescription', {
                  agent: agentName,
                  host: props.target.host,
                })
              : t('subtitleTenant', { name: props.selectedTenant.display_name })}
          </DialogDescription>
        </DialogHeader>
        <LinkPrinterMachineForm
          agents={adopt ? [] : props.agents}
          defaultHost={adopt ? props.target.host : undefined}
          defaultName={adopt ? (props.target.name ?? '') : undefined}
          fixedAgentId={adopt ? props.agentId : undefined}
          key={
            adopt
              ? `${props.target.serial_number ?? 'unknown'}-${props.target.host}`
              : 'add-printer'
          }
          onLinked={() => setOpen(false)}
          selectedTenant={props.selectedTenant}
          submitLabel={adopt ? tDiscovery('adoptSubmit') : undefined}
        />
      </DialogContent>
    </Dialog>
  )
}
