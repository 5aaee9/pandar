'use server'

import { redirect } from 'next/navigation'

import { apiHeaders } from './api-auth'
import type { Agent, TenantToken } from './dashboard-types'

const apiUrl = process.env.APP_API_URL ?? 'http://localhost:8080'

export type SecretActionState =
  | {
      ok: true
      kind: 'tenant_token'
      token: string
      tenantToken: TenantToken
      message: string
    }
  | {
      ok: true
      kind: 'agent_pairing'
      agentEnv: string
      agent: Agent
      message: string
    }
  | {
      ok: false
      error: string
    }
  | null

export async function discoverPrinters(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const agentId = stringField(formData, 'agent_id')
  const timeoutValue = stringField(formData, 'timeout_seconds')
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/agents/${agentId}/discover-printers`,
    {
      method: 'POST',
      headers: await apiHeaders('application/json'),
      body: JSON.stringify({
        timeout_seconds: Number(timeoutValue || '5'),
      }),
    },
  )

  if (!response.ok) {
    throw new Error(`Discover printers returned ${response.status}`)
  }

  const command = (await response.json()) as { id: string }
  redirect(`/?tenant=${encodeURIComponent(tenantId)}&command=${encodeURIComponent(command.id)}`)
}

export async function refreshPrinters(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const agentId = stringField(formData, 'agent_id')
  const response = await postJson(
    `/api/v1/tenants/${tenantId}/agents/${agentId}/refresh-printers`,
    {},
  )
  redirect(statusUrl(tenantId, response.ok ? 'refresh_queued' : await errorCode(response)))
}

export async function diagnosePrinter(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const agentId = stringField(formData, 'agent_id')
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/agents/${agentId}/diagnose-printer`,
    {
      method: 'POST',
      headers: await apiHeaders('application/json'),
      body: JSON.stringify({
        serial_number: stringField(formData, 'serial_number'),
      }),
    },
  )

  if (!response.ok) {
    throw new Error(`Diagnose printer returned ${response.status}`)
  }

  const command = (await response.json()) as { id: string }
  redirect(`/?tenant=${encodeURIComponent(tenantId)}&command=${encodeURIComponent(command.id)}`)
}

export async function createTenantToken(_previousState: SecretActionState, formData: FormData): Promise<SecretActionState> {
  const tenantId = stringField(formData, 'tenant_id')
  const scopes = stringField(formData, 'scopes')
    .split(',')
    .map((scope) => scope.trim())
    .filter(Boolean)
  const response = await postJson(`/api/v1/tenants/${tenantId}/tenant-tokens`, {
    name: stringField(formData, 'name'),
    scopes,
    expires_at: nullableField(formData, 'expires_at'),
  })
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) }
  }
  const body = (await response.json()) as { tenant_token: TenantToken; token: string }
  return {
    ok: true,
    kind: 'tenant_token',
    tenantToken: body.tenant_token,
    token: body.token,
    message: 'Tenant token created',
  }
}

export async function revokeTenantToken(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const tokenId = stringField(formData, 'token_id')
  const response = await fetch(`${apiUrl}/api/v1/tenants/${tenantId}/tenant-tokens/${tokenId}`, {
    method: 'DELETE',
    headers: await apiHeaders('application/json'),
  })
  redirect(statusUrl(tenantId, response.ok ? 'tenant_token_revoked' : await errorCode(response)))
}

export async function rotateTenantToken(_previousState: SecretActionState, formData: FormData): Promise<SecretActionState> {
  const tenantId = stringField(formData, 'tenant_id')
  const tokenId = stringField(formData, 'token_id')
  const response = await postJson(`/api/v1/tenants/${tenantId}/tenant-tokens/${tokenId}/rotate`, {
    expires_at: nullableField(formData, 'expires_at'),
  })
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) }
  }
  const body = (await response.json()) as { tenant_token: TenantToken; token: string }
  return {
    ok: true,
    kind: 'tenant_token',
    tenantToken: body.tenant_token,
    token: body.token,
    message: 'Tenant token rotated',
  }
}

export async function createTenantUser(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const response = await postJson(`/api/v1/tenants/${tenantId}/users`, {
    email: stringField(formData, 'email'),
    display_name: stringField(formData, 'display_name'),
    role: stringField(formData, 'role'),
  })
  redirect(statusUrl(tenantId, response.ok ? 'user_created' : await errorCode(response)))
}

export async function updateTenantUserRole(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const userId = stringField(formData, 'user_id')
  const response = await fetch(`${apiUrl}/api/v1/tenants/${tenantId}/users/${userId}/role`, {
    method: 'PATCH',
    headers: await apiHeaders('application/json'),
    body: JSON.stringify({ role: stringField(formData, 'role') }),
  })
  redirect(statusUrl(tenantId, response.ok ? 'user_role_updated' : await errorCode(response)))
}

export async function linkUserIdentity(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const userId = stringField(formData, 'user_id')
  const response = await postJson(`/api/v1/tenants/${tenantId}/users/${userId}/identities`, {
    provider: stringField(formData, 'provider'),
    subject: stringField(formData, 'subject'),
  })
  redirect(statusUrl(tenantId, response.ok ? 'identity_linked' : await errorCode(response)))
}

export async function createAgentPairing(_previousState: SecretActionState, formData: FormData): Promise<SecretActionState> {
  const tenantId = stringField(formData, 'tenant_id')
  const response = await postJson(`/api/v1/tenants/${tenantId}/agent-pairings`, {
    name: stringField(formData, 'name'),
  })
  if (!response.ok) {
    return { ok: false, error: await errorCode(response) }
  }
  const body = (await response.json()) as { agent: Agent; agent_env: string }
  return {
    ok: true,
    kind: 'agent_pairing',
    agent: body.agent,
    agentEnv: body.agent_env,
    message: 'Agent pairing created',
  }
}

export async function retryDispatchJob(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const jobId = stringField(formData, 'job_id')
  const response = await postJson(`/api/v1/tenants/${tenantId}/jobs/${jobId}/retry-dispatch`, {
    reason: nullableField(formData, 'reason'),
  })
  redirect(statusUrl(tenantId, response.ok ? 'retry_queued' : await errorCode(response)))
}

export async function reprintJob(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const jobId = stringField(formData, 'job_id')
  const response = await postJson(`/api/v1/tenants/${tenantId}/jobs/${jobId}/reprint`, {
    reason: nullableField(formData, 'reason'),
  })
  redirect(statusUrl(tenantId, response.ok ? 'reprint_queued' : await errorCode(response)))
}

export async function duplicateJob(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const jobId = stringField(formData, 'job_id')
  const plateId = nullableField(formData, 'plate_id')
  const response = await postJson(`/api/v1/tenants/${tenantId}/jobs/${jobId}/duplicate`, {
    printer_id: nullableField(formData, 'printer_id'),
    plate_id: plateId ? Number(plateId) : null,
    use_ams: optionalBoolean(formData, 'use_ams'),
    flow_cali: optionalBoolean(formData, 'flow_cali'),
    timelapse: optionalBoolean(formData, 'timelapse'),
    ams_mapping: null,
    ams_mapping2: null,
  })
  redirect(statusUrl(tenantId, response.ok ? 'duplicate_queued' : await errorCode(response)))
}

export async function controlPrinter(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const printerId = stringField(formData, 'printer_id')
  const action = stringField(formData, 'action')
  const speedMode = nullableField(formData, 'speed_mode')
  const response = await postJson(`/api/v1/tenants/${tenantId}/printers/${printerId}/controls`, {
    action,
    speed_mode: speedMode ? Number(speedMode) : undefined,
  })
  redirect(statusUrl(tenantId, response.ok ? 'printer_control_queued' : await errorCode(response)))
}

export async function createPluginTicket(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const redirectUrl = stringField(formData, 'redirect_url')
  const response = await postJson(`/api/v1/tenants/${tenantId}/plugin/login-tickets`, {
    redirect_url: redirectUrl,
  })
  if (!response.ok) {
    redirect(statusUrl(tenantId, await errorCode(response)))
  }
  const body = (await response.json()) as { ticket: string; redirect_url: string }
  const url = new URL(body.redirect_url)
  url.searchParams.set('ticket', body.ticket)
  url.searchParams.set('redirect_url', body.redirect_url)
  redirect(url.toString())
}

async function postJson(path: string, body: unknown) {
  return fetch(`${apiUrl}${path}`, {
    method: 'POST',
    headers: await apiHeaders('application/json'),
    body: JSON.stringify(body),
  })
}

function stringField(formData: FormData, name: string) {
  const value = formData.get(name)
  return typeof value === 'string' ? value : ''
}

function nullableField(formData: FormData, name: string) {
  const value = stringField(formData, name).trim()
  return value.length > 0 ? value : null
}

function optionalBoolean(formData: FormData, name: string) {
  return formData.has(name) ? formData.get(name) === 'on' : null
}

async function errorCode(response: Response) {
  try {
    const body = (await response.json()) as { error?: string }
    return body.error ?? `http_${response.status}`
  } catch {
    return `http_${response.status}`
  }
}

function statusUrl(tenantId: string, status: string) {
  return `/?tenant=${encodeURIComponent(tenantId)}&status=${encodeURIComponent(status)}`
}
