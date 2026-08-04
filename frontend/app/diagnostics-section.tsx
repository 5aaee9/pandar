import { useTranslations } from 'next-intl'

import {
  diagnosePrinter,
} from './actions'
import { DetailLine, EmptyState, HelpTip, StatusBadge, Tag } from './dashboard-ui'
import type {
  Command,
  CommandResultData,
  DiagnosticResultData,
  DiscoveryResultData,
  PrinterLinkResultData,
  Printer,
  Tenant,
} from './dashboard-types'

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
    <section className="overflow-hidden rounded-md border border-border bg-card">
      <div className="flex flex-col gap-2 border-b border-border px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold">{t('title')}</h2>
          <p className="mt-0.5 text-sm text-muted-foreground">
            {selectedCommand
              ? `${selectedCommand.kind} · ${selectedCommand.status}`
              : t('noCommand')}
          </p>
        </div>
        {selectedCommand ? (
          <div className="font-mono text-xs text-muted-foreground">{selectedCommand.id}</div>
        ) : null}
      </div>

      {selectedTenant && printers.length > 0 ? (
        <div className="border-b border-border">
          <div className="divide-y divide-border">
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
                  <div className="truncate text-sm font-medium text-foreground">
                    {printer.name}
                  </div>
                  <div className="truncate font-mono text-xs text-muted-foreground">
                    {printer.serial_number}
                  </div>
                </div>
                <button
                  aria-label={t('diagnoseFor', { name: printer.name })}
                  className="h-9 rounded-md border border-border px-3 text-sm font-medium text-foreground transition-colors duration-150 ease-out hover:bg-muted"
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
      ) : commandData?.type === 'printer_link' ? (
        <PrinterLinkResult result={commandData} />
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
        <thead className="bg-muted/60 text-xs font-semibold text-muted-foreground">
          <tr>
            <th className="px-4 py-2">{t('colName')}</th>
            <th className="px-4 py-2">{t('colSerial')}</th>
            <th className="px-4 py-2">{t('colHost')}</th>
            <th className="px-4 py-2">{t('colModel')}</th>
            <th className="px-4 py-2">{t('colSource')}</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {result.printers.map((printer) => (
            <tr key={`${printer.serial_number ?? 'unknown'}-${printer.host}`}>
              <td className="px-4 py-3 font-medium text-foreground">{printer.name ?? '-'}</td>
              <td className="px-4 py-3 font-mono text-xs text-muted-foreground">
                {printer.serial_number ?? '-'}
              </td>
              <td className="px-4 py-3 font-mono text-xs text-muted-foreground">{printer.host}</td>
              <td className="px-4 py-3 text-muted-foreground">{printer.model ?? '-'}</td>
              <td className="px-4 py-3 text-muted-foreground">{printer.source ?? '-'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function PrinterLinkResult({ result }: { result: PrinterLinkResultData }) {
  const t = useTranslations('diagnostics')
  return (
    <div className="grid gap-3 px-4 py-3 text-sm sm:grid-cols-2 lg:grid-cols-5">
      <DetailLine label={t('colHost')} value={result.host} mono />
      <DetailLine label={t('colSerial')} value={result.serial_number} mono />
      <DetailLine label={t('colName')} value={result.name ?? '-'} />
      <DetailLine label={t('colModel')} value={result.model ?? '-'} />
      <DetailLine label={t('colStatus')} value={<StatusBadge value={result.status} />} />
    </div>
  )
}

function DiagnosticResult({ result }: { result: DiagnosticResultData }) {
  const t = useTranslations('diagnostics')
  const compatibility = result.compatibility
  const features = compatibility?.features ?? {}
  return (
    <div className="grid gap-0 lg:grid-cols-[minmax(0,1.4fr)_minmax(280px,0.8fr)]">
      <div className="border-b border-border lg:border-b-0 lg:border-r">
        <div className="flex flex-wrap items-center gap-2 border-b border-border px-4 py-3">
          <StatusBadge value={result.overall} />
          <span className="font-mono text-xs text-muted-foreground">{result.serial_number}</span>
          {result.host ? <span className="font-mono text-xs text-muted-foreground">{result.host}</span> : null}
          {result.model ? <span className="text-xs text-muted-foreground">{result.model}</span> : null}
        </div>
        <div className="overflow-x-auto">
          <table className="min-w-full border-collapse text-left text-sm">
            <thead className="bg-muted/60 text-xs font-semibold text-muted-foreground">
              <tr>
                <th className="px-4 py-2">{t('colCheck')}</th>
                <th className="px-4 py-2">{t('colStatus')}</th>
                <th className="px-4 py-2">{t('colMessage')}</th>
                <th className="px-4 py-2">{t('colDetails')}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {result.checks.map((check) => (
                <tr key={check.id}>
                  <td className="px-4 py-3 font-mono text-xs text-muted-foreground">{check.id}</td>
                  <td className="px-4 py-3">
                    <StatusBadge value={check.status} />
                  </td>
                  <td className="px-4 py-3 text-foreground">{check.message}</td>
                  <td className="px-4 py-3 text-xs text-muted-foreground">{check.details ?? '-'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      <div className="px-4 py-3">
        <h3 className="text-sm font-semibold text-foreground">{t('compatibility')}</h3>
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
    <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-border py-1.5 last:border-b-0">
      <dt className="flex min-w-0 items-center gap-1 text-muted-foreground">
        <span className="truncate">{label}</span>
        {help ? <HelpTip label={label}>{help}</HelpTip> : null}
      </dt>
      <dd className="flex items-center gap-2 text-right text-xs font-medium text-muted-foreground">
        <Tag value={available ? t('available') : t('unavailable')} tone={available ? 'success' : 'neutral'} />
        <span className="font-mono text-muted-foreground">{value}</span>
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
