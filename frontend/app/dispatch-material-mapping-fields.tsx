'use client'

import { useMemo, useState } from 'react'
import { useTranslations } from 'next-intl'

import type { ArtifactMetadata, Printer } from './dashboard-types'
import {
  autoMapSlotSelections,
  materialMappingPayload,
  printerAmsSlots,
  projectFilamentsForPlate,
  type PrinterAmsSlot,
  type ProjectFilament,
} from './dispatch-material-mapping'

export function DispatchMaterialMappingFields({
  metadata,
  plateId,
  printer,
}: {
  metadata: ArtifactMetadata
  plateId: number
  printer: Pick<Printer, 'id' | 'materials'>
}) {
  const filaments = useMemo(
    () => projectFilamentsForPlate(metadata, plateId),
    [metadata, plateId],
  )
  const slots = useMemo(() => printerAmsSlots(printer), [printer])
  if (filaments.length === 0) return null

  const editorKey = [
    printer.id,
    plateId,
    ...filaments.map((filament) =>
      `${filament.mappingIndex}:${filament.filamentType}:${filament.color}`),
    ...slots.map((slot) => `${slot.key}:${slot.filamentType}:${slot.color}`),
  ].join('|')

  return <MappingEditor key={editorKey} filaments={filaments} plateId={plateId} slots={slots} />
}

function MappingEditor({
  filaments,
  plateId,
  slots,
}: {
  filaments: ProjectFilament[]
  plateId: number
  slots: PrinterAmsSlot[]
}) {
  const t = useTranslations('dispatch')
  const [selections, setSelections] = useState(() => autoMapSlotSelections(filaments, slots))
  const payload = materialMappingPayload(filaments, slots, selections)

  const selectSlot = (mappingIndex: number, slotKey: string) => {
    setSelections((current) => {
      const next = new Map(current)
      if (slotKey) next.set(mappingIndex, slotKey)
      else next.delete(mappingIndex)
      return next
    })
  }

  return (
    <fieldset className="grid gap-3 rounded-md border border-slate-200 bg-slate-50 px-3 py-3 lg:col-span-2">
      <legend className="px-1 text-xs font-medium text-slate-700">{t('requiredMaterials')}</legend>
      <p className="text-xs text-slate-600">{t('mappingForPlate', { plate: plateId })}</p>
      <input name="ams_mapping" type="hidden" value={JSON.stringify(payload.amsMapping)} />
      <input name="ams_mapping2" type="hidden" value={JSON.stringify(payload.amsMapping2)} />
      <div className="grid gap-2 md:grid-cols-2">
        {filaments.map((filament) => {
          const name = materialName(filament)
          return (
            <label
              className="grid gap-2 rounded-md border border-slate-200 bg-white p-2 text-sm sm:grid-cols-[minmax(0,1fr)_minmax(10rem,1.2fr)] sm:items-center"
              key={filament.mappingIndex}
            >
              <span className="flex min-w-0 items-center gap-2">
                <ColorSwatch color={filament.color} />
                <span className="min-w-0">
                  <span className="block truncate font-medium text-slate-950">{name}</span>
                  <span className="block truncate text-xs text-slate-500">
                    {t('projectMaterialSlot', { slot: filament.mappingIndex + 1 })}
                  </span>
                </span>
              </span>
              <select
                aria-label={t('mapMaterial', { material: name })}
                className="h-9 min-w-0 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950"
                onChange={(event) => selectSlot(filament.mappingIndex, event.currentTarget.value)}
                value={selections.get(filament.mappingIndex) ?? ''}
              >
                <option value="">{t('unmapped')}</option>
                {slots.map((slot) => (
                  <option key={slot.key} value={slot.key}>
                    {slotLabel(slot)}
                  </option>
                ))}
              </select>
            </label>
          )
        })}
      </div>
      {slots.length === 0 ? <p className="text-xs text-amber-700">{t('noAmsSlots')}</p> : null}
    </fieldset>
  )
}

function materialName(filament: ProjectFilament) {
  const type = filament.filamentType || 'Unknown'
  return filament.filamentId ? `${type} (${filament.filamentId})` : type
}

function slotLabel(slot: PrinterAmsSlot) {
  const unitId = Number.parseInt(slot.unitId, 10)
  const unit = unitId >= 128 && unitId <= 135
    ? `AMS-HT ${unitId - 127}`
    : unitId >= 0 && unitId < 26
      ? `AMS-${String.fromCharCode(65 + unitId)}`
      : `AMS-${slot.unitId}`
  return `${unit} ${slot.slotId + 1} · ${slot.filamentType || slot.filamentId || 'Unknown'}`
}

function ColorSwatch({ color }: { color: string | null }) {
  const normalized = color?.trim().replace(/^#/, '')
  const backgroundColor = normalized && /^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(normalized)
    ? `#${normalized.slice(0, 6)}`
    : 'transparent'
  return (
    <span
      aria-hidden="true"
      className="size-5 shrink-0 rounded-full border border-slate-300"
      style={{ backgroundColor }}
    />
  )
}
