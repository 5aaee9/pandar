'use server'

import { redirect } from 'next/navigation'

const apiUrl = process.env.APP_API_URL ?? 'http://localhost:8080'

export async function createPrintJob(formData: FormData) {
  const tenantId = stringField(formData, 'tenant_id')
  const printerId = stringField(formData, 'printer_id')
  const response = await fetch(
    `${apiUrl}/api/v1/tenants/${tenantId}/printers/${printerId}/jobs`,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
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

function stringField(formData: FormData, name: string) {
  const value = formData.get(name)
  return typeof value === 'string' ? value : ''
}
