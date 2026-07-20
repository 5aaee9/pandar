'use client'

import { useId, useRef, useState, type FormEvent } from 'react'
import { useTranslations } from 'next-intl'

import { Button } from '@/components/ui/button'
import { inputClasses } from '@/lib/utils'
import type { ArtifactMetadata, Job, Printer } from './dashboard-types'
import { apiIdSegment } from './api-path'
import { ConfirmDialog } from './confirm-dialog'
import {
  DispatchArtifactField,
  maxArtifactBytes,
  type DispatchArtifactState,
  type MetadataPreviewState,
} from './dispatch-artifact-field'
import { DispatchMaterialMappingFields } from './dispatch-material-mapping-fields'
import {
  dispatchErrorCode,
  prepareDispatchSubmission,
  reprintRequestBody,
} from './dispatch-form-submission'
import { DispatchPrintOptions } from './dispatch-print-options'
import { HelpTip } from './dashboard-ui'

type DispatchTenant = {
  id: string
}

type DispatchPrinter = Pick<Printer, 'id' | 'name' | 'serial_number' | 'model' | 'materials'>

export function DispatchForm({
  selectedTenant,
  printers,
  sourceJob,
  onRedirect = (url) => window.location.assign(url),
}: {
  selectedTenant: DispatchTenant | null
  printers: DispatchPrinter[]
  sourceJob?: Job | null
  onRedirect?: (url: string) => void
}) {
  const t = useTranslations('dispatch')
  const [preferredPrinterId, setPreferredPrinterId] = useState(sourceJob?.printer_id ?? '')
  const [plateId, setPlateId] = useState<number | null>(() => sourcePlateId(sourceJob))
  const [artifact, setArtifact] = useState<DispatchArtifactState>(() => sourceArtifact(sourceJob))
  const [metadataPreview, setMetadataPreview] = useState<MetadataPreviewState>(() =>
    sourceMetadataPreview(sourceJob),
  )
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

  const sendSubmission = async (formData: FormData) => {
    if (!selectedTenant) return
    setSubmitting(true)

    try {
      const response = sourceJob
        ? await fetch(reprintPath(selectedTenant.id, sourceJob.id), {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify(reprintRequestBody(formData)),
          })
        : await fetch(uploadPath(selectedTenant.id, String(formData.get('printer_id'))), {
            method: 'POST',
            body: formData,
          })
      const status = response.ok
        ? sourceJob ? 'reprint_queued' : 'job_created'
        : await dispatchErrorCode(response)
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
      void sendSubmission(submission.formData)
      return
    }
    if (mismatch) {
      setMismatchFormData(formData)
    }
  }

  const selectedFilename = sourceJob?.artifact.filename ?? artifact.file?.name ?? ''
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
        action={sourceJob
          ? reprintPath(selectedTenant.id, sourceJob.id)
          : uploadPath(selectedTenant.id, selectedPrinterId)}
        className="grid gap-4 lg:grid-cols-2"
        encType={sourceJob ? undefined : 'multipart/form-data'}
        method="post"
        onSubmit={(event) => submitPrintJob(event)}
      >
        <label className="flex flex-col gap-1 text-sm lg:col-span-2">
          <span className="text-xs font-medium text-muted-foreground">{t('printer')}</span>
          <select
            name="printer_id"
            className={inputClasses}
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
        <DispatchArtifactField
          artifact={artifact}
          fileStatusId={fileStatusId}
          metadataPreview={metadataPreview}
          onSelectArtifact={selectArtifact}
          plateId={plateId}
          selectedFilename={selectedFilename}
          sourceArtifact={Boolean(sourceJob)}
        />
        <input name="use_ams" type="hidden" value={String(useAms)} />
      {plateId !== null && metadataPreview.state !== 'idle' && metadataPreview.state !== 'loading' ? (
        <div className="flex flex-col gap-1 text-sm lg:col-span-2" data-motion="dispatch-unlocked">
          <span className="flex items-center gap-1 text-xs font-medium text-muted-foreground">
            {t('plate')}
            <HelpTip label={t('plate')}>{t('plateHelp')}</HelpTip>
          </span>
          {parsedPlates.length > 0 ? (
            <select
              aria-label={t('plate')}
              className={inputClasses}
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
              className={inputClasses}
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
        <Button
          aria-busy={submitting}
          className="w-full sm:w-auto"
          disabled={
            artifact.state !== 'ready' ||
            plateId === null ||
            submitting ||
            !materialMappingValid
          }
          size="lg"
          type="submit"
        >
          {submitting
            ? sourceJob ? t('reprinting') : t('dispatching')
            : sourceJob ? t('reprint') : t('dispatch')}
        </Button>
      </div>
    </form>
    <ConfirmDialog
      open={mismatchFormData !== null}
      title={t('externalMaterialMismatchTitle')}
      message={t('externalMaterialMismatchWarning')}
      confirmLabel={sourceJob ? t('reprint') : t('dispatch')}
      cancelLabel={t('reviewMapping')}
      tone="default"
      onConfirm={() => {
        const pending = mismatchFormData
        setMismatchFormData(null)
        if (pending) {
          const submission = prepareDispatchSubmission(pending, () => true)
          if (submission) void sendSubmission(submission.formData)
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

function reprintPath(tenantId: string, jobId: string) {
  return `/api/tenants/${apiIdSegment(tenantId, 'tenant_id')}/jobs/${apiIdSegment(jobId, 'job_id')}/reprint`
}

function sourcePlateId(sourceJob?: Job | null) {
  if (!sourceJob) return null
  const metadata = sourceJob.artifact.metadata
  return metadata?.plates.find((plate) => plate.plate_id === metadata.default_plate_id)?.plate_id
    ?? metadata?.plates[0]?.plate_id
    ?? 1
}

function sourceArtifact(sourceJob?: Job | null): DispatchArtifactState {
  return sourceJob
    ? { file: null, size: sourceJob.artifact.size_bytes, state: 'ready' }
    : { file: null, size: 0, state: 'idle' }
}

function sourceMetadataPreview(sourceJob?: Job | null): MetadataPreviewState {
  if (!sourceJob) return { state: 'idle', metadata: null }
  return sourceJob.artifact.metadata
    ? { state: 'ready', metadata: sourceJob.artifact.metadata }
    : { state: 'unavailable', metadata: null }
}

function DispatchEmptyState({ title, message }: { title: string; message: string }) {
  return (
    <div className="px-4 py-12 text-center">
      <div className="text-sm font-semibold text-foreground">{title}</div>
      <p className="mx-auto mt-2 max-w-md text-sm text-muted-foreground">{message}</p>
    </div>
  )
}
