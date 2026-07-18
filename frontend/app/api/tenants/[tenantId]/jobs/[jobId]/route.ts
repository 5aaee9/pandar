import { apiHeaders } from '../../../../../api-auth'
import { apiIdSegment, invalidApiIdResponse, isApiId } from '@/app/api-path'

const apiUrl = process.env.APP_API_URL ?? 'http://localhost:8080'

export const dynamic = 'force-dynamic'

export async function DELETE(
  request: Request,
  context: { params: Promise<{ tenantId: string; jobId: string }> },
): Promise<Response> {
  const { tenantId, jobId } = await context.params
  if (!isApiId(tenantId) || !isApiId(jobId)) {
    return invalidApiIdResponse()
  }
  const upstream = await fetch(
    `${apiUrl}/api/v1/tenants/${apiIdSegment(tenantId, 'tenant_id')}/jobs/${apiIdSegment(jobId, 'job_id')}`,
    {
      method: 'DELETE',
      cache: 'no-store',
      headers: await apiHeaders(),
      signal: request.signal,
    },
  )

  const headers = new Headers({ 'cache-control': 'no-store' })
  const contentType = upstream.headers.get('content-type')
  if (contentType) {
    headers.set('content-type', contentType)
  }

  return new Response(upstream.body, {
    status: upstream.status,
    statusText: upstream.statusText,
    headers,
  })
}
