'use server'

import { redirect } from 'next/navigation'

import { apiHeaders } from './api-auth'

const apiUrl = process.env.APP_API_URL ?? 'http://localhost:8080'

export async function createPrintJob(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const printerId = stringField(formData, 'printer_id')
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/printers/${printerId}/jobs`,
    {
      method: 'POST',
      headers: await apiHeaders('application/json'),
      body: JSON.stringify({
        filename: stringField(formData, 'filename'),
        content_type: stringField(formData, 'content_type'),
        artifact_base64: stringField(formData, 'artifact_base64'),
        plate_id: Number(stringField(formData, 'plate_id')),
        use_ams: formData.get('use_ams') === 'on',
        flow_cali: formData.get('flow_cali') === 'on',
        timelapse: formData.get('timelapse') === 'on',
      }),
    },
  )

  if (!response.ok) {
    throw new Error(`Create print job returned ${response.status}`)
  }

  redirect(`/?tenant=${encodeURIComponent(tenantId)}`)
}

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

function stringField(formData: FormData, name: string) {
  const value = formData.get(name)
  return typeof value === 'string' ? value : ''
}
