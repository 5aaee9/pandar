'use client'

import { useId, useState, type FormEvent } from 'react'
import { useTranslations } from 'next-intl'
import { useMutation, useQueryClient } from '@tanstack/react-query'

import { Button } from '@/components/ui/button'
import { inputClasses } from '@/lib/utils'
import type { Job, Printer } from './dashboard-types'
import { apiIdSegment } from './api-path'
import { routeDataKeys } from './route-data'
import { ConfirmDialog } from './confirm-dialog'
import { DispatchArtifactField } from './dispatch-artifact-field'
import { DispatchMaterialMappingFields } from './dispatch-material-mapping-fields'
import { useDispatchMaterialMapping } from './use-dispatch-material-mapping'
import {
  dispatchErrorCode,
  prepareDispatchSubmission,
  reprintRequestBody,
} from './dispatch-form-submission'
import { DispatchPrintOptions } from './dispatch-print-options'
import { DispatchEmptyState } from './dispatch-form-empty-state'
import { HelpTip } from './dashboard-ui'
import { useDispatchArtifact } from './use-dispatch-artifact'

type DispatchTenant = { id: string }

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
  const queryClient = useQueryClient()
  const [preferredPrinterId, setPreferredPrinterId] = useState(sourceJob?.printer_id ?? '')
  const {
    artifact,
    metadataPreview,
    plateId,
    selectArtifact,
    setPlateId,
  } = useDispatchArtifact(selectedTenant, sourceJob)
  const [submitFailed, setSubmitFailed] = useState(false)
  const [mismatchFormData, setMismatchFormData] = useState<FormData | null>(null)
  const [useAms, setUseAms] = useState(true)
  const fileStatusId = useId()

  const dispatchMutation = useMutation({
    mutationFn: async (formData: FormData) => {
      const response = sourceJob
        ? await fetch(reprintPath(selectedTenant!.id, sourceJob.id), {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify(reprintRequestBody(formData)),
          })
        : await fetch(uploadPath(selectedTenant!.id, String(formData.get('printer_id'))), {
            method: 'POST',
            body: formData,
          })
      if (!response.ok) {
        const code = await dispatchErrorCode(response)
        throw { status: response.status, code }
      }
      return response
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: routeDataKeys.jobs(selectedTenant!.id) })
      onRedirect(
        `/jobs?status=${encodeURIComponent(
          sourceJob ? 'reprint_queued' : 'job_created',
        )}`,
      )
    },
    onError: () => {
      setSubmitFailed(true)
    },
  })

  const submitting = dispatchMutation.isPending

  const selectedPrinterId = printers.some((printer) => printer.id === preferredPrinterId)
    ? preferredPrinterId
    : (printers[0]?.id ?? '')
  const selectedPrinter = printers.find((printer) => printer.id === selectedPrinterId) ?? null
  const materialMapping = useDispatchMaterialMapping(
    metadataPreview.state === 'ready' ? metadataPreview.metadata : null,
    plateId,
    selectedPrinter,
    useAms,
  )


  const sendSubmission = (formData: FormData) => {
    if (!selectedTenant) return
    setSubmitFailed(false)
    dispatchMutation.mutate(formData)
  }

  const submitPrintJob = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (
      !selectedTenant ||
      artifact.state !== 'ready' ||
      plateId === null ||
      !selectedPrinterId ||
      !materialMapping.valid
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
      sendSubmission(submission.formData)
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
              onChange={(event) => {
                const value = event.currentTarget.value
                if (!value) return
                const parsed = Number(value)
                if (Number.isFinite(parsed)) setPlateId(parsed)
              }}
              type="number"
              required
              value={plateId}
            />
          )}
        </div>
      ) : null}
      {materialMapping.fields ? (
        <DispatchMaterialMappingFields {...materialMapping.fields} />
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
            !materialMapping.valid
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

function reprintPath(tenantId: string, jobId: string) {
  return `/api/tenants/${apiIdSegment(tenantId, 'tenant_id')}/jobs/${apiIdSegment(jobId, 'job_id')}/reprint`
}
