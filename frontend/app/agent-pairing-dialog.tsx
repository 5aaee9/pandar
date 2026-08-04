'use client'

import { useTranslations } from 'next-intl'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { CreateAgentPairingForm } from './create-agent-pairing-form'
import type { Tenant } from './dashboard-types'

export function AgentPairingDialog({
  tenant,
  open,
  onOpenChange,
}: {
  tenant: Tenant
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const t = useTranslations('agentPairing')

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg" closeLabel={t('closeDialog')}>
        <DialogHeader>
          <DialogTitle>{t('title')}</DialogTitle>
          <DialogDescription>{t('summary')}</DialogDescription>
        </DialogHeader>
        <div>
          <div className="text-xs font-medium text-muted-foreground">{t('stepsTitle')}</div>
          <ol className="mt-2 grid gap-2 text-sm text-foreground">
            <li className="flex gap-2">
              <StepNumber value="1" />
              <span>{t('stepCreate', { name: tenant.display_name })}</span>
            </li>
            <li className="flex gap-2">
              <StepNumber value="2" />
              <span>{t('stepCopy')}</span>
            </li>
            <li className="flex gap-2">
              <StepNumber value="3" />
              <span>{t('stepStart')}</span>
            </li>
          </ol>
        </div>
        <CreateAgentPairingForm tenantId={tenant.id} />
      </DialogContent>
    </Dialog>
  )
}

function StepNumber({ value }: { value: string }) {
  return (
    <span className="mt-0.5 inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-border bg-muted text-[11px] font-medium tabular-nums text-muted-foreground">
      {value}
    </span>
  )
}
