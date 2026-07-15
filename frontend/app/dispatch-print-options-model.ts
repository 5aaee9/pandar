export type CalibrationMode = 0 | 1 | 2

export type CalibrationOption = {
  modes: readonly CalibrationMode[]
  defaultMode: CalibrationMode
}

export type DispatchPrintOptionCapabilities = {
  timelapse: boolean
  bedLeveling: CalibrationOption | null
  flowCalibration: CalibrationOption | null
  nozzleOffsetCalibration: CalibrationOption | null
}

const onOff: CalibrationOption = { modes: [1, 0], defaultMode: 1 }
const autoOnOff: CalibrationOption = { modes: [2, 1, 0], defaultMode: 2 }
const autoOnOffDefaultOff: CalibrationOption = { modes: [2, 1, 0], defaultMode: 0 }

const unknown: DispatchPrintOptionCapabilities = {
  timelapse: false,
  bedLeveling: null,
  flowCalibration: null,
  nozzleOffsetCalibration: null,
}

const x2d: DispatchPrintOptionCapabilities = {
  timelapse: true,
  bedLeveling: autoOnOff,
  flowCalibration: autoOnOff,
  nozzleOffsetCalibration: autoOnOffDefaultOff,
}

const autoCalibration: DispatchPrintOptionCapabilities = {
  timelapse: true,
  bedLeveling: autoOnOff,
  flowCalibration: autoOnOff,
  nozzleOffsetCalibration: autoOnOff,
}

const autoWithoutNozzle: DispatchPrintOptionCapabilities = {
  timelapse: true,
  bedLeveling: autoOnOff,
  flowCalibration: autoOnOff,
  nozzleOffsetCalibration: null,
}

const onOffCalibration: DispatchPrintOptionCapabilities = {
  timelapse: true,
  bedLeveling: onOff,
  flowCalibration: onOff,
  nozzleOffsetCalibration: null,
}

const bedOnly: DispatchPrintOptionCapabilities = {
  timelapse: true,
  bedLeveling: onOff,
  flowCalibration: null,
  nozzleOffsetCalibration: null,
}

export function dispatchPrintOptionCapabilities(
  model: string | null,
): DispatchPrintOptionCapabilities {
  switch (normalizedModelKey(model)) {
    case 'N6':
    case 'X2D':
      return x2d
    case 'N7':
    case 'P2S':
    case 'O1S':
    case 'H2S':
    case 'N9':
    case 'A2L':
      return autoWithoutNozzle
    case 'N1':
    case 'A1MINI':
    case 'N2S':
    case 'A1':
    case 'BLP001':
    case 'X1C':
    case 'X1CARBON':
    case '3DPRINTERX1CARBON':
    case 'BLP002':
    case 'X1':
    case '3DPRINTERX1':
    case 'C13':
    case 'X1E':
      return onOffCalibration
    case 'C11':
    case 'P1P':
    case 'C12':
    case 'P1S':
      return bedOnly
    case 'O1C':
    case 'O1C2':
    case 'H2C':
    case 'O1D':
    case 'H2D':
    case 'O1E':
    case 'H2DPRO':
      return autoCalibration
    default:
      return unknown
  }
}

function normalizedModelKey(model: string | null) {
  const compact = model?.trim().toUpperCase().replace(/[\s_-]/g, '') ?? ''
  return compact.startsWith('BAMBULAB') ? compact.slice('BAMBULAB'.length) : compact
}
