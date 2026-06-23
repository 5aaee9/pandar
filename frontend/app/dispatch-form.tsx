'use client'

import { useState } from 'react'

import { createPrintJob } from './actions'
import { formatBytes } from './dashboard-ui'

type DispatchTenant = {
  id: string
}

type DispatchPrinter = {
  id: string
  name: string
  serial_number: string
}

const maxArtifactBytes = 268435456
const backendErrorCodes = [
  'artifact_empty',
  'artifact_invalid_base64',
  'artifact_invalid_plate',
  'artifact_too_large',
  'printer_not_found',
]

export function DispatchForm({
  selectedTenant,
  printers,
}: {
  selectedTenant: DispatchTenant | null
  printers: DispatchPrinter[]
}) {
  const [artifact, setArtifact] = useState<{
    filename: string
    contentType: string
    size: number
    base64: string
    state: 'idle' | 'converting' | 'ready' | 'too_large' | 'failed'
  }>({
    filename: '',
    contentType: '',
    size: 0,
    base64: '',
    state: 'idle',
  })

  const selectArtifact = async (file: File | null) => {
    if (!file) {
      setArtifact({ filename: '', contentType: '', size: 0, base64: '', state: 'idle' })
      return
    }

    if (file.size > maxArtifactBytes) {
      setArtifact({
        filename: file.name,
        contentType: file.type || 'model/3mf',
        size: file.size,
        base64: '',
        state: 'too_large',
      })
      return
    }

    setArtifact({
      filename: file.name,
      contentType: file.type || 'model/3mf',
      size: file.size,
      base64: '',
      state: 'converting',
    })

    try {
      const buffer = await file.arrayBuffer()
      let binary = ''
      const bytes = new Uint8Array(buffer)
      const chunkSize = 0x8000
      for (let offset = 0; offset < bytes.length; offset += chunkSize) {
        const chunk = bytes.subarray(offset, offset + chunkSize)
        binary += String.fromCharCode(...chunk)
      }
      setArtifact({
        filename: file.name,
        contentType: file.type || 'model/3mf',
        size: file.size,
        base64: btoa(binary),
        state: 'ready',
      })
    } catch {
      setArtifact({
        filename: file.name,
        contentType: file.type || 'model/3mf',
        size: file.size,
        base64: '',
        state: 'failed',
      })
    }
  }

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
          <input name="filename" type="hidden" value={artifact.filename} />
          <input name="content_type" type="hidden" value={artifact.contentType} />
          <input name="artifact_base64" type="hidden" value={artifact.base64} />
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
            <span className="text-xs font-medium text-slate-500">Artifact</span>
            <input
              accept=".3mf,.gcode,.gcode.3mf,application/octet-stream,model/3mf"
              className="rounded-md border border-slate-300 px-2 py-2 text-sm text-slate-950 file:mr-3 file:rounded file:border-0 file:bg-slate-100 file:px-3 file:py-1.5 file:text-sm file:font-medium"
              onChange={(event) => void selectArtifact(event.currentTarget.files?.[0] ?? null)}
              type="file"
              required
            />
            <span className="text-xs text-slate-600">Maximum artifact size {formatBytes(maxArtifactBytes)}</span>
          </label>
          <div className="rounded-md border border-slate-200 bg-slate-50 px-3 py-2 text-sm text-slate-700 lg:col-span-2">
            <div className="font-medium text-slate-950">
              {artifact.filename || 'No artifact selected'}
            </div>
            <div className="mt-1 text-xs">
              {artifact.state === 'converting'
                ? 'Converting artifact for form submission'
                : artifact.state === 'ready'
                  ? `${formatBytes(artifact.size)} ready`
                  : artifact.state === 'too_large'
                    ? `${formatBytes(artifact.size)} exceeds the configured limit`
                    : artifact.state === 'failed'
                      ? 'Artifact conversion failed'
                      : 'Choose a file to convert in the browser before dispatch.'}
            </div>
            <div className="mt-2 flex flex-wrap gap-1 text-xs">
              {backendErrorCodes.map((code) => (
                <span key={code} className="rounded bg-white px-2 py-1 text-slate-600">
                  {code}
                </span>
              ))}
            </div>
          </div>
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
              className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white disabled:bg-slate-300 disabled:text-slate-600"
              disabled={artifact.state !== 'ready'}
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
