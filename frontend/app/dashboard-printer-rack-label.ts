import type { PrinterNozzleSystem } from './dashboard-types'

export type RackNozzle = PrinterNozzleSystem['nozzle']['info'][number]
export type Translate = (key: string) => string

export function nozzleLabel(nozzle: RackNozzle, t: Translate, fallback: string) {
  return (
    [nozzleDiameterLabel(nozzle), nozzleMaterialLabel(nozzle.type, t)]
      .filter(Boolean)
      .join(' ') || fallback
  )
}

export function nozzleDiameterLabel(nozzle: RackNozzle) {
  return Number.isFinite(nozzle.diameter)
    ? `${Number(nozzle.diameter.toFixed(2))} mm`
    : null
}

// Maps hotend type codes to material display names. Codes are either full
// text ("hardened_steel") or 4-character codes ("HS01", "XH05") whose last
// two digits carry the material: 00 stainless, 01 hardened, 05 tungsten.
export function nozzleMaterialLabel(
  type: string | null | undefined,
  t: Translate,
): string | null {
  const raw = type?.trim()
  if (!raw) {
    return null
  }
  const lower = raw.toLowerCase()
  if (lower.includes('hardened')) return t('nozzleHardenedSteel')
  if (lower.includes('stainless')) return t('nozzleStainlessSteel')
  if (lower.includes('tungsten')) return t('nozzleTungstenCarbide')
  if (lower.includes('brass')) return t('nozzleBrass')
  if (raw.length >= 4) {
    const material = raw.slice(2, 4)
    if (material === '00') return t('nozzleStainlessSteel')
    if (material === '01') return t('nozzleHardenedSteel')
    if (material === '05') return t('nozzleTungstenCarbide')
  }
  if (raw === '00') return t('nozzleStainlessSteel')
  if (raw === '01') return t('nozzleHardenedSteel')
  if (raw === '05') return t('nozzleTungstenCarbide')
  return raw
}

// Flow type from the hotend code: "HH" high flow, "HS" standard; otherwise
// the second character follows Bambu Studio's map (A/X standard, E high,
// U TPU high, B E3D high). Returns null when the code carries no flow data.
export function nozzleFlowLabel(
  type: string | null | undefined,
  t: Translate,
): string | null {
  const raw = type?.trim()
  if (!raw || raw.length < 2) {
    return null
  }
  if (raw.startsWith('HH')) return t('nozzleHighFlow')
  if (raw.startsWith('HS')) return t('nozzleStandardFlow')
  switch (raw.charAt(1)) {
    case 'A':
    case 'X':
      return t('nozzleStandardFlow')
    case 'E':
      return t('nozzleHighFlow')
    case 'U':
      return t('nozzleTpuHighFlow')
    case 'B':
      return t('nozzleE3dHighFlow')
    default:
      return null
  }
}

export function formatWear(wear: number) {
  if (!Number.isFinite(wear)) {
    return '-'
  }
  return wear <= 1 ? `${Math.round(wear * 100)}%` : wear.toFixed(2)
}
