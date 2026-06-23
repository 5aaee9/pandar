import { duplicateJob, refreshPrinters, reprintJob, retryDispatchJob } from './actions'
import type { Agent, Job, Printer, Tenant } from './dashboard-types'
import { EmptyState, SectionHeader } from './dashboard-ui'
import { formatJobRecoveryState } from './dashboard-runtime-helpers'

export function RecoveryActions({
  selectedTenant,
  agents,
  printers,
  jobs,
}: {
  selectedTenant: Tenant | null
  agents: Agent[]
  printers: Printer[]
  jobs: Job[]
}) {
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <SectionHeader
        title="Recovery actions"
        subtitle="Manual refresh, dispatch retry, reprint, and duplicate-and-print"
        meta={`${jobs.length} jobs`}
      />
      {!selectedTenant ? (
        <EmptyState title="No tenant selected" message="Select a tenant to run recovery actions." />
      ) : (
        <div>
          <div className="flex flex-wrap gap-2 border-b border-slate-200 px-4 py-3">
            {agents.length === 0 ? (
              <div className="text-sm text-slate-600">No agents available for manual refresh</div>
            ) : agents.map((agent) => (
              <form key={agent.id} action={refreshPrinters}>
                <input name="tenant_id" type="hidden" value={selectedTenant.id} />
                <input name="agent_id" type="hidden" value={agent.id} />
                <button className="h-9 rounded-md border border-slate-300 px-3 text-sm font-medium text-slate-800" type="submit">
                  Refresh {agent.name}
                </button>
              </form>
            ))}
          </div>
          {jobs.length === 0 ? (
            <EmptyState title="No jobs" message="Jobs will appear here when dispatch history exists." />
          ) : (
            <div className="divide-y divide-slate-200">
              {jobs.slice(0, 8).map((job) => (
                <div key={job.id} className="grid gap-3 px-4 py-3 text-sm lg:grid-cols-[minmax(0,1fr)_minmax(320px,auto)]">
                  <div className="min-w-0">
                    <div className="truncate font-medium text-slate-950">{job.artifact.filename}</div>
                    <div className="mt-1 text-xs text-slate-600">{formatJobRecoveryState(job)}</div>
                    <div className="mt-1 text-xs text-slate-600">Pause, resume, and stop are unavailable until live printer control is implemented.</div>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <ReasonForm action={retryDispatchJob} tenantId={selectedTenant.id} jobId={job.id} label="Retry dispatch" />
                    <ReasonForm action={reprintJob} tenantId={selectedTenant.id} jobId={job.id} label="Reprint" />
                    <DuplicateForm tenantId={selectedTenant.id} jobId={job.id} printers={printers} />
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  )
}

function ReasonForm({ action, tenantId, jobId, label }: { action: (formData: FormData) => void; tenantId: string; jobId: string; label: string }) {
  return (
    <form action={action} className="flex gap-2">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <input name="job_id" type="hidden" value={jobId} />
      <input className="h-8 w-28 rounded border border-slate-300 px-2 text-xs" name="reason" placeholder="reason" />
      <button className="h-8 rounded border border-slate-300 px-2 text-xs font-medium" type="submit">{label}</button>
    </form>
  )
}

function DuplicateForm({ tenantId, jobId, printers }: { tenantId: string; jobId: string; printers: Printer[] }) {
  return (
    <form action={duplicateJob} className="flex flex-wrap gap-2">
      <input name="tenant_id" type="hidden" value={tenantId} />
      <input name="job_id" type="hidden" value={jobId} />
      <select name="printer_id" className="h-8 rounded border border-slate-300 bg-white px-2 text-xs">
        <option value="">Same printer</option>
        {printers.map((printer) => <option key={printer.id} value={printer.id}>{printer.name}</option>)}
      </select>
      <input className="h-8 w-16 rounded border border-slate-300 px-2 text-xs" min="1" name="plate_id" placeholder="plate" type="number" />
      <button className="h-8 rounded border border-slate-300 px-2 text-xs font-medium" type="submit">Duplicate</button>
    </form>
  )
}
