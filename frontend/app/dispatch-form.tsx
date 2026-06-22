import { createPrintJob } from './actions'

type DispatchTenant = {
  id: string
}

type DispatchPrinter = {
  id: string
  name: string
  serial_number: string
}

export function DispatchForm({
  selectedTenant,
  printers,
}: {
  selectedTenant: DispatchTenant | null
  printers: DispatchPrinter[]
}) {
  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <div className="flex flex-col gap-2 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold">Dispatch print job</h2>
          <p className="mt-0.5 text-sm text-slate-600">
            Submit a base64 project artifact to the selected tenant printer
          </p>
        </div>
      </div>

      {!selectedTenant ? (
        <DispatchEmptyState title="No tenant selected" message="Select a tenant to dispatch jobs." />
      ) : printers.length === 0 ? (
        <DispatchEmptyState
          title="No printers available"
          message="A reported printer is required before jobs can be dispatched."
        />
      ) : (
        <form action={createPrintJob} className="grid gap-4 px-4 py-4 lg:grid-cols-2">
          <input name="tenant_id" type="hidden" value={selectedTenant.id} />
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">Printer</span>
            <select
              name="printer_id"
              className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950"
              required
            >
              {printers.map((printer) => (
                <option key={printer.id} value={printer.id}>
                  {printer.name} ({printer.serial_number})
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">Filename</span>
            <input
              name="filename"
              className="h-9 rounded-md border border-slate-300 px-2 text-sm text-slate-950"
              defaultValue="plate.3mf"
              required
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">Content type</span>
            <input
              name="content_type"
              className="h-9 rounded-md border border-slate-300 px-2 text-sm text-slate-950"
              defaultValue="model/3mf"
              required
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">Plate</span>
            <input
              name="plate_id"
              className="h-9 rounded-md border border-slate-300 px-2 text-sm text-slate-950"
              defaultValue="1"
              min="1"
              type="number"
              required
            />
          </label>
          <label className="flex flex-col gap-1 text-sm lg:col-span-2">
            <span className="text-xs font-medium text-slate-500">Base64 artifact</span>
            <textarea
              name="artifact_base64"
              className="min-h-28 rounded-md border border-slate-300 px-2 py-2 font-mono text-xs text-slate-950"
              required
            />
          </label>
          <div className="flex flex-wrap gap-4 text-sm text-slate-700 lg:col-span-2">
            <label className="flex items-center gap-2">
              <input name="use_ams" type="checkbox" defaultChecked />
              Use AMS
            </label>
            <label className="flex items-center gap-2">
              <input name="flow_cali" type="checkbox" />
              Flow calibration
            </label>
            <label className="flex items-center gap-2">
              <input name="timelapse" type="checkbox" />
              Timelapse
            </label>
          </div>
          <div className="lg:col-span-2">
            <button
              className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white"
              type="submit"
            >
              Dispatch
            </button>
          </div>
        </form>
      )}
    </section>
  )
}

function DispatchEmptyState({ title, message }: { title: string; message: string }) {
  return (
    <div className="px-4 py-12 text-center">
      <div className="text-sm font-semibold text-slate-950">{title}</div>
      <p className="mx-auto mt-2 max-w-md text-sm text-slate-600">{message}</p>
    </div>
  )
}
