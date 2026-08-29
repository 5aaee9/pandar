'use client'

import { useActionState, useEffect, useId, useRef, useState } from 'react'
import { useTranslations } from 'next-intl'
import { useQueryClient } from '@tanstack/react-query'
import { EyeIcon, EyeOffIcon, Loader2Icon } from 'lucide-react'
import { toast } from 'sonner'

import { Button } from '@/components/ui/button'
import { inputClasses } from '@/lib/utils'
import { linkPrinter } from './actions'
import { apiIdSegment } from './api-path'
import { isTerminalCommandStatus } from './command-status'
import type { Agent, Command, Tenant } from './dashboard-types'
import { EmptyState } from './dashboard-ui'
import { invalidateTenantResources, mutationResources } from './mutation-invalidation'

const POLL_INTERVAL_MS = 2000
const POLL_TIMEOUT_MS = 90_000

type LinkFailure = {
  code: string | null
  detail: string | null
}

type PollOutcome =
  | { commandId: string; status: 'succeeded' }
  | ({ commandId: string; status: 'failed' } & LinkFailure)

export function LinkPrinterMachineForm({
  selectedTenant,
  agents,
  fixedAgentId,
  defaultHost,
  defaultName,
  submitLabel,
  onLinked,
}: {
  selectedTenant: Tenant | null
  agents: Agent[]
  fixedAgentId?: string
  defaultHost?: string
  defaultName?: string
  submitLabel?: string
  onLinked?: () => void
}) {
  const t = useTranslations('linkPrinter')
  const queryClient = useQueryClient()
  const [state, formAction, dispatching] = useActionState(linkPrinter, null)
  const [pollOutcome, setPollOutcome] = useState<PollOutcome | null>(null)
  const [accessCode, setAccessCode] = useState('')
  const [accessCodeVisible, setAccessCodeVisible] = useState(false)
  const accessCodeId = useId()
  const onLinkedRef = useRef(onLinked)

  useEffect(() => {
    onLinkedRef.current = onLinked
  }, [onLinked])

  const handledCommandRef = useRef<string | null>(null)

  useEffect(() => {
    if (!state?.ok || !selectedTenant) {
      return
    }
    const commandId = state.commandId
    if (handledCommandRef.current === commandId) {
      return
    }
    const tenantId = selectedTenant.id
    const deadline = Date.now() + POLL_TIMEOUT_MS
    let active = true

    const poll = async () => {
      while (active) {
        try {
          const command = await fetchLinkCommand(tenantId, commandId)
          if (!active) {
            return
          }
          if (isTerminalCommandStatus(command.status)) {
            handledCommandRef.current = commandId
            if (command.status === 'succeeded') {
              toast.success(t('linked'))
              await invalidateTenantResources(
                queryClient,
                tenantId,
                mutationResources.printerLink,
              )
              if (!active) {
                return
              }
              setPollOutcome({ commandId, status: 'succeeded' })
              onLinkedRef.current?.()
            } else {
              setPollOutcome({
                commandId,
                status: 'failed',
                code: linkErrorCode(command),
                detail: command.error,
              })
            }
            return
          }
        } catch {
          // Transient poll failure; keep polling until the deadline.
        }
        if (Date.now() >= deadline) {
          handledCommandRef.current = commandId
          if (active) {
            setPollOutcome({
              commandId,
              status: 'failed',
              code: 'link_timeout',
              detail: null,
            })
          }
          return
        }
        await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS))
      }
    }
    void poll()

    return () => {
      active = false
    }
  }, [state, selectedTenant, queryClient, t])

  const defaultAgent =
    agents.find((agent) => agent.status.toLowerCase() === 'online') ?? agents[0]

  const outcome =
    state?.ok === true && pollOutcome?.commandId === state.commandId
      ? pollOutcome
      : null
  const busy = dispatching || (state?.ok === true && outcome === null)
  const failure: LinkFailure | null =
    state && !state.ok
      ? { code: state.error, detail: null }
      : outcome?.status === 'failed'
        ? { code: outcome.code, detail: outcome.detail }
        : null

  if (!selectedTenant) {
    return <EmptyState title={t('noTenantTitle')} message={t('noTenantMessage')} />
  }

  if (!fixedAgentId && (agents.length === 0 || !defaultAgent)) {
    return <EmptyState title={t('noAgentsTitle')} message={t('noAgentsMessage')} />
  }

  return (
    <form action={formAction} className="grid gap-4">
      <input name="tenant_id" type="hidden" value={selectedTenant.id} />
      <input name="type" type="hidden" value="BambuLab" />
      {fixedAgentId ? (
        <input name="agent_id" type="hidden" value={fixedAgentId} />
      ) : defaultAgent ? (
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-xs font-medium text-muted-foreground">{t('agent')}</span>
          <select
            className={inputClasses}
            defaultValue={defaultAgent.id}
            name="agent_id"
            required
          >
            {agents.map((agent) => (
              <option key={agent.id} value={agent.id}>
                {t('agentOption', { name: agent.name, status: agent.status })}
              </option>
            ))}
          </select>
        </label>
      ) : null}
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-xs font-medium text-muted-foreground">{t('host')}</span>
        <input
          className={inputClasses}
          defaultValue={defaultHost}
          name="host"
          required
          type="text"
        />
      </label>
      <div className="flex flex-col gap-1 text-sm">
        <label
          className="text-xs font-medium text-muted-foreground"
          htmlFor={accessCodeId}
        >
          {t('accessCode')}
        </label>
        <div className="relative">
          <input
            autoComplete="off"
            className={`${inputClasses} pr-10`}
            id={accessCodeId}
            name="access_code"
            onChange={(event) => setAccessCode(event.target.value)}
            required
            type={accessCodeVisible ? 'text' : 'password'}
            value={accessCode}
          />
          <Button
            aria-label={t(accessCodeVisible ? 'hideAccessCode' : 'showAccessCode')}
            aria-pressed={accessCodeVisible}
            className="absolute right-1 top-1"
            onClick={() => setAccessCodeVisible((visible) => !visible)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            {accessCodeVisible ? <EyeOffIcon /> : <EyeIcon />}
          </Button>
        </div>
      </div>
      <label className="flex flex-col gap-1 text-sm">
        <span className="text-xs font-medium text-muted-foreground">{t('name')}</span>
        <input
          className={inputClasses}
          defaultValue={defaultName}
          name="name"
          type="text"
        />
      </label>
      {failure ? (
        <div
          className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive"
          role="alert"
        >
          <p className="font-medium">{failureMessage(t, failure)}</p>
        </div>
      ) : null}
      <div>
        <Button disabled={busy} type="submit">
          {busy ? <Loader2Icon className="animate-spin" /> : null}
          {busy ? t('linking') : (submitLabel ?? t('submit'))}
        </Button>
      </div>
    </form>
  )
}

type MessageTranslator = {
  (key: string, values?: Record<string, string>): string
  has: (key: string) => boolean
}

function failureMessage(t: MessageTranslator, failure: LinkFailure) {
  if (failure.code && t.has(`errors.${failure.code}`)) {
    const message = t(`errors.${failure.code}`)
    if (failure.code === 'link_failed' && failure.detail) {
      return t('errorWithDetail', {
        message: message.replace(/[.!?。！？]+$/, ''),
        detail: failure.detail,
        code: failure.code,
      })
    }
    return t('errorWithCode', { message, code: failure.code })
  }
  if (failure.code && failure.detail) {
    return t('errorWithCode', { message: failure.detail, code: failure.code })
  }
  return t('errorWithoutCode', {
    message: failure.detail ?? failure.code ?? t('errors.link_failed'),
  })
}

function linkErrorCode(command: Command): string | null {
  if (!command.result_json) {
    return null
  }
  try {
    const parsed: unknown = JSON.parse(command.result_json)
    if (!parsed || typeof parsed !== 'object') {
      return null
    }
    const record = parsed as Record<string, unknown>
    return record.type === 'printer_link_error' &&
      typeof record.error_code === 'string'
      ? record.error_code
      : null
  } catch {
    return null
  }
}

async function fetchLinkCommand(tenantId: string, commandId: string): Promise<Command> {
  const response = await fetch(
    `/api/tenants/${apiIdSegment(tenantId, 'tenant_id')}/commands/${apiIdSegment(commandId, 'command_id')}`,
  )
  if (!response.ok) {
    throw new Error(`Link command poll failed: ${response.status}`)
  }
  return (await response.json()) as Command
}
