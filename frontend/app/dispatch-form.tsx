'use client'

import { useId, useRef, useState, type FormEvent } from 'react'
import { useFormatter, useTranslations } from 'next-intl'

import type { ArtifactMetadata, Printer } from './dashboard-types'
import { apiIdSegment } from './api-path'
import { ConfirmDialog } from './confirm-dialog'
import { formatBytes } from './dashboard-format'
import { DispatchMaterialMappingFields } from './dispatch-material-mapping-fields'
import { dispatchErrorCode, prepareDispatchSubmission } from './dispatch-form-submission'
import { DispatchPrintOptions } from './dispatch-print-options'
import { HelpTip } from './dashboard-ui'

type DispatchTenant = {
  id: string
}

type DispatchPrinter = Pick<Printer, 'id' | 'name' | 'serial_number' | 'model' | 'materials'>

const maxArtifactBytes = 268435456

export function DispatchForm({
  selectedTenant,
  printers,
  onRedirect = (url) => window.location.assign(url),
}: {
  selectedTenant: DispatchTenant | null
  printers: DispatchPrinter[]
  onRedirect?: (url: string) => void
}) {
  const t = useTranslations('dispatch')
  const format = useFormatter()
  const num = (n: number) => format.number(n)
  const [preferredPrinterId, setPreferredPrinterId] = useState('')
  const [plateId, setPlateId] = useState<number | null>(null)
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
  const [submitFailed, setSubmitFailed] = useState(false)
  const [mismatchFormData, setMismatchFormData] = useState<FormData | null>(null)
  const [materialMappingValid, setMaterialMappingValid] = useState(true)
  const [useAms, setUseAms] = useState(true)
  const previewRequestRef = useRef(0)
  const fileStatusId = useId()

  const selectedPrinterId = printers.some((printer) => printer.id === preferredPrinterId)
    ? preferredPrinterId
    : (printers[0]?.id ?? '')
  const selectedPrinter = printers.find((printer) => printer.id === selectedPrinterId) ?? null

  const selectArtifact = (file: File | null) => {
    setMaterialMappingValid(true)
    if (!file) {
      previewRequestRef.current += 1
      setPlateId(null)
      setArtifact({ file: null, size: 0, state: 'idle' })
      setMetadataPreview({ state: 'idle', metadata: null })
      return
    }

    if (file.size > maxArtifactBytes) {
      previewRequestRef.current += 1
      setPlateId(null)
      setArtifact({ file, size: file.size, state: 'too_large' })
      setMetadataPreview({ state: 'idle', metadata: null })
      return
    }

    setPlateId(null)
    setArtifact({ file, size: file.size, state: 'ready' })
    void previewArtifact(file)
  }

  const previewArtifact = async (file: File) => {
    if (!selectedTenant) {
      setPlateId(null)
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
    const isStale = () => requestId !== previewRequestRef.current

    try {
      const response = await fetch(metadataPreviewPath(selectedTenant.id), {
        method: 'POST',
        body: formData,
      })
      if (isStale()) {
        return
      }
      if (!response.ok) {
        setPlateId(1)
        setMetadataPreview({ state: 'error', metadata: null })
        return
      }
      const body = (await response.json()) as { metadata?: ArtifactMetadata | null }
      if (isStale()) {
        return
      }
      const defaultPlate = body.metadata?.plates.find(
        (plate) => plate.plate_id === body.metadata?.default_plate_id,
      )
      setPlateId(defaultPlate?.plate_id ?? body.metadata?.plates[0]?.plate_id ?? 1)
      setMetadataPreview(
        body.metadata
          ? { state: 'ready', metadata: body.metadata }
          : { state: 'unavailable', metadata: null },
      )
    } catch {
      if (isStale()) {
        return
      }
      setPlateId(1)
      setMetadataPreview({ state: 'error', metadata: null })
    }
  }

  const uploadSubmission = async (formData: FormData) => {
    if (!selectedTenant) return
    const submission = prepareDispatchSubmission(formData, () => true)
    if (!submission) return
    setSubmitting(true)

    try {
      const response = await fetch(uploadPath(selectedTenant.id, submission.printerId), {
        method: 'POST',
        body: submission.formData,
      })
      const status = response.ok ? 'job_created' : await dispatchErrorCode(response)
      onRedirect(
        `/jobs?tenant=${encodeURIComponent(selectedTenant.id)}&status=${encodeURIComponent(status)}`,
      )
    } catch {
      setSubmitFailed(true)
    } finally {
      setSubmitting(false)
    }
  }

  const submitPrintJob = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (
      !selectedTenant ||
      artifact.state !== 'ready' ||
      plateId === null ||
      !selectedPrinterId ||
      !materialMappingValid
    ) {
      return
    }

    setSubmitFailed(false)
    const formData = new FormData(event.currentTarget)
    let mismatch = false
    const submission = prepareDispatchSubmission(formData, () => {
      mismatch = true
      return false
    })
    if (submission) {
      void uploadSubmission(formData)
      return
    }
    if (mismatch) {
      setMismatchFormData(formData)
    }
  }

  const selectedFilename = artifact.file?.name ?? ''
  const parsedPlates = metadataPreview.state === 'ready'
    ? (metadataPreview.metadata?.plates ?? [])
    : []

  if (!selectedTenant) {
    return <DispatchEmptyState title={t('noTenantTitle')} message={t('noTenantMessage')} />
  }
  if (printers.length === 0) {
    return (
      <DispatchEmptyState
        title={t('noPrintersTitle')}
        message={t('noPrintersMessage')}
      />
    )
  }

  return (
    <>
      <form
        action={uploadPath(selectedTenant.id, selectedPrinterId)}
        className="grid gap-4 lg:grid-cols-2"
        encType="multipart/form-data"
        method="post"
        onSubmit={(event) => submitPrintJob(event)}
      >
        <label className="flex flex-col gap-1 text-sm lg:col-span-2">
          <span className="text-xs font-medium text-muted-foreground">{t('printer')}</span>
          <select
            name="printer_id"
            className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
            onChange={(event) => setPreferredPrinterId(event.currentTarget.value)}
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
        <label className="flex flex-col gap-1 text-sm lg:col-span-2">
          <span className="text-xs font-medium text-muted-foreground">{t('artifact')}</span>
          <input
            accept=".3mf,.gcode,.gcode.3mf,application/octet-stream,model/3mf"
            aria-describedby={fileStatusId}
            aria-invalid={artifact.state === 'too_large'}
            className="rounded-md border border-input px-2 py-2 text-sm text-foreground file:mr-3 file:rounded file:border-0 file:bg-muted file:px-3 file:py-1.5 file:text-sm file:font-medium"
            name="file"
            onChange={(event) => selectArtifact(event.currentTarget.files?.[0] ?? null)}
            type="file"
            required
          />
          <span className="text-xs text-muted-foreground">{t('maxSize', { size: formatBytes(maxArtifactBytes, num) })}</span>
        </label>
        <input name="use_ams" type="hidden" value={String(useAms)} />
        <div className="rounded-md border border-border bg-muted/50 px-3 py-2 text-sm text-muted-foreground lg:col-span-2">
          <div className="font-medium text-foreground">
            {selectedFilename || t('noArtifact')}
          </div>
          <div
            className="mt-1 text-xs"
            id={fileStatusId}
            role={artifact.state === 'too_large' ? 'alert' : undefined}
          >
            {artifact.state === 'ready'
              ? t('readySize', { size: formatBytes(artifact.size, num) })
              : artifact.state === 'too_large'
                ? t('tooLargeSize', { size: formatBytes(artifact.size, num) })
                : t('chooseFile')}
          </div>
          <MetadataPreview plateId={plateId} preview={metadataPreview} />
        </div>
      {plateId !== null && metadataPreview.state !== 'idle' && metadataPreview.state !== 'loading' ? (
        <div className="flex flex-col gap-1 text-sm lg:col-span-2" data-motion="dispatch-unlocked">
          <span className="flex items-center gap-1 text-xs font-medium text-muted-foreground">
            {t('plate')}
            <HelpTip label={t('plate')}>{t('plateHelp')}</HelpTip>
          </span>
          {parsedPlates.length > 0 ? (
            <select
              aria-label={t('plate')}
              className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
              name="plate_id"
              onChange={(event) => setPlateId(Number(event.currentTarget.value))}
              required
              value={plateId}
            >
              {parsedPlates.map((plate) => (
                <option key={plate.plate_id} value={plate.plate_id}>
                  {t('plateOption', { id: plate.plate_id, name: plate.name })}
                </option>
              ))}
            </select>
          ) : (
            <input
              aria-label={t('plate')}
              className="h-9 rounded-md border border-input px-2 text-sm text-foreground"
              min="1"
              name="plate_id"
              onChange={(event) => setPlateId(Number(event.currentTarget.value))}
              type="number"
              required
              value={plateId}
            />
          )}
        </div>
      ) : null}
      {metadataPreview.state === 'ready' && metadataPreview.metadata && plateId !== null && selectedPrinter ? (
        <DispatchMaterialMappingFields
          metadata={metadataPreview.metadata}
          plateId={plateId}
          printer={selectedPrinter}
          onValidityChange={setMaterialMappingValid}
          useAms={useAms}
        />
      ) : null}
      <div className="flex flex-wrap gap-4 text-sm text-muted-foreground lg:col-span-2">
        <span className="flex items-center gap-1.5">
          <label className="flex items-center gap-2">
            <input
              checked={useAms}
              onChange={(event) => setUseAms(event.currentTarget.checked)}
              type="checkbox"
            />
            {t('useAms')}
          </label>
          <HelpTip label={t('useAms')}>{t('useAmsHelp')}</HelpTip>
        </span>
      </div>
      <DispatchPrintOptions
        key={selectedPrinter ? selectedPrinter.id + ':' + (selectedPrinter.model ?? 'unknown') : 'unknown'}
        model={selectedPrinter?.model ?? null}
      />
      {submitFailed ? (
        <div
          className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive dark:bg-destructive/20 lg:col-span-2"
          role="alert"
        >
          {t('submitFailed')}
        </div>
      ) : null}
      <div className="lg:col-span-2">
        <button
          className="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground transition-colors duration-150 ease-out hover:bg-primary/80 disabled:bg-muted disabled:text-muted-foreground"
          disabled={
            artifact.state !== 'ready' ||
            plateId === null ||
            submitting ||
            !materialMappingValid
          }
          type="submit"
        >
          {submitting ? t('dispatching') : t('dispatch')}
        </button>
      </div>
    </form>
    <ConfirmDialog
      open={mismatchFormData !== null}
      title={t('externalMaterialMismatchTitle')}
      message={t('externalMaterialMismatchWarning')}
      confirmLabel={t('dispatch')}
      cancelLabel={t('reviewMapping')}
      tone="default"
      onConfirm={() => {
        const pending = mismatchFormData
        setMismatchFormData(null)
        if (pending) {
          void uploadSubmission(pending)
        }
      }}
      onCancel={() => setMismatchFormData(null)}
    />
  </>
  )
}

function uploadPath(tenantId: string, printerId: string) {
  return `/api/tenants/${apiIdSegment(tenantId, 'tenant_id')}/printers/${apiIdSegment(printerId, 'printer_id')}/jobs`
}

function metadataPreviewPath(tenantId: string) {
  return `/api/tenants/${apiIdSegment(tenantId, 'tenant_id')}/artifact-metadata-preview`
}


function MetadataPreview({
  plateId,
  preview,
}: {
  plateId: number | null
  preview: {
    state: 'idle' | 'loading' | 'ready' | 'unavailable' | 'error'
    metadata: ArtifactMetadata | null
  }
}) {
  const t = useTranslations('dispatch')
  if (preview.state === 'idle') {
    return null
  }
  if (preview.state === 'loading') {
    return <div className="mt-2 text-xs text-muted-foreground" role="status">{t('readingMetadata')}</div>
  }
  if (preview.state === 'unavailable') {
    return <div className="mt-2 text-xs text-muted-foreground" role="status">{t('metadataUnavailableFound')}</div>
  }
  if (preview.state === 'error' || !preview.metadata) {
    return <div className="mt-2 text-xs text-muted-foreground" role="status">{t('metadataUnavailable')}</div>
  }

  const metadata = preview.metadata
  const primaryPlate =
    metadata.plates.find((plate) => plate.plate_id === plateId) ??
    metadata.plates.find((plate) => plate.plate_id === metadata.default_plate_id) ??
    metadata.plates[0]

  return (
    <div className="mt-2 grid gap-1 text-xs text-muted-foreground sm:grid-cols-3">
      <div className="min-w-0">
        <span className="text-muted-foreground">{t('project')} </span>
        <span className="font-medium text-foreground">{metadata.display_name}</span>
      </div>
      <div>
        <span className="text-muted-foreground">{t('plateLabel')} </span>
        <span className="font-medium text-foreground">
          {primaryPlate?.plate_id ?? '-'}
        </span>
      </div>
      <div className="truncate">
        <span className="text-muted-foreground">{t('objects')} </span>
        <span className="font-medium text-foreground">
          {primaryPlate?.objects.length ? primaryPlate.objects.join(', ') : '-'}
        </span>
      </div>
    </div>
  )
}

function DispatchEmptyState({ title, message }: { title: string; message: string }) {
  return (
    <div className="px-4 py-12 text-center">
      <div className="text-sm font-semibold text-foreground">{title}</div>
      <p className="mx-auto mt-2 max-w-md text-sm text-muted-foreground">{message}</p>
    </div>
  )
}
