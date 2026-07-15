'use client'

import { useState } from 'react'
import { useTranslations } from 'next-intl'

import { HelpTip } from './dashboard-ui'
import {
  type CalibrationMode,
  dispatchPrintOptionCapabilities,
} from './dispatch-print-options-model'

type Option = {
  label: string
  value: '0' | '1' | '2' | 'true' | 'false'
}

export function DispatchPrintOptions({ model }: { model: string | null }) {
  const t = useTranslations('dispatch')
  const capabilities = dispatchPrintOptionCapabilities(model)
  const modelKey = model ?? 'unknown'
  const onOff: Option[] = [
    { label: t('modeOn'), value: 'true' },
    { label: t('modeOff'), value: 'false' },
  ]
  const calibrationOptions = (modes: readonly CalibrationMode[]): Option[] =>
    modes.map((mode) => ({
      label: mode === 2 ? t('modeAuto') : mode === 1 ? t('modeOn') : t('modeOff'),
      value: String(mode) as Option['value'],
    }))

  const hasVisibleOption =
    capabilities.timelapse ||
    capabilities.bedLeveling ||
    capabilities.flowCalibration ||
    capabilities.nozzleOffsetCalibration

  if (!hasVisibleOption) {
    return (
      <>
        <input name="timelapse" type="hidden" value="false" />
        <HiddenCalibration modeName="auto_bed_leveling" pairedBoolName="bed_leveling" />
        <HiddenCalibration modeName="auto_flow_cali" pairedBoolName="flow_cali" />
        <HiddenCalibration modeName="auto_offset_cali" />
      </>
    )
  }
  return (
    <section className="grid gap-3 rounded-md border border-slate-200 bg-slate-50 p-3 text-sm lg:col-span-2 sm:grid-cols-2">
      {capabilities.timelapse ? (
        <PrintOption
          defaultValue="true"
          help={t('timelapseHelp')}
          label={t('timelapse')}
          name="timelapse"
          options={onOff}
        />
      ) : (
        <input name="timelapse" type="hidden" value="false" />
      )}
      {capabilities.bedLeveling ? (
        <PrintOption
          key={modelKey + ':bed'}
          defaultValue={String(capabilities.bedLeveling.defaultMode) as Option['value']}
          help={t('autoBedLevelingHelp')}
          label={t('autoBedLeveling')}
          pairedBoolName="bed_leveling"
          name="auto_bed_leveling"
          options={calibrationOptions(capabilities.bedLeveling.modes)}
        />
      ) : (
        <HiddenCalibration modeName="auto_bed_leveling" pairedBoolName="bed_leveling" />
      )}
      {capabilities.flowCalibration ? (
        <PrintOption
          key={modelKey + ':flow'}
          defaultValue={String(capabilities.flowCalibration.defaultMode) as Option['value']}
          help={t('flowCaliHelp')}
          label={t('flowCali')}
          pairedBoolName="flow_cali"
          name="auto_flow_cali"
          options={calibrationOptions(capabilities.flowCalibration.modes)}
        />
      ) : (
        <HiddenCalibration modeName="auto_flow_cali" pairedBoolName="flow_cali" />
      )}
      {capabilities.nozzleOffsetCalibration ? (
        <PrintOption
          key={modelKey + ':nozzle'}
          defaultValue={String(capabilities.nozzleOffsetCalibration.defaultMode) as Option['value']}
          help={t('nozzleOffsetCaliHelp')}
          label={t('nozzleOffsetCali')}
          name="auto_offset_cali"
          options={calibrationOptions(capabilities.nozzleOffsetCalibration.modes)}
        />
      ) : (
        <HiddenCalibration modeName="auto_offset_cali" />
      )}
    </section>
  )
}
function HiddenCalibration({
  modeName,
  pairedBoolName,
}: {
  modeName: 'auto_bed_leveling' | 'auto_flow_cali' | 'auto_offset_cali'
  pairedBoolName?: 'bed_leveling' | 'flow_cali'
}) {
  return (
    <>
      <input name={modeName} type="hidden" value="0" />
      {pairedBoolName ? <input name={pairedBoolName} type="hidden" value="false" /> : null}
    </>
  )
}

function PrintOption({
  defaultValue,
  help,
  label,
  pairedBoolName,
  name,
  options,
}: {
  defaultValue: Option['value']
  help: string
  label: string
  pairedBoolName?: 'bed_leveling' | 'flow_cali'
  name: string
  options: Option[]
}) {
  const [value, setValue] = useState<Option['value']>(defaultValue)

  return (
    <fieldset className="flex min-w-0 items-center justify-between gap-3">
      <legend className="sr-only">{label}</legend>
      <span className="flex min-w-0 items-center gap-1.5 text-slate-700">
        <span>{label}</span>
        <HelpTip label={label}>{help}</HelpTip>
      </span>
      <div className="flex shrink-0 overflow-hidden rounded border border-slate-300 bg-white">
        {options.map((option) => (
          <label key={option.value} className="cursor-pointer">
            <input
              className="peer sr-only"
              checked={option.value === value}
              name={name}
              onChange={() => setValue(option.value)}
              type="radio"
              value={option.value}
            />
            <span className="block px-2.5 py-1 text-xs text-slate-600 transition-colors peer-checked:bg-emerald-50 peer-checked:text-emerald-700 peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-offset-[-2px] peer-focus-visible:outline-primary">
              {option.label}
            </span>
          </label>
        ))}
      </div>
      {pairedBoolName ? <input name={pairedBoolName} type="hidden" value={value === '1' ? 'true' : 'false'} /> : null}
    </fieldset>
  )
}
