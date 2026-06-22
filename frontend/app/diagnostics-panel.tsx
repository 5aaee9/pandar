import { diagnosePrinter, discoverPrinters } from './actions'
import { EmptyState, formatDate, StatusBadge } from './dashboard-ui'
import type {
  Agent,
  Command,
  CommandResultData,
  DiagnosticResultData,
  DiscoveryResultData,
  Printer,
  Tenant,
} from './dashboard-types'

export function LinkedAgentsSection({
  selectedTenant,
  agents,
}: {
  selectedTenant: Tenant | null
  agents: Agent[]
}) {
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <div className="flex flex-col gap-2 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold">Linked agents</h2>
          <p className="mt-0.5 text-sm text-slate-600">
            {selectedTenant
              ? `${selectedTenant.display_name} (${selectedTenant.slug})`
              : 'No tenant selected'}
          </p>
        </div>
        <div className="text-sm text-slate-600">{agents.length} linked</div>
      </div>

      {!selectedTenant ? (
        <EmptyState title="No tenant selected" message="Select a tenant to inspect agents." />
      ) : agents.length === 0 ? (
        <EmptyState
          title="No agents linked"
          message="Create an agent pairing before running discovery."
        />
      ) : (
        <div className="overflow-x-auto">
          <table className="min-w-full border-collapse text-left text-sm">
            <thead className="bg-slate-50 text-xs font-semibold uppercase text-slate-600">
              <tr>
                <th className="px-4 py-2">Agent</th>
                <th className="px-4 py-2">Status</th>
                <th className="px-4 py-2">Created</th>
                <th className="px-4 py-2">Discovery</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-200">
              {agents.map((agent) => (
                <tr key={agent.id}>
                  <td className="px-4 py-3">
                    <div className="font-medium text-slate-950">{agent.name}</div>
                    <div className="font-mono text-xs text-slate-600">{agent.id}</div>
                  </td>
                  <td className="px-4 py-3">
                    <StatusBadge value={agent.status} />
                  </td>
                  <td className="px-4 py-3 text-slate-700">{formatDate(agent.created_at)}</td>
                  <td className="px-4 py-3">
                    <form action={discoverPrinters} className="flex flex-wrap items-end gap-2">
                      <input name="tenant_id" type="hidden" value={selectedTenant.id} />
                      <input name="agent_id" type="hidden" value={agent.id} />
                      <label className="flex flex-col gap-1 text-xs font-medium text-slate-500">
                        Timeout
                        <input
                          className="h-9 w-20 rounded-md border border-slate-300 px-2 text-sm font-normal text-slate-950"
                          defaultValue="5"
                          max="15"
                          min="1"
                          name="timeout_seconds"
                          type="number"
                        />
                      </label>
                      <button
                        className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white"
                        type="submit"
                      >
                        Discover
                      </button>
                    </form>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}

export function DiagnosticsSection({
  selectedTenant,
  printers,
  selectedCommand,
  commandData,
}: {
  selectedTenant: Tenant | null
  printers: Printer[]
  selectedCommand: Command | null
  commandData: CommandResultData | null
}) {
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <div className="flex flex-col gap-2 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold">Discovery and diagnostics</h2>
          <p className="mt-0.5 text-sm text-slate-600">
            {selectedCommand
              ? `${selectedCommand.kind} · ${selectedCommand.status}`
              : 'No command selected'}
          </p>
        </div>
        {selectedCommand ? (
          <div className="font-mono text-xs text-slate-600">{selectedCommand.id}</div>
        ) : null}
      </div>

      {selectedTenant && printers.length > 0 ? (
        <div className="border-b border-slate-200">
          <div className="divide-y divide-slate-200">
            {printers.map((printer) => (
              <form
                key={printer.id}
                action={diagnosePrinter}
                className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
              >
                <input name="tenant_id" type="hidden" value={selectedTenant.id} />
                <input name="agent_id" type="hidden" value={printer.agent_id} />
                <input name="serial_number" type="hidden" value={printer.serial_number} />
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium text-slate-950">
                    {printer.name}
                  </div>
                  <div className="truncate font-mono text-xs text-slate-600">
                    {printer.serial_number}
                  </div>
                </div>
                <button
                  className="h-9 rounded-md border border-slate-300 px-3 text-sm font-medium text-slate-800"
                  type="submit"
                >
                  Diagnose
                </button>
              </form>
            ))}
          </div>
        </div>
      ) : null}

      {!selectedCommand ? (
        <EmptyState
          title="No command selected"
          message="Run discovery or diagnostics to inspect the latest structured result."
        />
      ) : commandData?.type === 'printer_discovery' ? (
        <DiscoveryResult result={commandData} />
      ) : commandData?.type === 'printer_diagnostic' ? (
        <DiagnosticResult result={commandData} />
      ) : (
        <EmptyState
          title="No structured result"
          message={selectedCommand.error ?? 'The selected command has not returned result data.'}
        />
      )}
    </section>
  )
}

function DiscoveryResult({ result }: { result: DiscoveryResultData }) {
  return result.printers.length === 0 ? (
    <EmptyState title="No printers discovered" message="Discovery completed with no SSDP responses." />
  ) : (
    <div className="overflow-x-auto">
      <table className="min-w-full border-collapse text-left text-sm">
        <thead className="bg-slate-50 text-xs font-semibold uppercase text-slate-600">
          <tr>
            <th className="px-4 py-2">Name</th>
            <th className="px-4 py-2">Serial</th>
            <th className="px-4 py-2">Host</th>
            <th className="px-4 py-2">Model</th>
            <th className="px-4 py-2">Source</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-200">
          {result.printers.map((printer) => (
            <tr key={`${printer.serial_number ?? 'unknown'}-${printer.host}`}>
              <td className="px-4 py-3 font-medium text-slate-950">{printer.name ?? '-'}</td>
              <td className="px-4 py-3 font-mono text-xs text-slate-700">
                {printer.serial_number ?? '-'}
              </td>
              <td className="px-4 py-3 font-mono text-xs text-slate-700">{printer.host}</td>
              <td className="px-4 py-3 text-slate-700">{printer.model ?? '-'}</td>
              <td className="px-4 py-3 text-slate-700">{printer.source ?? '-'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function DiagnosticResult({ result }: { result: DiagnosticResultData }) {
  const compatibility = result.compatibility
  const features = compatibility?.features ?? {}
  return (
    <div className="grid gap-0 lg:grid-cols-[minmax(0,1.4fr)_minmax(280px,0.8fr)]">
      <div className="border-b border-slate-200 lg:border-b-0 lg:border-r">
        <div className="flex flex-wrap items-center gap-2 border-b border-slate-200 px-4 py-3">
          <StatusBadge value={result.overall} />
          <span className="font-mono text-xs text-slate-600">{result.serial_number}</span>
          {result.host ? <span className="font-mono text-xs text-slate-600">{result.host}</span> : null}
          {result.model ? <span className="text-xs text-slate-600">{result.model}</span> : null}
        </div>
        <div className="overflow-x-auto">
          <table className="min-w-full border-collapse text-left text-sm">
            <thead className="bg-slate-50 text-xs font-semibold uppercase text-slate-600">
              <tr>
                <th className="px-4 py-2">Check</th>
                <th className="px-4 py-2">Status</th>
                <th className="px-4 py-2">Message</th>
                <th className="px-4 py-2">Details</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-200">
              {result.checks.map((check) => (
                <tr key={check.id}>
                  <td className="px-4 py-3 font-mono text-xs text-slate-700">{check.id}</td>
                  <td className="px-4 py-3">
                    <StatusBadge value={check.status} />
                  </td>
                  <td className="px-4 py-3 text-slate-800">{check.message}</td>
                  <td className="px-4 py-3 text-xs text-slate-600">{check.details ?? '-'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      <div className="px-4 py-3">
        <h3 className="text-sm font-semibold text-slate-950">Compatibility</h3>
        <dl className="mt-3 grid gap-2 text-sm">
          <CompatibilityRow
            label="Model"
            value={compatibility?.normalized_model ?? '-'}
            available={Boolean(compatibility?.normalized_model)}
          />
          <CompatibilityRow
            label="External storage"
            value={compatibility?.external_storage ?? 'unknown'}
            available={compatibility?.external_storage === 'supported'}
          />
          <CompatibilityRow
            label="FTPS TLS 1.2 cap"
            value={compatibility?.ftps_tls_1_2_cap ? 'available' : 'unavailable'}
            available={compatibility?.ftps_tls_1_2_cap === true}
          />
          <CompatibilityRow
            label="Clear-data fallback"
            value={compatibility?.ftps_clear_data_fallback ? 'available' : 'unavailable'}
            available={compatibility?.ftps_clear_data_fallback === true}
          />
          {Object.entries(features).map(([name, value]) => (
            <CompatibilityRow
              key={name}
              label={formatCapabilityName(name)}
              value={value}
              available={value === 'supported'}
            />
          ))}
        </dl>
      </div>
    </div>
  )
}

function CompatibilityRow({
  label,
  value,
  available,
}: {
  label: string
  value: string
  available: boolean
}) {
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-slate-100 py-1.5 last:border-b-0">
      <dt className="min-w-0 truncate text-slate-700">{label}</dt>
      <dd className="flex items-center gap-2 text-right text-xs font-medium text-slate-700">
        <span
          className={
            available
              ? 'inline-flex rounded bg-emerald-700 px-2 py-1 text-white'
              : 'inline-flex rounded bg-slate-200 px-2 py-1 text-slate-700'
          }
        >
          {available ? 'Available' : 'Unavailable'}
        </span>
        <span className="font-mono text-slate-500">{value}</span>
      </dd>
    </div>
  )
}

function formatCapabilityName(value: string) {
  return value
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}
