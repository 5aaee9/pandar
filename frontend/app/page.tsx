import { apiHeaders } from './api-auth'
import { parseCommandResult } from './command-result-parser'
import { DiagnosticsSection, LinkedAgentsSection } from './diagnostics-panel'
import { DispatchForm } from './dispatch-form'
import type {
  AgentList,
  Command,
  FetchResult,
  JobList,
  PrinterList,
  Summary,
  TenantList,
} from './dashboard-types'
import { EmptyState, formatBytes, formatDate, Metric } from './dashboard-ui'
import { formatLayers, formatProgress, formatRemaining } from './job-format'

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
  ].filter(Boolean)

  return (
    <main className="min-h-screen bg-slate-100 px-4 py-5 text-slate-950 sm:px-6 lg:px-8">
      <section className="mx-auto flex max-w-7xl flex-col gap-5">
        <header className="flex flex-col gap-3 border-b border-slate-300 pb-4 md:flex-row md:items-end md:justify-between">
          <div>
            <h1 className="text-2xl font-semibold">Pandar Operations</h1>
            <p className="mt-1 text-sm text-slate-600">
              Tenant printer inventory from {apiUrl}
            </p>
          </div>
          {tenants.length > 1 ? (
            <form className="flex min-w-72 items-end gap-2" action="/">
              <label className="flex flex-1 flex-col gap-1 text-sm">
                <span className="text-xs font-medium text-slate-500">Tenant</span>
                <select
                  name="tenant"
                  defaultValue={selectedTenant?.id}
                  className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950"
                >
                  {tenants.map((tenant) => (
                    <option key={tenant.id} value={tenant.id}>
                      {tenant.display_name}
                    </option>
                  ))}
                </select>
              </label>
              <button
                className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white"
                type="submit"
              >
                View
              </button>
            </form>
          ) : null}
        </header>

        {errors.length > 0 ? (
          <div className="rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-950">
            Hub data is incomplete. {errors.join('; ')}.
          </div>
        ) : null}

        <section className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <Metric label="Tenants" value={summaryResult.data?.tenants ?? (configuredTenantId ? 1 : undefined)} />
          <Metric label="Agents" value={summaryResult.data?.agents ?? (configuredTenantId ? agents.length : undefined)} />
          <Metric label="Printers" value={summaryResult.data?.printers ?? (configuredTenantId ? printers.length : undefined)} />
          <Metric label="Commands" value={summaryResult.data?.commands} />
        </section>

        <LinkedAgentsSection selectedTenant={selectedTenant} agents={agents} />

        <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
          <div className="flex flex-col gap-2 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h2 className="text-base font-semibold">Printer inventory</h2>
              <p className="mt-0.5 text-sm text-slate-600">
                {selectedTenant
                  ? `${selectedTenant.display_name} (${selectedTenant.slug})`
                  : 'No tenant selected'}
              </p>
            </div>
            <div className="text-sm text-slate-600">{printers.length} reported</div>
          </div>

          {!selectedTenant ? (
            <EmptyState
              title="No tenants"
              message="Create a tenant through the hub API before printers can be reported."
            />
          ) : printers.length === 0 ? (
            <EmptyState
              title="No printers reported"
              message="Connect an agent and run a printer refresh to populate this inventory."
            />
          ) : (
            <div className="overflow-x-auto">
              <table className="min-w-full border-collapse text-left text-sm">
                <thead className="bg-slate-50 text-xs font-semibold uppercase text-slate-600">
                  <tr>
                    <th className="px-4 py-2">Name</th>
                    <th className="px-4 py-2">Serial</th>
                    <th className="px-4 py-2">Model</th>
                    <th className="px-4 py-2">Status</th>
                    <th className="px-4 py-2">Dispatch API</th>
                    <th className="px-4 py-2">Agent ID</th>
                    <th className="px-4 py-2">Last seen</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-200">
                  {printers.map((printer) => (
                    <tr key={printer.id}>
                      <td className="px-4 py-3 font-medium text-slate-950">{printer.name}</td>
                      <td className="px-4 py-3 font-mono text-xs text-slate-700">
                        {printer.serial_number}
                      </td>
                      <td className="px-4 py-3 text-slate-700">{printer.model ?? 'Unknown'}</td>
                      <td className="px-4 py-3">
                        <span className="rounded bg-emerald-700 px-2 py-1 text-xs font-medium text-white">
                          {printer.status}
                        </span>
                      </td>
                      <td className="px-4 py-3 font-mono text-xs text-slate-700">
                        POST /api/v1/tenants/{selectedTenant.id}/printers/{printer.id}/jobs
                      </td>
                      <td className="px-4 py-3 font-mono text-xs text-slate-700">
                        {printer.agent_id}
                      </td>
                      <td className="px-4 py-3 text-slate-700">
                        {formatDate(printer.last_seen_at)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>

        <DispatchForm selectedTenant={selectedTenant} printers={printers} />

        <DiagnosticsSection
          selectedTenant={selectedTenant}
          printers={printers}
          selectedCommand={selectedCommand}
          commandData={commandData}
        />

        <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
          <div className="flex flex-col gap-2 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h2 className="text-base font-semibold">Print jobs</h2>
              <p className="mt-0.5 text-sm text-slate-600">
                Queued and dispatched project-file jobs for the selected tenant
              </p>
            </div>
            <div className="text-sm text-slate-600">{jobs.length} jobs</div>
          </div>

          {!selectedTenant ? (
            <EmptyState title="No tenant selected" message="Select a tenant to inspect jobs." />
          ) : jobs.length === 0 ? (
            <EmptyState
              title="No jobs"
              message="Create a print job through the printer dispatch API to populate history."
            />
          ) : (
            <div className="overflow-x-auto">
              <table className="min-w-full border-collapse text-left text-sm">
                <thead className="bg-slate-50 text-xs font-semibold uppercase text-slate-600">
                  <tr>
                    <th className="px-4 py-2">Job</th>
                    <th className="px-4 py-2">Artifact</th>
                    <th className="px-4 py-2">Printer</th>
                    <th className="px-4 py-2">Dispatch</th>
                    <th className="px-4 py-2">Print</th>
                    <th className="px-4 py-2">Progress</th>
                    <th className="px-4 py-2">Created</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-200">
                  {jobs.map((job) => (
                    <tr key={job.id}>
                      <td className="px-4 py-3 font-mono text-xs text-slate-700">{job.id}</td>
                      <td className="px-4 py-3">
                        <div className="font-medium text-slate-950">{job.artifact.filename}</div>
                        <div className="text-xs text-slate-600">
                          {job.artifact.content_type} · {formatBytes(job.artifact.size_bytes)}
                        </div>
                      </td>
                      <td className="px-4 py-3 font-mono text-xs text-slate-700">
                        {job.printer_id}
                      </td>
                      <td className="px-4 py-3">
                        <div className="font-mono text-xs text-slate-700">{job.command.id}</div>
                        <div className="text-xs text-slate-600">{job.command.kind}</div>
                        <span className="mt-1 inline-flex rounded bg-slate-800 px-2 py-1 text-xs font-medium text-white">
                          {job.status}
                        </span>
                        {job.error ? <div className="mt-1 text-xs text-red-700">{job.error}</div> : null}
                      </td>
                      <td className="px-4 py-3">
                        <span className="rounded bg-cyan-700 px-2 py-1 text-xs font-medium text-white">
                          {job.print.status}
                        </span>
                        {job.print.printer_state ? (
                          <div className="mt-1 text-xs text-slate-600">{job.print.printer_state}</div>
                        ) : null}
                        {job.print.error ? (
                          <div className="mt-1 text-xs text-red-700">{job.print.error}</div>
                        ) : null}
                      </td>
                      <td className="px-4 py-3 text-slate-700">
                        <div>{formatProgress(job)}</div>
                        <div className="text-xs text-slate-600">{formatLayers(job)}</div>
                        <div className="text-xs text-slate-600">
                          {formatRemaining(job.print.remaining_time_minutes)}
                        </div>
                      </td>
                      <td className="px-4 py-3 text-slate-700">{formatDate(job.created_at)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
      </section>
    </main>
  )
}
