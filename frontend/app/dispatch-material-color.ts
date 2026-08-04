export function materialPayloadColor(value: string | null) {
  const normalized = value?.trim().replace(/^#/, '').toUpperCase() ?? ''
  if (/^[0-9A-F]{6}$/.test(normalized)) return `#${normalized}FF`
  return /^[0-9A-F]{8}$/.test(normalized) ? `#${normalized}` : ''
}

export function materialColorDistance(
  left: string | null,
  right: string | null,
): number | null {
  const a = parsedColor(left)
  const b = parsedColor(right)
  if (!a || !b || a.alpha !== b.alpha) return null
  return Math.hypot(a.red - b.red, a.green - b.green, a.blue - b.blue)
}

function parsedColor(value: string | null) {
  const normalized = value?.trim().replace(/^#/, '')
  if (!normalized || !/^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(normalized)) {
    return null
  }
  return {
    red: Number.parseInt(normalized.slice(0, 2), 16),
    green: Number.parseInt(normalized.slice(2, 4), 16),
    blue: Number.parseInt(normalized.slice(4, 6), 16),
    alpha:
      normalized.length === 8
        ? Number.parseInt(normalized.slice(6, 8), 16)
        : 255,
  }
}
