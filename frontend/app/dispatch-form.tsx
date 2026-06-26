'use client'

import { useEffect, useRef, useState, type FormEvent } from 'react'

import type { ArtifactMetadata } from './dashboard-types'
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
  'artifact_invalid_upload',
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
  const [selectedPrinterId, setSelectedPrinterId] = useState(printers[0]?.id ?? '')
  const [artifact, setArtifact] = useState<{
    file: File | null
    size: number
    state: 'idle' | 'ready' | 'too_large'
  }>({
    file: null,
    size: 0,
    state: 'idle',
  })
  const [metadataPreview, setMetadataPreview] = useState<{
    state: 'idle' | 'loading' | 'ready' | 'unavailable' | 'error'
    metadata: ArtifactMetadata | null
  }>({
    state: 'idle',
    metadata: null,
  })
  const [submitting, setSubmitting] = useState(false)
  const previewRequestRef = useRef(0)

  useEffect(() => {
    if (!printers.some((printer) => printer.id === selectedPrinterId)) {
      setSelectedPrinterId(printers[0]?.id ?? '')
    }
  }, [printers, selectedPrinterId])

  const selectArtifact = (file: File | null) => {
    if (!file) {
      previewRequestRef.current += 1
      setArtifact({ file: null, size: 0, state: 'idle' })
      setMetadataPreview({ state: 'idle', metadata: null })
      return
    }

    if (file.size > maxArtifactBytes) {
      previewRequestRef.current += 1
      setArtifact({ file, size: file.size, state: 'too_large' })
      setMetadataPreview({ state: 'idle', metadata: null })
      return
    }

    setArtifact({ file, size: file.size, state: 'ready' })
    void previewArtifact(file)
  }

  const previewArtifact = async (file: File) => {
    if (!selectedTenant) {
      setMetadataPreview({ state: 'idle', metadata: null })
      return
    }

    const formData = new FormData()
    formData.set('filename', file.name)
    formData.set('content_type', file.type || 'application/octet-stream')
    formData.set('file', file)
    const requestId = previewRequestRef.current + 1
    previewRequestRef.current = requestId
    setMetadataPreview({ state: 'loading', metadata: null })

    try {
      const response = await fetch(metadataPreviewPath(selectedTenant.id), {
        method: 'POST',
        body: formData,
      })
      if (requestId !== previewRequestRef.current) {
        return
      }
      if (!response.ok) {
        setMetadataPreview({ state: 'error', metadata: null })
        return
      }
      const body = (await response.json()) as { metadata?: ArtifactMetadata | null }
      if (requestId !== previewRequestRef.current) {
        return
      }
      setMetadataPreview(
        body.metadata
          ? { state: 'ready', metadata: body.metadata }
          : { state: 'unavailable', metadata: null },
      )
    } catch {
      if (requestId !== previewRequestRef.current) {
        return
      }
      setMetadataPreview({ state: 'error', metadata: null })
    }
  }

  const submitPrintJob = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!selectedTenant || artifact.state !== 'ready' || !selectedPrinterId) {
      return
    }

    const formData = new FormData(event.currentTarget)
    const printerId = String(formData.get('printer_id') ?? '')
    setSubmitting(true)

    try {
      const response = await fetch(uploadPath(selectedTenant.id, printerId), {
        method: 'POST',
        body: formData,
      })
      const status = response.ok ? 'job_created' : await errorCode(response)
      window.location.assign(
        `/?tenant=${encodeURIComponent(selectedTenant.id)}&status=${encodeURIComponent(status)}`,
      )
    } finally {
      setSubmitting(false)
    }
  }

  const selectedFilename = artifact.file?.name ?? ''

  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <div className="flex flex-col gap-2 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-base font-semibold">Dispatch print job</h2>
          <p className="mt-0.5 text-sm text-slate-600">
            Upload a project artifact to the selected tenant printer
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
        <form
          action={uploadPath(selectedTenant.id, selectedPrinterId)}
          className="grid gap-4 px-4 py-4 lg:grid-cols-2"
          encType="multipart/form-data"
          method="post"
          onSubmit={(event) => void submitPrintJob(event)}
        >
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">Printer</span>
            <select
              name="printer_id"
              className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950"
              onChange={(event) => setSelectedPrinterId(event.currentTarget.value)}
              required
              value={selectedPrinterId}
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
              name="file"
              onChange={(event) => selectArtifact(event.currentTarget.files?.[0] ?? null)}
              type="file"
              required
            />
            <span className="text-xs text-slate-600">Maximum artifact size {formatBytes(maxArtifactBytes)}</span>
          </label>
          <input name="use_ams" type="hidden" value="false" />
          <input name="flow_cali" type="hidden" value="false" />
          <input name="timelapse" type="hidden" value="false" />
          <div className="rounded-md border border-slate-200 bg-slate-50 px-3 py-2 text-sm text-slate-700 lg:col-span-2">
            <div className="font-medium text-slate-950">
              {selectedFilename || 'No artifact selected'}
            </div>
            <div className="mt-1 text-xs">
              {artifact.state === 'ready'
                ? `${formatBytes(artifact.size)} selected`
                : artifact.state === 'too_large'
                  ? `${formatBytes(artifact.size)} exceeds the configured limit`
                  : 'Choose a file before dispatch.'}
            </div>
            <MetadataPreview preview={metadataPreview} />
            <details className="mt-2 text-xs text-slate-600">
              <summary className="cursor-pointer select-none text-slate-500">Developer error codes</summary>
              <div className="mt-1 flex flex-wrap gap-1">
                {backendErrorCodes.map((code) => (
                  <code key={code} className="rounded bg-white px-1.5 py-0.5 text-slate-600">
                    {code}
                  </code>
                ))}
              </div>
            </details>
          </div>
          <div className="flex flex-wrap gap-4 text-sm text-slate-700 lg:col-span-2">
            <label className="flex items-center gap-2">
              <input name="use_ams" type="checkbox" value="true" defaultChecked />
              Use AMS
            </label>
            <label className="flex items-center gap-2">
              <input name="flow_cali" type="checkbox" value="true" />
              Flow calibration
            </label>
            <label className="flex items-center gap-2">
              <input name="timelapse" type="checkbox" value="true" />
              Timelapse
            </label>
          </div>
          <div className="lg:col-span-2">
            <button
              className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white disabled:bg-slate-300 disabled:text-slate-600"
              disabled={artifact.state !== 'ready' || submitting}
              type="submit"
            >
              {submitting ? 'Dispatching' : 'Dispatch'}
            </button>
          </div>
        </form>
      )}
    </section>
  )
}

function uploadPath(tenantId: string, printerId: string) {
  return `/api/tenants/${encodeURIComponent(tenantId)}/printers/${encodeURIComponent(printerId)}/jobs`
}

function metadataPreviewPath(tenantId: string) {
  return `/api/tenants/${encodeURIComponent(tenantId)}/artifact-metadata-preview`
}

async function errorCode(response: Response) {
  try {
    const body = (await response.json()) as { error?: string }
    return body.error ?? `http_${response.status}`
  } catch {
    return `http_${response.status}`
  }
}

function MetadataPreview({
  preview,
}: {
  preview: {
    state: 'idle' | 'loading' | 'ready' | 'unavailable' | 'error'
    metadata: ArtifactMetadata | null
  }
}) {
  if (preview.state === 'idle') {
    return null
  }
  if (preview.state === 'loading') {
    return <div className="mt-2 text-xs text-slate-600">Reading slicer metadata</div>
  }
  if (preview.state === 'unavailable') {
    return <div className="mt-2 text-xs text-slate-600">No slicer metadata found</div>
  }
  if (preview.state === 'error' || !preview.metadata) {
    return <div className="mt-2 text-xs text-slate-600">Metadata preview unavailable</div>
  }

  const metadata = preview.metadata
  const primaryPlate =
    metadata.plates.find((plate) => plate.plate_id === metadata.default_plate_id) ??
    metadata.plates[0]

  return (
    <div className="mt-2 grid gap-1 text-xs text-slate-700 sm:grid-cols-3">
      <div className="min-w-0">
        <span className="text-slate-500">Project </span>
        <span className="font-medium text-slate-900">{metadata.display_name}</span>
      </div>
      <div>
        <span className="text-slate-500">Plate </span>
        <span className="font-medium text-slate-900">
          {metadata.default_plate_id ?? '-'}
        </span>
      </div>
      <div className="truncate">
        <span className="text-slate-500">Objects </span>
        <span className="font-medium text-slate-900">
          {primaryPlate?.objects.length ? primaryPlate.objects.join(', ') : '-'}
        </span>
      </div>
    </div>
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
