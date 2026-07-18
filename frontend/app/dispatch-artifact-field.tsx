'use client'

import { useFormatter, useTranslations } from 'next-intl'

import { formatBytes } from './dashboard-format'
import type { ArtifactMetadata } from './dashboard-types'

export const maxArtifactBytes = 268435456

export type DispatchArtifactState = {
  file: File | null
  size: number
  state: 'idle' | 'ready' | 'too_large'
}

export type MetadataPreviewState = {
  state: 'idle' | 'loading' | 'ready' | 'unavailable' | 'error'
  metadata: ArtifactMetadata | null
}

export function DispatchArtifactField({
  artifact,
  fileStatusId,
  metadataPreview,
  onSelectArtifact,
  plateId,
  selectedFilename,
  sourceArtifact,
}: {
  artifact: DispatchArtifactState
  fileStatusId: string
  metadataPreview: MetadataPreviewState
  onSelectArtifact: (file: File | null) => void
  plateId: number | null
  selectedFilename: string
  sourceArtifact: boolean
}) {
  const t = useTranslations('dispatch')
  const format = useFormatter()
  const num = (n: number) => format.number(n)

  return (
    <div className="flex flex-col gap-1 text-sm lg:col-span-2">
      <span className="text-xs font-medium text-muted-foreground">{t('artifact')}</span>
      {!sourceArtifact ? (
        <>
          <input
            accept=".3mf,.gcode,.gcode.3mf,application/octet-stream,model/3mf"
            aria-describedby={fileStatusId}
            aria-invalid={artifact.state === 'too_large'}
            className="rounded-md border border-input px-2 py-2 text-sm text-foreground file:mr-3 file:rounded file:border-0 file:bg-muted file:px-3 file:py-1.5 file:text-sm file:font-medium"
            name="file"
            onChange={(event) => onSelectArtifact(event.currentTarget.files?.[0] ?? null)}
            type="file"
            required
          />
          <span className="text-xs text-muted-foreground">
            {t('maxSize', { size: formatBytes(maxArtifactBytes, num) })}
          </span>
        </>
      ) : null}
      <div className="rounded-md border border-border bg-muted/50 px-3 py-2 text-sm text-muted-foreground">
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
    </div>
  )
}

function MetadataPreview({
  plateId,
  preview,
}: {
  plateId: number | null
  preview: MetadataPreviewState
}) {
  const t = useTranslations('dispatch')
  if (preview.state === 'idle') return null
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
        <span>{t('project')} </span>
        <span className="font-medium text-foreground">{metadata.display_name}</span>
      </div>
      <div>
        <span>{t('plateLabel')} </span>
        <span className="font-medium text-foreground">{primaryPlate?.plate_id ?? '-'}</span>
      </div>
      <div className="truncate">
        <span>{t('objects')} </span>
        <span className="font-medium text-foreground">
          {primaryPlate?.objects.length ? primaryPlate.objects.join(', ') : '-'}
        </span>
      </div>
    </div>
  )
}
