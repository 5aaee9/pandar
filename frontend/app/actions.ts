'use server'

import { redirect } from 'next/navigation'

const apiUrl = process.env.APP_API_URL ?? 'http://localhost:8080'
const apiToken = process.env.APP_API_TOKEN

export async function createPrintJob(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const printerId = stringField(formData, 'printer_id')
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/printers/${printerId}/jobs`,
    {
      method: 'POST',
      headers: apiHeaders(),
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

function apiHeaders() {
  const headers: Record<string, string> = { 'content-type': 'application/json' }
  if (apiToken) {
    headers.authorization = `Bearer ${apiToken}`
  }
  return headers
}

function stringField(formData: FormData, name: string) {
  const value = formData.get(name)
  return typeof value === 'string' ? value : ''
}
