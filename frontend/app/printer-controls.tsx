'use client'

import { controlPrinter } from './actions'
import type { Printer } from './dashboard-types'
import { useActionStatusFeedback } from './mutation-feedback'
import { mutationResources } from './mutation-invalidation'

export function usePrinterControl() {
  return useActionStatusFeedback(controlPrinter, 'printer_control_queued', {
    invalidate: mutationResources.printer,
  })
}

/** Field names for user-editable inputs; hidden contract fields stay inside PrinterControlFields. */
export const printerControlFieldNames = {
  temperatureCelsius: 'temperature_celsius',
  filament: 'filament',
  durationHours: 'duration_hours',
  rotateTray: 'rotate_tray',
} as const

type AmsSlotAction = 'ams_load_filament' | 'ams_unload_filament' | 'ams_reread_rfid'

export type PrinterTemperatureAction = 'set_bed_temperature' | 'set_chamber_temperature'
export type PrinterSpeedMode = 1 | 2 | 3 | 4
export type PrinterAxis = 'x' | 'y' | 'z'
export type PrinterAxisMovement = { axis: PrinterAxis; deltaMm: number }

export type PrinterControlIntent =
  | { action: 'home' | 'stop' | 'pause' | 'resume' }
  | { action: 'set_print_speed'; speedMode: PrinterSpeedMode }
  | { action: 'set_fan_speed'; fanIndex: 1 | 2 | 3; speedPercent: number; airduct: boolean }
  | {
      action: 'move_axes'
      movements: readonly [PrinterAxisMovement]
      feedrateMmPerMin: number
    }
  | { action: PrinterTemperatureAction; temperatureCelsius?: number }
  | { action: 'set_hotend_temperature'; extruderId: number | null; temperatureCelsius?: number }
  | { action: 'select_extruder'; extruderId: number }
  | { action: 'set_chamber_light'; lightOn: boolean }
  | {
      action: AmsSlotAction
      amsId: number | null
      slotId: number | null
      globalTrayId: number | null
      externalId: string | null
      extruderId: number | null
    }
  | { action: 'ams_start_drying'; amsId: number }
  | { action: 'ams_stop_drying'; amsId: number }
  | { action: 'nozzle_holder_ctrl'; holderAction: number }
  | { action: 'holder_nozzle_refresh' | 'nozzle_info_confirm'; nozzleId: number }

export function PrinterControlFields({
  printer,
  intent,
}: {
  printer: Printer
  intent: PrinterControlIntent
}) {
  return (
    <>
      <input name="tenant_id" type="hidden" value={printer.tenant_id} />
      <input name="printer_id" type="hidden" value={printer.id} />
      <input name="action" type="hidden" value={intent.action} />
      <IntentFields intent={intent} />
    </>
  )
}

function IntentFields({ intent }: { intent: PrinterControlIntent }) {
  switch (intent.action) {
    case 'home':
    case 'stop':
    case 'pause':
    case 'resume':
      return null
    case 'set_print_speed':
      return <input name="speed_mode" type="hidden" value={intent.speedMode} />
    case 'set_fan_speed':
      return (
        <>
          <input name="fan_index" type="hidden" value={intent.fanIndex} />
          <input name="speed_percent" type="hidden" value={intent.speedPercent} />
          <input name="airduct" type="hidden" value={String(intent.airduct)} />
        </>
      )
    case 'move_axes': {
      const [movement] = intent.movements
      return (
        <>
          <input name="axis" type="hidden" value={movement.axis} />
          <input name="delta_mm" type="hidden" value={movement.deltaMm} />
          <input name="feedrate_mm_per_min" type="hidden" value={intent.feedrateMmPerMin} />
        </>
      )
    }
    case 'set_bed_temperature':
    case 'set_chamber_temperature':
      return intent.temperatureCelsius !== undefined ? (
        <input name="temperature_celsius" type="hidden" value={intent.temperatureCelsius} />
      ) : null
    case 'set_hotend_temperature':
      return (
        <>
          {intent.temperatureCelsius !== undefined ? (
            <input name="temperature_celsius" type="hidden" value={intent.temperatureCelsius} />
          ) : null}
          {intent.extruderId !== null ? (
            <input name="extruder_id" type="hidden" value={intent.extruderId} />
          ) : null}
        </>
      )
    case 'select_extruder':
      return <input name="extruder_id" type="hidden" value={intent.extruderId} />
    case 'set_chamber_light':
      return <input name="light_on" type="hidden" value={String(intent.lightOn)} />
    case 'ams_load_filament':
    case 'ams_unload_filament':
    case 'ams_reread_rfid': {
      const includeTarget = intent.action !== 'ams_reread_rfid'
      return (
        <>
          {intent.amsId !== null ? <input name="ams_id" type="hidden" value={intent.amsId} /> : null}
          {intent.slotId !== null ? <input name="slot_id" type="hidden" value={intent.slotId} /> : null}
          {includeTarget && intent.globalTrayId !== null ? (
            <input name="global_tray_id" type="hidden" value={intent.globalTrayId} />
          ) : null}
          {includeTarget && intent.externalId ? (
            <input name="external_id" type="hidden" value={intent.externalId} />
          ) : null}
          {intent.action === 'ams_load_filament' && intent.extruderId !== null ? (
            <input name="extruder_id" type="hidden" value={intent.extruderId} />
          ) : null}
        </>
      )
    }
    case 'ams_start_drying':
    case 'ams_stop_drying':
      return <input name="ams_id" type="hidden" value={intent.amsId} />
    case 'nozzle_holder_ctrl':
      return <input name="holder_action" type="hidden" value={intent.holderAction} />
    case 'holder_nozzle_refresh':
    case 'nozzle_info_confirm':
      return <input name="nozzle_id" type="hidden" value={intent.nozzleId} />
  }
}
