import { useTranslations } from 'next-intl'

import { FormattedDate } from '../components/formatted-date'
import { diagnosePrinter, discoverPrinters } from './actions'
import { EmptyState, HelpTip, StatusBadge, Tag } from './dashboard-ui'
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
  const t = useTranslations('diagnostics')
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <div className="flex flex-col gap-2 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold">{t('agentsTitle')}</h2>
          <p className="mt-0.5 text-sm text-slate-600">
            {selectedTenant
              ? t('agentsSubtitleTenant', { name: selectedTenant.display_name, slug: selectedTenant.slug })
              : t('agentsSubtitleNone')}
          </p>
        </div>
        <div className="text-sm text-slate-600">{t('agentsMeta', { count: agents.length })}</div>
      </div>

      {!selectedTenant ? (
        <EmptyState title={t('noTenantTitle')} message={t('noTenantMessage')} />
      ) : agents.length === 0 ? (
        <EmptyState title={t('noAgentsTitle')} message={t('noAgentsMessage')} />
      ) : (
        <div className="overflow-x-auto">
          <table className="min-w-full border-collapse text-left text-sm">
            <thead className="bg-slate-50 text-xs font-semibold text-slate-600">
              <tr>
                <th className="px-4 py-2">{t('colAgent')}</th>
                <th className="px-4 py-2">{t('colStatus')}</th>
                <th className="px-4 py-2">{t('colCreated')}</th>
                <th className="px-4 py-2">{t('colDiscovery')}</th>
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
                  <td className="px-4 py-3 text-slate-700">
                    <FormattedDate value={agent.created_at} />
                  </td>
                  <td className="px-4 py-3">
                    <form action={discoverPrinters} className="flex flex-wrap items-end gap-2">
                      <input name="tenant_id" type="hidden" value={selectedTenant.id} />
                      <input name="agent_id" type="hidden" value={agent.id} />
                      <label className="flex flex-col gap-1 text-xs font-medium text-slate-500">
                        {t('timeout')}
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
                        className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white hover:bg-cyan-800"
                        type="submit"
                      >
                        {t('discover')}
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
  const t = useTranslations('diagnostics')
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <div className="flex flex-col gap-2 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold">{t('title')}</h2>
          <p className="mt-0.5 text-sm text-slate-600">
            {selectedCommand
              ? `${selectedCommand.kind} · ${selectedCommand.status}`
              : t('noCommand')}
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
                  {t('diagnose')}
                </button>
              </form>
            ))}
          </div>
        </div>
      ) : null}

      {!selectedCommand ? (
        <EmptyState title={t('noCommandTitle')} message={t('noCommandMessage')} />
      ) : commandData?.type === 'printer_discovery' ? (
        <DiscoveryResult result={commandData} />
      ) : commandData?.type === 'printer_diagnostic' ? (
        <DiagnosticResult result={commandData} />
      ) : (
        <EmptyState
          title={t('noStructuredTitle')}
          message={selectedCommand.error ?? t('noStructuredMessage')}
        />
      )}
    </section>
  )
}

function DiscoveryResult({ result }: { result: DiscoveryResultData }) {
  const t = useTranslations('diagnostics')
  return result.printers.length === 0 ? (
    <EmptyState title={t('noPrintersDiscoveredTitle')} message={t('noPrintersDiscoveredMessage')} />
  ) : (
    <div className="overflow-x-auto">
      <table className="min-w-full border-collapse text-left text-sm">
        <thead className="bg-slate-50 text-xs font-semibold text-slate-600">
          <tr>
            <th className="px-4 py-2">{t('colName')}</th>
            <th className="px-4 py-2">{t('colSerial')}</th>
            <th className="px-4 py-2">{t('colHost')}</th>
            <th className="px-4 py-2">{t('colModel')}</th>
            <th className="px-4 py-2">{t('colSource')}</th>
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
  const t = useTranslations('diagnostics')
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
            <thead className="bg-slate-50 text-xs font-semibold text-slate-600">
              <tr>
                <th className="px-4 py-2">{t('colCheck')}</th>
                <th className="px-4 py-2">{t('colStatus')}</th>
                <th className="px-4 py-2">{t('colMessage')}</th>
                <th className="px-4 py-2">{t('colDetails')}</th>
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
        <h3 className="text-sm font-semibold text-slate-950">{t('compatibility')}</h3>
        <dl className="mt-3 grid gap-2 text-sm">
          <CompatibilityRow
            label={t('model')}
            value={compatibility?.normalized_model ?? '-'}
            available={Boolean(compatibility?.normalized_model)}
          />
          <CompatibilityRow
            label={t('externalStorage')}
            value={compatibility?.external_storage ?? t('unknown')}
            available={compatibility?.external_storage === 'supported'}
            help={t('externalStorageHelp')}
          />
          <CompatibilityRow
            label={t('ftpsCap')}
            value={compatibility?.ftps_tls_1_2_cap ? 'available' : 'unavailable'}
            available={compatibility?.ftps_tls_1_2_cap === true}
            help={t('ftpsCapHelp')}
          />
          <CompatibilityRow
            label={t('clearDataFallback')}
            value={compatibility?.ftps_clear_data_fallback ? 'available' : 'unavailable'}
            available={compatibility?.ftps_clear_data_fallback === true}
            help={t('clearDataFallbackHelp')}
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
  help,
}: {
  label: string
  value: string
  available: boolean
  help?: string
}) {
  const t = useTranslations('diagnostics')
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-slate-100 py-1.5 last:border-b-0">
      <dt className="flex min-w-0 items-center gap-1 text-slate-700">
        <span className="truncate">{label}</span>
        {help ? <HelpTip label={label}>{help}</HelpTip> : null}
      </dt>
      <dd className="flex items-center gap-2 text-right text-xs font-medium text-slate-700">
        <Tag value={available ? t('available') : t('unavailable')} tone={available ? 'success' : 'neutral'} />
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
