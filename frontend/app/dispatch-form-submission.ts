export function prepareDispatchSubmission(
  formData: FormData,
  confirmExternalMismatch: () => boolean,
) {
  const printerId = String(formData.get('printer_id') ?? '')
  const externalMaterialMismatch = formData.get('external_material_mismatch') === 'true'
  const mappingValid = formData.get('material_mapping_valid') !== 'false'
  formData.delete('material_mapping_valid')
  if (!mappingValid) return null

  const mappingUsesAms = formData.get('material_mapping_uses_ams')
  formData.delete('material_mapping_uses_ams')
  if (mappingUsesAms !== null) {
    formData.delete('use_ams')
    formData.set('use_ams', String(mappingUsesAms))
  }

  formData.delete('external_material_mismatch')
  if (externalMaterialMismatch && !confirmExternalMismatch()) return null
  return { formData, printerId }
}

export async function dispatchErrorCode(response: Response) {
  try {
    const body = (await response.json()) as { error?: string }
    return body.error ?? `http_${response.status}`
  } catch {
    return `http_${response.status}`
  }
}
