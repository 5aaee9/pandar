import { apiHeaders, authSource } from './api-auth'
import { parseCommandResult } from './command-result-parser'
import type {
  AgentList,
  AuditEventList,
  Command,
  FetchResult,
  JoinLinkList,
  JobList,
  MeResponse,
  PrinterList,
  Summary,
  TenantList,
  TenantTokenList,
  UserIdentityList,
  UserList,
} from './dashboard-types'
import { DashboardRuntime } from './dashboard-runtime'
import { OnboardingPanel } from './onboarding-panel'

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
  const auth = await authSource()
  const useExternalOnboarding = auth.provider !== 'none' && !configuredTenantId
  const [summaryResult, tenantsResult, meResult] = await Promise.all([
    configuredTenantId || useExternalOnboarding
      ? Promise.resolve<FetchResult<Summary>>({
          data: null,
          error: null,
        })
      : fetchJson<Summary>('/api/v1/summary', 'Summary'),
    configuredTenantId || useExternalOnboarding
      ? Promise.resolve<FetchResult<TenantList>>({
          data: { tenants: [] },
          error: null,
        })
      : fetchJson<TenantList>('/api/v1/tenants', 'Tenants'),
    auth.provider === 'none'
      ? Promise.resolve<FetchResult<MeResponse>>({ data: null, error: null })
      : fetchJson<MeResponse>('/api/v1/me', 'Current identity'),
  ])

  const externalTenants =
    meResult.data?.tenants.map((tenant) => ({
      id: tenant.tenant_id,
      slug: tenant.tenant_slug,
      display_name: tenant.display_name,
      created_at: '',
    })) ?? []
  const tenants = auth.provider === 'none' ? (tenantsResult.data?.tenants ?? []) : externalTenants
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
  const [usersResult, tenantTokensResult, joinLinksResult, auditEventsResult] = selectedTenant
    ? await Promise.all([
        fetchJson<UserList>(`/api/v1/tenants/${selectedTenant.id}/users`, 'Users'),
        fetchJson<TenantTokenList>(
          `/api/v1/tenants/${selectedTenant.id}/tenant-tokens`,
          'Tenant tokens',
        ),
        fetchJson<JoinLinkList>(
          `/api/v1/tenants/${selectedTenant.id}/join-links`,
          'Join links',
        ),
        fetchJson<AuditEventList>(
          `/api/v1/tenants/${selectedTenant.id}/audit-events?limit=20`,
          'Audit events',
        ),
      ])
    : [null, null, null, null]
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
  const joinLinks = joinLinksResult?.data?.join_links ?? []
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
      joinLinksResult?.error ||
      auditEventsResult?.error ||
      identityResults.some((result) => result.error),
  )
  const selectedCommand = commandResult?.data ?? null
  const commandData = parseCommandResult(selectedCommand)
  const errors = [
    summaryResult.error,
    tenantsResult.error,
    meResult.error && tenants.length === 0 ? meResult.error : null,
    printersResult?.error,
    agentsResult?.error,
    jobsResult?.error,
    commandResult?.error,
  ].filter((error): error is string => Boolean(error))

  return meResult.data && tenants.length === 0 ? (
    <OnboardingPanel me={meResult.data} />
  ) : (
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
      joinLinks={joinLinks}
      auditEvents={auditEvents}
      adminUnavailable={adminUnavailable}
      actionStatus={actionStatus}
      selectedCommand={selectedCommand}
      commandData={commandData}
      errors={errors}
      auth={auth}
    />
  )
}
