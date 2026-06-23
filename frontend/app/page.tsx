import { apiHeaders, authSource } from './api-auth'
import { parseCommandResult } from './command-result-parser'
import type {
  AgentList,
  AuditEventList,
  Command,
  FetchResult,
  JobList,
  PrinterList,
  Summary,
  TenantList,
  TenantTokenList,
  UserIdentityList,
  UserList,
} from './dashboard-types'
import { DashboardRuntime } from './dashboard-runtime'

const apiUrl = process.env.APP_API_URL ?? 'http://localhost:8080'
const configuredTenantId = process.env.APP_TENANT_ID

type PageProps = {
  searchParams?: Promise<{
    tenant?: string | string[]
    command?: string | string[]
    status?: string | string[]
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
  const actionStatus = Array.isArray(params?.status) ? params.status[0] : params?.status
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
  const [usersResult, tenantTokensResult, auditEventsResult] = selectedTenant
    ? await Promise.all([
        fetchJson<UserList>(`/api/v1/tenants/${selectedTenant.id}/users`, 'Users'),
        fetchJson<TenantTokenList>(
          `/api/v1/tenants/${selectedTenant.id}/tenant-tokens`,
          'Tenant tokens',
        ),
        fetchJson<AuditEventList>(
          `/api/v1/tenants/${selectedTenant.id}/audit-events?limit=20`,
          'Audit events',
        ),
      ])
    : [null, null, null]
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
  const users = usersResult?.data?.users ?? []
  const tenantTokens = tenantTokensResult?.data?.tenant_tokens ?? []
  const auditEvents = auditEventsResult?.data?.audit_events ?? []
  const identityResults = selectedTenant
    ? await Promise.all(
        users.map((user) =>
          fetchJson<UserIdentityList>(
            `/api/v1/tenants/${selectedTenant.id}/users/${user.id}/identities`,
            `Identities for ${user.email}`,
          ),
        ),
      )
    : []
  const userIdentities = identityResults.flatMap((result) => result.data?.identities ?? [])
  const adminUnavailable = Boolean(
    usersResult?.error ||
      tenantTokensResult?.error ||
      auditEventsResult?.error ||
      identityResults.some((result) => result.error),
  )
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
      users={users}
      userIdentities={userIdentities}
      tenantTokens={tenantTokens}
      auditEvents={auditEvents}
      adminUnavailable={adminUnavailable}
      actionStatus={actionStatus}
      selectedCommand={selectedCommand}
      commandData={commandData}
      errors={errors}
      auth={await authSource()}
    />
  )
}
