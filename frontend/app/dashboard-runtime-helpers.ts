import type { AuthMetadata, Job, Printer } from './dashboard-types'
import { formatDate } from './dashboard-ui'

export type LiveState = 'idle' | 'connecting' | 'live' | 'disconnected' | 'unavailable' | 'error'

export type RuntimeNotification = {
  key: string
  title: string
  detail: string
  timestamp: string
}

export function mergePrinter(printers: Printer[], printer: Printer) {
  return printers.some((current) => current.id === printer.id)
    ? printers.map((current) => (current.id === printer.id ? printer : current))
    : [printer, ...printers]
}

export function mergeJob(jobs: Job[], job: Job) {
  return jobs.some((current) => current.id === job.id)
    ? jobs.map((current) => (current.id === job.id ? job : current))
    : [job, ...jobs]
}

export function printerEventWebSocketUrl(apiUrl: string, tenantId: string, ticket: string) {
  const base = new URL(apiUrl)
  const basePath = base.pathname.replace(/\/$/, '')
  const url = new URL(
    `${basePath}/api/v1/tenants/${encodeURIComponent(tenantId)}/printer-events`,
    base,
  )
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
  url.searchParams.set('ticket', ticket)
  return url.toString()
}

export function formatAuthSource(source: AuthMetadata['source']) {
  switch (source) {
    case 'request_cookie':
      return 'Request cookie'
    case 'app_auth_bearer_token':
      return 'App bearer token'
    case 'app_api_token':
      return 'App API token'
    case 'none':
      return 'No auth'
  }
}

export function formatPrinterMaterials(printer: Printer) {
  const materials = printer.materials
  if (!materials) {
    return { summary: 'No material state', detail: 'Awaiting printer report' }
  }
  const amsTrays = materials.ams_units.reduce(
    (count, unit) => count + (unit.trays?.filter((tray) => tray.exists !== false).length ?? 0),
    0,
  )
  const external = materials.external_spools.filter((spool) => spool.exists !== false).length
  const active = materials.active_tray
    ? materials.active_tray.kind === 'external'
      ? 'External spool'
      : `AMS ${materials.active_tray.ams_id ?? '-'}:${materials.active_tray.tray_id ?? '-'}`
    : 'No active tray'
  return {
    summary: `${amsTrays} AMS tray${amsTrays === 1 ? '' : 's'}, ${external} external`,
    detail: `${active} · ${formatDate(materials.observed_at)}`,
  }
}

export function formatJobMaterial(job: Job) {
  const usage = job.material.filament_usage
  if (usage.length > 0) {
    return usage
      .map((row) => {
        const slot =
          row.external_id !== null
            ? `external ${row.tray_id ?? '-'}`
            : `AMS ${row.ams_id ?? '-'}:${row.tray_id ?? '-'}`
        return `${row.slot_index}: ${slot} ${row.filament_type ?? row.filament_id ?? ''}`.trim()
      })
      .join(', ')
  }
  const mappings = [
    job.material.ams_mapping ? `ams_mapping ${job.material.ams_mapping.length}` : null,
    job.material.ams_mapping2 ? `ams_mapping2 ${job.material.ams_mapping2.length}` : null,
  ].filter(Boolean)
  return mappings.length > 0 ? mappings.join(', ') : 'No material mapping'
}
