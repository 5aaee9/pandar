export type Summary = {
  tenants: number
  agents: number
  printers: number
  commands: number
}

export type Tenant = {
  id: string
  slug: string
  display_name: string
  created_at: string
}

export type Printer = {
  id: string
  tenant_id: string
  agent_id: string
  serial_number: string
  name: string
  model: string | null
  status: string
  last_seen_at: string
  created_at: string
}

export type Agent = {
  id: string
  tenant_id: string
  name: string
  status: string
  created_at: string
}

export type TenantList = {
  tenants: Tenant[]
}

export type PrinterList = {
  printers: Printer[]
}

export type AgentList = {
  agents: Agent[]
}

export type Command = {
  id: string
  tenant_id: string
  agent_id: string
  printer_id: string | null
  kind: string
  status: string
  payload_json: string
  error: string | null
  result_json: string | null
  created_at: string
  updated_at: string
}

export type DiscoveryResultData = {
  type: 'printer_discovery'
  printers: Array<{
    serial_number?: string
    host: string
    name?: string
    model?: string
    source?: string
  }>
}

export type DiagnosticResultData = {
  type: 'printer_diagnostic'
  serial_number: string
  host?: string
  model?: string
  overall: string
  checks: Array<{
    id: string
    status: string
    message: string
    details?: string
  }>
  compatibility?: {
    normalized_model?: string | null
    external_storage?: string
    ftps_tls_1_2_cap?: boolean
    ftps_clear_data_fallback?: boolean
    features?: Record<string, string>
  }
}

export type CommandResultData = DiscoveryResultData | DiagnosticResultData

export type Job = {
  id: string
  printer_id: string
  agent_id: string
  artifact_id: string
  command_id: string
  status: string
  error: string | null
  created_at: string
  updated_at: string
  print: {
    status: string
    printer_state: string | null
    progress_percent: number | null
    remaining_time_minutes: number | null
    current_layer: number | null
    total_layers: number | null
    active_file: string | null
    last_progress_percent: number | null
    last_layer: number | null
    error: string | null
    started_at: string | null
    finished_at: string | null
    updated_at: string | null
  }
  command: {
    id: string
    kind: string
    status: string
  }
  artifact: {
    filename: string
    content_type: string
    size_bytes: number
  }
}

export type JobList = {
  jobs: Job[]
}

export type FetchResult<T> =
  | { data: T; error: null }
  | { data: null; error: null }
  | { data: null; error: string }
