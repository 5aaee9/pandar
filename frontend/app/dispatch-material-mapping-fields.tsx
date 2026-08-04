'use client'

import { useId } from 'react'
import { useTranslations } from 'next-intl'

import {
  type PrinterAmsSlot,
  type ProjectFilament,
  materialMappingPayload,
} from './dispatch-material-mapping'
import {
  DispatchMaterialSlotMenu,
  MaterialColorSwatch,
} from './dispatch-material-slot-menu'

export function DispatchMaterialMappingFields({
  editorKey,
  filaments,
  model,
  onSelectSlot,
  payload,
  plateId,
  selections,
  slots,
  useAms,
}: {
  editorKey: string
  filaments: ProjectFilament[]
  model: string | null
  onSelectSlot: (mappingIndex: number, slotKey: string) => void
  payload: ReturnType<typeof materialMappingPayload>
  plateId: number
  selections: ReadonlyMap<number, string>
  slots: PrinterAmsSlot[]
  useAms: boolean
}) {
  const t = useTranslations('dispatch')
  const errorId = useId()

  if (filaments.length === 0) return null

  return (
    <div className="lg:col-span-2" data-motion="dispatch-unlocked">
      <fieldset
        className="grid gap-3 rounded-md border border-border bg-muted/50 px-3 py-3"
        key={editorKey}
      >
        <legend className="px-1 text-xs font-medium text-muted-foreground">
          {t('requiredMaterials')}
        </legend>
        <p className="text-xs text-muted-foreground">
          {t('mappingForPlate', { plate: plateId })}
        </p>
        <input name="ams_mapping" type="hidden" value={JSON.stringify(payload.amsMapping)} />
        <input name="ams_mapping2" type="hidden" value={JSON.stringify(payload.amsMapping2)} />
        {payload.amsMappingInfo ? (
          <input
            name="ams_mapping_info"
            type="hidden"
            value={JSON.stringify(payload.amsMappingInfo)}
          />
        ) : null}
        <input
          name="external_material_mismatch"
          type="hidden"
          value={String(payload.externalTypeMismatch)}
        />
        <input
          name="material_mapping_valid"
          type="hidden"
          value={String(payload.mappingValid)}
        />
        <input
          name="material_mapping_uses_ams"
          type="hidden"
          value={String(payload.usesAms)}
        />
        <div className="grid gap-4 md:grid-cols-2">
          {filaments.map((filament) => {
            const name = materialName(filament, t)
            return (
              <div
                className="grid content-start gap-2"
                key={filament.mappingIndex}
              >
                <div className="text-xs font-semibold text-muted-foreground">
                  {nozzleName(t, model, filament.nozzleId)}
                </div>
                <div className="flex min-w-0 items-center gap-2">
                  <MaterialColorSwatch color={filament.color} />
                  <span className="min-w-0">
                    <span className="block truncate text-sm font-medium text-foreground">
                      {name}
                    </span>
                    <span className="block truncate text-xs text-muted-foreground">
                      {t('projectMaterialSlot', {
                        slot: filament.mappingIndex + 1,
                      })}
                    </span>
                  </span>
                </div>
                <DispatchMaterialSlotMenu
                  errorId={errorId}
                  filament={filament}
                  invalid={!payload.mappingValid && !selections.has(filament.mappingIndex)}
                  materialName={name}
                  model={model}
                  onSelect={(slotKey) => onSelectSlot(filament.mappingIndex, slotKey)}
                  selectedKey={selections.get(filament.mappingIndex) ?? ''}
                  slots={slots}
                  useAms={useAms}
                />
              </div>
            )
          })}
        </div>
        {!payload.mappingValid ? (
          <p className="text-xs text-destructive" id={errorId} role="alert">
            {t('requiredMaterialMappingIncomplete')}
          </p>
        ) : null}
        {payload.externalTypeMismatch ? (
          <p className="text-xs text-warning">
            {t('externalMaterialMismatchInline')}
          </p>
        ) : null}
        {slots.length === 0 ? (
          <p className="text-xs text-warning">{t('noAmsSlots')}</p>
        ) : null}
      </fieldset>
    </div>
  )
}

function materialName(
  filament: ProjectFilament,
  t: ReturnType<typeof useTranslations>,
) {
  const type = filament.filamentType || t('unknownMaterial')
  return filament.filamentId ? `${type} (${filament.filamentId})` : type
}

function nozzleName(
  t: ReturnType<typeof useTranslations>,
  model: string | null,
  nozzleId: 0 | 1 | null,
) {
  const normalized = model?.trim().toUpperCase().replace(/^BAMBU LAB /, '')
  if (normalized === 'N6' || normalized === 'X2D') {
    if (nozzleId === 1) return t('mainNozzle')
    if (nozzleId === 0) return t('auxiliaryNozzle')
  }
  if (nozzleId === 1) return t('leftNozzle')
  if (nozzleId === 0) return t('rightNozzle')
  return t('selectedNozzle')
}
