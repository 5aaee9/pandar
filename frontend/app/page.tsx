import { apiHeaders, authSource } from './api-auth'
import { parseCommandResult } from './command-result-parser'
import type {
  AgentList,
  Command,
  FetchResult,
  JobList,
  PrinterList,
  Summary,
  TenantList,
} from './dashboard-types'
import { DashboardRuntime } from './dashboard-runtime'

const apiUrl = process.env.APP_API_URL ?? 'http://localhost:8080'
const configuredTenantId = process.env.APP_TENANT_ID

type PageProps = {
  searchParams?: Promise<{
    tenant?: string | string[]
    command?: string | string[]
  }>
}

async function fetchJson<T>(path: string, label: string): Promise<FetchResult<T>> {
  try {
    const response = await fetch(`${apiUrl}${path}`, {
      cache: 'no-store',
      headers: await apiHeaders(),
    })
    if (!response.ok) {
      return { data: null, error: `${label} returned ${response.status}` }
    }

    return { data: (await response.json()) as T, error: null }
  } catch (error) {
    return {
      data: null,
      error: `${label} failed: ${error instanceof Error ? error.message : 'unknown error'}`,
    }
  }
}

export default async function Page({ searchParams }: PageProps) {
  const [summaryResult, tenantsResult] = await Promise.all([
    configuredTenantId
      ? Promise.resolve<FetchResult<Summary>>({
          data: null,
          error: null,
        })
      : fetchJson<Summary>('/api/v1/summary', 'Summary'),
    configuredTenantId
      ? Promise.resolve<FetchResult<TenantList>>({
          data: { tenants: [] },
          error: null,
        })
      : fetchJson<TenantList>('/api/v1/tenants', 'Tenants'),
  ])

  const tenants = tenantsResult.data?.tenants ?? []
  const params = await searchParams
  const requestedTenant = Array.isArray(params?.tenant) ? params.tenant[0] : params?.tenant
  const requestedCommand = Array.isArray(params?.command) ? params.command[0] : params?.command
  const selectedTenant = configuredTenantId
    ? {
        id: configuredTenantId,
        slug: configuredTenantId,
        display_name: configuredTenantId,
        created_at: '',
      }
    : tenants.find((tenant) => tenant.id === requestedTenant) ?? tenants[0] ?? null
  const printersResult = selectedTenant
    ? await fetchJson<PrinterList>(
        `/api/v1/tenants/${selectedTenant.id}/printers`,
        'Printers',
      )
    : null
  const agentsResult = selectedTenant
    ? await fetchJson<AgentList>(`/api/v1/tenants/${selectedTenant.id}/agents`, 'Agents')
    : null
  const jobsResult = selectedTenant
    ? await fetchJson<JobList>(`/api/v1/tenants/${selectedTenant.id}/jobs`, 'Jobs')
    : null
  const commandResult =
    selectedTenant && requestedCommand
      ? await fetchJson<Command>(
          `/api/v1/tenants/${selectedTenant.id}/commands/${requestedCommand}`,
          'Command',
        )
      : null
  const printers = printersResult?.data?.printers ?? []
  const agents = agentsResult?.data?.agents ?? []
  const jobs = jobsResult?.data?.jobs ?? []
  const selectedCommand = commandResult?.data ?? null
  const commandData = parseCommandResult(selectedCommand)
  const errors = [
    summaryResult.error,
    tenantsResult.error,
    printersResult?.error,
    agentsResult?.error,
    jobsResult?.error,
    commandResult?.error,
  ].filter((error): error is string => Boolean(error))

  return (
    <DashboardRuntime
      apiUrl={apiUrl}
      configuredTenantId={configuredTenantId}
      summary={summaryResult.data}
      tenants={tenants}
      selectedTenant={selectedTenant}
      initialPrinters={printers}
      agents={agents}
      initialJobs={jobs}
      selectedCommand={selectedCommand}
      commandData={commandData}
      errors={errors}
      auth={await authSource()}
    />
  )
}
